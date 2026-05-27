# Phase 06 — completion report

## Status

**Completed** 2026-05-27.

## Delivered

- `src/core/profiles.rs` (~280 LoC, mostly data) — 17 hardcoded profiles ported verbatim from npkill `BASE_PROFILES`; `resolve_targets(names)` dedupes via `BTreeSet`; `"all"` expands to union; `profile_names()` for CLI help.
- `src/core/sort.rs` (~120 LoC) — `sort_results(items, by)`:
  - Path: lexicographic asc
  - Size: desc, tiebreak by path asc, `None` last
  - Age: oldest first, tiebreak by path asc, `None` last
- `src/core/filter.rs` (~50 LoC) — case-insensitive substring filter on path.
- Inline tests: 9 (profiles) + 7 (sort) + 4 (filter) = 20 new tests.

## Gates

| Gate | Result |
|---|---|
| `cargo test` | 115 passed (Phase 05: 95 → +20) |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo fmt --check` | clean |

## Notes

- Profile targets are stored as `&'static [&'static str]` (zero-alloc).
- `resolve_targets` returns owned `Vec<String>` because the CLI layer needs to combine with `--target` extras and pass to `ScanOptions.targets`.
- Sort comparators match npkill's `FOLDER_SORT` semantics exactly (verified by inspection against `src/constants/sort.result.ts`).

## Next

Phase 08 (CLI) consumes all three modules.
