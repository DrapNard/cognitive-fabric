# Capability-gap source catalog discovery

Architecture sections 4 and 8 call for available-skill reuse and source discovery
when a capability is missing. The catalog workflow connects a recorded Jev+ gap
to operator-reviewed remote source metadata. It performs no DNS, downloads,
inference, candidate creation or automatic approval.

## Discover a plan

```sh
cogfabric --home ./state --json candidate discover-remote-sources TASK_UUID \
  --capability rust.async.s1 --catalog ./catalog.json --sha256 CATALOG_SHA256 \
  --max-sources 8 --output ./source-plan-draft
```

The task must contain a valid recorded `capability_gap` event for that exact
capability. Failed/completed tasks are eligible. A currently available matching
skill causes refusal so it can be reused first. The command opens an existing
current-schema database without bootstrap or inference initialization.

`CATALOG_SHA256` is the lowercase SHA-256 of exact catalog bytes. All source
entries are validated, including entries that do not match the requested
capability. Selection uses exact declared capability IDs, then ascending numeric
priority and source ID. Repeated content hashes are counted and retain only the
first ranked source. Priority is an operator declaration, not measured usefulness.

## Catalog format

```json
{
  "format_version": 1,
  "reviewer": "operator review identity",
  "catalog_reviewed": true,
  "sources": [
    {
      "id": "docs.async",
      "url": "https://docs.your-project.org/async.txt",
      "sha256": "REPLACE_WITH_DOCUMENT_LOWERCASE_SHA256",
      "source": "operator-selected upstream async documentation",
      "license": "REPLACE_WITH_REVIEWED_LICENSE_LABEL",
      "capabilities": ["rust.async.s1"],
      "priority": 10
    }
  ]
}
```

Placeholders are not valid pins. The strict JSON schema refuses duplicate keys
and unknown fields. It requires reviewed catalog metadata and 1–128 sources,
unique source IDs and canonical URLs, 1–16 unique capability IDs per source, and
priority 0–1000. Source IDs, URLs, hashes and provenance/license labels use the
same bounded syntax rules as [source acquisition](SOURCE_ACQUISITION.md).
Catalogs are at most 65,536 bytes; selected output is limited to 1–32 sources and
the acquisition plan's 32 KB bound. All parsed content remains untrusted metadata.

## Output and stale-gap checks

A new private output directory contains:

- `acquisition-plan.json`: a normal acquisition manifest with an empty reviewer
  and `acquisition_approved: false`.
- `discovery.json`: catalog hash and reviewer assertion, task/capability identity,
  exact gap event ID/checksum, selected source IDs, matched/duplicate counts, plan
  hash and explicit no-network/no-permission/no-evaluation/no-promotion flags.

Before publication the original gap identity is checked again and the capability
must still be unavailable. A new matching gap, changed history, deleted task or
newly available skill makes the draft stale. This is an observation at that check,
not an exclusive lease on the registry after publication. Output never overwrites
an existing directory. Selection and publication checks share a five-second
logical deadline; filesystem calls and mutex acquisition are not hard real-time
operations. Gap inspection also has SQLite interruption and bounded strict history.

Review the proposed sources and licenses, retain the discovery record, set an
acquisition reviewer and explicit approval in the plan, then hash the edited plan
and invoke `candidate fetch-sources`. The fetcher revalidates URL/DNS/IP safety,
response framing, document hashes and resource bounds. An unedited discovery plan
is refused before any network request. Editing approval changes its exact hash;
the discovery report's hash continues to identify the original unapproved draft.

This is discovery within an explicitly reviewed catalog. It does not search the
open web, prove dataset quality or establish legal/independent review. The separate
discovery record does not confer evaluation authority or become a learned skill.
Preparation, inactive candidate registration, independent evaluation, promotion
and rollback remain separate gates.

## Verification record — 2026-10-24

- `cargo fmt --all -- --check` and `git diff --check`: passed.
- `cargo +1.99.0 check --workspace --all-targets --all-features --locked`: passed.
- `cargo +1.99.0 clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo +1.99.0 test -p cogfabric --test source_catalog --test source_acquisition --test source_discovery --test source_preparation --test prepared_candidate --locked`: fourteen passed across the five targets.
- `cargo +1.99.0 test -p cogfabric --lib gaps::tests --locked`: two passed, including strict duplicate-key refusal, expired-query admission and progress-handler cleanup after interruption.
- `cargo +1.99.0 test -p cogfabric --lib evolution::catalog::tests --locked`: one passed.
- `cargo +1.99.0 test -p cogfabric --lib evolution::acquisition --locked`: nine passed using local fixtures.

Catalog tests cover deterministic CLI/library output, no automatic acquisition
approval, failed-task gaps, new/modified/deleted gap refusal (including orphaned
history), available-skill refusal, invalid unmatched entries, padded source labels,
128-entry/32-selection success, 129-entry refusal and symlink/no-overwrite behavior.
Shared source-label validation counts raw bytes, including padding, at a 100-byte
limit; license labels retain their 128-byte bound. Review labels do not establish
independent review. No public fetching, reflection campaign or reasoning benchmark
was performed for this change.
