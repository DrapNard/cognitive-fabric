//! Bounded SELECT execution over task-local inline data; never opens host databases.
use anyhow::{bail, Context, Result};
use rusqlite::{
    hooks::{AuthAction, AuthContext, Authorization},
    limits::Limit,
    types::{Value as SqlValue, ValueRef},
    Connection,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    name: String,
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Task {
    tables: Vec<Table>,
    query: String,
}
fn identifier(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        && !name.starts_with("sqlite_")
}
fn cell(value: &Value) -> Result<SqlValue> {
    Ok(match value {
        Value::Null => SqlValue::Null,
        Value::Bool(value) => SqlValue::Integer(i64::from(*value)),
        Value::Number(value) if value.is_i64() => SqlValue::Integer(value.as_i64().unwrap()),
        Value::Number(value) if value.is_f64() => {
            SqlValue::Real(value.as_f64().context("Invalid SQL number")?)
        }
        Value::String(value) if value.len() <= 2048 => SqlValue::Text(value.clone()),
        _ => bail!(
            "SQL cells require bounded strings, signed integers, real numbers, booleans or null"
        ),
    })
}

pub async fn execute(raw: String) -> Result<String> {
    // Bounded SQLite work never occupies a Tokio executor thread.
    static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
    let permit = SLOTS
        .try_acquire()
        .context("SQL worker capacity exhausted")?;
    struct CancelOnDrop(Arc<AtomicBool>);
    impl Drop for CancelOnDrop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let _guard = CancelOnDrop(cancelled.clone());
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        evaluate_inner(&raw, cancelled)
    })
    .await?
}

#[cfg(test)]
fn evaluate(raw: &str) -> Result<String> {
    evaluate_inner(raw, Arc::new(AtomicBool::new(false)))
}
fn evaluate_inner(raw: &str, cancelled: Arc<AtomicBool>) -> Result<String> {
    if cancelled.load(Ordering::Relaxed) {
        bail!("SQL task cancelled")
    }
    if raw.len() > 32_000 {
        bail!("SQL task exceeds input limit")
    }
    let task: Task = serde_json::from_str(raw)?;
    if task.tables.len() > 4 || task.query.trim().is_empty() || task.query.len() > 4096 {
        bail!("Invalid SQL task bounds")
    }
    let mut names = BTreeSet::new();
    for table in &task.tables {
        if cancelled.load(Ordering::Relaxed) {
            bail!("SQL task cancelled")
        }
        let columns = table.columns.iter().collect::<BTreeSet<_>>();
        if !identifier(&table.name)
            || !names.insert(table.name.clone())
            || table.columns.is_empty()
            || table.columns.len() > 16
            || columns.len() != table.columns.len()
            || table.columns.iter().any(|name| !identifier(name))
            || table.rows.len() > 256
        {
            bail!("Invalid or duplicate SQL table/column identity or size")
        }
        for row in &table.rows {
            if row.len() != table.columns.len() {
                bail!("SQL row width mismatch")
            }
            for value in row {
                cell(value)?;
            }
        }
    }
    let db = Connection::open_in_memory()?;
    db.execute_batch("PRAGMA temp_store=MEMORY; PRAGMA max_page_count=128;")?;
    for (limit, maximum) in [
        (Limit::SQLITE_LIMIT_LENGTH, 65536),
        (Limit::SQLITE_LIMIT_SQL_LENGTH, 4096),
        (Limit::SQLITE_LIMIT_COLUMN, 16),
        (Limit::SQLITE_LIMIT_EXPR_DEPTH, 32),
        (Limit::SQLITE_LIMIT_COMPOUND_SELECT, 8),
        (Limit::SQLITE_LIMIT_VDBE_OP, 10000),
        (Limit::SQLITE_LIMIT_FUNCTION_ARG, 16),
        (Limit::SQLITE_LIMIT_ATTACHED, 0),
        (Limit::SQLITE_LIMIT_VARIABLE_NUMBER, 16),
        (Limit::SQLITE_LIMIT_WORKER_THREADS, 0),
    ] {
        db.set_limit(limit, maximum);
    }
    for table in &task.tables {
        let columns = table
            .columns
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(",");
        db.execute_batch(&format!("CREATE TABLE \"{}\" ({columns});", table.name))?;
        let placeholders = vec!["?"; table.columns.len()].join(",");
        let mut insert = db.prepare(&format!(
            "INSERT INTO \"{}\" VALUES ({placeholders})",
            table.name
        ))?;
        for row in &table.rows {
            let values = row.iter().map(cell).collect::<Result<Vec<_>>>()?;
            insert.execute(rusqlite::params_from_iter(values))?;
        }
    }
    db.execute_batch("PRAGMA query_only=ON;")?;
    db.authorizer(Some(move |context: AuthContext<'_>| {
        let allowed = match context.action {
            AuthAction::Select | AuthAction::Recursive => true,
            AuthAction::Read {
                table_name,
                column_name,
            } => {
                // SQLite uses a column-less read with no database name for
                // count(*) and constant projections over an authorized table.
                names.contains(table_name)
                    && (context.database_name == Some("main")
                        || (context.database_name.is_none() && column_name.is_empty()))
            }
            AuthAction::Function { function_name } => matches!(
                function_name.to_ascii_lowercase().as_str(),
                "count"
                    | "sum"
                    | "avg"
                    | "min"
                    | "max"
                    | "abs"
                    | "round"
                    | "lower"
                    | "upper"
                    | "length"
                    | "coalesce"
                    | "ifnull"
                    | "nullif"
                    | "typeof"
                    | "substr"
                    | "trim"
            ),
            _ => false,
        };
        if allowed {
            Authorization::Allow
        } else {
            Authorization::Deny
        }
    }));
    let deadline = Instant::now() + Duration::from_millis(100);
    let mut ticks = 0;
    db.progress_handler(
        100,
        Some(move || {
            ticks += 1;
            ticks >= 1000 || Instant::now() >= deadline || cancelled.load(Ordering::Relaxed)
        }),
    );
    let mut batch = rusqlite::Batch::new(&db, &task.query);
    let mut statement = batch.next()?.context("Missing SQL SELECT")?;
    if batch.next()?.is_some()
        || !statement.readonly()
        || statement.parameter_count() != 0
        || statement.column_count() == 0
    {
        bail!("SQL requires one read-only query without parameters")
    }
    let columns = statement
        .column_names()
        .iter()
        .map(|name| name.to_string())
        .collect::<Vec<_>>();
    if columns.iter().any(|name| name.len() > 128) {
        bail!("SQL output column label exceeds limit")
    }
    let mut result = Vec::new();
    let mut bytes = 0;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        if result.len() >= 64 || Instant::now() >= deadline {
            bail!("SQL result or deadline limit exhausted")
        }
        let mut values = Vec::new();
        for index in 0..columns.len() {
            values.push(match row.get_ref(index)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(value) => json!(value),
                ValueRef::Real(value) if value.is_finite() => json!(value),
                ValueRef::Text(value) => Value::String(std::str::from_utf8(value)?.to_string()),
                _ => bail!("SQL result contains unsupported blob or nonfinite number"),
            });
        }
        bytes += serde_json::to_vec(&values)?.len();
        if bytes > 16000 {
            bail!("SQL result exceeds byte limit")
        }
        result.push(values);
    }
    let answer = json!({"columns":columns,"rows":result}).to_string();
    if answer.len() > 16384 {
        bail!("SQL response exceeds byte limit")
    }
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(query: &str) -> String {
        json!({"tables":[{"name":"items","columns":["name","amount"],"rows":[["a",2],["b",3]]}],"query":query}).to_string()
    }
    #[test]
    fn columnless_reads_allow_only_declared_inline_tables() {
        let answer: Value =
            serde_json::from_str(&evaluate(&task("SELECT count(*) AS n FROM items")).unwrap())
                .unwrap();
        assert_eq!(answer, json!({"columns":["n"],"rows":[[2]]}));
        let answer: Value =
            serde_json::from_str(&evaluate(&task("SELECT 1 AS value FROM main.items")).unwrap())
                .unwrap();
        assert_eq!(answer, json!({"columns":["value"],"rows":[[1],[1]]}));
        for query in [
            "SELECT count(*) FROM sqlite_schema",
            "SELECT 1 FROM sqlite_schema",
            "SELECT count(*) FROM temp.items",
            "SELECT count(*) FROM missing",
            "ATTACH DATABASE ':memory:' AS other",
        ] {
            assert!(evaluate(&task(query)).is_err(), "{query}");
        }
    }

    #[test]
    fn aggregates_only_inline_data_and_preserves_types() {
        assert!(evaluate(r#"{"tables":[],"query":"SELECT * FROM items"}"#).is_err());
        assert!(evaluate(&task("WITH RECURSIVE x(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM x WHERE n<5) SELECT sum(n) FROM x")).unwrap().contains("15"));
        let result: Value = serde_json::from_str(
            &evaluate(&task(
                "SELECT sum(amount) AS total, count(*) AS n FROM items",
            ))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result, json!({"columns":["total","n"],"rows":[[5,2]]}));
        assert!(
            evaluate(&task("SELECT name FROM items ORDER BY amount DESC"))
                .unwrap()
                .contains("b")
        );
    }
    #[test]
    fn denies_external_access_mutations_extensions_and_multiple_statements() {
        for query in [
            "ATTACH '/tmp/forbidden.db' AS external",
            "PRAGMA database_list",
            "DELETE FROM items",
            "CREATE TABLE bad(x)",
            "SELECT * FROM sqlite_master",
            "SELECT load_extension('/tmp/evil')",
            "SELECT readfile('/etc/passwd')",
            "SELECT * FROM pragma_database_list",
            "SELECT 1e308 * 1e308",
            "SELECT randomblob(10000000)",
            "SELECT 1; SELECT 2",
            "SELECT ?1",
            "VACUUM INTO '/tmp/forbidden.db'",
        ] {
            assert!(
                evaluate(&task(query)).is_err(),
                "Unexpectedly accepted {query}"
            );
        }
    }
    #[test]
    fn bounds_recursion_rows_input_and_identifiers() {
        let exhausted = evaluate(&task(
            "WITH RECURSIVE x(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM x) SELECT sum(n) FROM x",
        ))
        .unwrap_err();
        assert!(exhausted.to_string().contains("interrupted"));
        assert!(
            evaluate_inner(&task("SELECT 1"), Arc::new(AtomicBool::new(true)))
                .unwrap_err()
                .to_string()
                .contains("cancelled")
        );
        assert!(evaluate(&task("WITH RECURSIVE x(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM x WHERE n<65) SELECT n FROM x")).is_err());
        assert!(evaluate(&" ".repeat(32001)).is_err());
        assert!(evaluate(&task("SELECT 1").replace("\"items\"", "\"sqlite_secret\"")).is_err());
        assert!(evaluate(&task("SELECT 1").replace("[\"a\",2]", "[\"a\"]")).is_err());
    }
}
