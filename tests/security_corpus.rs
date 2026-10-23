//! Regression coverage for the bounded deterministic offline security corpus.
use cogfabric::{
    security_evaluation::corpus::evaluate_file,
    store::sha256,
    wire::{self, ControlAction, Message},
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

struct CorpusFile(PathBuf);
impl CorpusFile {
    fn new(bytes: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "cogfabric-security-corpus-{}.json",
            uuid::Uuid::new_v4()
        ));
        fs::write(&path, bytes).unwrap();
        Self(path)
    }

    fn write(&self, bytes: &[u8]) {
        fs::write(&self.0, bytes).unwrap();
    }

    fn digest(bytes: &[u8]) -> String {
        sha256(bytes)
    }
}
impl Drop for CorpusFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn manifest(cases: Vec<Value>) -> Value {
    json!({
        "format_version":1,
        "id":"reviewed.boundaries.v1",
        "source":"operator-reviewed deterministic fixtures",
        "source_sha256":"a".repeat(64),
        "license":"CC0-1.0",
        "split":"held_out",
        "review":{
            "reviewer":"security-reviewer",
            "reviewed":true,
            "expected_labels_independent":true
        },
        "cases":cases
    })
}

fn global_case(id: &str, principal: &str, capability: &str, expected: bool) -> Value {
    json!({
        "id":id,
        "probe":{"kind":"global_policy","principal":principal,"capability":capability},
        "expected_accept":expected
    })
}

#[test]
fn fixed_probes_score_independent_labels_and_disclose_review_limits() {
    let valid_wire = wire::encode(&Message::Control {
        id: "security-control".into(),
        action: ControlAction::Status,
        task_id: None,
        after: 0,
    })
    .unwrap();
    let valid_wire_hex = valid_wire
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let cases = vec![
        global_case("global.allowed", "human-cli", "evaluation.security", true),
        global_case("global.denied", "jev-r", "candidate.promote", false),
        json!({
            "id":"task.same_scope",
            "probe":{
                "kind":"task_policy",
                "policy":"reasoning",
                "policy_task":"00000000-0000-4000-8000-000000000001",
                "request_task":"00000000-0000-4000-8000-000000000001",
                "actor":"jev-plus",
                "capability":"sorting.verify"
            },
            "expected_accept":true
        }),
        json!({
            "id":"task.cross_scope",
            "probe":{
                "kind":"task_policy",
                "policy":"reasoning",
                "policy_task":"00000000-0000-4000-8000-000000000001",
                "request_task":"00000000-0000-4000-8000-000000000002",
                "actor":"jev-plus",
                "capability":"sorting.verify"
            },
            "expected_accept":false
        }),
        json!({
            "id":"json.unique",
            "probe":{"kind":"strict_json","input":"{\"key\":[1,true]}"},
            "expected_accept":true
        }),
        json!({
            "id":"json.duplicate",
            "probe":{"kind":"strict_json","input":"{\"key\":1,\"key\":2}"},
            "expected_accept":false
        }),
        json!({
            "id":"json.fixed_size_gate",
            "probe":{"kind":"strict_json","input":format!("\"{}\"","x".repeat(16_385))},
            "expected_accept":false
        }),
        json!({
            "id":"frame.valid",
            "probe":{"kind":"flatbuffers_wire","bytes_hex":valid_wire_hex},
            "expected_accept":true
        }),
        json!({
            "id":"frame.malformed",
            "probe":{"kind":"flatbuffers_wire","bytes_hex":"ffffffff"},
            "expected_accept":false
        }),
    ];
    let value = manifest(cases);
    let bytes = serde_json::to_vec(&value).unwrap();
    let file = CorpusFile::new(&bytes);
    let report = evaluate_file(&file.0, &CorpusFile::digest(&bytes)).unwrap();

    assert_eq!(report["manifest_id"], "reviewed.boundaries.v1");
    assert_eq!(report["split"], "held_out");
    assert_eq!(report["total"], 9);
    assert_eq!(report["passed"], 9);
    assert_eq!(report["success"], true);
    assert_eq!(report["expected_accepts"], 4);
    assert_eq!(report["expected_denials"], 5);
    assert_eq!(report["unexpected_accepts"], 0);
    assert_eq!(report["unexpected_denials"], 0);
    assert_eq!(report["file_sha256"], CorpusFile::digest(&bytes));
    assert_eq!(report["provenance"]["source_sha256"], "a".repeat(64));
    assert_eq!(
        report["review_attestation"]["expected_labels_independent"],
        true
    );
    assert_eq!(
        report["review_attestation"]["independently_verified"],
        false
    );
    assert_eq!(report["claims"]["arbitrary_code_executed"], false);
    assert_eq!(report["claims"]["model_executed"], false);
    assert_eq!(report["claims"]["neural_safety_evaluated"], false);
    assert_eq!(report["claims"]["sandbox_safety_evaluated"], false);
    assert!(report.get("promoted_model").is_none());
    assert!(report.get("promotion_approved").is_none());
    assert_eq!(report["results"].as_array().unwrap().len(), 9);
}

#[test]
fn exact_file_hash_is_required_before_manifest_evaluation() {
    let bytes = serde_json::to_vec(&manifest(vec![global_case(
        "allowed",
        "human-cli",
        "evaluation.security",
        true,
    )]))
    .unwrap();
    let file = CorpusFile::new(&bytes);
    assert!(evaluate_file(&file.0, &"0".repeat(64)).is_err());
    assert!(evaluate_file(&file.0, "not-a-digest").is_err());
    assert!(evaluate_file(&file.0, &CorpusFile::digest(&bytes)).is_ok());
}

#[test]
fn manifest_rejects_unknown_fields_probes_duplicate_ids_and_metadata_gate_changes() {
    let file = CorpusFile::new(b"{}");
    let base_case = global_case("one", "human-cli", "evaluation.security", true);
    let cases = [
        {
            let mut value = manifest(vec![base_case.clone()]);
            value["unreviewed_extension"] = json!(true);
            value
        },
        {
            let mut value = manifest(vec![base_case.clone()]);
            value["cases"][0]["probe"]["arbitrary_code"] = json!("execute me");
            value
        },
        {
            let mut value = manifest(vec![base_case.clone()]);
            value["cases"][0]["probe"] = json!({
                "kind":"arbitrary_code",
                "source":"unsafe payload"
            });
            value
        },
        {
            manifest(vec![json!({
                "id":"fixed_gate",
                "probe":{"kind":"strict_json","input":"{}","max_bytes":1},
                "expected_accept":true
            })])
        },
        {
            manifest(vec![
                base_case.clone(),
                global_case("one", "jev-r", "candidate.promote", false),
            ])
        },
        {
            let mut value = manifest(vec![base_case]);
            value["review"]["reviewed"] = json!(false);
            value
        },
        {
            let mut value = manifest(vec![global_case(
                "one",
                "human-cli",
                "evaluation.security",
                true,
            )]);
            value["split"] = json!("promotion_approved");
            value
        },
    ];
    for value in cases {
        let bytes = serde_json::to_vec(&value).unwrap();
        file.write(&bytes);
        assert!(
            evaluate_file(&file.0, &CorpusFile::digest(&bytes)).is_err(),
            "invalid corpus was accepted: {value}"
        );
    }
}

#[test]
fn parser_rejects_duplicate_keys_excessive_depth_and_probe_bounds() {
    let file = CorpusFile::new(b"{}");
    let duplicate = br#"{"format_version":1,"format_version":1}"#;
    file.write(duplicate);
    assert!(evaluate_file(&file.0, &CorpusFile::digest(duplicate)).is_err());

    let deep = format!("{}0{}", "[".repeat(140), "]".repeat(140));
    file.write(deep.as_bytes());
    assert!(evaluate_file(&file.0, &CorpusFile::digest(deep.as_bytes())).is_err());

    let too_long_probe = manifest(vec![json!({
        "id":"oversized_json",
        "probe":{"kind":"strict_json","input":"x".repeat(32_769)},
        "expected_accept":false
    })]);
    let bytes = serde_json::to_vec(&too_long_probe).unwrap();
    file.write(&bytes);
    assert!(evaluate_file(&file.0, &CorpusFile::digest(&bytes)).is_err());

    let too_many_cases = manifest(
        (0..257)
            .map(|index| {
                global_case(
                    &format!("case_{index}"),
                    "human-cli",
                    "evaluation.security",
                    true,
                )
            })
            .collect(),
    );
    let bytes = serde_json::to_vec(&too_many_cases).unwrap();
    file.write(&bytes);
    assert!(evaluate_file(&file.0, &CorpusFile::digest(&bytes)).is_err());

    let too_long_wire = manifest(vec![json!({
        "id":"oversized_frame",
        "probe":{"kind":"flatbuffers_wire","bytes_hex":"00".repeat(256 * 1024 + 1)},
        "expected_accept":false
    })]);
    let bytes = serde_json::to_vec(&too_long_wire).unwrap();
    file.write(&bytes);
    assert!(evaluate_file(&file.0, &CorpusFile::digest(&bytes)).is_err());
}

#[test]
fn bounded_file_read_rejects_oversize_and_final_symlink() {
    let file = CorpusFile::new(&vec![b' '; 1_048_577]);
    assert!(evaluate_file(&file.0, &CorpusFile::digest(&vec![b' '; 1_048_577])).is_err());

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let target_bytes = serde_json::to_vec(&manifest(vec![global_case(
            "allowed",
            "human-cli",
            "evaluation.security",
            true,
        )]))
        .unwrap();
        let target = CorpusFile::new(&target_bytes);
        let link = std::env::temp_dir().join(format!(
            "cogfabric-security-corpus-link-{}.json",
            uuid::Uuid::new_v4()
        ));
        symlink(&target.0, &link).unwrap();
        assert!(evaluate_file(&link, &CorpusFile::digest(&target_bytes)).is_err());
        let _ = fs::remove_file(link);
    }
}
