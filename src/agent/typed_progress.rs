//! Bounded, host-owned progress snapshots for already checked typed work.
//!
//! These snapshots are advisory context for trusted host orchestration. They
//! are never durable evidence or proof of the user's original goal.

use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;

const MAX_OPERATIONS: usize = 4;
const MAX_OPERATION_ID_BYTES: usize = 64;
const MAX_RESULT_BYTES: usize = 16_384;
const MAX_CONTEXT_BYTES: usize = 32_768;
const MAX_CHECKER_BYTES: usize = 128;

/// Cloneable latest-value state; fields remain private and cannot be forged by
/// deserializing model or provider output.
#[derive(Clone)]
pub(super) struct Snapshot {
    root_query_sha256: String,
    expected_operation_ids: Vec<String>,
    completed: Vec<Part>,
    failed: bool,
    complete: bool,
}

/// One completed and independently checked typed operation.
#[derive(Clone)]
pub(super) struct Part {
    operation_id: String,
    result: String,
    result_sha256: String,
    criterion_sha256: String,
    checker: String,
    provider_identity: Value,
    provider_identity_sha256: String,
    origin_query_sha256: String,
    original_goal_semantics_verified: bool,
    language_translated: bool,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct Context<'a> {
    trust: &'static str,
    status: &'static str,
    root_query_sha256: &'a str,
    expected_operation_ids: Vec<&'a String>,
    completed: Vec<Projection<'a>>,
    original_goal_semantics_verified: bool,
    durable_final_proof: bool,
}

#[derive(Serialize)]
struct Projection<'a> {
    operation_id: &'a str,
    pointer: String,
    result: Box<serde_json::value::RawValue>,
    result_sha256: String,
    source_result_sha256: &'a str,
    criterion_sha256: &'a str,
    checker: &'a str,
    provider_identity: &'a Value,
    provider_identity_sha256: &'a str,
    origin_query_sha256: &'a str,
    original_goal_semantics_verified: bool,
    language_translated: bool,
}

impl Snapshot {
    pub(super) fn pending(
        root_query_sha256: &str,
        expected_operation_ids: &[String],
    ) -> Result<Self> {
        validate_digest(root_query_sha256, "root query")?;
        if expected_operation_ids.is_empty() || expected_operation_ids.len() > MAX_OPERATIONS {
            bail!("Typed progress operation count is outside its bound")
        }
        let mut seen = BTreeSet::new();
        for id in expected_operation_ids {
            validate_id(id)?;
            if !seen.insert(id.as_str()) {
                bail!("Typed progress operation IDs must be unique")
            }
        }
        Ok(Self {
            root_query_sha256: root_query_sha256.to_owned(),
            expected_operation_ids: expected_operation_ids.to_vec(),
            completed: Vec::new(),
            failed: false,
            complete: false,
        })
    }

    pub(super) fn completed_ids(&self) -> impl Iterator<Item = &str> {
        self.completed.iter().map(|part| part.operation_id.as_str())
    }

    pub(super) fn expected_ids(&self) -> impl Iterator<Item = &str> {
        self.expected_operation_ids.iter().map(String::as_str)
    }

    pub(super) fn failed(&self) -> bool {
        self.failed
    }

    pub(super) fn complete(&self) -> bool {
        self.complete
    }

    pub(super) fn terminal(&self) -> bool {
        self.failed || self.complete()
    }

    pub(super) fn context_for(&self, required: &[super::typed_input::Input]) -> Result<String> {
        if required.len() > 8 {
            bail!("Typed input selector count exceeds its bound")
        }
        let mut selectors = BTreeSet::new();
        let mut required_ids = BTreeSet::new();
        let mut completed = Vec::new();
        for input in required {
            input.validate()?;
            if !selectors.insert((input.operation_id(), input.pointer()))
                || !self
                    .expected_operation_ids
                    .iter()
                    .any(|id| id == input.operation_id())
            {
                bail!("Typed context selector is duplicated or names an unknown prerequisite")
            }
            required_ids.insert(input.operation_id());
            if let Some(part) = self
                .completed
                .iter()
                .find(|part| part.operation_id == input.operation_id())
            {
                let selected =
                    super::typed_experts::select_raw_output(&part.result, input.pointer())?;
                let result = serde_json::value::RawValue::from_string(selected)?;
                completed.push(Projection {
                    operation_id: &part.operation_id,
                    pointer: input.pointer().to_owned(),
                    result_sha256: crate::store::sha256(result.get().as_bytes()),
                    result,
                    source_result_sha256: &part.result_sha256,
                    criterion_sha256: &part.criterion_sha256,
                    checker: &part.checker,
                    provider_identity: &part.provider_identity,
                    provider_identity_sha256: &part.provider_identity_sha256,
                    origin_query_sha256: &part.origin_query_sha256,
                    original_goal_semantics_verified: part.original_goal_semantics_verified,
                    language_translated: part.language_translated,
                });
            }
        }
        let status = if self.failed {
            "failed"
        } else if self.complete {
            "complete"
        } else {
            "partial"
        };
        let context = Context {
            trust: "untrusted checked-operation context; not a final answer or proof",
            status,
            root_query_sha256: &self.root_query_sha256,
            expected_operation_ids: self
                .expected_operation_ids
                .iter()
                .filter(|id| required_ids.contains(id.as_str()))
                .collect(),
            completed,
            // Typed operation criteria do not prove the user's original goal.
            original_goal_semantics_verified: false,
            durable_final_proof: false,
        };
        let text = serde_json::to_string(&context)?;
        if text.len() > MAX_CONTEXT_BYTES {
            bail!("Typed progress context exceeds its byte bound")
        }
        Ok(text)
    }

    pub(super) fn record(&mut self, part: Part) -> Result<()> {
        if self.failed || self.complete {
            bail!("Cannot add a part to terminal typed progress")
        }
        validate_id(&part.operation_id)?;
        if !self
            .expected_operation_ids
            .iter()
            .any(|id| id == &part.operation_id)
            || self
                .completed
                .iter()
                .any(|item| item.operation_id == part.operation_id)
        {
            bail!("Typed progress part is unexpected or duplicated")
        }
        validate_digest(&part.result_sha256, "result")?;
        validate_digest(&part.criterion_sha256, "criterion")?;
        validate_digest(&part.provider_identity_sha256, "provider identity")?;
        validate_digest(&part.origin_query_sha256, "origin query")?;
        if serde_json::to_vec(&part.provider_identity)?.len() > 2_048
            || part.result.len() > MAX_RESULT_BYTES
            || part.checker.is_empty()
            || part.checker.len() > MAX_CHECKER_BYTES
        {
            bail!("Typed progress part exceeds its field bound")
        }
        self.completed.push(part);
        Ok(())
    }

    pub(super) fn mark_failed(&mut self) {
        if !self.complete {
            self.failed = true;
        }
    }

    pub(super) fn mark_complete(&mut self) -> Result<()> {
        if self.failed
            || self.completed.len() != self.expected_operation_ids.len()
            || self
                .expected_operation_ids
                .iter()
                .any(|id| !self.completed.iter().any(|part| &part.operation_id == id))
        {
            bail!("Typed progress cannot complete before all operations are checked")
        }
        self.complete = true;
        Ok(())
    }
}

impl Part {
    pub(super) fn checked(
        operation_id: String,
        result: String,
        criterion_sha256: String,
        checker: String,
        provider_identity: Value,
        origin_query_sha256: String,
        language_translated: bool,
    ) -> Result<Self> {
        let result = result.trim().to_owned();
        let result_sha256 = crate::store::sha256(result.as_bytes());
        let provider_identity_sha256 =
            crate::store::sha256(&serde_json::to_vec(&provider_identity)?);
        Ok(Self {
            operation_id,
            result,
            result_sha256,
            criterion_sha256,
            checker,
            provider_identity,
            provider_identity_sha256,
            origin_query_sha256,
            original_goal_semantics_verified: false,
            language_translated,
        })
    }
}

fn validate_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > MAX_OPERATION_ID_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        bail!("Typed progress operation ID is invalid")
    }
    Ok(())
}

fn validate_digest(value: &str, name: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("Typed progress {name} digest is invalid")
    }
    Ok(())
}
