# Phase 05 — completion report

## Status

**Completed** 2026-05-27.

## Delivered

- `src/core/delete.rs` (~95 LoC) — `delete(path, scan_root, targets, dry_run) -> DeleteResult`:
  - Guard 1: basename in `targets` (via Phase 04 `safe_delete::is_safe_to_delete`)
  - Guard 2: `std::fs::canonicalize(path).starts_with(canonicalize(scan_root))` — catches symlink + `..` escape
  - Real delete via `tokio::task::spawn_blocking(|| std::fs::remove_dir_all(canon_path))`
  - Dry-run: random 200–4200 ms sleep, no FS touch, returns success
- `tests/delete_guard.rs` — 8 integration tests:
  1. happy path delete
  2. dry-run never touches FS
  3. basename-not-in-targets rejected
  4. path-outside-root rejected
  5. nonexistent path → failure (not panic)
  6. symlink to outside (non-target basename) — caught by guard 1
  7. **`..` traversal — caught by guard 2** (added post-review)
  8. symlink-with-target-basename to outside — caught by guard 2
- `src/core/mod.rs` — added `pub mod delete;`

## Behavioral invariants — verified

| # | Invariant | Test |
|---|---|---|
| 8 | Delete path must be inside scan root | 4, 7, 8 |
| Guard 1 | Basename must be in targets | 3 |
| Symlinks never followed by `remove_dir_all` | Guard 2 + Rust std (CVE-2022-21658 hardened) | 6, 8 |

## Gates

| Gate | Result |
|---|---|
| `cargo test` | 95 passed across 7 suites (was 84, +11 from Phase 05 — including 1 added in review fix) |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| Independent code review | DONE_WITH_CONCERNS → all addressed |

## Review outcome

Reviewer security walkthrough found **no bypass paths**:
- symlink → `/etc` escape: caught by guard 2 (verified empirically on macOS)
- `..` traversal: caught by guard 2 (now has explicit test — MEDIUM #1 fix)
- `remove_dir_all` follow-symlinks: confirmed Rust std does NOT follow (CVE-2022-21658 hardened)
- macOS `/private` prefix: both sides canonicalize symmetrically — no false negative
- Windows `\\?\` prefix: same logic
- TOCTOU race: same as npkill, documented in plan

**Addressed**:
- **MEDIUM #1** — added `parent_traversal_resolving_outside_root_is_rejected` test
- **MEDIUM #2** — updated phase-05 plan doc to use `std::fs::canonicalize` (matching implementation); `dunce` deferred to v0.2 polish (display-only on Windows)

**Deferred** to Phase 07 / v0.2:
- **MEDIUM #3** — `DeleteResult::path` echoes caller path, not canonical. Acceptable for v0.1 (UI displays caller's path)
- **LOW** — `DeleteResult` could distinguish "rejected at guard" vs "partial delete" — Phase 07 might want this distinction for UX
- **LOW** — `spawn_blocking` error path loses `io::ErrorKind` — formats to string

Full review: [`phase-05-review.md`](phase-05-review.md).

## Unresolved questions (for Phase 07)

1. Should `DeleteResult.path` be the canonical path (audit trail) or the caller path (UI display)?
2. Phase 07 should call `is_safe_to_delete` to gate the keybind itself, before invoking `delete::delete`. Wire this in Phase 07.

## Next

Phase 06 (profiles + sort + filter) is unblocked and independent.
Phase 07 (TUI) needs Phase 06 first.
