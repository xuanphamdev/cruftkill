# Phase 02 — Code Review

**Date:** 2026-05-27
**Reviewer:** code-reviewer
**Scope:** `src/core/scanner.rs`, `src/core/ignore.rs`, `src/core/mod.rs`, `tests/scanner_smoke.rs`, `Cargo.toml` (new deps).

## Verdict

**APPROVED with two non-blocking concerns** (one MEDIUM dispatch-backpressure deadlock under pathological trees, one LOW <100ms vs <500ms doc/test discrepancy). All Phase 02 acceptance criteria met. Phase 03 (sizes) and Phase 04 (risk) are not blocked.

## Verification (re-run locally)

| Check | Result |
|---|---|
| `cargo test` | 30 passed (4 suites, 0.05s) |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo fmt --check` | clean |

Toolchain: `$HOME/.cargo/bin/cargo` 1.89.0 (rustup).

## Acceptance Criteria

| # | Criterion | Status |
|---|---|---|
| 1 | `start_scan(root, opts) -> ScannerHandle{results, cancel, stats}` | ✅ scanner.rs:88 |
| 2 | N workers, default `min(num_cpus, 8)` | ✅ `optimal_workers()` = `clamp(1, 8)` |
| 3 | Round-robin dispatch | ✅ dispatcher.rs:142,151 |
| 4 | Cancellation <500ms | ✅ test: `cancellation_stops_scan_quickly` |
| 5 | Live streaming (no batch wait) | ✅ each match `send().await` in explore_dir |
| 6a | targets = exact basename | ✅ `targets.iter().any(|t| t == name_str)` |
| 6b | exclude = substring of full path | ✅ `subpath_str.contains(ex)` |
| 6c | no descent into matched targets | ✅ test: `finds_targets_and_skips_nested_targets` |
| 6d | GLOBAL_IGNORE skipped unless itself a target | ✅ tests: `global_ignored_can_be_target`, `does_not_descend_into_global_ignored_unless_target` |
| 6e | symlinks never followed | ✅ `ft.is_symlink()` early continue; test on Unix |
| 6f | permission errors silently skipped | ✅ `match ... Err(_) => continue/return` |
| 7 | Tests cover invariants | ✅ 8 integration tests |

## Correctness analysis

### Completion-detection (`pending: AtomicUsize`)

**No races / underflow / ordering bugs.**

- Invariant: every successful `fetch_add(1)` is paired with exactly one `fetch_sub(1)`.
  - Pair 1: parent seed `add` → worker's `explore_dir` end `sub` (or rollback on `try_send` err).
  - Pair 2: worker enqueue child `add` → child's `explore_dir` end `sub` (or rollback on `send` err).
- `explore_dir` has two return paths (`read_dir` err and end-of-loop); each calls `decrement_pending` exactly once. No double-decrement.
- `fetch_sub(1)` returns prev value; `== 1` test fires `cancel.cancel()` at most once per logical job draining. `cancel.cancel()` is idempotent.
- SeqCst ordering is correct (overkill but safe). The "decrement, observe prev, cancel" sequence is linearizable because `fetch_sub` is atomic and only the observer who saw `prev == 1` cancels.
- Underflow impossible under the invariant above. Reviewed all paths.

### Channel-close safety

**Shutdown ordering correct.**

1. Parent drops its `result_tx` and `job_tx` clones after spawning (scanner.rs:127-128). The only `result_tx` clones live in `WorkerHandles`; the only `job_tx` clones live in `WorkerHandles` (dispatcher holds `rx`, not `tx`).
2. On natural completion: last decrement → cancel.cancel() → all workers' `biased` select wakes on cancel → workers `break` → drop `WorkerHandles` → all `tx_dispatch` and `tx_results` clones drop → dispatcher `rx.recv()` returns None → dispatcher drops `outs` (worker inboxes) → worker inboxes close.
3. On cancel from caller: same path, but pending counts may leak — harmless (no resource leak).
4. `result_rx` closes after the last worker exits → `drain()` terminates. ✅

**No deadlocks identified in the cooperative-cancel path** (see one caveat under "Backpressure" below).

### Resource leaks (spawned tasks)

All spawned tasks have two exits:
- Cancel branch in `biased` `select!` (immediate).
- `None` from `recv()` (channel closed).

No task awaits indefinitely on a future without cancel branch except inside `explore_dir` itself, where it polls `cancel.is_cancelled()` once per directory entry. The single un-instrumented await is `h.tx_dispatch.send(...).await` — bounded by dispatcher reaction time (<1ms in practice; <500ms worst case from tests).

## Findings

### MEDIUM

**M1. Potential dispatch-graph deadlock under pathological fan-out.**
`src/core/scanner.rs:227`. The worker's `h.tx_dispatch.send(Job::Explore(subpath)).await` is the only un-cancellable await inside `explore_dir`. With bounded channels (`job_rx`=1024, each worker inbox=256), there exists a theoretical deadlock state:

- All 8 workers are inside `explore_dir`, each blocked on `tx_dispatch.send().await`.
- `job_rx` is full (1024 buffered).
- Each worker inbox is full (256 buffered).
- Dispatcher is blocked on `outs[idx].send(job).await` → next worker is itself blocked sending.

Reachability requires a tree where the in-flight set of explore jobs simultaneously exceeds `1024 + 8*256 = 3072` AND every worker is in the middle of its own explore — i.e., a single directory with >~3000 non-target subdirs, or a tree where fan-out outpaces drain. Unlikely on a typical home dir; not impossible on `/`, large monorepos, or `node_modules` of build artifacts.

Recommendations (in increasing impact, all suitable for a follow-up):
1. Wrap the send in `select!` with `cancel.cancelled()` so cancellation always frees the worker.
2. Use `try_send` with fallback: on `Full`, push to a thread-local Vec and drain after the read_dir loop.
3. Switch `job_tx` to `mpsc::unbounded_channel()` — memory bound becomes tree breadth, but no backpressure deadlock.

Option 1 is the smallest change and turns the deadlock into "scan stops on cancel" — acceptable; consumer can re-tune buffers later. The plan's risk table mentions only the *results* channel deadlock and prescribes capacity 1024 there — this is a different (and worse) class.

### LOW

**L1. Plan promises cancel <100ms, test asserts <500ms.**
phase-02-core-scanner.md:178 ("`Ctrl-C` ... stops within 100 ms") vs `tests/scanner_smoke.rs:153` (`< 500ms`). The plan's own Requirements section says "~50 ms". Test threshold is generous to avoid CI flakes but undersells the real performance. Either tighten the test (e.g., 100ms with a single retry) or reword the plan's success criterion to <500ms.

**L2. `try_send` error branch is functionally dead.**
`scanner.rs:117`. At construction the channel cannot be `Full` (cap 1024, single send) or `Closed` (dispatcher just spawned, holds `rx`). Defensive code is fine; consider an `expect("seed: dispatcher not running")` or a `debug_assert!` to surface a real future bug (e.g., if the spawn ordering changes).

**L3. `to_string_lossy()` is called twice per non-target entry.**
`scanner.rs:196` (name) and `scanner.rs:213` (subpath). For paths containing non-UTF8 bytes this produces lossy-mangled strings used for both `exclude` matching and global-ignore lookup. npkill's TS code has the same property (strings only), so behavior matches — but it is worth a code comment so a future reviewer doesn't try to "fix" it to OsStr-level matching, which would diverge from npkill semantics.

**L4. `cancellation_stops_scan_quickly` does not assert pending invariant on cancel-leak path.**
Cancel-with-pending-not-zero is correct (we documented it as acceptable to leak the counter), but the test cannot detect a regression that double-decrements pending → underflows → `usize::MAX` → never cancels naturally. A targeted test (e.g., probabilistic via many parallel scans + miri) is out of scope for Phase 02, but flagging for Phase 02.5 polish or a future loom-based test.

### NIT

**N1. `Drop` impl on `ScannerHandle` could auto-cancel.**
Currently if the caller drops the handle without consuming the receiver, workers may keep running until the result channel fills (1024 + buffer in each worker). Adding `impl Drop for ScannerHandle { fn drop(&mut self) { self.cancel.cancel(); } }` makes the API fail-safe. Optional; document either way.

**N2. `MAX_WORKERS = 8` is hard-coded, no override.**
Reasonable default. Phase 03 (size) and Phase 06 (TUI) may want to tune this for very-many-core machines or very-slow disks. Consider exposing via `ScanOptions.workers: Option<usize>` later.

**N3. `ScanStats` lacks a `pending` field.**
The phase plan's architecture section (lines 43–47) lists `pending: AtomicUsize` on `ScanStats`. Implementation has `pending` as a *separate* `Arc<AtomicUsize>` (internal only). This is a deliberate, sensible deviation — `pending` is an internal completion signal, not a user-facing stat — but worth either documenting on `ScanStats` ("does not expose in-flight jobs by design") or adding a `pub fn pending(&self) -> usize` on `ScannerHandle` for live progress UI in Phase 06.

**N4. `WorkerHandles` clones happen per worker, not per job.**
Each worker holds one `WorkerHandles`; cloned 8 times at spawn. Senders are cheap to clone. Good. ✅

**N5. `worker_loop`'s biased `cancel`-first select is right.**
On cancel during an in-flight `rx.recv()`, the worker breaks and drops the receiver. ✅

## Documented deviation from npkill (per task brief)

**Accepted.** npkill's per-worker 100 concurrent `readdir`s exploit Node's libuv pool; the Rust port uses tokio's multi-thread runtime with 8 workers processing jobs serially. Tokio's `tokio::fs::read_dir` is backed by `spawn_blocking` (default blocking-thread pool size 512), so concurrency at the syscall level is bounded by the runtime, not by our worker count. For interactive workloads this is sufficient. The comment in `scanner.rs:14-18` correctly identifies the path to escalate if benchmarks later show contention. ✅

If Phase 03's size computation lands in the same worker abstraction and adds further serial blocking I/O per worker, revisit. A per-worker `Semaphore::new(100)` + `JoinSet` is a contained change.

## Phase 03/04 readiness

- **Phase 03 (sizes)**: scanner's `WorkerHandles` shape (`tx_dispatch`, `tx_results`, `cfg`, `stats`, `cancel`) is general enough to extend with `Job::Size(PathBuf)`. The current `Job` enum has only one variant; adding more is trivial. Recommend Phase 03 keep `size` jobs in the *same* worker pool to avoid duplicating cancellation/completion plumbing.
- **Phase 04 (risk)**: `RiskAnalysis::safe()` is wired but always returns safe. `explore_dir:219` is the single call site to swap for real analysis. ✅ No structural change needed.
- **Phase 06 (TUI)**: `ScannerHandle` is `Send` (all fields are). The `Receiver<ScanFoundFolder>` integrates cleanly with `tokio::select!` in the event loop. ✅

## Idiomatic Rust checklist

- [x] `snake_case` files/functions, `PascalCase` types.
- [x] No `unwrap()`/`expect()` outside `#[cfg(test)]`.
- [x] No `unsafe`.
- [x] Doc comments on every `pub` item.
- [x] Atomics + `Arc` for shared mutable state.
- [x] `tokio::select! { biased; ... }` priorities cancel correctly.
- [x] Error types: silent skips for I/O, matching npkill semantics.

## Acceptance criteria NOT met

None.

## Recommended actions before Phase 03

1. (Optional, recommended) Wrap `h.tx_dispatch.send(...)` in a `select!` with `cancel.cancelled()` — addresses M1 without architectural change. ~5 lines.
2. (Optional) Add `Drop for ScannerHandle` that calls `cancel.cancel()` (N1).
3. (Optional) Resolve plan-vs-test cancel threshold (L1).

None of the above are blocking. Phase 03 can start.

## Unresolved questions

- Should `tx_dispatch.send` be wrapped in `select!` against `cancel.cancelled()` in Phase 02.5, or deferred to Phase 03 when the worker abstraction is touched anyway? (Recommendation: do it now while the file is fresh; 5-line change.)
- Should `ScanStats` expose `pending` (in-flight jobs) for a live progress indicator in the TUI (Phase 06)? Plan says yes; implementation says no. Confirm before Phase 06 starts.
