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
    let prepared = preparation::prepare(&bundle, &manifest_path, &sha256(&reviewed)).unwrap();
    prepared.write(&root.join("prepared")).unwrap();
    let artifact = std::fs::read_to_string(root.join("prepared/artifact.md")).unwrap();
    assert!(artifact.contains("Reference documentation"));
    assert!(!artifact.contains("Validation document") && !artifact.contains("Held-out test"));
    assert!(acquired.write(&bundle).is_err());
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
