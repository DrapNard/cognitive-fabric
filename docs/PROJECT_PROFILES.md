# Explicit project profiles

Architecture §3 calls for tracking user preferences and project goals. Project
profiles implement explicit operator-declared tracking. They do not infer user
preferences, learn a personality, or establish the truth of a project goal.

## Publish and select

Publish a strict local JSON manifest:

```json
{
  "format_version": 1,
  "id": "example.project",
  "name": "Example project",
  "goal": "Document and measure the Rust framework.",
  "preferences": {"verbosity": "brief", "format": "bullets"}
}
```

```bash
cogfabric project-publish --manifest /absolute/path/project.json
cogfabric project-show example.project@1
cogfabric project-list
cogfabric ask 'Explain the next milestone' --project example.project@1
```

Publishing the same ID creates the next immutable version. A task must select an
exact `id@version`; there is no automatic latest-profile selection. `--socket`
uses the same optional typed FlatBuffers field. The HTTP compatibility server
currently does not expose this selector. No profile is loaded for an unselected
task. Existing conversation references remain a separate feature.

Preferences are closed enums: verbosity is `brief`, `balanced`, or `detailed`;
format is `prose` or `bullets`. The default is balanced prose. Unknown fields,
permissions, budgets and executable settings are rejected. IDs use the existing
bounded lowercase identifier convention; names are limited to 120 bytes and
goals to 2,000 bytes. The registry allows 64 projects and 32 immutable versions
per project. Exact canonical hashes bind the profile fields. Listing and reads
validate bounds and checksums; hashes are not publisher signatures.

## Task behavior and recovery

Only the explicit selection reaches task admission. The task records the exact
profile version, canonical hash and bounded JSON snapshot. Its project goal is
quoted background data. Presentation preferences reach interpretation and eligible
explanation calls; they cannot replace the current objective, change structured
output criteria, grant tools, increase budgets, or establish verification.
Original prompt-byte, model-call, token and wall-clock limits still apply.

The task's context hash binds the selected snapshot. Continuation validates and
restores that original snapshot, not a newly published version or a mutable
profile lookup. Missing, duplicate, inconsistent or tampered profile evidence
refuses continuation. Existing unsupported-checkpoint and uncertain-call gates
remain in force. Legacy tasks without profiles retain their recovery rules.

Profiles and snapshots are persisted in the local SQLite database (schema 9).
Operator publication/read permissions remain outside model roles. Profiles are
explicitly stored text, not a secret store. Backup/restore includes this data;
ordinary task retention does not rewrite immutable profile versions. A hostile
database owner can alter local data and hashes; local integrity checks do not
provide external attestation.

## Verification scope

Store tests cover strict input bounds, checksums, immutability, quotas, isolation,
operator-only permissions and migration. Integration tests cover CLI publication
and selection, no default application, invalid-input refusal before initialization,
and socket transport. Engine/recovery tests cover bounded context, presentation
requests and original snapshot restoration. These checks establish mechanics, not
live-model adherence to presentation preferences or general preference learning.
Final serialized workspace suites pass 1,048 default tests and 1,057 all-feature
tests across 72 targets, zero failed or ignored. Formatting, Rust 1.99 workspace
check and strict default/all-feature Clippy pass. No additional test or installed
smoke run was started after the operator requested implementation work.
