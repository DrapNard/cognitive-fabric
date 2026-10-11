//! Typed inline SQL RPC; JSON is parsed at the CLI/backend boundary only.
use super::{Descriptor, Identity};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::{future::Future, pin::Pin};
#[allow(dead_code, unused_imports, clippy::all)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/sql_tool_generated.rs"));
}
use generated::sql_tool as fb;
pub mod unix;
pub trait SqlBackend: Send + Sync {
    fn module_descriptor(&self) -> Option<&Descriptor> {
        None
    }

    fn query<'a>(
        &'a self,
        task: &'a str,
        input: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>>;
}
pub(super) struct LocalSql;
impl SqlBackend for LocalSql {
    fn query<'a>(
        &'a self,
        _: &'a str,
        input: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(crate::sql::execute(input.to_string()))
    }
}
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Output {
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
}
fn output(raw: &str) -> Result<Output> {
    if raw.len() > 16384 {
        bail!("SQL response exceeds bounds")
    }
    let output: Output = serde_json::from_str(raw)?;
    if output.columns.is_empty()
        || output.columns.len() > 16
        || output.columns.iter().any(|c| c.len() > 128)
        || output.rows.len() > 64
        || output.rows.iter().any(|r| r.len() != output.columns.len())
    {
        bail!("SQL response shape exceeds bounds")
    }
    Ok(output)
}
fn cell<'a>(
    b: &mut flatbuffers::FlatBufferBuilder<'a>,
    value: &Value,
) -> Result<flatbuffers::WIPOffset<fb::Cell<'a>>> {
    let mut args = fb::CellArgs::default();
    match value {
        Value::Null => (),
        Value::Bool(v) => {
            args.kind = fb::Kind::Boolean;
            args.boolean = *v;
        }
        Value::Number(v) if v.is_i64() => {
            args.kind = fb::Kind::Integer;
            args.integer = v.as_i64().unwrap();
        }
        Value::Number(v) if v.is_f64() => {
            args.kind = fb::Kind::Real;
            args.real = v.as_f64().context("Invalid SQL real")?;
        }
        Value::String(v) if v.len() <= 65536 => {
            args.kind = fb::Kind::Text;
            args.text = Some(b.create_string(v));
        }
        _ => bail!("Invalid SQL cell type or size"),
    }
    Ok(fb::Cell::create(b, &args))
}
fn rows<'a>(
    b: &mut flatbuffers::FlatBufferBuilder<'a>,
    rows: &[Vec<Value>],
) -> Result<
    flatbuffers::WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<fb::Row<'a>>>>,
> {
    let mut encoded = Vec::new();
    for row in rows {
        let cells = row.iter().map(|v| cell(b, v)).collect::<Result<Vec<_>>>()?;
        let cells = b.create_vector(&cells);
        encoded.push(fb::Row::create(b, &fb::RowArgs { cells: Some(cells) }));
    }
    Ok(b.create_vector(&encoded))
}
fn decode_rows(
    rows: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::Row<'_>>>,
    max: usize,
    budget: &mut usize,
) -> Result<Vec<Vec<Value>>> {
    if rows.len() > max {
        bail!("SQL row count exceeds bounds")
    }
    rows.iter()
        .map(|row| {
            if row.cells().len() > 16 {
                bail!("SQL column count exceeds bounds")
            }
            row.cells()
                .iter()
                .map(|v| {
                    // Inactive scalar fields must retain defaults; reject mixed values.
                    if (v.kind() != fb::Kind::Integer && v.integer() != 0)
                        || (v.kind() != fb::Kind::Real && v.real() != 0.0)
                        || (v.kind() != fb::Kind::Text && v.text().is_some())
                        || (v.kind() != fb::Kind::Boolean && v.boolean())
                    {
                        bail!("Mixed SQL scalar fields")
                    }
                    let value = match v.kind() {
                        fb::Kind::Null => Value::Null,
                        fb::Kind::Integer => Value::from(v.integer()),
                        fb::Kind::Real => Value::Number(
                            serde_json::Number::from_f64(v.real()).context("Nonfinite SQL real")?,
                        ),
                        fb::Kind::Boolean => Value::Bool(v.boolean()),
                        fb::Kind::Text => {
                            let text = v.text().context("Missing SQL text")?;
                            if text.len() > 65536 {
                                bail!("SQL text exceeds bounds")
                            }
                            if text.len() > *budget {
                                bail!("SQL decoded text exceeds byte budget")
                            }
                            Value::String(text.into())
                        }
                        _ => bail!("Unknown SQL scalar kind"),
                    };
                    let size = serde_json::to_vec(&value)?.len();
                    *budget = budget
                        .checked_sub(size)
                        .context("SQL decoded data exceeds byte budget")?;
                    Ok(value)
                })
                .collect()
        })
        .collect()
}
fn columns(
    columns: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<&str>>,
    max_bytes: usize,
) -> Result<Vec<String>> {
    if columns.is_empty() || columns.len() > 16 {
        bail!("SQL columns exceed bounds")
    }
    columns
        .iter()
        .map(|c| {
            if c.len() > max_bytes {
                bail!("SQL column identity exceeds bounds")
            }
            Ok(c.into())
        })
        .collect()
}
fn encode(
    identity: &Identity,
    request: Option<&str>,
    result: Option<&str>,
    failed: bool,
) -> Result<Vec<u8>> {
    let mut b = flatbuffers::FlatBufferBuilder::new();
    let request = if let Some(raw) = request {
        let task = crate::sql::parse_task(raw)?;
        let mut tables = Vec::new();
        for table in task.tables {
            let name = b.create_string(&table.name);
            let cs = table
                .columns
                .iter()
                .map(|c| b.create_string(c))
                .collect::<Vec<_>>();
            let cs = b.create_vector(&cs);
            let rs = rows(&mut b, &table.rows)?;
            tables.push(fb::Table::create(
                &mut b,
                &fb::TableArgs {
                    name: Some(name),
                    columns: Some(cs),
                    rows: Some(rs),
                },
            ));
        }
        let tables = b.create_vector(&tables);
        let query = b.create_string(&task.query);
        Some(fb::Request::create(
            &mut b,
            &fb::RequestArgs {
                tables: Some(tables),
                query: Some(query),
            },
        ))
    } else {
        None
    };
    let result = if let Some(raw) = result {
        let out = output(raw)?;
        let cs = out
            .columns
            .iter()
            .map(|c| b.create_string(c))
            .collect::<Vec<_>>();
        let cs = b.create_vector(&cs);
        let rs = rows(&mut b, &out.rows)?;
        Some(fb::QueryResult::create(
            &mut b,
            &fb::QueryResultArgs {
                columns: Some(cs),
                rows: Some(rs),
            },
        ))
    } else {
        None
    };
    let args = fb::PacketArgs {
        version: 1,
        correlation_id: Some(b.create_string(&identity.correlation)),
        task_id: Some(b.create_string(&identity.task)),
        actor: Some(b.create_string(&identity.actor)),
        module_id: Some(b.create_string(&identity.module)),
        module_version: Some(b.create_string(&identity.version)),
        expires_at_ms: identity.expires,
        request,
        result,
        failed,
    };
    let root = fb::Packet::create(&mut b, &args);
    fb::finish_packet_buffer(&mut b, root);
    let bytes = b.finished_data().to_vec();
    decode(&bytes)?;
    Ok(bytes)
}
struct Decoded {
    identity: Identity,
    request: Option<String>,
    result: Option<String>,
    failed: bool,
}
fn decode(bytes: &[u8]) -> Result<Decoded> {
    if !(8..=262144).contains(&bytes.len()) || !fb::packet_buffer_has_identifier(bytes) {
        bail!("Invalid SQL capability frame")
    }
    let options = flatbuffers::VerifierOptions {
        max_depth: 16,
        max_tables: 25000,
        max_apparent_size: 1_048_576,
        ..Default::default()
    };
    let p = fb::root_as_packet_with_opts(&options, bytes)?;
    if p.version() != 1 || p.actor() != "jev-plus" {
        bail!("SQL protocol or actor denied")
    }
    uuid::Uuid::parse_str(p.task_id())?;
    uuid::Uuid::parse_str(p.correlation_id())?;
    Descriptor {
        id: p.module_id().into(),
        version: p.module_version().into(),
        capability: "sql.query".into(),
    }
    .validate()?;
    if !matches!(
        (p.request().is_some(), p.result().is_some(), p.failed()),
        (true, false, false) | (false, true, false) | (false, false, true)
    ) {
        bail!("Invalid SQL packet shape")
    }
    let request = p
        .request()
        .map(|r| -> Result<String> {
            if r.tables().len() > 4 || r.query().len() > 4096 {
                bail!("SQL request exceeds bounds")
            }
            let mut budget = 32000;
            let tables = r
                .tables()
                .iter()
                .map(|t| -> Result<crate::sql::Table> {
                    if t.name().len() > 32 {
                        bail!("SQL table identity exceeds bounds")
                    }
                    Ok(crate::sql::Table {
                        name: t.name().into(),
                        columns: columns(t.columns(), 32)?,
                        rows: decode_rows(t.rows(), 256, &mut budget)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let raw = serde_json::to_string(&crate::sql::Task {
                tables,
                query: r.query().into(),
            })?;
            crate::sql::parse_task(&raw)?;
            Ok(raw)
        })
        .transpose()?;
    let result = p
        .result()
        .map(|r| -> Result<String> {
            let mut budget = 16384;
            let raw = serde_json::to_string(&Output {
                columns: columns(r.columns(), 128)?,
                rows: decode_rows(r.rows(), 64, &mut budget)?,
            })?;
            output(&raw)?;
            Ok(raw)
        })
        .transpose()?;
    Ok(Decoded {
        identity: Identity {
            correlation: p.correlation_id().into(),
            task: p.task_id().into(),
            actor: p.actor().into(),
            module: p.module_id().into(),
            version: p.module_version().into(),
            expires: p.expires_at_ms(),
        },
        request,
        result,
        failed: p.failed(),
    })
}
pub(super) fn request(identity: &Identity, raw: &str) -> Result<Vec<u8>> {
    encode(identity, Some(raw), None, false)
}
pub(super) fn response(bytes: &[u8], identity: &Identity) -> Result<String> {
    let decoded = decode(bytes)?;
    if decoded.identity != *identity
        || decoded.identity.expires <= crate::fabric::protocol::now_ms()
        || decoded.failed
    {
        bail!("SQL response identity, deadline or execution failed")
    }
    decoded.result.context("SQL worker did not return a result")
}
pub(super) async fn execute(
    bytes: &[u8],
    identity: &Identity,
    descriptor: &Descriptor,
    backend: &dyn SqlBackend,
) -> Result<Vec<u8>> {
    let decoded = decode(bytes)?;
    if decoded.identity != *identity
        || identity.module != descriptor.id
        || identity.version != descriptor.version
        || identity.expires <= crate::fabric::protocol::now_ms()
    {
        bail!("SQL request identity or deadline mismatch")
    }
    crate::security::TaskPolicy::reasoning(&identity.task)?.authorize(
        &identity.task,
        &identity.actor,
        "sql.query",
    )?;
    let raw = decoded.request.context("SQL worker expects a request")?;
    let result = backend.query(&identity.task, &raw).await;
    match result {
        Ok(result) if output(&result).is_ok() => encode(identity, None, Some(&result), false),
        _ => encode(identity, None, None, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    fn descriptor(version: &str) -> Descriptor {
        Descriptor {
            id: "sql.local".into(),
            version: version.into(),
            capability: "sql.query".into(),
        }
    }
    fn identity() -> Identity {
        Identity {
            correlation: uuid::Uuid::new_v4().to_string(),
            task: uuid::Uuid::new_v4().to_string(),
            actor: "jev-plus".into(),
            module: "sql.local".into(),
            version: "1.0.0".into(),
            expires: crate::fabric::protocol::now_ms() + 30000,
        }
    }
    fn input() -> String {
        serde_json::json!({"tables":[{"name":"items","columns":["value"],"rows":[[2],[3]]}],"query":"SELECT sum(value) AS total FROM items"}).to_string()
    }
    struct Fixed(&'static str);
    impl SqlBackend for Fixed {
        fn query<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
            Box::pin(async move { Ok(self.0.into()) })
        }
    }
    #[test]
    fn structured_round_trip_preserves_cell_types_and_rejects_mixed_packets() {
        let identity = identity();
        let raw = serde_json::json!({"tables":[{"name":"items","columns":["a","b","c","d","e"],"rows":[[null,true,-9223372036854775808i64,1.5,"text"]]}],"query":"SELECT * FROM items"}).to_string();
        let bytes = request(&identity, &raw).unwrap();
        assert!(!bytes.windows(8).any(|w| w == b"\"tables\""));
        let decoded = decode(&bytes).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&decoded.request.unwrap()).unwrap(),
            serde_json::from_str::<Value>(&raw).unwrap()
        );
        assert!(decode(&[0; 16]).is_err());
        assert!(decode(&vec![0; 262145]).is_err());
        assert!(encode(
            &identity,
            Some(&raw),
            Some(r#"{"columns":["x"],"rows":[[1]]}"#),
            false
        )
        .is_err());
        let mut wrong = identity.clone();
        wrong.actor = "jev-r".into();
        assert!(request(&wrong, &raw).is_err());
        let response_bytes = encode(
            &identity,
            None,
            Some(r#"{"columns":["x"],"rows":[[1]]}"#),
            false,
        )
        .unwrap();
        wrong = identity.clone();
        wrong.task = uuid::Uuid::new_v4().to_string();
        assert!(response(&response_bytes, &wrong).is_err());
    }
    #[test]
    fn aliased_rows_cannot_expand_past_decoded_budget() {
        let i = identity();
        let mut b = flatbuffers::FlatBufferBuilder::new();
        let text = b.create_string(&"a".repeat(100));
        let cell = fb::Cell::create(
            &mut b,
            &fb::CellArgs {
                kind: fb::Kind::Text,
                text: Some(text),
                ..Default::default()
            },
        );
        let cells = b.create_vector(&[cell; 16]);
        let row = fb::Row::create(&mut b, &fb::RowArgs { cells: Some(cells) });
        let rows = b.create_vector(&[row; 64]);
        let label = b.create_string("x");
        let columns = b.create_vector(&[label; 16]);
        let result = fb::QueryResult::create(
            &mut b,
            &fb::QueryResultArgs {
                columns: Some(columns),
                rows: Some(rows),
            },
        );
        let args = fb::PacketArgs {
            correlation_id: Some(b.create_string(&i.correlation)),
            task_id: Some(b.create_string(&i.task)),
            actor: Some(b.create_string(&i.actor)),
            module_id: Some(b.create_string(&i.module)),
            module_version: Some(b.create_string(&i.version)),
            expires_at_ms: i.expires,
            result: Some(result),
            ..Default::default()
        };
        let root = fb::Packet::create(&mut b, &args);
        fb::finish_packet_buffer(&mut b, root);
        assert!(b.finished_data().len() < 4096);
        let error = decode(b.finished_data()).err().unwrap().to_string();
        assert!(error.contains("budget"), "{error}");
    }
    #[tokio::test]
    async fn capability_lookup_preserves_sql_pins_and_separates_arithmetic() {
        let broker = super::super::Broker::default();
        let old = broker.pin_capability("sql.query").unwrap().unwrap();
        let task = uuid::Uuid::new_v4().to_string();
        broker
            .register_sql(
                descriptor("2.0.0"),
                Arc::new(Fixed(r#"{"columns":["total"],"rows":[[99]]}"#)),
            )
            .unwrap();
        let new = broker.pin_capability("sql.query").unwrap().unwrap();
        let answer = old.query_sql(&task, "jev-plus", &input()).await.unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&answer).unwrap()["rows"][0][0],
            5
        );
        assert!(new
            .query_sql(&task, "jev-plus", &input())
            .await
            .unwrap()
            .contains("99"));
        assert!(new.query_sql(&task, "jev-r", &input()).await.is_err());
        assert!(new.evaluate(&task, "jev-plus", "6*7").await.is_err());
        assert!(broker
            .pin()
            .unwrap()
            .unwrap()
            .query_sql(&task, "jev-plus", &input())
            .await
            .is_err());
        let mut conflict = descriptor("3.0.0");
        conflict.id = "arithmetic.local".into();
        assert!(broker.register_sql(conflict, Arc::new(LocalSql)).is_err());
        assert!(broker.pin_capability("shell.run").is_err());
        broker.unregister("sql.local").unwrap();
        assert!(broker.pin_capability("sql.query").unwrap().is_none());
        assert!(broker.pin().unwrap().is_some());
        broker.shutdown().unwrap();
        assert!(old.query_sql(&task, "jev-plus", &input()).await.is_err());
        assert!(new.query_sql(&task, "jev-plus", &input()).await.is_err());
    }
    #[tokio::test]
    async fn engine_rejects_wrong_and_malformed_sql_backend_answers_without_proof() {
        for answer in [
            r#"{"columns":["total"],"rows":[[5]]}"#,
            r#"{"columns":["total"],"rows":[[99]]}"#,
            "invalid",
        ] {
            let store = Arc::new(crate::store::Store::in_memory().unwrap());
            let engine =
                crate::engine::Engine::new(crate::config::Config::default(), store.clone());
            engine
                .tools
                .register_sql(descriptor("2.0.0"), Arc::new(Fixed(answer)))
                .unwrap();
            let out = engine
                .run(&format!("sql:{}", input()), "fast")
                .await
                .unwrap();
            let correct = answer.contains("[5]");
            assert_eq!(out.status, if correct { "completed" } else { "failed" });
            assert_eq!(out.calls, 0);
            assert_eq!(out.resources.tool_calls, 2);
            assert_eq!(
                out.evidence
                    .iter()
                    .any(|e| e.starts_with("verified:sqlite_inline")),
                correct
            );
            let trace = store.task(&out.task_id).unwrap();
            assert_eq!(
                trace["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|e| e["kind"] == "tool_dispatch"
                        && e["payload"]["module_version"] == "2.0.0")
                    .count(),
                1
            );
        }
    }
    #[tokio::test]
    async fn shutdown_drops_active_sql_future_and_rejects_queued_requests() {
        struct Pending {
            entered: Arc<tokio::sync::Notify>,
            dropped: Arc<std::sync::atomic::AtomicBool>,
        }
        impl SqlBackend for Pending {
            fn query<'a>(
                &'a self,
                _: &'a str,
                _: &'a str,
            ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
                Box::pin(async {
                    struct Guard(Arc<std::sync::atomic::AtomicBool>);
                    impl Drop for Guard {
                        fn drop(&mut self) {
                            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
                        }
                    }
                    let _guard = Guard(self.dropped.clone());
                    self.entered.notify_one();
                    std::future::pending().await
                })
            }
        }
        let broker = super::super::Broker::default();
        let entered = Arc::new(tokio::sync::Notify::new());
        let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        broker
            .register_sql(
                descriptor("2.0.0"),
                Arc::new(Pending {
                    entered: entered.clone(),
                    dropped: dropped.clone(),
                }),
            )
            .unwrap();
        let first = broker.pin_capability("sql.query").unwrap().unwrap();
        let second = first.clone();
        let task = uuid::Uuid::new_v4().to_string();
        let task2 = task.clone();
        let active =
            tokio::spawn(async move { first.query_sql(&task, "jev-plus", &input()).await });
        entered.notified().await;
        let queued =
            tokio::spawn(async move { second.query_sql(&task2, "jev-plus", &input()).await });
        broker.shutdown().unwrap();
        assert!(active.await.unwrap().is_err());
        assert!(queued.await.unwrap().is_err());
        assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
    }
    #[tokio::test]
    async fn sql_budget_denial_precedes_dispatch_and_external_access_stays_denied() {
        let store = Arc::new(crate::store::Store::in_memory().unwrap());
        let engine = crate::engine::Engine::new(
            crate::config::Config {
                max_tool_calls: 1,
                ..Default::default()
            },
            store.clone(),
        );
        let out = engine
            .run(&format!("sql:{}", input()), "fast")
            .await
            .unwrap();
        assert_eq!(out.status, "failed");
        let trace = store.task(&out.task_id).unwrap();
        assert!(!trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "tool_dispatch"));
        let engine = crate::engine::Engine::new(crate::config::Config::default(), store);
        let out = engine
            .run(
                r#"sql:{"tables":[],"query":"SELECT readfile('/etc/passwd')"}"#,
                "fast",
            )
            .await
            .unwrap();
        assert_eq!(out.status, "rejected");
        assert!(!out
            .evidence
            .iter()
            .any(|e| e.starts_with("verified:sqlite_inline")));
    }
}
