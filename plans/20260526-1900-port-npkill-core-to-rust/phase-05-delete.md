# Phase 05 — Delete operation with containment guard

## Context

Port `delete$` from `src/core/npkill.ts` + Unix/Windows `deleteDir` services. Decision C3 (locked): pure Rust `std::fs::remove_dir_all`, single implementation for all OS.

## Priority

P0 — the action that gives the tool its purpose.

## Status

completed (2026-05-27)

## Requirements

- `pub async fn delete(path: &Path, scan_root: &Path, targets: &[String], dry_run: bool) -> DeleteResult`
- Reject path NOT contained in `scan_root` (canonicalize both, then `starts_with`)
- Reject path whose basename is NOT in `targets` (defense in depth via `safe_delete::is_safe_to_delete`)
- `dry_run=true`: sleep 200–4200 ms (mimic source `fakeDeleteDir`), return success without touching FS
- Errors mapped to `DeleteResult.error` not panics

## Architecture

```rust
// src/core/delete.rs

pub async fn delete(
    path: &Path,
    scan_root: &Path,
    targets: &[String],
    dry_run: bool,
) -> DeleteResult {
    // Guard 1: basename must be in targets
    if !safe_delete::is_safe_to_delete(path, targets) {
        return DeleteResult::fail(path, "Path basename not in target list");
    }

    // Guard 2: must be inside scan_root
    // v0.1: use std::fs::canonicalize — both sides are canonicalized
    // symmetrically, so the `\\?\` prefix on Windows is harmless for
    // `starts_with`. `dunce::canonicalize` deferred to v0.2 polish (display only).
    let canon_path = match std::fs::canonicalize(path) {
        Ok(p) => p,
        Err(e) => return DeleteResult::fail(path, format!("canonicalize failed: {e}")),
    };
    let canon_root = match std::fs::canonicalize(scan_root) {
        Ok(p) => p,
        Err(e) => return DeleteResult::fail(path, format!("root canonicalize failed: {e}")),
    };
    if !canon_path.starts_with(&canon_root) {
        return DeleteResult::fail(path, "Path is outside scan root");
    }

    if dry_run {
        let ms = 200 + (rand_u64() % 4000);
        tokio::time::sleep(Duration::from_millis(ms)).await;
        return DeleteResult::ok(path);
    }

    let pb = path.to_path_buf();
    let res = tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&pb)).await;
    match res {
        Ok(Ok(())) => DeleteResult::ok(path),
        Ok(Err(e)) => DeleteResult::fail(path, &e.to_string()),
        Err(e) => DeleteResult::fail(path, &format!("join error: {e}")),
    }
}

fn rand_u64() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos() as u64
}
```

## Files to create

- `src/core/delete.rs` (~80 LoC)
- `tests/delete_guard.rs` — tests for guard rejections + happy path

## Files to modify

- `src/core/mod.rs` — `pub mod delete; pub use delete::delete;`

## Implementation steps

1. Implement guards (basename + containment) **before** any FS mutation.
2. Implement `dry_run` branch (random sleep).
3. Implement real delete via `spawn_blocking` (so the async runtime stays responsive on huge trees).
4. Tests:
   - delete a tempdir → success, dir gone
   - delete with `dry_run=true` → returns success, dir still exists
   - delete with basename NOT in targets → rejected
   - delete a path OUTSIDE scan_root (e.g., `/tmp/other` when root is `/tmp/scan`) → rejected
   - delete a nonexistent path → returns failure (canonicalize fails)
   - delete with symlink-traversal attempt: `scan_root/escape -> ../../etc` → after canonicalize, `starts_with` rejects

## Todo

- [ ] `DeleteResult::ok/fail` constructors
- [ ] Guard 1: basename in targets
- [ ] Guard 2: containment via canonicalize + starts_with
- [ ] dry_run branch
- [ ] real delete via `spawn_blocking`
- [ ] All 6 test cases pass

## Success criteria

- All test cases pass
- No way to delete outside scan root (verified by test 4 and test 6)
- `cargo clippy` clean

## Risks

| Risk | Mitigation |
|---|---|
| TOCTOU race: path resolved differently between guard and delete | acceptable — same race exists in npkill. Mitigate via final canonicalize-before-delete if paranoid. |
| Symlink escape | `dunce::canonicalize` resolves symlinks; `starts_with` on canonical paths catches it |
| `remove_dir_all` follows symlinks on some OS — could delete OUTSIDE the dir | document risk; do NOT use `--follow-symlinks`. Rust std `remove_dir_all` does NOT follow symlinks by default (it removes the symlink, not target). |
| Windows long-path (>260 chars) | `dunce` handles `\\?\` prefix; std::fs uses Win32 long-path on recent Rust |
| Permission denied mid-delete leaves partial state | document; surface error in DeleteResult |

## Security considerations

- **Path traversal attack surface**: a malicious symlink inside the scan tree pointing to `/etc` must not cause `/etc` deletion. Verified by test 6.
- Never run as root.
- README warning: "this tool deletes recursively without recycle bin."

## Next steps

Phase 07 wires `delete` to the TUI key handler ("d" / Space). Phase 08 adds CLI `--dry-run` flag.
