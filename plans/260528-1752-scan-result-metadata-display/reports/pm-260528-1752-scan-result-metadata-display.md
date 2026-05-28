---
type: pm-report
plan: 260528-1752-scan-result-metadata-display
created: 2026-05-28
---

# PM Report: Scan Result Metadata Display

## Summary

Implemented per-result metadata for scan results. TUI now shows ecosystem, cleanup category, risk level, and hint under each path. NDJSON keeps old keys and adds metadata keys.

## Plan Status

| Phase | Status |
|---|---|
| Audit existing result flow | completed |
| Design metadata model | completed |
| Implement metadata enrichment | completed |
| Update TUI and JSON output | completed |
| Add tests and docs | completed |

## Verification

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo test` | pass, 181 tests + 2 doc tests |
| `cargo clippy --all-targets --all-features -- -D warnings` | pass |
| Tester subagent | pass |

## Review Follow-Up

- Removed root-level metadata re-export to reduce public API lock-in.
- Marked metadata structs/enums `#[non_exhaustive]`.
- Added cached reverse target-to-ecosystem lookup via `OnceLock`.
- Converted metadata to borrowed/static fields to reduce TUI redraw allocation.
- Added NDJSON tests for normal, custom-target, and sensitive-path branches.
- Moved scanner risk emission from placeholder-safe to `risk::analyze_with_home`.
- Treat disabled risk analysis as medium advisory metadata instead of low/safe.
- Updated README sample to match actual ecosystem ordering.

## Docs Impact

Minor. README and `docs/architecture.md` updated.

## Unresolved Questions

None.
