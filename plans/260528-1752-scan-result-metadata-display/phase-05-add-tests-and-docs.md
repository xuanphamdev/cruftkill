---
phase: 5
title: Add tests and docs
status: completed
priority: P1
effort: 1h
dependencies:
  - 4
---

# Phase 5: Add tests and docs

## Context Links

- Tests: [tests](../../tests)
- README: [Project README](../../README.md)
- Cargo checks: [Cargo.toml](../../Cargo.toml)

## Overview

Lock behavior with targeted tests and update user-facing docs/examples.

## Requirements

- Functional: tests cover metadata classification and output shape.
- Non-functional: no syntax/build errors; no network-dependent tests.
- Documentation: README explains new fields and TUI metadata line.

## Architecture

Test at core level first; add integration checks only where cheap.

Suggested tests:

- `metadata_known_target_maps_to_ecosystem`
- `metadata_overlapping_target_lists_all_ecosystems`
- `metadata_sensitive_path_is_high_risk`
- `metadata_custom_target_is_medium_review`
- `no_tui_json_contains_metadata_fields`

## Related Code Files

- Create: `tests/metadata_table.rs` or add unit tests in `src/core/metadata.rs`
- Modify: `tests/scanner_smoke.rs` if scanner payload changes.
- Modify: `README.md`

## Implementation Steps

1. Add table-driven tests for metadata model.
2. Update existing constructor tests for new fields.
3. Add no-tui output assertion if current test harness can run binary cheaply; otherwise cover serialization helper in unit tests.
4. Run:
   ```bash
   cargo fmt --all -- --check
   cargo test
   cargo clippy --all-targets --all-features -- -D warnings
   ```
5. Update README examples and safety model wording.

## Success Criteria

- [ ] All new metadata logic has tests.
- [ ] Existing tests pass.
- [ ] Cargo fmt/test/clippy pass.
- [ ] README reflects TUI and NDJSON metadata.

## Risk Assessment

Risk: clippy flags large display helpers. Mitigation: keep helpers focused, no clever iterator chains if a loop is clearer.

Risk: binary integration test flaky due filesystem timing. Mitigation: use tempdir and deterministic small folders.

## Security Considerations

Docs must state metadata is advisory; deletion remains permanent and guarded by existing confirmation/delete checks.

## Next Steps

Implementation handoff: `/ck:cook /Users/reiz/Data/Workspace/MyProject/nodemoduleskiller/plans/260528-1752-scan-result-metadata-display/plan.md`
