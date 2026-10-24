# Operator-selected remote source acquisition

Architecture section 8 calls for source acquisition with provenance, hashes,
licensing checks, cleaning and independent evaluation. `candidate fetch-sources`
adds a bounded acquisition step before the existing preparation/quarantine flow.
It fetches only explicitly selected, hash-pinned text documents. It is not a web
crawler, independent license audit, trusted dataset generator or model tool.

## Command

```sh
cogfabric --json candidate fetch-sources \
  --manifest ./acquisition-plan.json --sha256 PLAN_SHA256 \
  --output ./downloaded-sources
```

`PLAN_SHA256` pins the exact manifest bytes, using lowercase SHA-256. The output
must be a new directory. This command runs before workspace/inference/database
initialization and needs no model. Only the human CLI has `candidate.acquire`;
Jev+, Jev-R and expert actors cannot request network acquisition.

## Approved manifest

```json
{
  "format_version": 1,
  "reviewer": "operator review identity",
  "acquisition_approved": true,
  "sources": [
    {
      "id": "docs.runtime",
      "url": "https://docs.your-project.org/runtime.txt",
      "sha256": "REPLACE_WITH_THE_DOCUMENT_LOWERCASE_SHA256",
      "source": "operator-selected upstream runtime documentation",
      "license": "REPLACE_WITH_REVIEWED_LICENSE_LABEL"
    }
  ]
}
```

The document hash must be obtained independently; the command never adopts an
unreviewed server response as its expected hash. The example placeholders are
not valid acquisition inputs. Approval authorizes acquisition, not dataset quality,
legal compatibility, permissions for the document's instructions or promotion.

The strict manifest is at most 32 KB, has no duplicate/unknown keys, has a nonempty
reviewer (at most 100 bytes), and contains 1–32 unique bounded source IDs. All
entries are validated before the first DNS query. Sources need explicit provenance
and license labels (at most 100 and 128 raw bytes respectively, including padding); identifiers become flat filenames, never remote paths.

## Network and resource boundaries

- HTTPS on port 443 only, with normal certificate/hostname verification.
- Fully qualified public hostnames only; IP literals, local/special names,
  credentials, query strings and fragments are refused. URLs are at most 2,048 bytes.
- Every resolved address must pass conservative public-unicast filters. Mixed
  public/private results, empty answers and more than 16 raw addresses refuse.
  The complete accepted address set is pinned into the HTTP client, preventing a
  second uncontrolled resolution when connecting. Each source resolves separately.
- Proxy settings, redirects, authentication headers, cookies and automatic body
  decompression are disabled/not supplied. Requests ask for identity encoding.
- Responses need one `text/plain` or `text/markdown` content type, optionally UTF-8.
  Compressed bodies, ambiguous lengths, duplicate framing headers and unsupported
  transfer codings refuse. These are format checks; a server's media label does
  not attest the semantics or safety of text.
- At most 64 KB per document and 512 KB total original bytes. Stream admission
  uses the smaller remaining allowance; exact hashes and UTF-8/control-character
  validation precede publication. Any failure produces no completed bundle.
- A shared 30-second deadline covers manifest validation, resolver waiting, HTTP
  and body reads. Local filesystem calls cannot be forcibly preempted by that
  timer. At most four OS resolver threads can run per process. A timed-out libc
  resolver may continue holding its permit, but cannot initiate HTTP; independent
  threads do not make Tokio runtime shutdown wait for DNS. Separate processes
  still require host-level quotas. No provider-termination guarantee is implied.

No task prompt, model credential or inference context is sent to the source.
Network errors omit remote URLs and response headers. The operator's selected
URLs remain in the retained private plan for reproducibility.

## Output and review

A new owner-private directory (`0700`) contains files (`0600`):

- `<source-id>.txt`: unchanged, hash-pinned original bytes.
- `acquisition-plan.json`: the exact validated input manifest.
- `acquisition.json`: plan hash, reviewer/approval assertion, document counts,
  original hashes, URL hashes, provenance/license labels and non-promotion flags.
- `sources.json`: a preparation draft with `sources_reviewed: false`, an empty
  reviewer and all entries in `reference`.

Existing directories are never overwritten. Partial publication is cleaned up on
write failure, following the existing preparation writer's rules. The report
contains no neural evaluation authority.

Review the downloaded text and labels; select independent validation and test
sources; edit `sources.json` to record that review and split assignment. Then hash
that edited manifest and run `candidate prepare-sources --root ./downloaded-sources`
with the manifest, hash and a new output directory as documented in
[source preparation](SOURCE_PREPARATION.md). Preparation deduplicates/checks exact
overlap; candidate import, independent evaluation, approval and rollback remain
separate gates. Acquisition and text retrieval benchmarks cannot approve weights.

## Verification scope

Unit tests cover URL forms, public-address filtering, mixed/excess DNS lists,
strict manifests, original-byte pins, response framing and byte limits. A private
HTTP executor fixture covers successful body reads and refusal of redirects,
HTML media types, compressed responses and exhausted remaining allowances. It
isolates response handling; production URL validation still requires HTTPS.
A cancellation test verifies resolver permits remain held until actual worker completion.
A bundle test checks original plan preservation, private modes, no overwrite,
review refusal and reference/held-out separation through preparation. The CLI
regression checks unsafe plans fail without workspace/output creation.

No public HTTPS download, live DNS failure campaign, license attestation or
independent poisoning/neural-safety evaluation was run for this change.

### Verification record — 2026-10-24

- `cargo fmt --all -- --check` and `git diff --check`: passed.
- `cargo +1.99.0 check --workspace --all-targets --all-features --locked`: passed.
- `cargo +1.99.0 clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo +1.99.0 test -p cogfabric --lib evolution::acquisition --locked`: nine passed, including retained DNS slots after cancellation, private HTTP response handling and the bundle/preparation flow.
- `cargo +1.99.0 test -p cogfabric --lib security::tests::remote_acquisition_is_operator_only --locked`: one passed.
- `cargo +1.99.0 test -p cogfabric --test source_acquisition --test source_discovery --test source_preparation --test prepared_candidate --locked`: nine passed across the four targets.

No full reflection suite or reasoning benchmark was run. The private response
fixture permits HTTP only inside the private executor test; no public API exposes
that test path. Production manifest validation still rejects every non-HTTPS URL.

## Retaining origins through candidate import

New acquisition drafts include an `acquisition` binding with `plan_sha256` and
`report_sha256`, calculated from the exact two retained acquisition files. Keep
those files beside the downloaded documents when reviewing `sources.json`.
Preparation reads their fixed filenames relative to the retained root directory
without following final symlinks, validates both hashes and strict schemas, and
matches every selected source's ID, filename, original hash, provenance, license
and original byte count. Selecting a subset is allowed; mixing untracked local
sources into a claimed acquisition is refused.

Bound preparation reports use format version **3**. They embed the exact approved
plan/report JSON and their hashes under `acquisition`; unchanged local preparation
without a binding still emits version **2**. The complete prepared report remains
limited to 65,536 bytes. A metadata-heavy batch can therefore exceed the report
limit even when its downloads fit the byte allowance; preparation refuses before
publishing an output bundle.

`candidate create-prepared` and transactional database insertion use the same
version and lineage validator. Version 2 cannot claim acquisition evidence;
version 3 requires it. Source correspondence and original input-byte totals are
rechecked before registration. Candidate inspection revalidates that embedded
snapshot, so origin URLs and approval assertions survive after the source bundle
is moved or removed. The existing candidate/provenance transaction, retention
protection and independent evaluation/promotion gates still apply.

These are consistent, hash-bound operator records. They do not prove a remote
server was contacted, a publisher's identity, a license's legal validity, or
independent review against a malicious owner who can construct new local records.
No held-out document text is added to the candidate artifact or origin snapshot.

Older acquisition drafts without a binding continue through the local v2 path;
they do not acquire origin claims automatically. An operator can explicitly add
the two exact file hashes after reviewing and preserving the matching plan and
report. No original source body is recovered from a hash or fetched during import.

### Lineage verification — 2026-10-24

- Formatting and `git diff --check`: passed.
- `cargo +1.99.0 check --workspace --all-targets --all-features --locked`: passed.
- `cargo +1.99.0 clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo +1.99.0 test -p cogfabric --lib evolution::acquisition --locked`: nine passed. The pipeline test exercises valid v3 import, changed origins, rehashed invalid approval flags, symlink refusal, version/evidence downgrade refusal, audit rollback, no-evaluation promotion denial and durable inspection after bundle removal.
- `cargo +1.99.0 test -p cogfabric --test source_acquisition --test source_discovery --test source_preparation --test prepared_candidate --locked`: nine passed, including legacy local v2 workflows.
- `cargo +1.99.0 test -p cogfabric --lib store::tests --locked`: fifteen passed, including existing migration and restore checks.

The first v3 import regression exposed a stale v2-only Store gate. It was fixed
by sharing the same report validator across import, transaction and inspection;
the rerun passed. No full reflection suite, live public fetch or reasoning-quality
benchmark was run for this lineage change.

## Discover from a recorded capability gap

`candidate discover-remote-sources` can draft this acquisition schema from a
hash-pinned reviewed catalog and recorded missing capability. It performs no
network and leaves approval false. See [source catalogs](SOURCE_CATALOG.md).
