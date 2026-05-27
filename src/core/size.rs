//! Recursive folder size calculation.
//!
//! Mirrors npkill's `runGetFolderSize` + `runGetFolderSizeChild` algorithm
//! (`src/core/services/files/files.worker.ts`) but as a standalone async
//! function rather than an extension of the scanner pool. Rationale:
//! - the scanner pool is one-shot (workers exit when `pending` reaches 0),
//!   so it cannot service `get_folder_size` calls fired AFTER the scan ends
//! - size calc has a different concurrency profile (one tree at a time, fan
//!   out from a known root) that does not benefit from round-robin dispatch
//! - tokio's runtime already parallelizes spawned tasks; no extra pool needed
//!
//! Invariants preserved from npkill:
//! - directories themselves count 4096 bytes (inode block estimate)
//! - symlinks NEVER followed (no count, no descent)
//! - Unix uses `blocks * 512` (true on-disk size); other OS uses `metadata.len()`
//! - permission errors silently produce 0 for the affected subtree
//! - 60-second top-level timeout (per `SIZE_TIMEOUT_MS = 60000`)
//!
//! On timeout (or caller-side cancel), all in-flight walker tasks observe a
//! shared [`CancellationToken`] and exit at the next `read_dir`/`next_entry`
//! boundary — no unbounded background CPU/I/O after the caller has given up.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::core::error::NpkillError;

/// Per-folder timeout, matching npkill's `SIZE_TIMEOUT_MS`.
pub const SIZE_TIMEOUT: Duration = Duration::from_secs(60);

/// Compute the recursive size of `path` in bytes.
///
/// The root directory itself is NOT counted as 4096 bytes — only its
/// children-that-are-directories contribute the 4096 overhead each.
///
/// On Unix, returns the on-disk size (`blocks * 512` per file). On other
/// platforms, returns the sum of logical file sizes. Directory entries
/// themselves contribute a flat 4096 bytes (inode block estimate).
///
/// Returns `NpkillError::SizeTimeout` if the walk does not complete in
/// [`SIZE_TIMEOUT`]. Returns `Ok(0)` if `path` is missing or unreadable.
pub async fn get_folder_size(path: PathBuf) -> Result<u64, NpkillError> {
    let total = Arc::new(AtomicU64::new(0));
    let pending = Arc::new(AtomicUsize::new(1));
    let (done_tx, done_rx) = oneshot::channel();
    let done = Arc::new(Mutex::new(Some(done_tx)));
    let cancel = CancellationToken::new();

    let ctx = WalkCtx {
        total: total.clone(),
        pending: pending.clone(),
        done: done.clone(),
        cancel: cancel.clone(),
    };
    spawn_walk(path.clone(), ctx);

    let result = match tokio::time::timeout(SIZE_TIMEOUT, done_rx).await {
        Ok(_) => Ok(total.load(Ordering::SeqCst)),
        Err(_) => Err(NpkillError::SizeTimeout(path)),
    };
    // If we timed out, signal walkers to stop. If we completed naturally,
    // cancel is a no-op since no walkers are still running.
    cancel.cancel();
    result
}

/// Bundle of shared state that every walker task needs. Cheap to clone.
#[derive(Clone)]
struct WalkCtx {
    total: Arc<AtomicU64>,
    pending: Arc<AtomicUsize>,
    done: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    cancel: CancellationToken,
}

fn spawn_walk(path: PathBuf, ctx: WalkCtx) {
    tokio::spawn(async move {
        walk(path, ctx).await;
    });
}

async fn walk(path: PathBuf, ctx: WalkCtx) {
    if ctx.cancel.is_cancelled() {
        decrement(&ctx);
        return;
    }

    let mut rd = match tokio::fs::read_dir(&path).await {
        Ok(r) => r,
        Err(_) => {
            decrement(&ctx);
            return;
        }
    };

    let mut current: u64 = 0;
    let mut subdirs: Vec<PathBuf> = Vec::new();

    loop {
        if ctx.cancel.is_cancelled() {
            break;
        }
        let entry = match rd.next_entry().await {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(_) => continue,
        };
        let ft = match entry.file_type().await {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ft.is_symlink() {
            // Never follow, never count: matches npkill semantics.
            continue;
        }
        if ft.is_dir() {
            current = current.saturating_add(4096);
            subdirs.push(entry.path());
        } else if let Ok(meta) = entry.metadata().await {
            current = current.saturating_add(real_size(&meta));
        }
    }

    ctx.total.fetch_add(current, Ordering::SeqCst);
    if !subdirs.is_empty() && !ctx.cancel.is_cancelled() {
        ctx.pending.fetch_add(subdirs.len(), Ordering::SeqCst);
        for sub in subdirs {
            spawn_walk(sub, ctx.clone());
        }
    }
    decrement(&ctx);
}

fn decrement(ctx: &WalkCtx) {
    if ctx.pending.fetch_sub(1, Ordering::SeqCst) == 1
        && let Ok(mut g) = ctx.done.lock()
        && let Some(tx) = g.take()
    {
        let _ = tx.send(());
    }
}

#[cfg(unix)]
fn real_size(m: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    m.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn real_size(m: &std::fs::Metadata) -> u64 {
    m.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_constant_is_60_seconds() {
        assert_eq!(SIZE_TIMEOUT, Duration::from_secs(60));
    }

    #[cfg(unix)]
    #[test]
    fn real_size_uses_blocks_on_unix() {
        // The function is private and pure; confirm it returns a multiple of
        // 512 on Unix per `blocks * 512`. Use this very source file.
        let meta = std::fs::metadata(file!()).unwrap();
        let s = real_size(&meta);
        assert!(s % 512 == 0, "expected multiple of 512, got {s}");
    }
}
