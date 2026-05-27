# Phase 03 — Code Review

**Date:** 2026-05-27
**Reviewer:** code-reviewer
**Scope:** `src/core/size.rs`, `src/core/mod.rs`, `tests/size_smoke.rs`.

## Verdict

**APPROVED with two non-blocking concerns** (one MEDIUM post-timeout task leak, one LOW empty-dir semantic). All Phase 03 acceptance criteria met. Deviation from plan (standalone async vs scanner-pool extension) is accepted. Phase 05 (delete) is NOT blocked.

## Verification (re-run locally)

Toolchain: `$HOME/.cargo/bin/cargo` 1.89.0 (rustup), rustc 1.89.0.

| Check | Result |
|---|---|
| `cargo test` | 84 passed (6 suites, 0.08s) |
| `cargo test --test size_smoke` | 6 passed (0.03s) |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo fmt --check` | clean |

## Acceptance criteria

| # | Criterion | Status |
|---|---|---|
| 1 | `get_folder_size(path) -> Future<Result<u64, NpkillError>>` | size.rs:40 |
| 2 | 60s top-level timeout (`SIZE_TIMEOUT_MS = 60000`) | size.rs:30 |
| 3 | Unix uses `blocks * 512`, others `metadata.len()` | size.rs:123-132 |
| 4 | Directories count 4096 bytes each | size.rs:97 |
| 5 | Symlinks NEVER followed, never counted | size.rs:92-95 |
| 6 | Permission errors silently → 0 for subtree | size.rs:73-77,86,89 |
| 7 | Returns `NpkillError::SizeTimeout(path)` on timeout | size.rs:50, error.rs:23 |
| 8 | Integration test with known-size tempdir | tests/size_smoke.rs (6 tests) |

## Correctness analysis — refcount termination

**No races, underflow, dropped-without-send, or ordering bugs.**

### Pending invariant

Every `pending.fetch_add(n)` is paired with exactly `n` `decrement` calls:

- **Pair 1**: initial seed `AtomicUsize::new(1)` ↔ root walker's terminal `decrement`.
- **Pair 2**: `pending.fetch_add(subdirs.len())` at size.rs:106 ↔ one `decrement` per spawned child walker (size.rs:111).

Each `walk` has exactly two return paths (early-return on `read_dir` err, end-of-fn); both call `decrement` once. No double-decrement, no missed decrement.

### Premature-zero impossible

`fetch_add(subdirs.len())` happens **before** the spawn loop and **before** self-decrement. So while children are being queued and started, the parent's "slot" still counts. The earliest a child could decrement is after the parent's add — both subsequent to the add. Linearizable.

### Exactly-once send on `oneshot`

`fetch_sub(1, SeqCst)` returns the previous value. Only one caller can ever observe `prev == 1`. That caller takes `Mutex<Option<Sender>>::lock().take()` — also exactly-once. `let _ = tx.send(())` ignores Err from a dropped rx (timeout case). ✅

### Empty-subdirs path

`if !subdirs.is_empty()` skips the add+spawn but still calls `decrement`. Correct — no leftover increments.

### Memory ordering

`Ordering::SeqCst` throughout. Overkill (AcqRel would suffice for the add/sub pairs and Release/Acquire for the cancel-style observation) but correct, and Rust's atomic compiler intrinsics make the perf hit negligible for this workload. Leave as-is. ✅

### Mutex<Option<Sender>>

Standard pattern. Lock is only held inside `decrement` on the terminal call, briefly, and never across `.await`, so no risk of poisoning impact or deadlock. The `if let Ok(mut g) = done.lock()` swallows a poisoned mutex — acceptable here because a poisoned `done` means a previous walker panicked, and the timeout path will surface the failure to the caller. Alternative `.expect("done mutex poisoned")` would be louder and arguably better surface — see L2.

## Symlink handling

Confirmed. `entry.file_type().await` on `tokio::fs::DirEntry` calls `lstat` semantics under the hood — it does NOT follow symlinks. `ft.is_symlink()` short-circuits before `is_dir()` (size.rs:92). The `symlinks_are_not_followed` test (size_smoke.rs:64) creates a `loopback` → root cycle and asserts total < 1 MB; would hang or stack-explode if the symlink were followed. ✅

## Unix vs non-Unix split for `real_size`

`#[cfg(unix)]` covers all tier-1/2 Unix targets (Linux, macOS, FreeBSD, NetBSD, OpenBSD, illumos, DragonflyBSD, Android, iOS). `std::os::unix::fs::MetadataExt::blocks()` is available on all of these. WASI and Redox don't have it, but WASI uses a separate cfg (`target_os = "wasi"`) and Redox is `target_os = "redox"`; neither matches `cfg(unix)` by default. Windows hits the `cfg(not(unix))` `len()` branch. ✅

Plan suggested `cfg(any(target_os="linux", target_os="macos", target_os="freebsd"))` — current `cfg(unix)` is **broader and better**.

## Resource leak — timeout vs spawned tasks (MEDIUM)

**M1. Spawned walker tasks keep running after the 60s timeout fires.**
`src/core/size.rs:48-51`. When `tokio::time::timeout` returns Err, `get_folder_size` returns `Err(NpkillError::SizeTimeout)` and drops its `done_rx`. The spawned walker tasks hold `Arc<AtomicU64>`, `Arc<AtomicUsize>`, and `Arc<Mutex<Option<oneshot::Sender>>>` clones — none of these are cancellation-aware. The walkers continue traversing the (huge) tree until they finish naturally, then the last walker calls `tx.send(())`, gets `Err` (rx dropped), and exits.

**Impact**:
- Caller treats the operation as complete; tasks keep consuming CPU and FS I/O for an unbounded duration.
- Memory: per-task stack + `tokio::fs::read_dir` handle + path Vecs. Bounded by tree breadth, not depth (walkers don't yield-await on each other), but still potentially large.
- No correctness bug (results are discarded), but a misuse-resistant API should free resources on timeout.

**Recommendations** (any of the following; pick one):
1. **CancellationToken (recommended)**: add `cancel: CancellationToken` to the per-walk state, check `is_cancelled()` once per `next_entry`, cancel from the timeout branch. ~10 lines. Matches scanner.rs prior art.
2. **Drop guard**: a `Drop` impl on a state struct that, when dropped by the caller, flips an `AtomicBool` checked by walkers. Slightly more elegant but same concept.
3. **JoinSet**: collect spawned handles into a `JoinSet`; abort on timeout. Trickier because walkers recursively spawn — would need to share the JoinSet via Arc<Mutex<...>>, adding contention.

Option 1 is the cleanest and aligns with Phase 02's existing cancel-token pattern. Suggest filing as Phase 03.5 polish, not blocking Phase 05 — current behavior matches npkill (which has the same leak: oneshot resolves but workers keep running until natural completion).

**Why not CRITICAL**: in practice, a 60s tree walk on real `node_modules` directories completes well before the timeout. Leak is theoretical for the typical case but real for pathological trees.

## Findings

### MEDIUM

See **M1** above (post-timeout task leak).

### LOW

**L1. `empty_dir_size_is_dir_overhead_only` test name vs assertion mismatch.**
`tests/size_smoke.rs:26`. The test name says "size_is_dir_overhead_only" (implying 4096), but the assertion is `total == 0`. The body's comment clarifies why (root itself is not counted, only children), and the behavior matches npkill semantics — but the test name reads as a contradiction. Rename to `empty_dir_size_is_zero` and keep the comment.

**L2. `decrement` silently swallows poisoned mutex.**
`src/core/size.rs:114-121`. `if let Ok(mut g) = done.lock()` skips the send when a previous walker panicked. The walker tasks have no panic surfaces *currently* (all I/O is matched with `Err(_) => continue/return`), so poisoning is unreachable in practice. But if a future change introduces an unwrap (e.g., a metadata transform), a poisoned mutex would mean `done.send()` is never called, and `get_folder_size` would hang the full 60s before returning `SizeTimeout`. Prefer `if let Some(tx) = done.lock().expect("size walker mutex poisoned").take()` — loud failure beats silent hang.

**L3. Empty-dir returns 0 instead of 4096.**
The plan says "Directories themselves count 4096 bytes". The implementation counts 4096 **per subdir** (size.rs:97) but **not for the root** — only the root's children. So `get_folder_size("/empty-dir") = 0`, but `get_folder_size("/parent-of-empty-dir") = 4096`. This matches npkill's `runGetFolderSize` (it walks **into** the root and only adds 4096 for each descendant dir), so semantically consistent. Worth a doc comment on `get_folder_size` explicitly stating "root not included; first level of children adds 4096 each" so a future caller doesn't assume `get_folder_size(node_modules) ≥ 4096`.

**L4. `let _ = f.sync_all()` in test write helper.**
`tests/size_smoke.rs:22`. `sync_all().expect("sync")` blocks on fsync — slow on macOS. Tests pass in 30ms total, so non-issue, but `f.flush()` would suffice for size-correctness because the tempdir is read in the same process.

### NIT

**N1. `Arc<Mutex<Option<oneshot::Sender>>>` could be a `Notify`.**
`tokio::sync::Notify::notify_one()` is idempotent and lock-free, eliminating the Mutex. But the current shape is conventional for "wait for last child" termination, easy to reason about, and identical to what every "scoped task pool" tutorial shows. Keep as-is.

**N2. `spawn_walk` is a 3-line wrapper around `tokio::spawn(walk(...))`.**
Adds indirection without saving callers anything (4 args either way). Could inline at the two call sites. Stylistic only.

**N3. Public re-export missing from `core/mod.rs`.**
`mod.rs:13` declares `pub mod size;` but does not re-export `get_folder_size` or `SIZE_TIMEOUT`. Test code at `tests/size_smoke.rs:14` reaches in via `nodemoduleskiller::core::size`. The plan's "Files to modify" said `pub use size::get_folder_size;`. Either intentional (avoid name pollution at the core:: top level) or oversight. Add `pub use size::get_folder_size;` if you want `nodemoduleskiller::core::get_folder_size` as the canonical import path. Low priority.

**N4. `saturating_add` for u64 totals.**
`size.rs:97, 100`. `u64::MAX` bytes is ~16 exabytes — unreachable on any real filesystem. `saturating_add` is fine; `wrapping_add` would also be fine; pure `+` would panic only on overflow which is unreachable. Belt-and-suspenders — keep.

**N5. `SIZE_TIMEOUT` is pub but `walk` and `spawn_walk` are private.**
Correct visibility split. ✅

**N6. Test file imports `nodemoduleskiller::core::size` but no `pub use`.**
See N3. Currently works because `core` is `pub mod` and `size` is `pub mod`.

## Documented deviation — standalone async fn vs scanner pool

**Accepted.** The rationale at `size.rs:6-10` is sound:

1. **Lifecycle mismatch**: scanner workers exit when `pending == 0`. After Phase 02's scan completes, calling `get_folder_size` for each result would need to re-spawn the pool. Either size jobs run **during** scan (couples concerns) or the pool needs a "keep-alive" mode (more complexity than the current standalone fn).
2. **Concurrency profile**: scanner uses round-robin dispatch optimized for "many independent trees from one root". Size calc fans out from one tree at a time; round-robin adds latency for no benefit.
3. **Tokio runtime parallelism**: `tokio::spawn` already round-robins onto the worker threads of the multi-thread runtime. We don't need an explicit pool to get parallelism — we get it for free.

Phase 02 review (line 131) recommended "Phase 03 keep size jobs in the same worker pool". The current decision **diverges** from that advice. After reviewing the code, I agree with the **divergence**:

- The Phase 02 reviewer's recommendation rested on "avoid duplicating cancellation/completion plumbing". But the size completion model (refcounted children waiting for a oneshot) is **different** from the scan completion model (every queued job decrements once and the *last* one cancels). They share *shape* but not *semantics*; merging would have required parameterizing the worker over a `CompletionSignal` trait — more code than two clear independent implementations.
- The standalone fn is testable in isolation (size_smoke.rs runs without ever instantiating the scanner). That's a meaningful win for the Phase 05 (delete) integration: delete needs to call `get_folder_size` to compute "freed bytes" without booting a scanner.

**However**: if benchmarks later show that per-tree size scans contend (e.g., 1000s of small trees each spawning their own walker tree), reconsider a shared bounded semaphore. Not needed now.

## Phase 05 (delete) readiness

- **No blockers.** Delete needs `(path, size) -> Future<delete_result>`. Phase 03's `get_folder_size(path)` slots in cleanly as a pre-delete probe for "freed bytes" reporting.
- **API surface**: delete should accept already-computed sizes (avoid double-walk) — i.e. Phase 05 caller does `let size = get_folder_size(p).await?; delete(p).await?; report_freed(size);`. Phase 03 supports this.
- **Cancellation propagation**: Phase 05's delete loop should not be interruptible by a `get_folder_size` hang. After M1 is fixed, this concern goes away. Until then, callers should `tokio::time::timeout` the delete loop's size-prefetch separately, or batch-prefetch sizes from Phase 02 scan results.

## Idiomatic Rust checklist

- [x] No `unwrap()` / `expect()` outside `#[cfg(test)]`.
- [x] No `unsafe`.
- [x] Doc comments on every `pub` item (`get_folder_size`, `SIZE_TIMEOUT`).
- [x] Module-level `//!` doc explains the algorithm and invariants.
- [x] `snake_case` for fns, `PascalCase` for types, `SCREAMING_SNAKE_CASE` for const.
- [x] Error returns use typed `NpkillError`; no `anyhow` in library code.
- [x] Atomics + Arc for shared state; no `RwLock`/`Mutex` over awaits.
- [x] `cfg(unix)` / `cfg(not(unix))` split rather than runtime branching.
- [x] `let-else` pattern via `let Ok(t) = ... else continue` would be slightly cleaner but `match` style chosen is also valid. NIT.
- [x] `if let && let` chain at size.rs:115-117 is the new Rust 2024 let-chains syntax — idiomatic and clean.

## Acceptance criteria NOT met

None.

## Recommended actions

1. **(Optional, recommended)** Fix M1 — wire a CancellationToken through the walk so timeout cancels in-flight walkers. ~10 lines. Phase 03.5.
2. **(Optional)** L2 — replace `if let Ok(mut g) = done.lock()` with `.expect(...)`. ~1 line.
3. **(Optional)** L1 — rename `empty_dir_size_is_dir_overhead_only` to `empty_dir_size_is_zero`.
4. **(Optional)** L3 — add a sentence to `get_folder_size` doc comment clarifying "root dir not counted; first-level children +4096 each".
5. **(Optional)** N3 — `pub use size::get_folder_size;` in `core/mod.rs` if you want it at `core::` top level.

None of the above blocks Phase 05.

## Unresolved questions

- Should Phase 03.5 add CancellationToken plumbing now, or defer to Phase 06 (TUI) when caller-side cancellation becomes user-facing? (Recommendation: do it now; the standalone-fn pattern won't be touched again.)
- Should `get_folder_size` be exposed at `core::` top level or stay nested under `core::size::`? (Plan said top level; implementation kept it nested. Confirm before Phase 05 starts so the call-site path is stable.)
