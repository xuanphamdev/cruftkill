# Phase 01 — Code Review

**Date:** 2026-05-26
**Reviewer:** code-reviewer
**Scope:** Project bootstrap — Cargo.toml, rustfmt.toml, LICENSE, README, .gitignore, src/{lib,main,cli}.rs, src/core/{mod,types,error}.rs, src/tui/mod.rs.

## Verdict

**APPROVED.** All Phase 01 acceptance criteria met. Greenfield bootstrap is clean, idiomatic, well-documented. No blockers for Phase 02 (scanner) or Phase 04 (risk) work.

## Verification (re-run locally)

| Check | Result |
|---|---|
| `cargo build --release` | clean (24 crates, 12.29s) |
| `cargo clippy --all-targets --all-features -- -D warnings` | no warnings |
| `cargo test` | 16 passed (3 suites) |
| `cargo fmt --check` | clean |
| `cargo run -- --help` | prints help with `nmk` name, version, `--dry-run` flag |
| `cargo run -- --version` | `nmk 0.1.0` |
| `cargo run -- /tmp/nmk-smoke --dry-run` | stub prints expected line |
| `cargo` from `$HOME/.cargo/bin/cargo` | 1.89.0, rustc 1.94.1 |

Acceptance criteria 1–6 satisfied. Lib + bin layout matches plan (lib `nodemoduleskiller`, bin `nmk`). `core::types` exports `ScanOptions, ScanFoundFolder, RiskAnalysis, FolderResult, DeleteResult, SortBy`. `NpkillError` is `thiserror`-derived. No `unwrap()`/`panic!` outside `#[cfg(test)]`.

## Findings

### MEDIUM

**M1. `CliArgs::root_path` doc lies about absolute resolution.** `src/cli.rs:29`. Doc says "Resolve the scan root to an absolute path", but when `self.root = Some(rel)`, the method returns the relative path verbatim — no `canonicalize()` or `current_dir().join(...)`. Phase 02 scanner likely assumes absolute roots (the plan's `NpkillError::PathEscape` invariant depends on it). Either:
- (a) actually canonicalize / join cwd when relative, or
- (b) reword doc to "Returns the scan root as supplied; cwd if `None`." and push canonicalization into the scanner.

Either is fine for Phase 01 but pick before Phase 02 hard-codes the wrong assumption. The existing test `root_path_falls_back_to_cwd` asserts `is_absolute() || is_relative()` which is tautological — replace with a real assertion once the contract is fixed.

### LOW

**L1. `cli.rs` test module duplicates `use clap::Parser`.** `src/cli.rs:41`. The outer module already imports `clap::Parser` (line 9); the test module re-imports it under `use clap::Parser;`. Not wrong, but `use super::*;` already brings nothing relevant for clap — keeping the local import is fine, just redundant. Remove for tidiness, or leave (clippy is silent either way).

**L2. `ScanOptions::default()` has `targets: Vec::new()`.** A consumer who builds default options and runs a scan will match nothing. Acceptable for Phase 01 (the defaults will likely be filled from profiles in Phase 05), but worth a one-line doc note: "Default targets are empty — pick a profile or set explicitly."

**L3. README install instruction will fail until Phase 07.** `README.md:18-22` says `cargo install --path .` works, which will succeed but the resulting binary just prints the stub line. Either soften the wording ("once Phase 07 lands, ...") or accept that the stub at least runs.

### NIT

**N1. `rustfmt.toml` uses `use_small_heuristics = "Max"`.** Matches plan. Fine. Just confirming it is intentional (it makes single-line `Self { … }` literals common, which we see in types.rs — that is why they format on one line).

**N2. `Cargo.toml` lacks `authors`, `repository`, `homepage`, `documentation` fields.** Not blocking for a private/internal crate but recommended before any `cargo publish`. Pre-publish polish — not Phase 01 scope.

**N3. `.gitignore` does not exclude `Cargo.lock`.** Correct for a binary crate (plan explicitly says "gitignored: no") — calling out so the next reviewer does not "fix" it.

**N4. `tui::run` is `async` but does no `.await`.** Expected stub. Phase 07 will populate it. Clippy is silent because the function is `pub` and trivially exported.

## Public API Surface

Cleanly minimal:
- `pub mod cli, core, tui` — all required for bin + library use.
- Re-exports of `NpkillError` and types from `lib.rs` are the right ergonomics.
- No accidental `pub` on internals.
- `RiskAnalysis::safe()` / `sensitive()`, `DeleteResult::ok()` / `fail()`, `ScanFoundFolder::new()`, `FolderResult::from_scan()` constructors are good ergonomics for downstream phases.

No suggested visibility changes.

## Idiomatic Rust Checklist

- [x] `snake_case` filenames, `PascalCase` types, `snake_case` fields.
- [x] `impl Into<PathBuf>` / `impl Into<String>` in constructors — idiomatic.
- [x] `#[derive(thiserror::Error, Debug)]` enum with `#[from]` for io.
- [x] No `unwrap()`/`expect()`/`panic!()` outside tests.
- [x] No `unsafe`.
- [x] Doc comments on every `pub` item.
- [x] `Default` impls on `SortBy` and `ScanOptions`.
- [x] Edition 2024, `rust-version = "1.85"`.

## Blockers for Phase 02

None. The `CliArgs::root_path` doc/contract (M1) should be tightened before Phase 02 wires the scanner, but it is not a hard blocker.

## Unresolved Questions

- M1: should `root_path()` canonicalize, or should the scanner own that? Recommend canonicalize in the scanner's `validate_root` (Phase 02) and reword the cli doc — keeps `cli.rs` dependency-free.
- Should `ScanOptions::default()` populate npkill's built-in `node_modules` target, or leave empty until Phase 05 profiles? Plan implies the latter; confirm before Phase 02 starts.
