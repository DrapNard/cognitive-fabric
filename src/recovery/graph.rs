//! Validate durable advisory graph history without treating model results as evidence.
use crate::{
    planning::{Plan, Subtask},
    security,
    store::{sha256, Store},
};
use anyhow::{bail, Result};
use serde_json::{json, Value};
impl Store {
    pub fn graph_audit(&self, task: &str) -> Result<Value> {
        security::authorize("human-cli", "task.observe")?;
        uuid::Uuid::parse_str(task)?;
        let mut db = self.lock()?;
        let tx = db.transaction()?;
        let (status, graph, states) = graph_from(&tx, task)?;
        tx.commit()?;
        let nodes = states.iter().map(|s| json!({"id":s.step.id,"depends_on":s.step.depends_on,"objective_hash":sha256(s.step.objective.as_bytes()),"state":s.state,"result_hash":s.result.as_ref().map(|r|sha256(r.as_bytes())),"result_verified":false})).collect::<Vec<_>>();
        Ok(
            json!({"format_version":1,"task_id":task,"task_status":status,"graph_recorded":graph.is_some(),"nodes":nodes,"automatic_replay":false,"resume_authorized":false,"permissions_restored":false}),
        )
    }
}

pub(super) fn graph_from(
    db: &rusqlite::Connection,
    task: &str,
) -> Result<(String, Option<Plan>, Vec<Subtask>)> {
    let status: String = db.query_row(
        "SELECT CASE WHEN length(status)<=64 THEN status END FROM tasks WHERE id=?1",
        [task],
        |r| r.get(0),
    )?;
    let mut graph: Option<Plan> = None;
    let mut states: Vec<Subtask> = Vec::new();
    let mut events = 0;
    {
        let mut stmt = db.prepare("SELECT kind,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind IN ('plan','subtask') ORDER BY id LIMIT 18")?;
        let mut rows = stmt.query([task])?;
        while let Some(row) = rows.next()? {
            events += 1;
            if events > 17 {
                bail!("Graph audit exceeds bounded history")
            }
            let kind: String = row.get(0)?;
            let source: String = row.get(1)?;
            let raw: String = row.get(2)?;
            let payload: Value = serde_json::from_str(&raw)?;
            if kind == "plan" {
                if source != "planner-r"
                    || graph.is_some()
                    || payload["trusted_permissions"] != false
                {
                    bail!("Invalid recorded graph identity")
                }
                let plan = Plan::parse(&serde_json::to_string(&payload["graph"])?)?;
                states = plan
                    .steps
                    .iter()
                    .cloned()
                    .map(|step| Subtask {
                        step,
                        state: "not_recorded".into(),
                        result: None,
                    })
                    .collect();
                graph = Some(plan);
            } else {
                if source != "jev-plus" {
                    bail!("Invalid subtask audit source")
                }
                let node: Subtask = serde_json::from_value(payload["node"].clone())?;
                let index = states
                    .iter()
                    .position(|s| s.step.id == node.step.id)
                    .ok_or_else(|| anyhow::anyhow!("Subtask has no recorded plan"))?;
                let previous = &states[index];
                if serde_json::to_value(&previous.step)? != serde_json::to_value(&node.step)? {
                    bail!("Recorded subtask changed its pinned definition")
                }
                let valid = match previous.state.as_str() {
                    "not_recorded" => node.state == "pending",
                    "pending" => matches!(
                        node.state.as_str(),
                        "running" | "skipped" | "failed" | "cancelled"
                    ),
                    "running" => matches!(
                        node.state.as_str(),
                        "unverified" | "skipped" | "failed" | "cancelled"
                    ),
                    _ => false,
                };
                if !valid
                    || (node.state == "unverified") != node.result.is_some()
                    || node
                        .result
                        .as_ref()
                        .is_some_and(|r| r.chars().count() > 1200)
                {
                    bail!("Invalid recorded subtask state transition")
                }
                if node.state == "running"
                    && node.step.depends_on.iter().any(|id| {
                        !states
                            .iter()
                            .any(|s| &s.step.id == id && s.state == "unverified")
                    })
                {
                    bail!("Subtask ran before dependency results were recorded")
                }
                states[index] = node;
            }
        }
    }
    Ok((status, graph, states))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plan(store: &Store, task: &str) {
        store.event(task,"plan","planner-r",json!({"graph":{"steps":[{"id":"a","objective":"private objective","depends_on":[]},{"id":"b","objective":"combine","depends_on":["a"]}]},"trusted_permissions":false})).unwrap();
    }
    fn node(store: &Store, task: &str, id: &str, state: &str, result: Option<&str>) {
        store.event(task,"subtask","jev-plus",json!({"node":{"step":{"id":id,"objective":if id=="a" {"private objective"} else {"combine"},"depends_on":if id=="a" {vec![]} else {vec!["a"]}},"state":state,"result":result}})).unwrap();
    }
    #[test]
    fn reconstructs_partial_graph_without_claiming_verified_results_or_replay() {
        let store = Store::in_memory().unwrap();
        let task = store.create_task("private goal", "deep").unwrap();
        assert_eq!(store.graph_audit(&task).unwrap()["graph_recorded"], false);
        plan(&store, &task);
        node(&store, &task, "a", "pending", None);
        node(&store, &task, "b", "pending", None);
        node(&store, &task, "a", "running", None);
        node(&store, &task, "a", "unverified", Some("private result"));
        node(&store, &task, "b", "running", None);
        let report = store.graph_audit(&task).unwrap();
        assert_eq!(report["nodes"][0]["state"], "unverified");
        assert_eq!(report["nodes"][1]["state"], "running");
        assert_eq!(report["nodes"][0]["result_verified"], false);
        assert_eq!(report["resume_authorized"], false);
        assert!(!report.to_string().contains("private"));
        let other = store.create_task("other", "fast").unwrap();
        assert_eq!(store.graph_audit(&other).unwrap()["graph_recorded"], false);
    }
    #[test]
    fn rejects_forged_graph_state_and_dependency_order() {
        for variant in 0..6 {
            let store = Store::in_memory().unwrap();
            let task = store.create_task("fixture", "deep").unwrap();
            plan(&store, &task);
            node(&store, &task, "a", "pending", None);
            node(&store, &task, "b", "pending", None);
            match variant {
                0 => node(&store, &task, "b", "running", None),
                1 => node(&store, &task, "a", "unverified", Some("invalid skip")),
                2 => node(&store, &task, "a", "pending", None),
                3 => {
                    node(&store, &task, "a", "running", None);
                    node(&store, &task, "a", "unverified", Some(&"x".repeat(1201)));
                }
                4 => plan(&store, &task),
                _ => {
                    store.event(&task,"subtask","jev-plus",json!({"node":{"step":{"id":"a","objective":"changed","depends_on":[]},"state":"running","result":null}})).unwrap();
                }
            }
            assert!(store.graph_audit(&task).is_err(), "variant {variant}");
        }
    }
}
