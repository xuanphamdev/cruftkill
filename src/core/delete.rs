//! Safe directory deletion.
//!
//! Port of npkill's `delete$` / `deleteDir` (`src/core/npkill.ts` +
//! `src/core/services/files/{unix,windows}-files.service.ts`) with two
//! layered guards before any FS mutation happens:
//!
//! 1. **Basename guard** (Phase 04 [`safe_delete::is_safe_to_delete`]):
//!    the basename of `path` must appear in the caller-supplied `targets`.
//!    Catches the obvious "wrong path" mistake.
//! 2. **Containment guard**: both `path` and `scan_root` are canonicalized
//!    (resolves symlinks) and then `canon_path.starts_with(canon_root)` must
//!    hold. Catches symlink-escape attacks where a link inside the scan tree
//!    points to a sensitive system path.
//!
//! Decision C3 (locked): we use `std::fs::remove_dir_all` cross-platform.
//! Rust std does NOT follow symlinks when removing a directory (the
//! symlink itself is removed instead of its target), so the containment
//! guard's symlink resolution is what gives the operation its safety.
//!
//! Dry-run sleeps a short random duration (200–4200 ms) to mimic the feel
//! of a real delete in the TUI — port of npkill's `fakeDeleteDir`.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::core::safe_delete::is_safe_to_delete;
use crate::core::types::DeleteResult;

/// Delete `path` if it is safe to do so.
///
/// Returns a [`DeleteResult`] describing the outcome — never panics, never
/// returns an `Err`. Detailed failure reason lives in
/// [`DeleteResult::error`].
///
/// When `dry_run` is `true`, no FS mutation happens; the function sleeps a
/// short randomized duration and reports success.
pub async fn delete(
    path: &Path,
    scan_root: &Path,
    targets: &[String],
    dry_run: bool,
) -> DeleteResult {
    // Guard 1: basename must be in the configured target list.
    if !is_safe_to_delete(path, targets) {
        return DeleteResult::fail(path, "Path basename is not in the target list");
    }

    // Guard 2: canonicalize and verify containment.
    let canon_path = match std::fs::canonicalize(path) {
        Ok(p) => p,
        Err(e) => return DeleteResult::fail(path, format!("canonicalize failed: {e}")),
    };
    let canon_root = match std::fs::canonicalize(scan_root) {
        Ok(p) => p,
        Err(e) => return DeleteResult::fail(path, format!("scan root canonicalize failed: {e}")),
    };
    if !canon_path.starts_with(&canon_root) {
        return DeleteResult::fail(path, "Path is outside the scan root");
    }

    if dry_run {
        let ms = 200 + (sub_nano_jitter() % 4000);
        tokio::time::sleep(Duration::from_millis(ms)).await;
        return DeleteResult::ok(path);
    }

    // Move the real removal onto the blocking pool — for huge trees this can
    // take seconds and we must not block the async runtime's worker thread.
    let pb: PathBuf = canon_path;
    match tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&pb)).await {
        Ok(Ok(())) => DeleteResult::ok(path),
        Ok(Err(e)) => DeleteResult::fail(path, format!("remove_dir_all failed: {e}")),
        Err(e) => DeleteResult::fail(path, format!("join error: {e}")),
    }
}

/// Cheap pseudo-random source for the dry-run sleep duration.
/// Not cryptographic — just enough variance so the TUI doesn't feel canned.
fn sub_nano_jitter() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos() as u64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_returns_a_value() {
        // Best we can do without controlling time. Just exercise the path.
        let _ = sub_nano_jitter();
    }

    #[tokio::test]
    async fn dry_run_against_nonexistent_path_still_rejected_at_canonicalize() {
        let res = delete(
            Path::new("/no/such/path/node_modules"),
            Path::new("/tmp"),
            &["node_modules".into()],
            true,
        )
        .await;
        assert!(!res.success);
        assert!(res.error.unwrap().to_lowercase().contains("canonicalize"));
    }

    #[tokio::test]
    async fn empty_targets_always_rejects() {
        let res = delete(Path::new("/tmp/foo"), Path::new("/tmp"), &[], false).await;
        assert!(!res.success);
    }
}
