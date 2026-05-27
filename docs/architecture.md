# Architecture

Short overview. Full design walkthrough, decisions, and behavioral invariants
live in
[`plans/20260526-1900-port-npkill-core-to-rust/`](../plans/20260526-1900-port-npkill-core-to-rust/).

## Layers

```
nmk (bin) ──► src/cli.rs            argv → CliArgs (clap)
                  │
                  ├──► src/tui/         interactive UI (Phase 07)
                  │       app.rs         AppState + reducer
                  │       render.rs      ratatui draw functions
                  │       mod.rs         async main loop + TerminalGuard
                  │
                  └──► run_no_tui()    streaming NDJSON (Phase 08)

src/core/                              public, framework-agnostic library
   ├── scanner.rs    parallel walker (tokio worker pool + CancellationToken)
   ├── size.rs       refcounted async folder size (60s timeout)
   ├── risk.rs       pure path classifier (no FS, no regex)
   ├── safe_delete.rs  basename-in-targets guard
   ├── delete.rs     canonicalize + std::fs::remove_dir_all
   ├── profiles.rs   17 hardcoded profiles + resolve_targets
   ├── sort.rs       path / size / age comparators
   ├── filter.rs     case-insensitive substring filter
   ├── ignore.rs     GLOBAL_IGNORE set (no descent)
   ├── types.rs      ScanOptions, ScanFoundFolder, FolderResult, …
   └── error.rs      NpkillError (thiserror)
```

## Key invariants

1. Targets matched by exact basename (not glob, not substring).
2. Exclude matched as substring against the full candidate path.
3. Walker **does NOT descend into matched targets**.
4. `GLOBAL_IGNORE` skips recursion unless the name is itself a target.
5. Symlinks never followed during scan, size, or delete.
6. Unix: size = `blocks × 512` (true on-disk). Other OS: `metadata.len()`.
7. Directory entries themselves contribute 4096 bytes to size.
8. Delete path must be inside the canonicalized scan root (containment).
9. `std::fs::remove_dir_all` is hardened against symlink traversal
   (CVE-2022-21658).

## Concurrency

- **Scanner**: 1 dispatcher task + N = `min(num_cpus, 8)` worker tasks.
  Completion detected via `Arc<AtomicUsize>` pending counter; reaching zero
  cancels a shared `CancellationToken`.
- **Size**: one `oneshot::Sender` + refcounted pending; recursive
  `tokio::spawn` per directory; same cancel pattern for fail-safe shutdown.
- **TUI**: single `tokio::select!` over tick, EventStream, scanner channel,
  size channel, delete channel. Renderer runs synchronously each iteration.

## See also

- Recon doc:
  [`plans/.../research/xia-recon-and-analysis.md`](../plans/20260526-1900-port-npkill-core-to-rust/research/xia-recon-and-analysis.md)
- Challenge decisions:
  [`plans/.../research/challenge-decisions.md`](../plans/20260526-1900-port-npkill-core-to-rust/research/challenge-decisions.md)
- Per-phase completion reports:
  [`plans/.../reports/`](../plans/20260526-1900-port-npkill-core-to-rust/reports/)
