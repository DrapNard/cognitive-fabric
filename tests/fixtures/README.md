# Frozen compatibility inputs

`legacy_checker_manifest.json` is copied verbatim from the checker example before
`87f278a` changed current project metadata to GPL-3.0-only. Its Apache-2.0 field
describes that historical manifest and must remain unchanged: the canonical-hash
regression verifies compatibility with previously stored immutable versions.
Current examples are parsed and validated separately. This fixture is historical
input data, not a declaration of the current Rust package's license.
