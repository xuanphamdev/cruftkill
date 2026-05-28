//! Parallel recursive directory scanner.
//!
//! Mirrors npkill's `FileWorkerService` + `FileWalker` (TypeScript worker
//! threads) using tokio tasks + mpsc channels. The shape:
//!
//! - one **dispatcher** task that pulls from a single inbound job queue and
//!   distributes jobs to workers in **round-robin** order
//! - N **worker** tasks; each owns a private `mpsc::UnboundedReceiver<Job>`
//!   and processes jobs serially with async I/O
//! - workers re-enqueue child directories via `tx_dispatch`
//! - completion is signalled by a shared `pending: AtomicUsize`: when it
//!   reaches 0, the [`CancellationToken`] is cancelled and all tasks exit
//!
//! **Why unbounded dispatch channels?** A bounded job queue + bounded worker
//! inboxes can deadlock under realistic load: when every worker is mid-
//! `explore_dir` and trying to push a child job, and the dispatch queue +
//! every worker inbox is full, every worker's `send().await` is blocked
//! waiting on space the dispatcher can't free (because the dispatcher is
//! also blocked, sending into a full worker inbox). Phase 02 review M1
//! flagged this as theoretical; it shows up in practice on deep `node_modules`
//! trees. Unbounded sends never block; memory is implicitly bounded by the
//! total pending-job count, which is bounded by the on-disk tree size.
//! The result channel stays bounded so a slow UI exerts backpressure on
//! `tx_results.send` (which IS wrapped in `select!` against cancel).
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
use crate::core::risk;
use crate::core::types::{ScanFoundFolder, ScanOptions};

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
    home: Option<PathBuf>,
}

impl From<ScanOptions> for ScanConfig {
    fn from(o: ScanOptions) -> Self {
        let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).ok();
        Self {
            targets: o.targets,
            exclude: o.exclude,
            perform_risk: o.perform_risk_analysis,
            home: home.map(PathBuf::from),
        }
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
    tx_dispatch: mpsc::UnboundedSender<Job>,
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

    // Bounded result channel — UI exerts backpressure on workers via this.
    let (result_tx, result_rx) = mpsc::channel::<ScanFoundFolder>(1024);
    // UNBOUNDED job dispatch — see module docs for the deadlock rationale.
    let (job_tx, job_rx) = mpsc::unbounded_channel::<Job>();

    let n = optimal_workers();
    let mut worker_inputs = Vec::with_capacity(n);
    for _ in 0..n {
        let (wtx, wrx) = mpsc::unbounded_channel::<Job>();
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
    // consistent: unbounded send is synchronous so this is fully race-free.
    pending.fetch_add(1, Ordering::SeqCst);
    if job_tx.send(Job::Explore(root)).is_err() {
        // Practically dead: dispatcher cannot have exited before its first
        // `recv`. Guard against spawn-order regressions.
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
    mut rx: mpsc::UnboundedReceiver<Job>,
    outs: Vec<mpsc::UnboundedSender<Job>>,
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
                    // Unbounded send is synchronous and only fails if the
                    // worker has dropped its receiver (i.e. it has exited
                    // already because cancel fired).
                    if target.send(job).is_err() {
                        break;
                    }
                }
            }
        }
    }
    // dropping `outs` here closes worker inboxes; workers will exit on `None`
}

async fn worker_loop(mut rx: mpsc::UnboundedReceiver<Job>, h: WorkerHandles) {
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
            let risk = h
                .cfg
                .perform_risk
                .then(|| risk::analyze_with_home(&subpath, h.cfg.home.as_deref()));
            // Result channel is bounded — wrap in cancel select so we can
            // bail out promptly if the consumer dropped or the scan was
            // cancelled mid-send.
            tokio::select! {
                biased;
                _ = h.cancel.cancelled() => break,
                _ = h.tx_results.send(ScanFoundFolder::new(subpath, risk)) => {}
            }
            h.stats.found.fetch_add(1, Ordering::SeqCst);
            // Invariant: do NOT recurse into a matched target.
        } else {
            // Increment-before-send keeps the completion check race-free.
            // Unbounded dispatch never blocks, so no select needed.
            h.pending.fetch_add(1, Ordering::SeqCst);
            if h.tx_dispatch.send(Job::Explore(subpath)).is_err() {
                // Dispatcher gone — roll back so completion math holds.
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
