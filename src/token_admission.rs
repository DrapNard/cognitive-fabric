//! Exact-wire-input token admission for llama.cpp's native server API.
//!
//! This adapter accounts for the token IDs actually submitted to `/completion`.
//! It does not attest tokenizer correctness or immutable model weights.

use crate::{
    backend::{self, Generation, ProviderMetricSource, ProviderMetrics},
    config::Config,
};
use anyhow::{bail, Context, Result};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
use tokio::process::Command;
use url::Url;

const ENVIRONMENT_VARIABLE: &str = "COGFABRIC_LLAMA_API_KEY";
const MAX_RENDERED_BYTES: usize = 256 * 1024;
const MAX_CONTROL_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_COMPLETION_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_COMPLETION_TEXT_BYTES: usize = 32_000;
const MAX_TOKEN_IDS: usize = 65_536;
const MAX_ADMISSION_EVENTS: usize = 64;
const MAX_EVENT_BYTES: usize = 4096;

/// Per-task limits for the exact token-ID input sent to llama.cpp.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub format_version: u32,
    pub max_input_tokens: u64,
    pub max_context_tokens: u32,
    #[serde(default)]
    pub authenticate: bool,
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        if self.format_version != 1
            || !(1..=16_777_216).contains(&self.max_input_tokens)
            || !(128..=131_072).contains(&self.max_context_tokens)
        {
            bail!("Invalid exact-token admission settings")
        }
        Ok(())
    }
}

/// Validate that this opt-in adapter can address only the expected llama.cpp
/// loopback API and cannot inherit role-specific model or effort overrides.
pub(crate) fn validate_config(cfg: &Config) -> Result<()> {
    let settings = cfg
        .input_token_admission
        .as_ref()
        .context("Exact-token admission is not configured")?;
    settings.validate()?;
    if cfg.backend != "openai-local"
        || !cfg.role_profiles.is_empty()
        || cfg.model.trim().is_empty()
        || cfg.model.len() > 256
        || cfg.model.chars().any(char::is_control)
        || cfg
            .reasoning_effort
            .as_deref()
            .is_some_and(|effort| effort != "none")
    {
        bail!("Exact-token admission requires openai-local without role profiles or provider reasoning effort")
    }
    let endpoint = backend::local_endpoint(&cfg.api_base)?;
    if endpoint.path() != "/v1/chat/completions" {
        bail!("Exact-token admission requires a /v1 API base")
    }
    Ok(())
}

/// Forward only this adapter's independently named key to an explicitly
/// authenticated managed child. The primary inference credential is untouched.
pub(crate) fn forward_credentials(cfg: &Config, command: &mut Command) -> Result<()> {
    command.env_remove(ENVIRONMENT_VARIABLE);
    let Some(settings) = cfg.input_token_admission.as_ref() else {
        return Ok(());
    };
    settings.validate()?;
    if !settings.authenticate {
        return Ok(());
    }
    let key = std::env::var(ENVIRONMENT_VARIABLE)
        .context("Dedicated llama.cpp API credential is unavailable")?;
    validate_bearer(&key)?;
    command.env(ENVIRONMENT_VARIABLE, key);
    Ok(())
}

/// Render, tokenize, durably charge, and submit a fixed llama.cpp completion.
/// `task` must be the UUID of an existing running task; the caller remains
/// responsible for supplying only role-only, tool-free chat messages.
pub(crate) async fn generate(
    cfg: &Config,
    task: &str,
    messages: Value,
    max_tokens: usize,
    temperature: f64,
) -> Result<Generation> {
    Ok(
        generate_with_finish(cfg, task, messages, max_tokens, temperature)
            .await?
            .0,
    )
}

pub(crate) async fn generate_with_finish(
    cfg: &Config,
    task: &str,
    messages: Value,
    max_tokens: usize,
    temperature: f64,
) -> Result<(Generation, &'static str)> {
    validate_config(cfg)?;
    let settings = cfg.input_token_admission.expect("validated setting");
    let task_id = uuid::Uuid::parse_str(task).context("Token admission requires a task UUID")?;
    if task_id.to_string() != task
        || max_tokens == 0
        || max_tokens > 16_384
        || !temperature.is_finite()
        || !(0.0..=2.0).contains(&temperature)
    {
        bail!("Invalid token-admission request")
    }
    validate_messages(&messages)?;

    let started = Instant::now();
    let deadline = started + Duration::from_secs(cfg.timeout_seconds);
    validate_task_before_preparation(cfg, task, deadline)?;
    let api = canonical_v1_base(&cfg.api_base)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let credential = if settings.authenticate {
        let key = std::env::var(ENVIRONMENT_VARIABLE)
            .context("Dedicated llama.cpp API credential is unavailable")?;
        validate_bearer(&key)?;
        Some(crate::credentials::Bearer::new(key)?)
    } else {
        None
    };

    let render_url = route(&api, "/apply-template")?;
    let rendered_value = request_json(
        &client,
        &render_url,
        json!({"messages":messages}),
        &credential,
        deadline,
        MAX_CONTROL_RESPONSE_BYTES,
    )
    .await?;
    let rendered = rendered_value
        .get("prompt")
        .and_then(Value::as_str)
        .context("llama.cpp template response has no prompt")?;
    if rendered.is_empty() || rendered.len() > MAX_RENDERED_BYTES {
        bail!("Rendered model input exceeds its byte limit")
    }

    let tokenize_url = route(&api, "/tokenize")?;
    let tokenized = request_json(
        &client,
        &tokenize_url,
        json!({"content":rendered,"add_special":false,"parse_special":true,"with_pieces":false}),
        &credential,
        deadline,
        MAX_CONTROL_RESPONSE_BYTES,
    )
    .await?;
    let ids = parse_token_ids(&tokenized)?;
    let input_tokens = u64::try_from(ids.len())?;
    let context_limit = u64::from(settings.max_context_tokens);
    if input_tokens
        .checked_add(u64::try_from(max_tokens)?)
        .is_none_or(|total| total > context_limit)
    {
        bail!("Exact input tokens plus output reservation exceed the configured context limit")
    }

    let cumulative = record_admission(Reservation {
        cfg,
        task,
        api: &api,
        model: &cfg.model,
        settings,
        input_tokens,
        output_tokens_reserved: u64::try_from(max_tokens)?,
        ids: &ids,
        deadline,
    })?;
    if cumulative > settings.max_input_tokens {
        // `record_admission` checks before commit; this is an invariant guard.
        bail!("Task input-token budget exceeded")
    }

    if Instant::now() >= deadline {
        bail!("llama.cpp request deadline exhausted before completion dispatch")
    }
    let completion_url = route(&api, "/completion")?;
    let response = request_json(
        &client,
        &completion_url,
        json!({"prompt":ids,"n_predict":max_tokens,"temperature":temperature,"stream":false,"cache_prompt":false}),
        &credential,
        deadline,
        MAX_COMPLETION_RESPONSE_BYTES,
    )
    .await?;
    if response.get("model").and_then(Value::as_str) != Some(cfg.model.as_str()) {
        bail!("llama.cpp completion model alias does not match the configured model")
    }
    if response.get("truncated").and_then(Value::as_bool) != Some(false) {
        bail!("llama.cpp completion was truncated or omitted its truncation status")
    }
    let text = response
        .get("content")
        .and_then(Value::as_str)
        .context("llama.cpp completion response has no content")?;
    if text.trim().is_empty() || text.len() > MAX_COMPLETION_TEXT_BYTES {
        bail!("llama.cpp completion content is empty or exceeds its byte limit")
    }
    let output_tokens = optional_u64(&response, "tokens_predicted")?;
    if output_tokens.is_some_and(|count| count > max_tokens as u64) {
        bail!("llama.cpp completion exceeded the reserved output-token count")
    }
    let reported_input = optional_u64(&response, "tokens_evaluated")?;
    let provider_metrics = llama_cpp_provider_metrics(response.get("timings"))?;
    let finish = match response.get("stop_type").and_then(Value::as_str) {
        Some("eos" | "word") => "stop",
        Some("limit") => "length",
        _ => bail!("llama.cpp completion omitted a supported stop reason"),
    };
    Ok((
        Generation {
            text: text.to_owned(),
            model: cfg.model.clone(),
            input_tokens: reported_input,
            output_tokens,
            elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
            queue_wait_ms: None,
            provider_metrics,
        },
        finish,
    ))
}

fn llama_cpp_provider_metrics(timings: Option<&Value>) -> Result<Option<ProviderMetrics>> {
    let Some(timings) = timings.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let timings = timings
        .as_object()
        .context("llama.cpp timings must be an object")?;
    let mut metrics = ProviderMetrics::new(ProviderMetricSource::LlamaCppNative);
    metrics.prompt_processing_ms = optional_timing(timings, "prompt_ms")?;
    metrics.generation_ms = optional_timing(timings, "predicted_ms")?;
    metrics.output_tokens_per_second = optional_timing(timings, "predicted_per_second")?;
    if metrics.prompt_processing_ms.is_none()
        && metrics.generation_ms.is_none()
        && metrics.output_tokens_per_second.is_none()
    {
        return Ok(None);
    }
    metrics.validate()?;
    Ok(Some(metrics))
}

fn optional_timing(timings: &serde_json::Map<String, Value>, name: &str) -> Result<Option<f64>> {
    timings
        .get(name)
        .map(|value| {
            value
                .as_f64()
                .with_context(|| format!("llama.cpp timing {name} must be a number"))
        })
        .transpose()
}

fn validate_messages(messages: &Value) -> Result<()> {
    let array = messages
        .as_array()
        .context("Chat messages must be an array")?;
    if array.is_empty()
        || array.len() > 128
        || serde_json::to_vec(messages)?.len() > MAX_RENDERED_BYTES
    {
        bail!("Chat message history exceeds bounds")
    }
    for message in array {
        let object = message
            .as_object()
            .context("Chat message must be an object")?;
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "role" | "content"))
        {
            bail!("Chat messages may contain only role and content")
        }
        let role = object.get("role").and_then(Value::as_str).unwrap_or("");
        if !matches!(role, "system" | "developer" | "user" | "assistant")
            || !object.get("content").is_some_and(Value::is_string)
        {
            bail!("Chat messages must be tool-free system, user, or assistant text")
        }
    }
    Ok(())
}

fn parse_token_ids(value: &Value) -> Result<Vec<i32>> {
    let tokens = value
        .get("tokens")
        .and_then(Value::as_array)
        .context("llama.cpp tokenizer response has no token array")?;
    if tokens.is_empty() || tokens.len() > MAX_TOKEN_IDS {
        bail!("llama.cpp token array is empty or exceeds its entry limit")
    }
    tokens
        .iter()
        .map(|token| {
            let value = token
                .as_u64()
                .context("Token ID must be an unsigned integer")?;
            if value > i32::MAX as u64 {
                bail!("Token ID exceeds llama.cpp's supported integer range")
            }
            Ok(value as i32)
        })
        .collect()
}

fn optional_u64(value: &Value, key: &str) -> Result<Option<u64>> {
    value
        .get(key)
        .map(|item| {
            item.as_u64()
                .with_context(|| format!("Invalid llama.cpp {key} usage"))
        })
        .transpose()
}

fn canonical_v1_base(base: &str) -> Result<Url> {
    let mut completion = backend::local_endpoint(base)?;
    if completion.path() != "/v1/chat/completions" {
        bail!("Exact-token admission requires the /v1 route")
    }
    completion.set_path("/v1");
    completion.set_query(None);
    completion.set_fragment(None);
    Ok(completion)
}

fn route(api: &Url, path: &str) -> Result<Url> {
    let mut url = api.clone();
    url.set_path(path);
    Ok(url)
}

async fn request_json(
    client: &reqwest::Client,
    url: &Url,
    payload: Value,
    credential: &Option<crate::credentials::Bearer>,
    deadline: Instant,
    max_bytes: usize,
) -> Result<Value> {
    let execute = async {
        let mut request = client.post(url.clone()).json(&payload);
        if let Some(credential) = credential {
            request = credential.apply(request)?;
        }
        let mut response = request.send().await?.error_for_status()?;
        if !response.status().is_success() {
            bail!("llama.cpp returned a non-success status")
        }
        if response
            .content_length()
            .is_some_and(|length| length > max_bytes as u64)
        {
            bail!("llama.cpp response exceeds byte limit")
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if chunk.len() > max_bytes.saturating_sub(body.len()) {
                bail!("llama.cpp response exceeds byte limit")
            }
            body.extend_from_slice(&chunk);
        }
        if let Some(credential) = credential {
            credential.reject_echo(&body)?;
        }
        let value = crate::strict_json::value(&body, max_bytes)
            .map_err(|_| anyhow::anyhow!("Invalid llama.cpp JSON response"))?;
        if let Some(credential) = credential {
            credential.reject_echo(&serde_json::to_vec(&value)?)?;
        }
        Ok(value)
    };
    tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), execute)
        .await
        .map_err(|_| anyhow::anyhow!("llama.cpp request deadline exhausted"))?
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AdmissionEvent {
    format_version: u32,
    task_id: String,
    api_base: String,
    target_model: String,
    settings: Settings,
    input_tokens: u64,
    output_tokens_reserved: u64,
    input_sha256: String,
    cumulative_input_tokens: u64,
    event_sha256: String,
}

// Preparation can expose the full prompt and consume provider work. Refuse
// stale task identities before contacting either native preparation endpoint.
// This read is advisory: record_admission rechecks status under its write lock.
fn validate_task_before_preparation(cfg: &Config, task: &str, deadline: Instant) -> Result<()> {
    let db = open_admission_database(cfg, deadline, false)?;
    let schema: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if schema != crate::store::CURRENT_SCHEMA_VERSION {
        bail!("Token admission requires the current database schema")
    }
    let status: Option<String> = db
        .query_row("SELECT status FROM tasks WHERE id=?1", [task], |row| {
            row.get(0)
        })
        .optional()?;
    if status.as_deref() != Some("running") {
        bail!("Token preparation requires an existing running task")
    }
    if Instant::now() >= deadline {
        bail!("Token preparation deadline exhausted during task inspection")
    }
    Ok(())
}

fn open_admission_database(
    cfg: &Config,
    deadline: Instant,
    writable: bool,
) -> Result<rusqlite::Connection> {
    if Instant::now() >= deadline {
        bail!("Token admission deadline exhausted before database open")
    }
    let db_path = cfg.db();
    let metadata = std::fs::symlink_metadata(&db_path)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > cfg.max_database_bytes
    {
        bail!("Token-admission database is not a bounded regular file")
    }
    let access = if writable {
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let db = crate::storage::wal_vfs::open(
        &db_path,
        access | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        bail!("Token admission deadline exhausted while opening database")
    }
    db.busy_timeout(remaining.min(Duration::from_millis(250)))?;
    db.progress_handler(1000, Some(move || Instant::now() >= deadline));
    Ok(db)
}

struct Reservation<'a> {
    cfg: &'a Config,
    task: &'a str,
    api: &'a Url,
    model: &'a str,
    settings: Settings,
    input_tokens: u64,
    output_tokens_reserved: u64,
    ids: &'a [i32],
    deadline: Instant,
}

fn record_admission(reservation: Reservation<'_>) -> Result<u64> {
    let Reservation {
        cfg,
        task,
        api,
        model,
        settings,
        input_tokens,
        output_tokens_reserved,
        ids,
        deadline,
    } = reservation;
    let api_base = api.as_str().trim_end_matches('/').to_owned();
    let model = model.to_owned();
    let ids_sha256 = hex::encode(Sha256::digest(serde_json::to_vec(ids)?));
    let task = task.to_owned();
    let mut db = open_admission_database(cfg, deadline, true)?;
    let result = (|| -> Result<u64> {
        if Instant::now() >= deadline {
            bail!("Token admission deadline exhausted after database open")
        }
        crate::storage::configure(&db, cfg.max_database_bytes)?;
        db.pragma_update(None, "foreign_keys", "ON")?;
        let schema: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if schema != crate::store::CURRENT_SCHEMA_VERSION {
            bail!("Token admission requires the current database schema")
        }
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if Instant::now() >= deadline {
            bail!("Token admission deadline exhausted before ledger inspection")
        }
        let status: Option<String> = tx
            .query_row("SELECT status FROM tasks WHERE id=?1", [&task], |row| {
                row.get(0)
            })
            .optional()?;
        if status.as_deref() != Some("running") {
            bail!("Token admission requires an existing running task")
        }
        let mut statement = tx.prepare(
            "SELECT CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END,source \
             FROM events WHERE task_id=?1 AND kind='input_token_admission' \
             ORDER BY id LIMIT 65",
        )?;
        let mut rows = statement.query([&task])?;
        let mut cumulative = 0u64;
        let mut count = 0usize;
        while let Some(row) = rows.next()? {
            if Instant::now() >= deadline {
                bail!("Token-admission history inspection deadline exhausted")
            }
            if count >= MAX_ADMISSION_EVENTS {
                bail!("Task token-admission history exceeds its event limit")
            }
            let raw: Option<String> = row.get(0)?;
            let raw = raw.context("Token-admission event exceeds its byte limit")?;
            let source: String = row.get(1)?;
            if source != "jev-r" {
                bail!("Token-admission event has an unexpected source")
            }
            let event: AdmissionEvent =
                serde_json::from_str(&raw).context("Malformed token-admission event")?;
            let mut event_body = event.clone();
            event_body.event_sha256.clear();
            if event.format_version != 1
                || event.task_id != task
                || event.api_base != api_base
                || event.target_model != model
                || event.settings != settings
                || event.input_tokens == 0
                || event.input_tokens > MAX_TOKEN_IDS as u64
                || !(1..=16_384).contains(&event.output_tokens_reserved)
                || event
                    .input_tokens
                    .checked_add(event.output_tokens_reserved)
                    .is_none_or(|total| total > u64::from(settings.max_context_tokens))
                || event.input_sha256.len() != 64
                || !event
                    .input_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                || event.event_sha256 != crate::store::sha256(&serde_json::to_vec(&event_body)?)
            {
                bail!("Token-admission history identity or format differs")
            }
            cumulative = cumulative
                .checked_add(event.input_tokens)
                .context("Token-admission history overflowed")?;
            if event.cumulative_input_tokens != cumulative || cumulative > settings.max_input_tokens
            {
                bail!("Token-admission history cumulative count is inconsistent")
            }
            count += 1;
        }
        drop(rows);
        drop(statement);
        if count >= MAX_ADMISSION_EVENTS {
            bail!("Task token-admission history has reached its event limit")
        }
        if input_tokens == 0
            || input_tokens > MAX_TOKEN_IDS as u64
            || !(1..=16_384).contains(&output_tokens_reserved)
            || input_tokens
                .checked_add(output_tokens_reserved)
                .is_none_or(|total| total > u64::from(settings.max_context_tokens))
        {
            bail!("Input and output token reservation exceeds its bounds")
        }
        let next = cumulative
            .checked_add(input_tokens)
            .context("Task token budget overflowed")?;
        if next > settings.max_input_tokens {
            bail!("Task input-token budget is exhausted")
        }
        let mut event = AdmissionEvent {
            format_version: 1,
            task_id: task.clone(),
            api_base,
            target_model: model,
            settings,
            input_tokens,
            output_tokens_reserved,
            input_sha256: ids_sha256,
            cumulative_input_tokens: next,
            event_sha256: String::new(),
        };
        event.event_sha256 = crate::store::sha256(&serde_json::to_vec(&event)?);
        let raw = serde_json::to_string(&event)?;
        if raw.len() > MAX_EVENT_BYTES {
            bail!("Token-admission audit payload exceeds its byte limit")
        }
        if Instant::now() >= deadline {
            bail!("Token admission deadline exhausted before audit commit")
        }
        tx.execute(
            "INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'input_token_admission','jev-r',?2,unixepoch())",
            rusqlite::params![task, raw],
        )?;
        tx.commit()?;
        Ok(next)
    })();
    db.progress_handler(0, None::<fn() -> bool>);
    if Instant::now() >= deadline && result.is_ok() {
        bail!("Token admission deadline exhausted after ledger commit")
    }
    result
}

fn validate_bearer(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 4096
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._~+/=".contains(&byte))
    {
        bail!("Invalid dedicated llama.cpp bearer credential")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use std::path::PathBuf;

    fn settings() -> Settings {
        Settings {
            format_version: 1,
            max_input_tokens: 4096,
            max_context_tokens: 8192,
            authenticate: false,
        }
    }

    fn ledger_fixture(max_input_tokens: u64) -> (PathBuf, Config, Store, String, Url, Settings) {
        let root =
            std::env::temp_dir().join(format!("cogfabric-token-ledger-{}", uuid::Uuid::new_v4()));
        let cfg = Config {
            home: root.clone(),
            ..Config::default()
        };
        let store = Store::open(&cfg.db()).unwrap();
        let task = store.create_task("bounded fixture task", "fast").unwrap();
        let api = canonical_v1_base(&cfg.api_base).unwrap();
        let settings = Settings {
            max_input_tokens,
            max_context_tokens: 128,
            ..settings()
        };
        (root, cfg, store, task, api, settings)
    }

    fn charge_fixture(
        cfg: &Config,
        task: &str,
        api: &Url,
        settings: Settings,
        input_tokens: u64,
    ) -> Result<u64> {
        let ids = vec![1_i32; input_tokens as usize];
        record_admission(Reservation {
            cfg,
            task,
            api,
            model: &cfg.model,
            settings,
            input_tokens,
            output_tokens_reserved: 2,
            ids: &ids,
            deadline: Instant::now() + Duration::from_secs(5),
        })
    }

    #[test]
    fn settings_are_versioned_bounded_and_default_authentication_off() {
        let parsed: Settings = serde_json::from_value(json!({
            "format_version": 1,
            "max_input_tokens": 1,
            "max_context_tokens": 128
        }))
        .unwrap();
        assert!(!parsed.authenticate);
        parsed.validate().unwrap();

        for invalid in [
            Settings {
                format_version: 2,
                ..settings()
            },
            Settings {
                max_input_tokens: 0,
                ..settings()
            },
            Settings {
                max_input_tokens: 16_777_217,
                ..settings()
            },
            Settings {
                max_context_tokens: 127,
                ..settings()
            },
            Settings {
                max_context_tokens: 131_073,
                ..settings()
            },
        ] {
            assert!(invalid.validate().is_err());
        }
        assert!(serde_json::from_value::<Settings>(json!({
            "format_version":1,
            "max_input_tokens":1,
            "max_context_tokens":128,
            "unexpected":true
        }))
        .is_err());
    }

    #[test]
    fn unsupported_configuration_fails_closed() {
        let mut cfg = Config {
            backend: "openai-local".into(),
            input_token_admission: Some(settings()),
            ..Config::default()
        };
        validate_config(&cfg).unwrap();
        cfg.backend = "demo".into();
        assert!(validate_config(&cfg).is_err());
        cfg.backend = "openai-local".into();
        cfg.reasoning_effort = Some("high".into());
        assert!(validate_config(&cfg).is_err());
        cfg.reasoning_effort = None;
        cfg.api_base = "https://example.com/v1".into();
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn chat_history_rejects_tool_roles_and_extra_fields() {
        validate_messages(&json!([
            {"role":"system","content":"rules"},
            {"role":"user","content":"question"}
        ]))
        .unwrap();
        for invalid in [
            json!([]),
            json!([{"role":"tool","content":"execute"}]),
            json!([{"role":"assistant","content":"x","tool_calls":[]}]),
            json!([{"role":"user","content":12}]),
        ] {
            assert!(validate_messages(&invalid).is_err());
        }
    }

    #[test]
    fn token_ids_require_a_bounded_nonempty_unsigned_i32_array() {
        assert_eq!(
            parse_token_ids(&json!({"tokens":[0,1,2147483647]})).unwrap(),
            [0, 1, i32::MAX]
        );
        for invalid in [
            json!({"tokens":[]}),
            json!({"tokens":[-1]}),
            json!({"tokens":[2147483648_u64]}),
            json!({"tokens":"1,2"}),
        ] {
            assert!(parse_token_ids(&invalid).is_err());
        }
    }

    #[test]
    fn bearer_validation_rejects_control_and_header_delimiters() {
        for accepted in ["a", "token-._~+/=", "0123456789"] {
            validate_bearer(accepted).unwrap();
        }
        for rejected in ["", "has space", "line\nbreak", "é"] {
            assert!(validate_bearer(rejected).is_err());
        }
    }

    #[test]
    fn ledger_keeps_failed_attempt_charged_and_refuses_over_budget_retry() {
        let (root, cfg, store, task, api, settings) = ledger_fixture(3);
        assert_eq!(charge_fixture(&cfg, &task, &api, settings, 2).unwrap(), 2);
        // The earlier provider attempt may have failed; admission is not refunded.
        assert!(charge_fixture(&cfg, &task, &api, settings, 2).is_err());
        let events = store.task(&task).unwrap()["events"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(
            events
                .iter()
                .filter(|event| event["kind"] == "input_token_admission")
                .count(),
            1
        );
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ledger_corruption_and_configuration_identity_changes_fail_closed() {
        for tamper in ["corrupt", "identity"] {
            let (root, cfg, store, task, api, settings) = ledger_fixture(8);
            charge_fixture(&cfg, &task, &api, settings, 1).unwrap();
            {
                let db = store.lock().unwrap();
                if tamper == "corrupt" {
                    db.execute(
                        "UPDATE events SET payload='{}' WHERE task_id=?1 AND kind='input_token_admission'",
                        [&task],
                    )
                    .unwrap();
                } else {
                    db.execute(
                        "UPDATE events SET payload=json_set(payload,'$.target_model','other') WHERE task_id=?1 AND kind='input_token_admission'",
                        [&task],
                    )
                    .unwrap();
                }
            }
            assert!(charge_fixture(&cfg, &task, &api, settings, 1).is_err());
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}

#[cfg(test)]
mod provider_timing_tests {
    use super::*;

    #[test]
    fn llama_metrics_preserve_missing_values_and_reject_invalid_numbers() {
        for value in [None, Some(json!(null)), Some(json!({"other": 1}))] {
            assert!(llama_cpp_provider_metrics(value.as_ref())
                .unwrap()
                .is_none());
        }
        let metrics = llama_cpp_provider_metrics(Some(&json!({
            "prompt_ms": 2.5, "predicted_ms": 8.0, "predicted_per_second": 125.0
        })))
        .unwrap()
        .unwrap();
        assert_eq!(metrics.prompt_processing_ms, Some(2.5));
        assert_eq!(metrics.generation_ms, Some(8.0));
        assert_eq!(metrics.output_tokens_per_second, Some(125.0));
        assert_eq!(metrics.model_load_ms, None);
        for value in [
            json!([]),
            json!({"prompt_ms": null}),
            json!({"prompt_ms": "1"}),
            json!({"predicted_ms": -1}),
            json!({"predicted_per_second": 1000000001_u64}),
        ] {
            assert!(llama_cpp_provider_metrics(Some(&value)).is_err());
        }
    }
}
