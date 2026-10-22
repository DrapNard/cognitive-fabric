use anyhow::Result;
use cogfabric::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::{Engine, ExecutionProfile},
    registry::{self, Manifest},
    store::Store,
};
use serde_json::{json, Value};
use std::{future::Future, pin::Pin, sync::Arc};

const QUERY: &str = r#"sort:{"values":[2,1]}"#;

struct Backend;

impl InferenceBackend for Backend {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        let text = match role {
            "interface" => {
                json!({
                    "objective": QUERY,
                    "constraints": [],
                    "required_capabilities": (0..12).map(|i| format!("domain.cap{i}")).collect::<Vec<_>>(),
                    "clarification": null
                })
                .to_string()
            }
            "planner" => "invalid graph".to_owned(),
            "solver" if prompt.contains("Provide domain advice") => "Optional advice".to_owned(),
            "solver" => "[1,2]".to_owned(),
            other => panic!("unexpected inference role {other}"),
        };
        Box::pin(async move {
            Ok(Generation {
                text,
                model: "capability-cover-fixture".into(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 0.0,
                queue_wait_ms: Some(0.0),
            })
        })
    }
}

fn manifest(id: &str, aliases: &[String], workflow: bool) -> Manifest {
    serde_json::from_value(json!({
        "id": id,
        "version": "1.0.0",
        "kind": if workflow { "workflow" } else { "reference" },
        "description": "Synthetic bounded capability-cover fixture",
        "tags": ["fixture"],
        "source": "Synthetic regression fixture",
        "license": "CC0-1.0",
        "capabilities": aliases.iter().map(|alias| json!({"id": alias, "version": "1.0.0"})).collect::<Vec<_>>(),
        "workflow": if workflow { Some("Provide domain advice") } else { None }
    }))
    .unwrap()
}

fn add(store: &Store, manifest: &Manifest) {
    registry::register(store, manifest).unwrap();
    registry::activate(store, &manifest.id, &manifest.version).unwrap();
}

fn trace_event<'a>(trace: &'a Value, kind: &str) -> &'a Value {
    trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["kind"] == kind)
        .unwrap_or_else(|| panic!("missing {kind} event"))
}

#[tokio::test]
async fn interpreted_minimum_cover_is_audited_and_used_by_the_engine() {
    let store = Arc::new(Store::in_memory().unwrap());
    let all = (0..12)
        .map(|i| format!("domain.cap{i}"))
        .collect::<Vec<_>>();
    // A greedy-first provider with five declarations makes a complete cover
    // require five routes. The four disjoint providers are the feasible
    // minimum full cover under the engine's existing four-route ceiling.
    add(
        &store,
        &manifest(
            "a.greedy-trap",
            &[
                all[0].clone(),
                all[3].clone(),
                all[6].clone(),
                all[9].clone(),
                all[1].clone(),
            ],
            false,
        ),
    );
    for (id, start) in [
        ("b.group-one", 0),
        ("c.group-two", 3),
        ("d.group-three", 6),
        ("e.group-four", 9),
    ] {
        add(&store, &manifest(id, &all[start..start + 3], true));
    }

    for profile in [
        ExecutionProfile::Full,
        ExecutionProfile::Collective,
        ExecutionProfile::Experts,
    ] {
        let engine = Engine::with_backend(
            Config {
                backend: "openai-local".into(),
                interpret_goals: true,
                max_calls: 8,
                ..Config::default()
            },
            store.clone(),
            Arc::new(Backend),
        );
        let outcome = engine.run_profile(QUERY, "deep", profile).await.unwrap();
        assert_eq!(outcome.status, "completed", "profile {profile:?}");
        assert_eq!(outcome.answer, "[1,2]", "profile {profile:?}");
        assert_eq!(
            outcome.skills,
            vec![
                "b.group-one",
                "c.group-two",
                "d.group-three",
                "e.group-four"
            ],
            "profile {profile:?}"
        );

        let trace = store.task(&outcome.task_id).unwrap();
        assert_eq!(trace_event(&trace, "route")["payload"]["skills"], json!([]));
        let audit = &trace_event(&trace, "capability_resolution")["payload"];
        assert_eq!(audit["algorithm"], "bounded-cover/v3");
        let resolution = &audit["resolution"];
        let search = &resolution["coverage_search"];
        assert_eq!(search["algorithm_version"], "exact-cover/1");
        assert_eq!(search["full_coverage_feasible"], true);
        assert_eq!(search["selected_additional_count"], 4);
        assert_eq!(search["greedy_fallback"], false);
        assert_eq!(search["declaration_only"], true);
        assert_eq!(search["permissions_granted"], false);
        assert_eq!(
            resolution["skills"],
            json!([
                "b.group-one",
                "c.group-two",
                "d.group-three",
                "e.group-four"
            ]),
            "profile {profile:?}"
        );
        assert_eq!(resolution["unselected"], json!([]));
        assert!(resolution["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|binding| binding["selected"] == true && binding["permissions_granted"] == false));
        assert!(!trace["events"].as_array().unwrap().iter().any(|event| {
            event["kind"] == "tool_dispatch" && event["payload"]["capability"] == "shell.execute"
        }));

        if profile == ExecutionProfile::Full {
            let dispatches = trace["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|event| {
                    event["kind"] == "expert_dispatch" && event["payload"]["admitted"] == true
                })
                .collect::<Vec<_>>();
            assert!(!dispatches.is_empty(), "the pinned workflow should be used");
            assert!(dispatches.iter().all(|event| {
                [
                    "b.group-one",
                    "c.group-two",
                    "d.group-three",
                    "e.group-four",
                ]
                .contains(&event["payload"]["id"].as_str().unwrap_or_default())
                    && event["payload"]["version"] == "1.0.0"
            }));
        }
    }
}
