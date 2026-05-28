//! Integration tests for Phase 05 — delete with safety guards.
//!
//! Invariants exercised:
//! - basename must be in `targets` (defense in depth via Phase 04 guard)
//! - canonicalized path must be inside the canonicalized scan root
//! - dry-run NEVER touches the filesystem
//! - permission / nonexistent / outside-root paths return failure, not panic
//! - symlink escape attempts are rejected after canonicalization

use std::fs;
use std::path::{Path, PathBuf};

use cruftkill::core::delete;

fn make_tree(root: &Path, name: &str) -> PathBuf {
    let p = root.join(name);
    fs::create_dir_all(p.join("inner")).expect("create inner");
    fs::write(p.join("inner/file.txt"), b"x").expect("write inner file");
    p
}

fn targets() -> Vec<String> {
    vec!["node_modules".into()]
}

#[tokio::test(flavor = "multi_thread")]
async fn deletes_a_real_node_modules_inside_root() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let nm = make_tree(&root, "node_modules");

    assert!(nm.exists(), "precondition: dir exists");
    let res = delete::delete(&nm, &root, &targets(), false).await;
    assert!(res.success, "expected success, got {res:?}");
    assert!(!nm.exists(), "node_modules should be removed");
}

#[tokio::test(flavor = "multi_thread")]
async fn dry_run_reports_success_but_does_not_touch_fs() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let nm = make_tree(&root, "node_modules");

    let res = delete::delete(&nm, &root, &targets(), true).await;
    assert!(res.success);
    assert!(nm.exists(), "dry-run must not delete");
    assert!(nm.join("inner/file.txt").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_basename_not_in_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let off = make_tree(&root, "totally-not-a-target");

    let res = delete::delete(&off, &root, &targets(), false).await;
    assert!(!res.success, "expected rejection");
    let msg = res.error.unwrap_or_default();
    assert!(msg.to_lowercase().contains("target"), "expected target-related error, got {msg:?}");
    assert!(off.exists(), "rejected delete must not touch FS");
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_path_outside_scan_root() {
    let tmp_root = tempfile::tempdir().unwrap();
    let tmp_other = tempfile::tempdir().unwrap();
    let root = tmp_root.path().to_path_buf();
    let outside_nm = make_tree(tmp_other.path(), "node_modules");

    let res = delete::delete(&outside_nm, &root, &targets(), false).await;
    assert!(!res.success);
    let msg = res.error.unwrap_or_default();
    assert!(
        msg.to_lowercase().contains("outside") || msg.to_lowercase().contains("scan root"),
        "expected containment error, got {msg:?}"
    );
    assert!(outside_nm.exists(), "rejected delete must not touch FS");
}

#[tokio::test(flavor = "multi_thread")]
async fn nonexistent_path_returns_failure_not_panic() {
    let tmp_root = tempfile::tempdir().unwrap();
    let root = tmp_root.path().to_path_buf();
    let fake = root.join("node_modules"); // does not exist

    let res = delete::delete(&fake, &root, &targets(), false).await;
    assert!(!res.success);
    assert!(res.error.is_some());
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn symlink_escape_attempt_is_rejected() {
    use std::os::unix::fs::symlink;

    let tmp_root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let root = tmp_root.path().to_path_buf();
    let outside_target = make_tree(outside.path(), "node_modules");

    // Place a symlink INSIDE root that points OUTSIDE: root/escape -> outside/node_modules
    let escape = root.join("escape");
    symlink(&outside_target, &escape).unwrap();

    // The basename "escape" is not a target, so guard 1 should reject first.
    // The deeper safety net is that even if it were named correctly, the
    // canonicalized path is outside `root` and guard 2 would reject.
    let res = delete::delete(&escape, &root, &targets(), false).await;
    assert!(!res.success, "symlink escape MUST be rejected");
    // The outside content must still exist.
    assert!(outside_target.exists());
    assert!(outside_target.join("inner/file.txt").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn parent_traversal_resolving_outside_root_is_rejected() {
    // Build a sibling tree: /tmp/<x>/scan_root/  and  /tmp/<x>/outside/node_modules
    // Then ask to delete `scan_root/../outside/node_modules`. After canonicalize
    // the path lives outside `scan_root`, so guard 2 must reject.
    let parent = tempfile::tempdir().unwrap();
    let scan_root = parent.path().join("scan_root");
    fs::create_dir_all(&scan_root).unwrap();
    let outside_nm = make_tree(parent.path(), "outside");
    let outside_target = make_tree(&outside_nm, "node_modules");

    let traversal_path = scan_root.join("..").join("outside").join("node_modules");
    let res = delete::delete(&traversal_path, &scan_root, &targets(), false).await;
    assert!(!res.success, "parent-traversal MUST be rejected");
    let msg = res.error.unwrap_or_default();
    assert!(
        msg.to_lowercase().contains("outside") || msg.to_lowercase().contains("scan root"),
        "expected containment error, got {msg:?}"
    );
    // The targeted directory must still be intact.
    assert!(outside_target.exists());
    assert!(outside_target.join("inner/file.txt").exists());
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn symlink_pointing_outside_with_target_basename_rejected_by_containment() {
    use std::os::unix::fs::symlink;

    let tmp_root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let root = tmp_root.path().to_path_buf();
    let outside_target = make_tree(outside.path(), "node_modules");

    // root/node_modules -> outside/node_modules
    // Now the basename IS a target ("node_modules"), so guard 1 passes.
    // Guard 2 (canonicalize + starts_with) must catch the escape.
    let link = root.join("node_modules");
    symlink(&outside_target, &link).unwrap();

    let res = delete::delete(&link, &root, &targets(), false).await;
    assert!(!res.success, "containment guard MUST reject the escape");
    assert!(outside_target.exists());
    // The symlink itself should still be intact (we never even attempted to delete it).
    assert!(link.exists() || link.symlink_metadata().is_ok());
}
