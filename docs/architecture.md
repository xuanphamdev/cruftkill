# Architecture

Short overview. Full original port design, decisions, and behavioral
invariants live in
[`plans/20260526-1900-port-npkill-core-to-rust/`](../plans/20260526-1900-port-npkill-core-to-rust/).

## Layers

```text
cft (bin) ──► src/cli.rs            argv -> CliArgs (clap)
                 │
                 ├──► src/tui/      interactive UI
                 │       app.rs     AppState + reducer
                 │       render.rs  ratatui draw functions
                 │       mod.rs     async main loop + TerminalGuard
                 │
                 └──► run_no_tui()  streaming NDJSON

src/core/                           public, framework-agnostic library
   ├── scanner.rs      parallel walker + truthful risk analysis
   ├── size.rs         refcounted async folder size (60s timeout)
   ├── risk.rs         pure path classifier (no FS, no regex)
   ├── metadata.rs     ecosystem/category/delete-risk labels
   ├── safe_delete.rs  basename-in-targets guard
   ├── delete.rs       canonicalize + std::fs::remove_dir_all
   ├── profiles.rs     hardcoded profiles + resolve_targets
   ├── sort.rs         path / size / age comparators
   ├── filter.rs       case-insensitive substring filter
   ├── ignore.rs       GLOBAL_IGNORE set (no descent)
   ├── types.rs        ScanOptions, ScanFoundFolder, FolderResult, ...
   └── error.rs        CruftError (thiserror)
```

## Result Flow

```text
CliArgs -> resolved_targets -> ScanOptions
  -> scanner::start_scan
  -> ScanFoundFolder { path, risk_analysis }
  -> FolderResult::from_scan
  -> FolderResult::metadata()
  -> TUI metadata line / NDJSON metadata fields
```

`risk_analysis` is computed in the scanner when enabled. `metadata.rs` is
advisory: it labels likely ecosystem, cleanup category, delete-risk level, and
rebuild hint. Delete confirmation and the delete guards remain authoritative.

## Key Invariants

1. Targets matched by exact basename (not glob, not substring).
2. Exclude matched as substring against the full candidate path.
3. Walker does not descend into matched targets.
4. `GLOBAL_IGNORE` skips recursion unless the name is itself a target.
5. Symlinks never followed during scan, size, or delete.
6. Unix: size = `blocks * 512` (true on-disk). Other OS: `metadata.len()`.
7. Directory entries themselves contribute 4096 bytes to size.
8. Delete path must be inside the canonicalized scan root.
9. `std::fs::remove_dir_all` is hardened against symlink traversal
   (CVE-2022-21658).
10. Metadata is display/JSON context only; it never bypasses delete guards.

## Concurrency

- Scanner: 1 dispatcher task + N = `min(num_cpus, 8)` worker tasks.
  Completion detected via `Arc<AtomicUsize>` pending counter; reaching zero
  cancels a shared `CancellationToken`.
- Size: one `oneshot::Sender` + refcounted pending; recursive `tokio::spawn`
  per directory; same cancel pattern for fail-safe shutdown.
- TUI: single `tokio::select!` over tick, EventStream, scanner channel, size
  channel, delete channel, and update-check channel. Renderer runs
  synchronously each iteration.
- Metadata: target-to-ecosystem reverse lookup is cached with `OnceLock`; row
  metadata borrows static labels and current path/risk data.

## See Also

- Scan metadata plan:
  [`plans/260528-1752-scan-result-metadata-display/`](../plans/260528-1752-scan-result-metadata-display/)
- Recon doc:
  [`plans/.../research/xia-recon-and-analysis.md`](../plans/20260526-1900-port-npkill-core-to-rust/research/xia-recon-and-analysis.md)
- Challenge decisions:
  [`plans/.../research/challenge-decisions.md`](../plans/20260526-1900-port-npkill-core-to-rust/research/challenge-decisions.md)
