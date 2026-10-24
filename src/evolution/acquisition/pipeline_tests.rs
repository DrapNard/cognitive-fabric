use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn private_http_executor_enforces_status_headers_deadline_and_remaining_budget() {
    // Production URLs are HTTPS-only. This private executor fixture isolates
    // response handling; it does not bypass production URL/DNS validation.
    for (status, headers, body, remaining, expected) in [
        (
            "200 OK",
            "Content-Type: text/plain\r\n",
            "reference body",
            64_000,
            true,
        ),
        (
            "302 Found",
            "Content-Type: text/plain\r\nLocation: http://127.0.0.1:1/\r\n",
            "redirect",
            64_000,
            false,
        ),
        (
            "200 OK",
            "Content-Type: text/html\r\n",
            "<html>data</html>",
            64_000,
            false,
        ),
        (
            "200 OK",
            "Content-Type: text/plain\r\nContent-Encoding: gzip\r\n",
            "compressed",
            64_000,
            false,
        ),
        (
            "200 OK",
            "Content-Type: text/plain\r\n",
            "too long",
            4,
            false,
        ),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let fixture = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut chunk = [0; 1024];
                let count = socket.read(&mut chunk).await.unwrap();
                assert!(count > 0 && request.len() + count < 16_384);
                request.extend_from_slice(&chunk[..count]);
                if request.windows(4).any(|part| part == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
            assert!(!request.contains("authorization:") && !request.contains("cookie:"));
            let reply = format!("HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            let _ = socket.write_all(reply.as_bytes()).await;
        });
        let url = Url::parse(&format!(
            "http://fixture.network-source.invalid:{}/document",
            address.port()
        ))
        .unwrap();
        let result = fetch_text(
            &url,
            "fixture.network-source.invalid",
            &[address],
            remaining,
            tokio::time::Instant::now() + Duration::from_secs(2),
        )
        .await;
        assert_eq!(result.is_ok(), expected);
        if expected {
            assert_eq!(result.unwrap(), body.as_bytes());
        }
        fixture.await.unwrap();
    }
    let url = Url::parse("https://uncontacted.network-source.invalid/document").unwrap();
    assert!(fetch_text(
        &url,
        "uncontacted.network-source.invalid",
        &[],
        64_000,
        tokio::time::Instant::now()
    )
    .await
    .is_err());
}

#[test]
fn acquisition_bundle_preserves_pins_and_requires_review_before_preparation() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!(
        "fabric-acquisition-pipeline-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir(&root).unwrap();
    let texts = [
        "Reference documentation about ownership and bounded orchestration.\r\n",
        "Validation document independently discusses SQL transaction guarantees.\n",
        "Held-out test document concerns API cancellation and worker reaping.\n",
    ];
    let mut sources = Vec::new();
    let mut plan_sources = Vec::new();
    for (index, text) in texts.iter().enumerate() {
        let source = Source {
            id: format!("fixture.source{index}"),
            url: format!("https://docs.network-source.org/{index}.txt"),
            sha256: sha256(text.as_bytes()),
            source: format!("fixture-{index}"),
            license: "GPL-3.0-only".into(),
        };
        assert_eq!(
            validate_downloaded(&source, text.as_bytes()).unwrap(),
            source.sha256
        );
        assert!(validate_downloaded(&source, b"changed bytes").is_err());
        plan_sources.push(json!({"id":source.id,"url":source.url,"sha256":source.sha256,"source":source.source,"license":source.license}));
        sources.push(AcquiredSource {
            id: source.id,
            bytes: text.as_bytes().to_vec(),
            sha256: source.sha256,
            source: source.source,
            license: source.license,
            url_sha256: sha256(source.url.as_bytes()),
        });
    }
    let manifest_bytes = serde_json::to_vec(&json!({"format_version":1,"reviewer":"fixture operator","acquisition_approved":true,"sources":plan_sources})).unwrap();
    let acquired = Acquired {
        sources,
        manifest_sha256: sha256(&manifest_bytes),
        manifest_bytes: manifest_bytes.clone(),
        total_bytes: texts.iter().map(|text| text.len()).sum(),
        reviewer: "fixture operator".into(),
    };
    let bundle = root.join("downloaded");
    let report = acquired.write(&bundle).unwrap();
    assert_eq!(report["candidate_created"], false);
    assert_eq!(
        std::fs::read(bundle.join("acquisition-plan.json")).unwrap(),
        manifest_bytes
    );
    assert_eq!(
        std::fs::metadata(&bundle).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(bundle.join("fixture.source0.txt"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let manifest_path = bundle.join("sources.json");
    let raw = std::fs::read(&manifest_path).unwrap();
    assert!(preparation::prepare(&bundle, &manifest_path, &sha256(&raw)).is_err());
    let mut manifest: Value = serde_json::from_slice(&raw).unwrap();
    manifest["reviewer"] = "reviewed fixture operator".into();
    manifest["sources_reviewed"] = true.into();
    manifest["sources"][1]["split"] = "validation".into();
    manifest["sources"][2]["split"] = "test".into();
    let reviewed = serde_json::to_vec(&manifest).unwrap();
    std::fs::write(&manifest_path, &reviewed).unwrap();
    // A claimed origin cannot be replaced by a newly hashed source label/path.
    for key in ["id", "path", "source", "license", "sha256"] {
        let mut changed: Value = serde_json::from_slice(&reviewed).unwrap();
        changed["sources"][0][key] = match key {
            "id" => json!("other.source"),
            "path" => json!("other.txt"),
            "sha256" => json!("0".repeat(64)),
            _ => json!("changed-origin"),
        };
        let changed = serde_json::to_vec(&changed).unwrap();
        std::fs::write(&manifest_path, &changed).unwrap();
        assert!(preparation::prepare(&bundle, &manifest_path, &sha256(&changed)).is_err());
    }
    std::fs::write(&manifest_path, &reviewed).unwrap();
    let acquisition_path = bundle.join("acquisition.json");
    let original_acquisition = std::fs::read(&acquisition_path).unwrap();
    let mut forged: Value = serde_json::from_slice(&original_acquisition).unwrap();
    forged["candidate_created"] = true.into();
    let forged = serde_json::to_vec(&forged).unwrap();
    std::fs::write(&acquisition_path, &forged).unwrap();
    let mut rebound: Value = serde_json::from_slice(&reviewed).unwrap();
    rebound["acquisition"]["report_sha256"] = sha256(&forged).into();
    let rebound = serde_json::to_vec(&rebound).unwrap();
    std::fs::write(&manifest_path, &rebound).unwrap();
    assert!(preparation::prepare(&bundle, &manifest_path, &sha256(&rebound)).is_err());
    std::fs::write(&acquisition_path, &original_acquisition).unwrap();
    std::fs::write(&manifest_path, &reviewed).unwrap();
    let saved_path = bundle.join("saved-acquisition.json");
    std::fs::rename(&acquisition_path, &saved_path).unwrap();
    std::os::unix::fs::symlink(&saved_path, &acquisition_path).unwrap();
    assert!(preparation::prepare(&bundle, &manifest_path, &sha256(&reviewed)).is_err());
    std::fs::remove_file(&acquisition_path).unwrap();
    std::fs::rename(&saved_path, &acquisition_path).unwrap();
    let prepared = preparation::prepare(&bundle, &manifest_path, &sha256(&reviewed)).unwrap();
    prepared.write(&root.join("prepared")).unwrap();
    let report_path = root.join("prepared/report.json");
    let original_prepared = std::fs::read(&report_path).unwrap();
    let valid_report: Value = serde_json::from_slice(&original_prepared).unwrap();
    assert_eq!(valid_report["format_version"], 3);
    assert_eq!(
        valid_report["acquisition"]["plan_json"],
        String::from_utf8(manifest_bytes.clone()).unwrap()
    );
    let cfg = crate::config::Config {
        home: root.join("state"),
        backend: "demo".into(),
        ..Default::default()
    };
    cfg.save().unwrap();
    let store = crate::store::Store::open(&cfg.db()).unwrap();
    store
        .ensure_skill("rust.s1", "Acquired provenance fixture", &["rust".into()])
        .unwrap();
    for case in 0..11 {
        let mut forged = valid_report.clone();
        match case {
            0 => forged["format_version"] = 2.into(),
            1 => {
                forged.as_object_mut().unwrap().remove("acquisition");
            }
            2 => forged["acquisition"]["plan_sha256"] = "0".repeat(64).into(),
            3 => forged["acquisition"]["report_sha256"] = "0".repeat(64).into(),
            4 => forged["sources"][0]["original_sha256"] = "0".repeat(64).into(),
            5 => forged["sources"][0]["path"] = "untracked.txt".into(),
            6 => forged["sources"][0]["source"] = "different origin".into(),
            7 => forged["sources"][0]["license"] = "MIT".into(),
            8 => forged["input_bytes"] = 1.into(),
            9 => forged["acquisition"]["evaluation_authority"] = true.into(),
            10 => {
                forged["format_version"] = 2.into();
                forged["acquisition"] = Value::Null;
            }
            _ => unreachable!(),
        }
        let bytes = serde_json::to_vec(&forged).unwrap();
        std::fs::write(&report_path, &bytes).unwrap();
        assert!(
            crate::evolution::prepared::create(
                &cfg,
                &store,
                "rust.s1",
                &root.join("prepared"),
                &sha256(&bytes),
                "GPL-3.0-only"
            )
            .is_err(),
            "accepted forged case {case}"
        );
        assert!(store.candidates().unwrap().is_empty());
    }
    std::fs::write(&report_path, &original_prepared).unwrap();
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    db.execute_batch("CREATE TRIGGER deny_acquired_provenance BEFORE INSERT ON events WHEN NEW.kind='candidate_preparation' BEGIN SELECT RAISE(FAIL,'fixture'); END;").unwrap();
    assert!(crate::evolution::prepared::create(
        &cfg,
        &store,
        "rust.s1",
        &root.join("prepared"),
        &sha256(&original_prepared),
        "GPL-3.0-only"
    )
    .is_err());
    assert!(store.candidates().unwrap().is_empty());
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM events WHERE kind='candidate_preparation'",
            [],
            |row| row.get::<_, u64>(0)
        )
        .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER deny_acquired_provenance")
        .unwrap();
    drop(db);
    let candidate = crate::evolution::prepared::create(
        &cfg,
        &store,
        "rust.s1",
        &root.join("prepared"),
        &sha256(&original_prepared),
        "GPL-3.0-only",
    )
    .unwrap();
    assert!(acquired.write(&bundle).is_err());
    std::fs::remove_dir_all(&bundle).unwrap();
    std::fs::remove_dir_all(root.join("prepared")).unwrap();
    let provenance = crate::evolution::prepared::provenance(&store, &candidate.id).unwrap();
    assert_eq!(
        provenance["prepared_provenance"]["acquisition"],
        valid_report["acquisition"]
    );
    assert_eq!(provenance["evaluation_authority"], false);
    assert!(crate::evaluation::promote(&store, &candidate.id, true).is_err());
    drop(store);
    let artifact = std::fs::read_to_string(&candidate.artifact).unwrap();
    assert!(artifact.contains("Reference documentation"));
    assert!(!artifact.contains("Validation document") && !artifact.contains("Held-out test"));
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn cancelled_dns_keeps_its_slot_until_the_os_worker_really_returns() {
    let slots = resolver_slots();
    let retained = slots.clone().acquire_many_owned(3).await.unwrap();
    let (entered, started) = tokio::sync::oneshot::channel();
    let (release, stopped) = std::sync::mpsc::channel();
    let request = tokio::spawn(resolve_with_worker(
        "fixture.network-source.org",
        tokio::time::Instant::now() + Duration::from_secs(5),
        move |_| {
            let _ = entered.send(());
            let _ = stopped.recv();
            Ok(vec!["8.8.8.8:443".parse().unwrap()])
        },
    ));
    tokio::time::timeout(Duration::from_secs(2), started)
        .await
        .unwrap()
        .unwrap();
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    assert_eq!(slots.available_permits(), 0);
    assert!(resolve_with_worker(
        "queued.network-source.org",
        tokio::time::Instant::now(),
        |_| { panic!("expired queued DNS must not start another worker") }
    )
    .await
    .is_err());
    release.send(()).unwrap();
    let recovered = tokio::time::timeout(Duration::from_secs(2), slots.clone().acquire_owned())
        .await
        .unwrap()
        .unwrap();
    drop(recovered);
    drop(retained);
    assert_eq!(slots.available_permits(), 4);
}
