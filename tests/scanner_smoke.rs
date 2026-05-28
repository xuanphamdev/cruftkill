//! Integration tests for the Phase 02 scanner.
//!
//! Behavioral invariants exercised here (from `research/xia-recon-and-analysis.md`):
//! - targets matched by exact basename
//! - exclude matched as substring
//! - walker does NOT descend into matched targets
//! - GLOBAL_IGNORE excluded unless the name is itself a target
//! - symlinks never followed
//! - cancel propagates quickly

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use cruftkill::ScanOptions;
use cruftkill::core::scanner;

fn touch_dir(p: &PathBuf) {
    fs::create_dir_all(p).expect("create dir");
}

fn opts(targets: &[&str], exclude: &[&str]) -> ScanOptions {
    ScanOptions {
        targets: targets.iter().map(|s| (*s).into()).collect(),
        exclude: exclude.iter().map(|s| (*s).into()).collect(),
        sort_by: None,
        perform_risk_analysis: false,
    }
}

async fn drain(rx: &mut tokio::sync::mpsc::Receiver<cruftkill::ScanFoundFolder>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    while let Some(f) = rx.recv().await {
        out.push(f.path);
    }
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn finds_targets_and_skips_nested_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();

    // First-level target
    touch_dir(&root.join("a/node_modules"));
    // Nested target inside another target — should NOT be emitted
    touch_dir(&root.join("a/node_modules/foo/node_modules"));
    // Deeper but reachable via a non-target path
    touch_dir(&root.join("b/c/node_modules"));
    // Decoy non-target file/dir
    touch_dir(&root.join("b/c/src"));

    let mut handle = scanner::start_scan(root.clone(), opts(&["node_modules"], &[]));
    let found: HashSet<PathBuf> = drain(&mut handle.results).await.into_iter().collect();

    assert_eq!(found.len(), 2, "expected 2 results, got {found:?}");
    assert!(found.contains(&root.join("a/node_modules")));
    assert!(found.contains(&root.join("b/c/node_modules")));
    assert!(!found.iter().any(|p| p.ends_with("a/node_modules/foo/node_modules")));
}

#[tokio::test(flavor = "multi_thread")]
async fn does_not_descend_into_global_ignored_unless_target() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();

    // .git is GLOBAL_IGNORE and NOT a target -> must be skipped entirely
    touch_dir(&root.join(".git/objects/node_modules"));
    // legitimate target outside .git
    touch_dir(&root.join("src/node_modules"));

    let mut handle = scanner::start_scan(root.clone(), opts(&["node_modules"], &[]));
    let found = drain(&mut handle.results).await;

    assert_eq!(found.len(), 1);
    assert_eq!(found[0], root.join("src/node_modules"));
}

#[tokio::test(flavor = "multi_thread")]
async fn global_ignored_can_be_target() {
    // If the target list itself contains a GLOBAL_IGNORE name, it should still match.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();

    touch_dir(&root.join("project/.cache/foo"));
    touch_dir(&root.join("project/src"));

    // .cache is in GLOBAL_IGNORE but we explicitly target it
    let mut handle = scanner::start_scan(root.clone(), opts(&[".cache"], &[]));
    let found = drain(&mut handle.results).await;

    assert_eq!(found.len(), 1);
    assert_eq!(found[0], root.join("project/.cache"));
}

#[tokio::test(flavor = "multi_thread")]
async fn exclude_substring_match() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();

    touch_dir(&root.join("keep/node_modules"));
    touch_dir(&root.join("skip-me/inner/node_modules"));

    let mut handle = scanner::start_scan(root.clone(), opts(&["node_modules"], &["skip-me"]));
    let found = drain(&mut handle.results).await;

    assert_eq!(found.len(), 1);
    assert_eq!(found[0], root.join("keep/node_modules"));
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn symlinks_are_not_followed() {
    use std::os::unix::fs::symlink;

    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();

    // Create a real target
    touch_dir(&root.join("real/node_modules"));
    // Create a symlink pointing to the parent dir of a target to entice descent
    touch_dir(&root.join("real"));
    symlink(root.join("real"), root.join("loopback")).unwrap();

    let mut handle = scanner::start_scan(root.clone(), opts(&["node_modules"], &[]));
    let found: HashSet<PathBuf> = drain(&mut handle.results).await.into_iter().collect();

    assert!(found.contains(&root.join("real/node_modules")));
    // Must NOT have emitted node_modules under the symlink
    assert!(!found.iter().any(|p| p.starts_with(root.join("loopback"))));
}

#[tokio::test(flavor = "multi_thread")]
async fn cancellation_stops_scan_quickly() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    // Build a fairly deep tree so scan would take measurable time without cancel
    for i in 0..50 {
        touch_dir(&root.join(format!("d{i}/e/f/g/h")));
    }

    let mut handle = scanner::start_scan(root.clone(), opts(&["__nope__"], &[]));
    let cancel = handle.cancel.clone();
    // Cancel immediately
    cancel.cancel();
    let start = std::time::Instant::now();
    let _ = drain(&mut handle.results).await;
    // Plan spec: cancel propagates within ~100 ms. 200 ms allows for
    // test-runner cold-start / scheduler jitter on slow CI machines.
    assert!(
        start.elapsed() < Duration::from_millis(200),
        "scan took {:?} after cancel — expected <200ms",
        start.elapsed()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_root_yields_no_results_and_terminates() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();

    let mut handle = scanner::start_scan(root, opts(&["node_modules"], &[]));
    let found = drain(&mut handle.results).await;
    assert!(found.is_empty());
    // stats should show one completed dir (the root itself)
    assert!(handle.stats.completed.load(Ordering::SeqCst) >= 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn nonexistent_root_terminates_with_no_results() {
    let mut handle = scanner::start_scan(
        PathBuf::from("/this/does/not/exist/at/all/please"),
        opts(&["node_modules"], &[]),
    );
    let found = drain(&mut handle.results).await;
    assert!(found.is_empty());
}
