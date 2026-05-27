# Phase 01 — completion report

## Status

**Completed** 2026-05-26.

## Delivered

Project scaffold:
- `Cargo.toml` — lib + `nmk` bin, edition 2024, rust-version 1.85
- deps: clap 4, thiserror 2, anyhow 1, tokio 1 (rt-multi-thread, macros, sync, time, fs)
- dev-deps: tempfile 3
- `LICENSE` (MIT, with attribution to npkill upstream)
- `README.md` (Phase 01 status, plan link, attribution)
- `rustfmt.toml`, expanded `.gitignore`

Source files:
- `src/lib.rs` — module declarations + re-exports
- `src/main.rs` — tokio multi-thread entry → `tui::run`
- `src/cli.rs` — `clap::Parser` with `root`, `--dry-run`
- `src/tui/mod.rs` — Phase 01 stub printing what would be scanned
- `src/core/mod.rs` — module aggregator
- `src/core/types.rs` — `SortBy`, `RiskAnalysis`, `ScanOptions`, `ScanFoundFolder`, `FolderResult`, `DeleteResult`
- `src/core/error.rs` — `NpkillError` (thiserror, variants: `PathEscape`, `Io`, `InvalidRoot`, `SizeTimeout`)

## Gates

| Gate | Result |
|---|---|
| `cargo build --release` | clean |
| `cargo test` | 16 passed (types: 8, error: 4, cli: 4) |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| `cargo run -- --help` | renders |
| `cargo run -- --version` | `nmk 0.1.0` |
| Smoke: `cargo run -- /tmp/nmk-smoke --dry-run` | prints stub message |
| Independent code review | DONE, no CRITICAL/HIGH |

## Review outcome

Reviewer (`code-reviewer` subagent) approved with no blockers. Applied:
- M1: doc-string on `CliArgs::root_path` now states canonicalization is the scanner's responsibility (Phase 02).
- LOW: removed redundant `use clap::Parser` in cli test module.

Deferred to v1-publish polish (NIT):
- Cargo.toml `authors`/`repository`/`homepage`.
- README install instructions to be rewritten when the binary actually scans.

Full review: [`phase-01-review.md`](phase-01-review.md).

## Unresolved questions (for Phase 02 to confirm)

1. `ScanOptions::default()` ships with empty `targets` — Phase 05 (profiles) will provide the default `node_modules` list. Confirm Phase 02 requires explicit `targets` from caller.
2. Root canonicalization happens in scanner (`validate_root`) — confirmed.

## Next

Phase 02 (core scanner) and Phase 04 (risk analyzer) are now unblocked and can proceed in parallel.
