# Reviewed local source preparation

## Discover a local source inventory

```sh
cogfabric --json candidate discover-sources --root ./documents \
  --source operator-selected-local-collection --license LICENSE_LABEL \
  --output ./source-draft
```

On Linux, this inventories visible `.md`/`.txt` files beneath the selected root
using retained directory descriptors and procfs enumeration. It writes a new
private `sources.json` plus `discovery.json`, with sorted paths, stable path-derived
IDs, original content hashes and explicit operator-supplied provenance/license
labels. Source text is not included in the discovery report. Hidden entries are
skipped and counted; other regular file extensions are ignored.

The draft has `sources_reviewed: false`, an empty reviewer and all sources assigned
to `reference`. **It is deliberately ineligible for preparation.** Review the
sources and labels, select independent validation/test documents, set the reviewer
and review assertion, then compute the edited manifest's hash before preparation.
Discovery cannot attest licensing, relevance, independence or safety.

The walk refuses incomplete inventories instead of returning a successful partial
plan: at most 256 entries, 32 documents, eight path components, 240-byte paths,
64 KB per document, 512 KB total and five seconds between bounded operations.
Symlinks, special files, invalid UTF-8, unsafe content and exhausted bounds refuse.
Output follows the same private/no-overwrite rules as preparation. No network,
runtime initialization, candidate creation or promotion occurs. Operator-selected remote fetching is available through [source acquisition](SOURCE_ACQUISITION.md);
remote discovery and independent curation remain unsupported.

## Prepare reviewed sources

Prepare a reproducible RAG input bundle from explicitly selected local documents:

```sh
cogfabric --json candidate prepare-sources \
  --root ./reviewed-documents --manifest ./sources.json \
  --sha256 MANIFEST_SHA256 --output ./prepared-bundle
```

`MANIFEST_SHA256` is the SHA-256 of the exact manifest bytes. Output must be a new
directory. No workspace database, inference server, network crawler or training
process is used. The operation does not create or promote a candidate.

## Manifest

The strict JSON object has these fields:

| Field | Meaning |
| --- | --- |
| `format_version` | Exactly `1`. |
| `reviewer` | Nonempty operator-selected review identity, at most 100 bytes. |
| `sources_reviewed` | Must be `true` after the operator reviews the selected sources. This is an assertion, not independent attestation. |
| `sources` | Between 3 and 32 source entries, with unique bounded identifier `id` values. |

Each source declares `id`, `path`, `sha256`, `source`, `license`, and `split`.
Paths name `.md` or `.txt` files relative to the selected root. Hashes refer to
original bytes before cleaning. Provenance and license labels must be explicit;
the preparer does not establish their legal correctness or compatibility.
`split` is `reference`, `validation` or `test`, with all three represented.
Unknown fields and duplicate JSON keys are refused.

## Output and use

- `artifact.md`: normalized, deduplicated **reference documents only**.
- `held-out.json`: normalized validation/test documents keyed by source ID.
- `report.json`: original/normalized hashes, split/provenance/license records,
  duplicate lineage, output hash, cleaning version and explicit non-authority flags.

The output directory is private (`0700`); files are `0600` and never overwrite an
existing bundle. Partial writes are removed on error. The reference artifact can
then be imported using the existing `candidate create` or `create-from-gap` flow,
with reviewed source/license metadata. Preserve the report and its manifest hash
as provenance. Candidate quarantine, independent evaluation, explicit approval and
rollback gates remain separate and unchanged.

`held-out.json` contains source documents, **not** an accepted quality test suite.
It must not be passed off as independent evaluation evidence. Operators still
supply the existing evaluator's independently reviewed query/expected-excerpt
cases. Public examples or model-authored labels do not establish independence.

## Import with durable preparation provenance

New preparation reports use version 2, adding exact normalized byte lengths and
the held-out snapshot hash. Import them with a pinned report hash:

```sh
cogfabric --home ./state --json candidate create-prepared --skill rust.s1 \
  --bundle ./prepared-bundle --report-sha256 REPORT_SHA256 --license LICENSE_LABEL
cogfabric --home ./state --json candidate provenance CANDIDATE_UUID
```

The importer opens fixed bundle entries relative to a retained directory
descriptor. It verifies exact report, artifact and held-out bytes, strict report
versions/flags, source metadata, normalized bodies, byte framing, duplicate lineage,
split coverage and overlap checks before quarantine. Embedded Markdown headings
remain data because byte lengths govern framing. Held-out text enters neither
the candidate artifact nor the provenance database snapshot.

Candidate registration and a metadata-only provenance task/event commit in one
SQLite transaction; audit failure rolls back candidate/version/anchor rows. A
content-addressed orphan quarantine blob can remain after database failure, as
with ordinary imports, and stays subject to artifact quotas. Provenance anchors
are protected from task-history retention. Inspection validates the saved report
hash/candidate binding using a read-only store and reports
`evaluation_authority: false`.

The aggregate candidate license label is an explicit operator assertion; each
original source/license declaration stays in the report. Hashes are local integrity
evidence, not signatures or proof against same-owner edits. Prepared provenance is
not an integrity/quality/regression evaluation and cannot promote a candidate.
Independent evaluation and explicit approval still apply. Version-1 preparation
bundles need re-preparation for this path; ordinary manual artifact import remains
available without invented provenance.

## Bounds and limits

Source reads use directory-relative `openat` with retained directory descriptors,
no-follow flags and nonblocking regular-file validation. Absolute paths, traversal,
symlinks, FIFOs, invalid UTF-8 and unsupported control characters are refused.
Each file is at most 64 KB; total input is at most 512 KB; the manifest is at most
32 KB; the reference artifact is at most 256 KB. Paths have at most eight components
and 240 bytes. A five-second elapsed ceiling is checked between bounded reads;
this cannot preempt a blocking kernel/filesystem operation.

Cleaning removes a leading UTF-8 BOM, normalizes newlines and trims trailing
whitespace. Exact normalized duplicate documents within a split are deduplicated.
Exact duplicate documents or paragraphs of at least 32 bytes crossing splits cause
refusal. This does not detect paraphrases, semantic leakage, factual errors,
poisoning or malicious instructions. All content remains untrusted retrieval data.
Automatic internet discovery and independently proven source quality remain open
requirements; no new model filesystem/network capability is introduced.

Remote hash-pinned text can first be fetched with `candidate fetch-sources`; see
[source acquisition](SOURCE_ACQUISITION.md). Its generated draft still requires
explicit review and independent split assignment before this preparation step.
