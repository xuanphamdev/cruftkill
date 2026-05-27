# Phase 02 — Core scanner (hand-rolled tokio worker pool)

## Context

Port npkill's parallel directory walker. Decision **C1 + C2 (locked)**: hand-rolled worker pool using tokio tasks + mpsc channels, mirroring `src/core/services/files/files.worker.service.ts` + `files.worker.ts`.

Behavioral invariants (from `research/xia-recon-and-analysis.md` §Behavioral invariants):
- targets = exact basename match
- exclude = substring match
- walker does NOT descend into matched targets
- GLOBAL_IGNORE excludes recursion but allows target match
- symlinks never followed
- permission errors silently skipped

## Priority

P0 — required for TUI and size phase.

## Status

completed (2026-05-27)

## Requirements

- async function returning `(impl Stream<Item = ScanFoundFolder>, CancellationToken)`
- N worker tasks (default `min(num_cpus, 8)`)
- per-worker bounded concurrent dir reads (MAX_PROCS=100 from source — keep)
- round-robin job dispatch
- cancellation propagates within ~50 ms
- emit results live (no batch wait)

## Architecture

```rust
// src/core/scanner.rs

pub struct ScannerHandle {
    pub results: mpsc::Receiver<ScanFoundFolder>,
    pub cancel: CancellationToken,
    pub stats: Arc<ScanStats>,
}

pub struct ScanStats {
    pub pending: AtomicUsize,    // jobs in flight
    pub completed: AtomicU64,    // dirs explored
    pub found: AtomicU64,        // matches emitted
}

pub fn start_scan(root: PathBuf, opts: ScanOptions) -> ScannerHandle { ... }

// internal:
enum Job { Explore(PathBuf) }    // (size jobs handled in phase 03)

struct Worker {
    id: usize,
    rx: mpsc::Receiver<Job>,
    tx_dispatch: mpsc::Sender<Job>,         // back-edge: enqueue new explore jobs
    tx_results: mpsc::Sender<ScanFoundFolder>,
    cancel: CancellationToken,
    stats: Arc<ScanStats>,
    cfg: Arc<ScanConfig>,                   // targets, exclude, perform_risk
}
```

### Dispatcher (round-robin)

```rust
async fn dispatch(mut rx: mpsc::Receiver<Job>, worker_txs: Vec<mpsc::Sender<Job>>) {
    let mut idx = 0usize;
    while let Some(job) = rx.recv().await {
        if worker_txs[idx].send(job).await.is_err() { break; }
        idx = (idx + 1) % worker_txs.len();
    }
}
```

### Worker loop

```rust
async fn worker_loop(mut w: Worker) {
    let sem = Arc::new(Semaphore::new(MAX_PROCS));  // 100 concurrent dir reads
    loop {
        tokio::select! {
            _ = w.cancel.cancelled() => break,
            job = w.rx.recv() => match job {
                None => break,
                Some(Job::Explore(path)) => {
                    let permit = sem.clone().acquire_owned().await.unwrap();
                    let w2 = w.clone_handles();
                    tokio::spawn(async move {
                        let _p = permit;
                        explore_dir(path, w2).await;
                    });
                }
            }
        }
    }
}
```

### Per-dir explore

```rust
async fn explore_dir(path: PathBuf, w: WorkerHandles) {
    w.stats.pending.fetch_add(1, SeqCst);
    let mut rd = match tokio::fs::read_dir(&path).await {
        Ok(r) => r, Err(_) => { w.stats.pending.fetch_sub(1, SeqCst); return }
    };
    while let Ok(Some(entry)) = rd.next_entry().await {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let ft = match entry.file_type().await { Ok(t) => t, Err(_) => continue };
        if ft.is_symlink() || !ft.is_dir() { continue; }

        let is_target = w.cfg.targets.iter().any(|t| t == name_str.as_ref());
        if !is_target && GLOBAL_IGNORE.contains(name_str.as_ref()) { continue; }

        let subpath = path.join(&name);
        if w.cfg.exclude.iter().any(|ex| subpath.to_string_lossy().contains(ex)) {
            continue;
        }
        if is_target {
            let risk = if w.cfg.perform_risk { Some(risk::analyze(&subpath)) } else { None };
            let _ = w.tx_results.send(ScanFoundFolder { path: subpath, risk_analysis: risk }).await;
            w.stats.found.fetch_add(1, SeqCst);
            // DO NOT recurse
        } else {
            let _ = w.tx_dispatch.send(Job::Explore(subpath)).await;
        }
    }
    w.stats.completed.fetch_add(1, SeqCst);
    w.stats.pending.fetch_sub(1, SeqCst);
}
```

### Completion detection

Track `pending` via `AtomicUsize`. When it hits 0 AND dispatcher channel is empty, close `tx_results` (drop it) so consumer stream ends. Use a small watchdog tokio task polling every 50 ms.

## Files to create

- `src/core/scanner.rs` (~200 LoC)
- `src/core/ignore.rs` — port `GLOBAL_IGNORE` set verbatim from `src/core/constants/global-ignored.constants.ts`

## Files to modify

- `src/core/mod.rs` add `pub mod scanner; pub mod ignore;`
- `src/lib.rs` re-export `pub use core::scanner::start_scan;`

## Implementation steps

1. Port GLOBAL_IGNORE constants → `phf::Set` or `OnceLock<HashSet<&'static str>>`.
2. Implement `ScanConfig`, `ScanStats`, `ScannerHandle`.
3. Implement dispatcher task with round-robin.
4. Implement worker loop with `Semaphore::new(MAX_PROCS)`.
5. Implement `explore_dir` with the exact match/skip rules above.
6. Wire completion watchdog.
7. Unit test in `tests/scanner_smoke.rs`:
   - tempfile tree: `root/a/node_modules`, `root/b/c/node_modules`, `root/b/c/node_modules/foo/node_modules` (nested target — should NOT appear), `root/.git/...` (ignored).
   - assert: exactly 2 paths emitted; `.git` not descended; nested node_modules not emitted.

## Todo

- [ ] `ignore.rs` with GLOBAL_IGNORE
- [ ] `scanner.rs` types (ScanConfig, ScanStats, ScannerHandle)
- [ ] dispatcher round-robin
- [ ] worker loop + semaphore
- [ ] `explore_dir` with full match/skip rules
- [ ] completion watchdog
- [ ] cancellation respected within 50 ms
- [ ] integration test with nested tree

## Success criteria

- Unit test passes on temp tree
- Scanning `~/Projects` (a directory with 5–10 node_modules) completes in <2× the equivalent `find . -name node_modules -prune -print` wall time
- `Ctrl-C` (test via `cancel.cancel()`) stops within 100 ms

## Risks

| Risk | Mitigation |
|---|---|
| Channel backpressure deadlock if results channel full | use `mpsc::channel(1024)` not `bounded(1)` |
| Completion never detected because watchdog races | use snapshot of `pending` then re-check + check channel `len()` |
| Per-worker semaphore starving other workers | each worker has its OWN semaphore — sized independently |
| Tokio runtime mismatched (single vs multi thread) | document `#[tokio::main(flavor = "multi_thread")]` requirement |

## Security considerations

- Never follow symlinks — already enforced via `ft.is_symlink()` early exit.
- Never canonicalize during walk (perf + symlink escape).

## Next steps

Phase 03 uses the same worker abstraction for `get_folder_size`. Phase 06 consumes the stream for the TUI.
