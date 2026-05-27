# Phase 03 — Folder size calculation (refcounted async parallel sum)

## Context

Port `runGetFolderSize` + `runGetFolderSizeChild` from `src/core/services/files/files.worker.ts`. Reuses Phase 02's worker pool abstraction — extends `Job` enum.

Invariants (locked):
- Unix size = `blocks × 512` (true on-disk)
- Windows size = `metadata.len()` (logical)
- Directories themselves count 4096 bytes
- Symlinks ALWAYS excluded (no follow, no count)
- Permission errors silently skipped

## Priority

P0 — UI needs size per-result.

## Status

completed (2026-05-27)

## Requirements

- API: `pub fn get_folder_size(path: PathBuf) -> impl Future<Output = Result<u64, NpkillError>>`
- 60s timeout per top-level call (mirror source's `SIZE_TIMEOUT_MS = 60000`)
- Parallelism reuses Phase 02 worker pool (extend `Job` enum)
- Returns `u64` bytes

## Architecture

```rust
// src/core/size.rs

pub async fn get_folder_size(scanner: &ScannerHandle, path: PathBuf) -> Result<u64, NpkillError> {
    let total = Arc::new(AtomicU64::new(0));
    let pending = Arc::new(AtomicUsize::new(1));
    let (done_tx, done_rx) = oneshot::channel();

    let collector = SizeCollector { total: total.clone(), pending: pending.clone(), done: Arc::new(Mutex::new(Some(done_tx))) };
    scanner.dispatch(Job::SizeChild(path, collector)).await;

    match tokio::time::timeout(Duration::from_secs(60), done_rx).await {
        Ok(Ok(())) => Ok(total.load(SeqCst)),
        _ => Err(NpkillError::SizeTimeout),
    }
}
```

### Extended Job enum (in scanner.rs)

```rust
enum Job {
    Explore(PathBuf),
    SizeChild(PathBuf, SizeCollector),
}

#[derive(Clone)]
struct SizeCollector {
    total: Arc<AtomicU64>,
    pending: Arc<AtomicUsize>,
    done: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}
```

### Per-dir size logic

```rust
async fn size_dir(path: PathBuf, c: SizeCollector, w: WorkerHandles) {
    let mut rd = match tokio::fs::read_dir(&path).await {
        Ok(r) => r,
        Err(_) => { decrement_pending(&c); return; }
    };
    let mut current = 0u64;
    let mut subdirs: Vec<PathBuf> = Vec::new();

    while let Ok(Some(entry)) = rd.next_entry().await {
        let ft = match entry.file_type().await { Ok(t) => t, Err(_) => continue };
        if ft.is_symlink() { continue; }
        if ft.is_dir() {
            current += 4096;
            subdirs.push(entry.path());
        } else {
            if let Ok(meta) = entry.metadata().await {
                current += real_size(&meta);
            }
        }
    }
    c.total.fetch_add(current, SeqCst);
    c.pending.fetch_add(subdirs.len(), SeqCst);
    for d in subdirs {
        let _ = w.tx_dispatch.send(Job::SizeChild(d, c.clone())).await;
    }
    decrement_pending(&c);
}

fn decrement_pending(c: &SizeCollector) {
    if c.pending.fetch_sub(1, SeqCst) == 1 {
        if let Some(tx) = c.done.lock().unwrap().take() { let _ = tx.send(()); }
    }
}

#[cfg(unix)]
fn real_size(m: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    m.blocks().saturating_mul(512)
}
#[cfg(not(unix))]
fn real_size(m: &std::fs::Metadata) -> u64 { m.len() }
```

## Files to create

- `src/core/size.rs` (~130 LoC)

## Files to modify

- `src/core/scanner.rs` — extend `Job` enum + dispatcher routing + `Worker` handling of `SizeChild`
- `src/core/mod.rs` — `pub mod size; pub use size::get_folder_size;`

## Implementation steps

1. Extend `Job` enum in scanner.rs with `SizeChild(PathBuf, SizeCollector)`.
2. Update worker loop `match` to call `size_dir` for `SizeChild`.
3. Implement `size_dir` with the algorithm above.
4. Implement `get_folder_size` facade with 60 s timeout.
5. Add `cfg(unix)` block for `real_size` using `MetadataExt::blocks()`.
6. Test in `tests/size_smoke.rs`:
   - tempdir tree with known total bytes
   - assert size returned is within 8 KB of expected (allows for 4096-byte dir overhead)

## Todo

- [ ] Extend `Job` enum
- [ ] `SizeCollector` struct
- [ ] `size_dir` async function with symlink skip + 4096 dir overhead
- [ ] `real_size` with cfg(unix)/cfg(not(unix)) split
- [ ] `decrement_pending` w/ oneshot done signal
- [ ] `get_folder_size` facade w/ 60 s timeout
- [ ] integration test with known-size tempdir

## Success criteria

- Test passes: `get_folder_size(small_known_tree)` returns within 8 KB of expected
- 60 s timeout actually fires (test with intentionally slow mock)
- No panic on permission-denied subtree

## Risks

| Risk | Mitigation |
|---|---|
| Refcount race: pending could hit 0 before all children spawned | increment `pending` BEFORE sending child job, decrement only at end of own work |
| Oneshot dropped before fire causing Err in `done_rx` | wrap `done` in `Mutex<Option<Sender>>` and `.take()` exactly once |
| `MetadataExt::blocks()` unavailable on some Unix variants | gate on `cfg(any(target_os="linux", target_os="macos", target_os="freebsd"))` and fall back to `len()` |
| Symlink to a huge target counted | already skipped at `ft.is_symlink()` |

## Security considerations

None beyond Phase 02.

## Next steps

Phase 07 calls `get_folder_size` for each result from Phase 02 to populate `FolderResult.size_bytes`.
