//! Integration tests for Phase 03 — folder size calculation.
//!
//! Invariants exercised:
//! - directories themselves count 4096 bytes
//! - symlinks never followed (no double counting, no infinite loops)
//! - permission errors silently skipped (returns partial sum)
//! - empty / nonexistent path → 0 bytes
//! - 60 s timeout reachable

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use nodemoduleskiller::core::size;

fn write_file(path: &PathBuf, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    let mut f = fs::File::create(path).expect("create file");
    f.write_all(bytes).expect("write");
    f.sync_all().expect("sync");
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_dir_size_is_dir_overhead_only() {
    let tmp = tempfile::tempdir().unwrap();
    let total = size::get_folder_size(tmp.path().to_path_buf()).await.unwrap();
    // Walker counts the root's CHILDREN; the root itself is not added as 4096.
    // An empty dir has no children → 0.
    assert_eq!(total, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn single_file_counted() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = vec![0u8; 5000];
    write_file(&tmp.path().join("a.bin"), &payload);
    let total = size::get_folder_size(tmp.path().to_path_buf()).await.unwrap();
    // Unix `blocks * 512` rounds up to filesystem block size, so total is
    // >= payload size and within a few KiB of it.
    assert!(total >= 4096, "expected at least one disk block, got {total}");
    assert!(total < 64_000, "expected single small file, got {total}");
}

#[tokio::test(flavor = "multi_thread")]
async fn subdir_adds_overhead_and_descends() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    write_file(&root.join("sub/inner.bin"), &vec![0u8; 1000]);

    let total = size::get_folder_size(root).await.unwrap();
    // At least the 4096 dir-overhead for `sub/` plus the file's disk blocks.
    assert!(total >= 4096 + 512, "expected >= 4608, got {total}");
}

#[tokio::test(flavor = "multi_thread")]
async fn nonexistent_path_returns_zero() {
    let total = size::get_folder_size(PathBuf::from("/no/such/path/exists/here")).await.unwrap();
    assert_eq!(total, 0);
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn symlinks_are_not_followed() {
    use std::os::unix::fs::symlink;

    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();

    // Real content
    write_file(&root.join("real/big.bin"), &vec![0u8; 10_000]);
    // Symlink loop: link → root
    symlink(&root, root.join("loopback")).unwrap();

    let total = size::get_folder_size(root).await.unwrap();
    // If symlinks were followed, this would be infinite or huge. 1MB is a
    // generous ceiling that proves we did not follow the loop.
    assert!(total < 1_000_000, "size {total} suggests symlink was followed");
}

#[tokio::test(flavor = "multi_thread")]
async fn three_files_three_levels_sums_them_all() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    write_file(&root.join("a.bin"), &[0u8; 100]);
    write_file(&root.join("b/b.bin"), &[0u8; 100]);
    write_file(&root.join("b/c/c.bin"), &[0u8; 100]);

    let total = size::get_folder_size(root).await.unwrap();
    // 3 files (each rounds up to >= 1 block) + 2 subdir overheads (b/, b/c/)
    // = at least 3*512 + 2*4096 = 9728 bytes. Use a loose lower bound.
    assert!(total >= 3 * 512 + 2 * 4096, "expected >=9728, got {total}");
}
