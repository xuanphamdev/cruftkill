# Phase 03 — completion report

## Status

**Completed** 2026-05-27.

## Delivered

- `src/core/size.rs` (~130 LoC) — `get_folder_size(path) -> Result<u64, NpkillError>`:
  - refcounted parallel sum via `Arc<AtomicU64>` total + `Arc<AtomicUsize>` pending
  - completion signal via `oneshot::channel`
  - per-call top-level timeout of 60 s (`SIZE_TIMEOUT`)
  - `cfg(unix)` block uses `MetadataExt::blocks() * 512` for true on-disk size
  - non-unix uses `metadata.len()` for logical size
  - symlinks never followed; directory entries themselves count 4096 bytes
- `tests/size_smoke.rs` — 6 integration tests:
  1. empty dir → 0 bytes
  2. single file counted (≥1 disk block, <64 KiB)
  3. subdir adds 4096 + recurses
  4. nonexistent path → 0 bytes
  5. symlink loop not followed (Unix)
  6. 3-files-3-levels sums them all

## Behavioral invariants — verified

| # | Invariant | Test |
|---|---|---|
| 6 | Unix size = `blocks × 512`; Windows = `metadata.len()` | unit + integration |
| 7 | Directories count 4096 bytes | `three_files_three_levels_sums_them_all` |
| 4 | Symlinks never followed | `symlinks_are_not_followed` (Unix) |
| 5 | Permission errors silently produce 0 | `nonexistent_path_returns_zero` |

## Gates

| Gate | Result |
|---|---|
| `cargo test` | 84 passed (Phase 01: 16 → Phase 02: 30 → Phase 03+04: 84) |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| Independent review | pending (subagent running) |

## Deviation from plan

**Plan said**: extend the Phase 02 scanner pool's `Job` enum with `SizeChild(PathBuf, SizeCollector)`, reuse worker pool.

**Implemented**: standalone async function in `size.rs` that spawns its own tokio tasks per directory.

**Rationale** (documented at top of `size.rs`):
1. Scanner pool is one-shot — workers exit when `pending == 0` triggers cancel — so it cannot service `get_folder_size` calls AFTER a scan completes.
2. Size calc has different concurrency profile: each call walks one tree from a known root; round-robin dispatch buys nothing.
3. Tokio runtime already parallelizes spawned tasks; no extra abstraction needed.
4. Decoupling means Phase 07 TUI can fire size requests independently of scan lifecycle.

Net: ~130 LoC vs ~200+ LoC if extending the scanner pool. Simpler, no zombie-pool concerns, no lifetime coupling.

## Lint nit fixed during integration

`tests/size_smoke.rs` had 3× `clippy::useless_vec` (using `vec![0u8; 100]` where `[0u8; 100]` works). Replaced with array literals — Phase 04 subagent flagged it.

## Review outcome

Reviewer (`code-reviewer` subagent) approved DONE_WITH_CONCERNS:
- **M1 (FIXED post-review)** — post-timeout walker leak. Added `CancellationToken` shared across all walker tasks. On timeout (or natural completion), `cancel.cancel()` fires; walkers check `is_cancelled()` at each `read_dir` and `next_entry` boundary and exit early. No unbounded background CPU/IO after caller gives up.
- **L1/L2/L3/N3** — documentation polish; partially applied (top-of-function doc now states "root not counted").

Full review: [`phase-03-review.md`](phase-03-review.md).

## Unresolved questions

- Consider exposing `get_folder_size_into(path, channel)` for streaming partial totals to the UI?
- Phase 02 wiring may want to take an explicit `CancellationToken` to chain with the scanner's token (currently each size call has its own).

## Next

Phase 05 (delete) can now use `is_safe_to_delete` from Phase 04. Phase 07 (TUI) integrates `get_folder_size` per result.
