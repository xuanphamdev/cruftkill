//! Parallel recursive directory scanner.
//!
//! Mirrors npkill's `FileWorkerService` + `FileWalker` (TypeScript worker
//! threads) using tokio tasks + mpsc channels. The shape:
//!
//! - one **dispatcher** task that pulls from a single inbound job queue and
//!   distributes jobs to workers in **round-robin** order
//! - N **worker** tasks; each owns a private `mpsc::Receiver<Job>` and
//!   processes jobs serially with async I/O
//! - workers re-enqueue child directories via `tx_dispatch`
//! - completion is signalled by a shared `pending: AtomicUsize`: when it
//!   reaches 0, the [`CancellationToken`] is cancelled and all tasks exit
//!
//! Deviation from npkill: npkill maintains 100 concurrent dir reads PER
//! worker via Node's event loop. We rely on tokio's multi-thread runtime
//! plus 8 workers — empirically sufficient for interactive use. If
//! benchmarks later show contention, switch to a per-worker `Semaphore`
//! + `JoinSet` spawn pattern.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::core::ignore::global_ignore;
use crate::core::types::{RiskAnalysis, ScanFoundFolder, ScanOptions};

/// Maximum number of worker tasks the scanner will spawn.
pub const MAX_WORKERS: usize = 8;

/// Live counters exposed to the UI / caller.
#[derive(Debug, Default)]
pub struct ScanStats {
    /// Directories whose contents have been read.
    pub completed: AtomicU64,
    /// Target folders emitted to the result channel.
    pub found: AtomicU64,
}

/// Handle returned by [`start_scan`].
///
/// Drop the handle (or call `cancel.cancel()`) to terminate the scan early.
/// The `Drop` impl below makes "forget to call cancel" a non-issue: dropping
/// the handle always cancels.
pub struct ScannerHandle {
    /// Live stream of matched folders. Closes when the scan completes or is cancelled.
    pub results: mpsc::Receiver<ScanFoundFolder>,
    /// Token to cancel the scan. The scanner itself also cancels this when work is done.
    pub cancel: CancellationToken,
    /// Shared, atomic progress counters.
    pub stats: Arc<ScanStats>,
}

impl Drop for ScannerHandle {
    fn drop(&mut self) {
        // Idempotent: if the scan already finished naturally, the token is
        // already cancelled and this is a no-op.
        self.cancel.cancel();
    }
}

/// Internal: scan configuration shared between dispatcher and workers.
struct ScanConfig {
    targets: Vec<String>,
    exclude: Vec<String>,
    perform_risk: bool,
}

impl From<ScanOptions> for ScanConfig {
    fn from(o: ScanOptions) -> Self {
        Self { targets: o.targets, exclude: o.exclude, perform_risk: o.perform_risk_analysis }
    }
}

/// Internal: unit of work passed between dispatcher and workers.
enum Job {
    /// Read `path`, emit any target children, and enqueue non-target children.
    Explore(PathBuf),
}

/// Internal: clones of all the handles a worker needs to do its job.
#[derive(Clone)]
struct WorkerHandles {
    tx_dispatch: mpsc::Sender<Job>,
    tx_results: mpsc::Sender<ScanFoundFolder>,
    cancel: CancellationToken,
    stats: Arc<ScanStats>,
    pending: Arc<AtomicUsize>,
    cfg: Arc<ScanConfig>,
}

/// Start a parallel scan rooted at `root`.
///
/// Returns immediately; results stream via the receiver. The function does
/// NOT canonicalize `root` — pass an absolute path if you need one.
pub fn start_scan(root: PathBuf, opts: ScanOptions) -> ScannerHandle {
    let cancel = CancellationToken::new();
    let stats = Arc::new(ScanStats::default());
    let cfg = Arc::new(ScanConfig::from(opts));
    let pending = Arc::new(AtomicUsize::new(0));

    let (result_tx, result_rx) = mpsc::channel::<ScanFoundFolder>(1024);
    let (job_tx, job_rx) = mpsc::channel::<Job>(1024);

    let n = optimal_workers();
    let mut worker_inputs = Vec::with_capacity(n);
    for _ in 0..n {
        let (wtx, wrx) = mpsc::channel::<Job>(256);
        worker_inputs.push(wtx);
        let h = WorkerHandles {
            tx_dispatch: job_tx.clone(),
            tx_results: result_tx.clone(),
            cancel: cancel.clone(),
            stats: stats.clone(),
            pending: pending.clone(),
            cfg: cfg.clone(),
        };
        tokio::spawn(worker_loop(wrx, h));
    }
    tokio::spawn(dispatcher(job_rx, worker_inputs, cancel.clone()));

    // Seed the root exploration. Increment-before-send keeps pending counts
    // consistent: by the time `send` completes, the job is owned by a task.
    pending.fetch_add(1, Ordering::SeqCst);
    if job_tx.try_send(Job::Explore(root)).is_err() {
        // Practically dead: the channel was just created with capacity 1024
        // and the dispatcher cannot have exited before its first `recv`.
        // Guard against spawn-order regressions.
        debug_assert!(false, "scanner: seed job rejected at construction");
        if pending.fetch_sub(1, Ordering::SeqCst) == 1 {
            cancel.cancel();
        }
    }

    // Drop the parent's sender clones so the only senders are inside tasks.
    // When all tasks exit (on cancel or natural completion), the channels close.
    drop(result_tx);
    drop(job_tx);

    ScannerHandle { results: result_rx, cancel, stats }
}

fn optimal_workers() -> usize {
    num_cpus::get().clamp(1, MAX_WORKERS)
}

async fn dispatcher(
    mut rx: mpsc::Receiver<Job>,
    outs: Vec<mpsc::Sender<Job>>,
    cancel: CancellationToken,
) {
    let mut idx = 0usize;
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            job = rx.recv() => match job {
                None => break,
                Some(job) => {
                    let target = &outs[idx];
                    idx = (idx + 1) % outs.len();
                    // If the chosen worker has exited, the whole scan is shutting down.
                    if target.send(job).await.is_err() {
                        break;
                    }
                }
            }
        }
    }
    // dropping `outs` here closes worker inboxes; workers will exit on `None`
}

async fn worker_loop(mut rx: mpsc::Receiver<Job>, h: WorkerHandles) {
    loop {
        tokio::select! {
            biased;
            _ = h.cancel.cancelled() => break,
            job = rx.recv() => match job {
                None => break,
                Some(Job::Explore(path)) => explore_dir(path, &h).await,
            }
        }
    }
}

async fn explore_dir(path: PathBuf, h: &WorkerHandles) {
    let mut rd = match tokio::fs::read_dir(&path).await {
        Ok(r) => r,
        Err(_) => {
            decrement_pending(h);
            return;
        }
    };

    loop {
        if h.cancel.is_cancelled() {
            break;
        }
        let entry = match rd.next_entry().await {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(_) => continue,
        };

        let name = entry.file_name();
        // npkill matches on the lossy form too — preserve that semantics
        // exactly so non-UTF-8 directory names follow the same path.
        let name_str = name.to_string_lossy();

        let ft = match entry.file_type().await {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ft.is_symlink() || !ft.is_dir() {
            continue;
        }

        let is_target = h.cfg.targets.iter().any(|t| t.as_str() == name_str.as_ref());
        let is_global_ignored = global_ignore().contains(name_str.as_ref());
        if is_global_ignored && !is_target {
            continue;
        }

        let subpath = path.join(&name);
        let subpath_str = subpath.to_string_lossy();
        if h.cfg.exclude.iter().any(|ex| subpath_str.contains(ex.as_str())) {
            continue;
        }

        if is_target {
            let risk = h.cfg.perform_risk.then(RiskAnalysis::safe);
            // Receiver dropped means caller bailed; drop the result silently
            // and let the cancel branch above end the loop.
            tokio::select! {
                biased;
                _ = h.cancel.cancelled() => break,
                _ = h.tx_results.send(ScanFoundFolder::new(subpath, risk)) => {}
            }
            h.stats.found.fetch_add(1, Ordering::SeqCst);
            // Invariant: do NOT recurse into a matched target.
        } else {
            // Increment-before-send so completion-check is race-free.
            // The send is wrapped in `select!` against cancel to avoid a
            // theoretical fan-out deadlock when dispatcher + all worker
            // inboxes are saturated (see Phase 02 review M1).
            h.pending.fetch_add(1, Ordering::SeqCst);
            let send_result = tokio::select! {
                biased;
                _ = h.cancel.cancelled() => Err(()),
                r = h.tx_dispatch.send(Job::Explore(subpath)) => r.map_err(|_| ()),
            };
            if send_result.is_err() {
                // Cancelled or dispatcher gone — roll back so completion math holds.
                if h.pending.fetch_sub(1, Ordering::SeqCst) == 1 {
                    h.cancel.cancel();
                }
            }
        }
    }

    h.stats.completed.fetch_add(1, Ordering::SeqCst);
    decrement_pending(h);
}

fn decrement_pending(h: &WorkerHandles) {
    if h.pending.fetch_sub(1, Ordering::SeqCst) == 1 {
        h.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimal_workers_in_range() {
        let n = optimal_workers();
        assert!((1..=MAX_WORKERS).contains(&n));
    }

    #[test]
    fn scan_config_from_options_preserves_fields() {
        let opts = ScanOptions {
            targets: vec!["node_modules".into()],
            exclude: vec!["skip".into()],
            sort_by: None,
            perform_risk_analysis: false,
        };
        let cfg = ScanConfig::from(opts);
        assert_eq!(cfg.targets, vec!["node_modules"]);
        assert_eq!(cfg.exclude, vec!["skip"]);
        assert!(!cfg.perform_risk);
    }

    #[test]
    fn scan_stats_starts_zeroed() {
        let s = ScanStats::default();
        assert_eq!(s.completed.load(Ordering::SeqCst), 0);
        assert_eq!(s.found.load(Ordering::SeqCst), 0);
    }
}
