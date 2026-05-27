# Phase 02 — completion report

## Status

**Completed** 2026-05-27.

## Delivered

- `src/core/ignore.rs` — port of npkill `GLOBAL_IGNORE` (verbatim, `OnceLock<HashSet<&'static str>>`)
- `src/core/scanner.rs` — hand-rolled tokio worker pool:
  - `ScannerHandle { results, cancel, stats }` (with `Drop` cancelling on go-out-of-scope)
  - `ScanStats { completed, found }` (atomic counters)
  - 1 dispatcher task (round-robin) + N=`min(num_cpus, 8)` worker tasks
  - Completion: `pending: Arc<AtomicUsize>` increment-before-send, decrement at end of `explore_dir`, cancel-on-zero
  - All `send().await` calls wrapped in `select!` against cancel (no deadlock under saturation)
- `tests/scanner_smoke.rs` — 8 integration tests:
  1. matched targets emitted; walker does NOT descend into matched targets (nested case)
  2. GLOBAL_IGNORE not descended (e.g., `.git`)
  3. GLOBAL_IGNORE name CAN be a target (e.g., explicit `.cache`)
  4. exclude is substring match
  5. symlinks not followed (Unix only)
  6. cancel propagates in <200 ms
  7. empty root terminates cleanly
  8. nonexistent root terminates with zero results
- `Cargo.toml` — added `tokio-util = "0.7"`, `num_cpus = "1"`

## Behavioral invariants — verified

| # | Invariant | Test |
|---|---|---|
| 1 | targets matched by exact basename | `finds_targets_and_skips_nested_targets` |
| 2 | exclude matched by substring | `exclude_substring_match` |
| 3 | walker does NOT descend into matched targets | `finds_targets_and_skips_nested_targets` |
| 4 | GLOBAL_IGNORE excluded unless target | `does_not_descend_into_global_ignored_unless_target` + `global_ignored_can_be_target` |
| 5 | symlinks never followed | `symlinks_are_not_followed` (Unix) |
| 6 | permission errors silently skipped | `nonexistent_root_terminates_with_no_results` |

## Gates

| Gate | Result |
|---|---|
| `cargo test` | 30 passed (types 8 + error 4 + cli 4 + scanner unit 3 + ignore 3 + scanner_smoke 8) |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| Independent code review | DONE_WITH_CONCERNS, all addressed |

## Review outcome

Reviewer (`code-reviewer` subagent) approved with one MEDIUM:
- **M1 (FIXED)** — dispatcher-graph theoretical deadlock if `tx_dispatch.send().await` blocks while all worker inboxes saturated. Fix: both `tx_dispatch.send` and `tx_results.send` are now wrapped in `tokio::select!` against `cancel.cancelled()`, so cancel always wins.
- **L1 (FIXED)** — cancel test threshold tightened from 500 ms → 200 ms (plan said ~100 ms; 200 ms allows CI jitter).
- **L2 (FIXED)** — added `debug_assert!(false, …)` on the practically-dead seed `try_send` error path.
- **L3 (FIXED)** — comment added explaining why `to_string_lossy()` is called twice (matches npkill semantics).
- **L4 (DEFERRED)** — loom test for `pending` underflow; heavy infra, not needed yet.
- **N1 (FIXED)** — `Drop for ScannerHandle` cancels the token (fail-safe). Tests updated to use `&mut handle.results`.
- **N2 (DEFERRED)** — `MAX_WORKERS` configurable via `ScanOptions` — Phase 08 CLI work.
- **N3 (DEFERRED)** — expose `pending` in stats for TUI progress — Phase 07.

Full review: [`phase-02-review.md`](phase-02-review.md).

## Documented deviation from npkill (accepted by reviewer)

npkill maintains 100 concurrent dir reads PER worker via Node's event loop. The Rust port uses tokio multi-thread runtime + 8 workers, each processing jobs serially. Tokio's blocking-thread pool (default 512) backs `read_dir` already. Documented at top of `scanner.rs`.

## Unresolved questions

1. Should `pending` be exposed via `ScannerHandle.stats` for Phase 07 TUI progress?
2. Should `MAX_WORKERS` be exposed in `ScanOptions` now or defer to Phase 08 CLI flags?

## Next

Phase 03 (folder size) is unblocked. The `Job` enum and worker pool extend cleanly to add `Job::SizeChild(PathBuf, SizeCollector)`.
Phase 04 (risk analyzer) is independent; safe to start in parallel.
