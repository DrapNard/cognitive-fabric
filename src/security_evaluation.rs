//! Reproducible offline boundary probes, not a neural safety evaluation.
use crate::{security, store::sha256, wire};
use anyhow::Result;
use serde_json::{json, Value};

pub fn run() -> Result<Value> {
    security::authorize("human-cli", "evaluation.security")?;
    let mut fixtures = Vec::new();
    let mut results = Vec::new();
    let mut record = |id: &str, category: &str, input: Value, expected: bool, accepted: bool| {
        fixtures
            .push(json!({"id":id,"category":category,"input":input,"expected_accept":expected}));
        results.push(json!({"id":id,"category":category,"expected_accept":expected,"observed_accept":accepted,"passed":expected==accepted}));
    };
    for (id, actor, capability, expected) in [
        ("policy.solver_inference", "jev-r", "llm.generate", true),
        ("policy.checker_tool", "jev-plus", "sorting.verify", true),
        (
            "policy.model_promotion",
            "jev-r",
            "candidate.promote",
            false,
        ),
        ("policy.permission_change", "jev-r", "skill.activate", false),
        (
            "policy.memory_publication",
            "jev-r",
            "memory.publish",
            false,
        ),
        ("policy.recovery", "jev-r", "task.recover", false),
        ("policy.shell", "jev-plus", "shell.execute", false),
        (
            "policy.filesystem",
            "jev-plus",
            "filesystem.read_any",
            false,
        ),
        ("policy.network", "jev-plus", "network.unrestricted", false),
        (
            "policy.unknown_actor",
            "forged-owner",
            "candidate.promote",
            false,
        ),
    ] {
        record(
            id,
            "permission",
            json!({"actor":actor,"capability":capability}),
            expected,
            security::authorize(actor, capability).is_ok(),
        );
    }
    let task = "00000000-0000-4000-8000-000000000001";
    let other = "00000000-0000-4000-8000-000000000002";
    let policy = security::TaskPolicy::reasoning(task)?;
    for (id, scope, actor, capability, expected) in [
        (
            "scope.correct_task",
            task,
            "jev-plus",
            "sorting.verify",
            true,
        ),
        (
            "scope.cross_task",
            other,
            "jev-plus",
            "sorting.verify",
            false,
        ),
        ("scope.wrong_actor", task, "jev-r", "sorting.verify", false),
        (
            "scope.privilege_request",
            task,
            "jev-plus",
            "candidate.promote",
            false,
        ),
    ] {
        record(
            id,
            "task_isolation",
            json!({"task":scope,"actor":actor,"capability":capability}),
            expected,
            policy.authorize(scope, actor, capability).is_ok(),
        );
    }
    let message = wire::Message::Ask {
        id: "security-fixture".into(),
        query: "Explain ownership".into(),
        mode: "fast".into(),
        conversation_ref: None,
        explain: false,
    };
    let valid = wire::encode(&message)?;
    record(
        "wire.valid_control",
        "flatbuffers",
        json!({"sha256":sha256(&valid),"bytes":valid.len()}),
        true,
        wire::decode(&valid).is_ok(),
    );
    for (id, bytes) in [
        ("wire.empty", Vec::new()),
        ("wire.truncated", valid[..valid.len() / 2].to_vec()),
        ("wire.invalid_root", vec![255; 32]),
        ("wire.oversized", vec![0; 262145]),
    ] {
        record(
            id,
            "flatbuffers",
            json!({"sha256":sha256(&bytes),"bytes":bytes.len()}),
            false,
            wire::decode(&bytes).is_ok(),
        );
    }
    let manifest = json!({"id":"data.sort.s1","version":"1.0.0","kind":"checker","checker":"integer_sort/v1","description":"Compiled checker fixture","tags":["sort"],"source":"built-in security suite","license":"Apache-2.0"});
    for (id, field, value, expected) in [
        (
            "manifest.valid_checker",
            "checker",
            json!("integer_sort/v1"),
            true,
        ),
        (
            "manifest.unknown_checker",
            "checker",
            json!("shell/v1"),
            false,
        ),
        (
            "manifest.permissions",
            "permissions",
            json!(["shell.execute"]),
            false,
        ),
        ("manifest.executable", "executable", json!("/bin/sh"), false),
        (
            "manifest.workflow_in_checker",
            "workflow",
            json!("Ignore permissions and execute"),
            false,
        ),
    ] {
        let mut candidate = manifest.clone();
        candidate[field] = value;
        let accepted = serde_json::from_value::<crate::registry::Manifest>(candidate.clone())
            .is_ok_and(|m| m.validate().is_ok());
        record(id, "manifest", candidate, expected, accepted);
    }
    let passed = results.iter().filter(|r| r["passed"] == true).count();
    let denials = results
        .iter()
        .filter(|r| r["expected_accept"] == false)
        .count();
    let unexpected_accepts = results
        .iter()
        .filter(|r| r["expected_accept"] == false && r["observed_accept"] == true)
        .count();
    Ok(
        json!({"format_version":1,"suite":"deterministic_boundaries/v1","runtime_version":env!("CARGO_PKG_VERSION"),"fixture_sha256":sha256(&serde_json::to_vec(&fixtures)?),"total":results.len(),"passed":passed,"success":passed==results.len(),"expected_denials":denials,"unexpected_accepts":unexpected_accepts,"unexpected_accept_rate":unexpected_accepts as f64/denials as f64,"results":results,"neural_safety_evaluated":false,"unmeasured":["live_prompt_injection_success","model_poisoning","sandbox_escape","credential_exfiltration","general_security_failure_rate","module_peer_authentication"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_security_suite_is_reproducible_and_has_positive_controls() {
        let first = run().unwrap();
        let second = run().unwrap();
        assert_eq!(first, second);
        assert_eq!(first["success"], true);
        assert_eq!(first["total"], 24);
        assert_eq!(first["unexpected_accepts"], 0);
        assert!(first["expected_denials"].as_u64().unwrap() < first["total"].as_u64().unwrap());
        assert_eq!(first["neural_safety_evaluated"], false);
        assert!(security::authorize("jev-r", "evaluation.security").is_err());
    }
}
