//! Bounded operator-selected HTTPS source acquisition into an unpublished bundle.
//!
//! This module is intentionally separate from candidate preparation and promotion:
//! downloaded text is untrusted input, and acquisition never creates a candidate.

use crate::{evolution::preparation, security, store::sha256};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs},
    path::Path,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;
use url::Url;

const MAX_MANIFEST_BYTES: usize = 32_000;
const MAX_DOCUMENT_BYTES: usize = 64_000;
const MAX_TOTAL_BYTES: usize = 512_000;
const MAX_SOURCES: usize = 32;
const MAX_DNS_ADDRESSES: usize = 16;
const ACQUISITION_DEADLINE: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format_version: u32,
    reviewer: String,
    acquisition_approved: bool,
    sources: Vec<Source>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    id: String,
    url: String,
    sha256: String,
    source: String,
    license: String,
}

struct AcquiredSource {
    id: String,
    bytes: Vec<u8>,
    sha256: String,
    source: String,
    license: String,
    url_sha256: String,
}

/// A hash-pinned set of text sources that has not been reviewed or promoted.
pub struct Acquired {
    sources: Vec<AcquiredSource>,
    manifest_bytes: Vec<u8>,
    manifest_sha256: String,
    reviewer: String,
    total_bytes: usize,
}

/// Fetch an explicitly approved, hash-pinned manifest of public HTTPS documents.
///
/// The supplied hash pins the exact manifest bytes. Hostnames are resolved once,
/// every returned address is checked, and the HTTP client is pinned to that set.
/// Redirects, proxy configuration, credentials, and non-text responses are refused.
/// libc resolver calls may outlive a timed-out request, but process-wide concurrency
/// is bounded and no provider request starts after the acquisition deadline.
pub async fn acquire(manifest: &Path, expected_sha256: &str) -> Result<Acquired> {
    security::authorize("human-cli", "candidate.acquire")?;
    if !valid_sha256(expected_sha256) {
        bail!("Manifest hash must be lowercase SHA-256")
    }
    let started = Instant::now();
    let deadline = tokio::time::Instant::from_std(started + ACQUISITION_DEADLINE);
    let raw = security::read_regular_bytes(manifest, MAX_MANIFEST_BYTES)?;
    if sha256(&raw) != expected_sha256 {
        bail!("Acquisition manifest hash mismatch")
    }
    let parsed: Manifest =
        serde_json::from_value(crate::strict_json::value(&raw, MAX_MANIFEST_BYTES)?)?;
    if parsed.format_version != 1
        || !parsed.acquisition_approved
        || parsed.reviewer.trim().is_empty()
        || parsed.reviewer.len() > 100
        || parsed.sources.is_empty()
        || parsed.sources.len() > MAX_SOURCES
    {
        bail!("Acquisition manifest lacks bounded operator approval")
    }

    let mut ids = BTreeSet::new();
    // Validate every URL and all provenance before the first DNS query or GET.
    let validated = parsed
        .sources
        .into_iter()
        .map(|source| {
            validate_source(&source, &mut ids)?;
            let url = parse_source_url(&source.url)?;
            Ok((source, url))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut sources = Vec::with_capacity(validated.len());
    let mut total_bytes = 0usize;
    for (source, url) in validated {
        if started.elapsed() >= ACQUISITION_DEADLINE {
            bail!("Acquisition deadline exhausted")
        }
        let host = url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("Source URL requires an explicit hostname"))?;
        let remaining_total = MAX_TOTAL_BYTES.saturating_sub(total_bytes);
        if remaining_total == 0 {
            bail!("Acquisition total document limit exceeded")
        }
        let addresses = resolve_public_addresses(host, deadline).await?;
        let bytes = fetch_text(&url, host, &addresses, remaining_total, deadline).await?;
        let content_sha256 = validate_downloaded(&source, &bytes)?;
        total_bytes = total_bytes
            .checked_add(bytes.len())
            .ok_or_else(|| anyhow::anyhow!("Acquisition byte count overflow"))?;
        if total_bytes > MAX_TOTAL_BYTES {
            bail!("Acquisition total document limit exceeded")
        }
        sources.push(AcquiredSource {
            id: source.id,
            bytes,
            sha256: content_sha256,
            source: source.source,
            license: source.license,
            url_sha256: sha256(url.as_str().as_bytes()),
        });
    }
    if started.elapsed() >= ACQUISITION_DEADLINE {
        bail!("Acquisition deadline exhausted")
    }
    Ok(Acquired {
        sources,
        manifest_bytes: raw,
        manifest_sha256: expected_sha256.to_owned(),
        reviewer: parsed.reviewer,
        total_bytes,
    })
}

impl Acquired {
    /// Write a private, unpublished source bundle without overwriting existing files.
    ///
    /// The generated preparation manifest is deliberately unreviewed and assigns
    /// every source to the reference split. Operators must review and edit a copy
    /// before using the normal preparation command.
    pub fn write(&self, output: &Path) -> Result<Value> {
        let mut filenames = Vec::with_capacity(self.sources.len());
        let mut documents = Vec::with_capacity(self.sources.len());
        let mut draft_sources = Vec::with_capacity(self.sources.len());
        let mut provenance = Vec::with_capacity(self.sources.len());
        for source in &self.sources {
            let filename = format!("{}.txt", source.id);
            filenames.push(filename);
            documents.push(source.bytes.clone());
            draft_sources.push(json!({
                "id": source.id,
                "path": filenames.last().expect("filename just inserted"),
                "sha256": source.sha256,
                "source": source.source,
                "license": source.license,
                "split": "reference"
            }));
            provenance.push(json!({
                "id": source.id,
                "sha256": source.sha256,
                "url_sha256": source.url_sha256,
                "source": source.source,
                "license": source.license,
                "bytes": source.bytes.len()
            }));
        }
        let draft = json!({
            "format_version": 1,
            "reviewer": "",
            "sources_reviewed": false,
            "sources": draft_sources
        });
        let acquisition = json!({
            "format_version": 1,
            "manifest_sha256": self.manifest_sha256,
            "plan_sha256": sha256(&self.manifest_bytes),
            "reviewer": self.reviewer,
            "acquisition_approved": true,
            "source_count": self.sources.len(),
            "total_bytes": self.total_bytes,
            "untrusted_data": true,
            "candidate_created": false,
            "model_promoted": false,
            "sources": provenance
        });
        let mut payloads = filenames
            .iter()
            .zip(documents)
            .map(|(filename, bytes)| (filename.as_str(), bytes))
            .collect::<Vec<_>>();
        payloads.push(("sources.json", serde_json::to_vec_pretty(&draft)?));
        payloads.push(("acquisition-plan.json", self.manifest_bytes.clone()));
        payloads.push(("acquisition.json", serde_json::to_vec_pretty(&acquisition)?));
        preparation::write_bundle(output, payloads)?;
        Ok(acquisition)
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_source(source: &Source, ids: &mut BTreeSet<String>) -> Result<()> {
    if source.id.len() > 64
        || !crate::registry::identifier(&source.id)
        || !ids.insert(source.id.clone())
    {
        bail!("Invalid or duplicate source identity")
    }
    if !valid_sha256(&source.sha256) {
        bail!("Source hash must be lowercase SHA-256")
    }
    if source.source.trim().is_empty()
        || source.source.trim().len() > 100
        || source.license.trim().is_empty()
        || source.license.trim().len() > 128
    {
        bail!("Source provenance label or license is invalid")
    }
    security::validate_provenance(&source.source, &source.license)
}

fn validate_downloaded(source: &Source, bytes: &[u8]) -> Result<String> {
    // Normalize for validation using the same rules as preparation, but preserve
    // and pin the exact downloaded bytes in the acquisition bundle.
    preparation::normalize(bytes).context("Source is not valid normalized text")?;
    let hash = sha256(bytes);
    if hash != source.sha256 {
        bail!("Downloaded source hash mismatch")
    }
    Ok(hash)
}

fn parse_source_url(raw: &str) -> Result<Url> {
    if raw.len() > 2048 {
        bail!("Source URL exceeds its length limit")
    }
    let url = Url::parse(raw).map_err(|_| anyhow::anyhow!("Invalid source URL"))?;
    if url.scheme() != "https"
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port().is_some_and(|port| port != 443)
    {
        bail!("Source URL must be public HTTPS without credentials, query, or fragment")
    }
    let Some(url::Host::Domain(host)) = url.host() else {
        bail!("IP literal source URLs are not supported")
    };
    if host.is_empty()
        || host.len() > 253
        || !host.contains('.')
        || host.ends_with('.')
        || host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host.ends_with(".test")
        || host.ends_with(".example")
        || host.ends_with(".invalid")
        || host.ends_with(".onion")
    {
        bail!("Source URL requires a public fully qualified hostname")
    }
    Ok(url)
}

async fn resolve_public_addresses(
    host: &str,
    deadline: tokio::time::Instant,
) -> Result<Vec<SocketAddr>> {
    resolve_with_worker(host, deadline, |host| {
        (host.as_str(), 443)
            .to_socket_addrs()
            .map(|addresses| addresses.take(MAX_DNS_ADDRESSES + 1).collect::<Vec<_>>())
    })
    .await
}

fn resolver_slots() -> Arc<Semaphore> {
    static DNS_SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    DNS_SLOTS
        .get_or_init(|| Arc::new(Semaphore::new(4)))
        .clone()
}

async fn resolve_with_worker<F>(
    host: &str,
    deadline: tokio::time::Instant,
    resolve: F,
) -> Result<Vec<SocketAddr>>
where
    F: FnOnce(String) -> std::io::Result<Vec<SocketAddr>> + Send + 'static,
{
    // An independent OS thread retains its permit through libc completion.
    // Cancellation cannot queue unbounded DNS work or delay Tokio shutdown.
    let permit = tokio::time::timeout_at(deadline, resolver_slots().acquire_owned())
        .await
        .map_err(|_| anyhow::anyhow!("Acquisition deadline exhausted"))?
        .map_err(|_| anyhow::anyhow!("Source hostname resolution failed"))?;
    let host = host.to_owned();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let worker_deadline = deadline.into_std();
    std::thread::Builder::new()
        .name("cogfabric-source-dns".to_owned())
        .spawn(move || {
            let _permit = permit;
            if sender.is_closed() || Instant::now() >= worker_deadline {
                return;
            }
            let resolved = resolve(host);
            let _ = sender.send(resolved);
        })
        .map_err(|_| anyhow::anyhow!("Source hostname resolution failed"))?;
    let resolved = tokio::time::timeout_at(deadline, receiver)
        .await
        .map_err(|_| anyhow::anyhow!("Acquisition deadline exhausted"))?
        .map_err(|_| anyhow::anyhow!("Source hostname resolution failed"))?
        .map_err(|_| anyhow::anyhow!("Source hostname resolution failed"))?;
    validate_addresses(resolved)
}

fn validate_addresses(resolved: Vec<SocketAddr>) -> Result<Vec<SocketAddr>> {
    if resolved.len() > MAX_DNS_ADDRESSES {
        bail!("Source hostname resolved to too many addresses")
    }
    let mut addresses = Vec::with_capacity(resolved.len());
    let mut unique = BTreeSet::new();
    for address in resolved {
        if !is_public_address(address.ip()) {
            bail!("Source hostname resolved to a non-public address")
        }
        if unique.insert(address) {
            addresses.push(address);
        }
    }
    if addresses.is_empty() {
        bail!("Source hostname has no usable address")
    }
    Ok(addresses)
}

fn is_public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_public_ipv4(address),
        IpAddr::V6(address) => is_public_ipv6(address),
    }
}

fn is_public_ipv4(address: Ipv4Addr) -> bool {
    let [a, b, c, _] = address.octets();
    !(a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 88 && c == 99)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113))
}

fn is_public_ipv6(address: Ipv6Addr) -> bool {
    let octets = address.octets();
    // Permit only current global-unicast space (2000::/3), then remove special,
    // documentation, transition, and protocol-assignment ranges conservatively.
    let global_unicast = octets[0] & 0xe0 == 0x20;
    let in_2001_iana_special = octets[0] == 0x20 && octets[1] == 0x01 && octets[2] & 0xfe == 0;
    let in_2002_transition = octets[0] == 0x20 && octets[1] == 0x02;
    let in_documentation =
        octets[0] == 0x20 && octets[1] == 0x01 && octets[2] == 0x0d && octets[3] == 0xb8;
    let in_3fff_documentation = octets[0] == 0x3f && octets[1] == 0xff && octets[2] & 0xf0 == 0;
    global_unicast
        && !address.is_unspecified()
        && !address.is_loopback()
        && !address.is_unique_local()
        && !address.is_unicast_link_local()
        && !address.is_multicast()
        && !in_2001_iana_special
        && !in_2002_transition
        && !in_documentation
        && !in_3fff_documentation
}

async fn fetch_text(
    url: &Url,
    host: &str,
    addresses: &[SocketAddr],
    remaining_total: usize,
    deadline: tokio::time::Instant,
) -> Result<Vec<u8>> {
    let response_limit = MAX_DOCUMENT_BYTES.min(remaining_total);
    if response_limit == 0 {
        bail!("Acquisition total document limit exceeded")
    }
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    if remaining.is_zero() {
        bail!("Acquisition deadline exhausted")
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .connect_timeout(remaining)
        .timeout(remaining)
        .resolve_to_addrs(host, addresses)
        .build()
        .map_err(|_| anyhow::anyhow!("Could not initialize bounded source client"))?;
    let mut response = tokio::time::timeout_at(
        deadline,
        client
            .get(url.clone())
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Acquisition deadline exhausted"))?
    .map_err(|_| anyhow::anyhow!("Source request failed"))?;
    if !response.status().is_success() {
        bail!("Source returned an unsuccessful HTTP status")
    }
    validate_response_headers(
        response.headers(),
        response.content_length(),
        response_limit,
    )?;
    let mut bytes = Vec::new();
    loop {
        let next = tokio::time::timeout_at(deadline, response.chunk())
            .await
            .map_err(|_| anyhow::anyhow!("Acquisition deadline exhausted"))?
            .map_err(|_| anyhow::anyhow!("Source response read failed"))?;
        let Some(chunk) = next else { break };
        append_bounded_limit(&mut bytes, &chunk, response_limit)?;
    }
    if bytes.is_empty() {
        bail!("Source document is empty")
    }
    Ok(bytes)
}

#[cfg(test)]
fn append_bounded(output: &mut Vec<u8>, chunk: &[u8]) -> Result<()> {
    append_bounded_limit(output, chunk, MAX_DOCUMENT_BYTES)
}

fn append_bounded_limit(output: &mut Vec<u8>, chunk: &[u8], limit: usize) -> Result<()> {
    if output.len().saturating_add(chunk.len()) > limit {
        bail!("Source document exceeds the per-document limit")
    }
    output.extend_from_slice(chunk);
    Ok(())
}

fn validate_response_headers(
    headers: &reqwest::header::HeaderMap,
    content_length: Option<u64>,
    byte_limit: usize,
) -> Result<()> {
    let content_lengths = headers.get_all(reqwest::header::CONTENT_LENGTH);
    let transfer_encodings = headers.get_all(reqwest::header::TRANSFER_ENCODING);
    let transfer_encoding_count = transfer_encodings.iter().count();
    if content_lengths.iter().count() > 1
        || transfer_encoding_count > 1
        || (content_lengths.iter().next().is_some() && transfer_encoding_count != 0)
    {
        bail!("Ambiguous source response framing is not supported")
    }
    if let Some(encoding) = transfer_encodings.iter().next() {
        if !encoding
            .to_str()
            .is_ok_and(|value| value.trim().eq_ignore_ascii_case("chunked"))
        {
            bail!("Unsupported source response framing")
        }
    }
    if content_length.is_some_and(|length| length > byte_limit as u64) {
        bail!("Source document exceeds the per-document limit")
    }
    let content_types = headers.get_all(reqwest::header::CONTENT_TYPE);
    if content_types.iter().count() != 1 {
        bail!("Source response has no supported content type")
    }
    let content_type = content_types
        .iter()
        .next()
        .expect("content type count checked")
        .to_str()
        .map_err(|_| anyhow::anyhow!("Source response has no supported content type"))?;
    validate_content_type(content_type)?;
    let encodings = headers.get_all(reqwest::header::CONTENT_ENCODING);
    if encodings.iter().count() > 1 {
        bail!("Encoded source responses are not supported")
    }
    if let Some(encoding) = encodings.iter().next() {
        if !encoding
            .to_str()
            .is_ok_and(|value| value.trim().eq_ignore_ascii_case("identity"))
        {
            bail!("Encoded source responses are not supported")
        }
    }
    Ok(())
}

fn validate_content_type(value: &str) -> Result<()> {
    let mut parts = value.split(';');
    let media_type = parts.next().unwrap_or_default().trim();
    if !media_type.eq_ignore_ascii_case("text/plain")
        && !media_type.eq_ignore_ascii_case("text/markdown")
    {
        bail!("Source response is not plain text or Markdown")
    }
    if let Some(parameter) = parts.next() {
        let (name, value) = parameter
            .trim()
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("Unsupported source charset"))?;
        if !name.trim().eq_ignore_ascii_case("charset")
            || !value.trim().trim_matches('"').eq_ignore_ascii_case("utf-8")
            || parts.next().is_some()
        {
            bail!("Unsupported source charset")
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue, CONTENT_ENCODING, CONTENT_TYPE};

    #[test]
    fn source_urls_require_public_https_form_without_sensitive_components() {
        assert!(parse_source_url("https://example.org/docs/manual.md").is_ok());
        assert!(parse_source_url("https://example.org:443/docs/manual.md").is_ok());
        for url in [
            "http://example.org/file.txt",
            "https://user@example.org/file.txt",
            "https://example.org/file.txt?token=private",
            "https://example.org/file.txt#fragment",
            "https://example.org:444/file.txt",
            "https://127.0.0.1/file.txt",
            "https://[2606:4700:4700::1111]/file.txt",
        ] {
            assert!(parse_source_url(url).is_err(), "accepted {url}");
        }
    }

    #[test]
    fn address_classifier_rejects_special_ranges_and_accepts_public_unicast() {
        for address in [
            "0.1.2.3",
            "10.1.2.3",
            "100.64.0.1",
            "100.127.255.255",
            "127.0.0.1",
            "169.254.1.1",
            "172.16.0.1",
            "192.0.0.1",
            "192.0.2.1",
            "192.88.99.1",
            "192.168.1.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "fc00::1",
            "fe80::1",
            "ff02::1",
            "2001::1",
            "2001:db8::1",
            "2002::1",
            "3fff::1",
            "::ffff:8.8.8.8",
        ] {
            assert!(
                !is_public_address(address.parse().unwrap()),
                "accepted {address}"
            );
        }
        for address in ["8.8.8.8", "1.1.1.1", "2606:4700:4700::1111"] {
            assert!(
                is_public_address(address.parse().unwrap()),
                "rejected {address}"
            );
        }
    }

    #[test]
    fn resolved_address_lists_are_bounded_nonempty_and_all_public() {
        let public: SocketAddr = "8.8.8.8:443".parse().unwrap();
        assert!(validate_addresses(Vec::new()).is_err());
        assert!(validate_addresses(vec![public, "127.0.0.1:443".parse().unwrap()]).is_err());
        assert!(validate_addresses(vec![public; MAX_DNS_ADDRESSES + 1]).is_err());
        assert_eq!(validate_addresses(vec![public]).unwrap(), vec![public]);
    }

    #[test]
    fn strict_manifest_rejects_duplicate_unknown_and_unapproved_fields() {
        let valid = br#"{"format_version":1,"reviewer":"operator","acquisition_approved":true,"sources":[{"id":"rust.s1","url":"https://example.org/a.md","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","source":"project docs","license":"CC-BY-4.0"}]}"#;
        let parsed = crate::strict_json::value(valid, MAX_MANIFEST_BYTES).unwrap();
        let manifest: Manifest = serde_json::from_value(parsed).unwrap();
        assert_eq!(manifest.sources.len(), 1);
        assert!(validate_source(&manifest.sources[0], &mut BTreeSet::new()).is_ok());

        let duplicate = br#"{"format_version":1,"format_version":1,"reviewer":"operator","acquisition_approved":true,"sources":[]}"#;
        assert!(crate::strict_json::value(duplicate, MAX_MANIFEST_BYTES).is_err());
        let unknown = br#"{"format_version":1,"reviewer":"operator","acquisition_approved":true,"unexpected":0,"sources":[]}"#;
        let value = crate::strict_json::value(unknown, MAX_MANIFEST_BYTES).unwrap();
        assert!(serde_json::from_value::<Manifest>(value).is_err());
        assert!(!valid_sha256(&"A".repeat(64)));
    }

    #[test]
    fn downloaded_content_validation_pins_original_bytes() {
        let bytes = b"# Pinned source\r\n\r\nText content.\r\n";
        let source = Source {
            id: "docs.s1".to_owned(),
            url: "https://example.org/docs.md".to_owned(),
            sha256: sha256(bytes),
            source: "project docs".to_owned(),
            license: "CC-BY-4.0".to_owned(),
        };
        assert_eq!(validate_downloaded(&source, bytes).unwrap(), sha256(bytes));
        assert!(validate_downloaded(&source, b"other content").is_err());
        assert!(validate_downloaded(&source, &[0xff]).is_err());
    }

    #[test]
    fn content_and_body_limits_fail_closed() {
        assert!(validate_content_type("text/plain").is_ok());
        assert!(validate_content_type("text/markdown; charset=UTF-8").is_ok());
        for value in [
            "text/html",
            "application/octet-stream",
            "text/plain; charset=iso-8859-1",
            "text/plain; charset=utf-8; boundary=x",
        ] {
            assert!(validate_content_type(value).is_err(), "accepted {value}");
        }

        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        assert!(validate_response_headers(
            &headers,
            Some(MAX_DOCUMENT_BYTES as u64),
            MAX_DOCUMENT_BYTES
        )
        .is_ok());
        assert!(validate_response_headers(
            &headers,
            Some(MAX_DOCUMENT_BYTES as u64 + 1),
            MAX_DOCUMENT_BYTES
        )
        .is_err());
        headers.insert(CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        assert!(validate_response_headers(&headers, None, MAX_DOCUMENT_BYTES).is_err());
        headers.remove(CONTENT_ENCODING);
        headers.append(CONTENT_TYPE, HeaderValue::from_static("text/markdown"));
        assert!(validate_response_headers(&headers, None, MAX_DOCUMENT_BYTES).is_err());

        let mut body = vec![b'a'; MAX_DOCUMENT_BYTES - 1];
        assert!(append_bounded(&mut body, b"x").is_ok());
        assert!(append_bounded(&mut body, b"x").is_err());
    }
}

#[cfg(test)]
#[path = "acquisition/pipeline_tests.rs"]
mod pipeline_tests;
