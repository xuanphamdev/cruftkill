# Phase 04 Completion Report — Risk Analyzer + Safe-Delete Guard

## Delivered Files

| File | LoC | Role |
|---|---|---|
| `src/core/risk.rs` | ~205 | `analyze` + `analyze_with_home` + 5 pure helpers |
| `src/core/safe_delete.rs` | ~65 | `is_safe_to_delete` + 5 unit tests |
| `tests/risk_table.rs` | ~345 | 35 table-driven integration tests |

## Test Count

- **35** test functions in `tests/risk_table.rs`
- **9** unit tests inside module `#[cfg(test)]` blocks (risk.rs + safe_delete.rs)
- **Total new tests**: 44
- **Total suite (all phases)**: 84 passed, 0 failed

## Gate Results

| Gate | Status | Notes |
|---|---|---|
| `cargo test` | PASS — 84/84 | all phases |
| `cargo fmt --all -- --check` | PASS | |
| `cargo clippy --lib -- -D warnings` | PASS | |
| `cargo clippy --test risk_table -- -D warnings` | PASS | |
| `cargo clippy --all-targets --all-features -- -D warnings` | FAIL (3 pre-existing) | 3 `useless_vec` in `tests/size_smoke.rs` (Phase 03 — outside Phase 04 file ownership); confirmed not introduced by this phase |

## Key Design Decisions

### `analyze_with_home` split
Implemented `analyze_with_home(path, home: Option<&Path>)` as the pure testable core, with `analyze(path)` as a thin wrapper reading `HOME`/`USERPROFILE` from env. All 35 table tests target `analyze_with_home` with a fixed synthetic home (`/home/user`), making them hermetic and parallel-safe without env mutation or `serial_test`.

### `normalized` vs `original_norm`
In npkill, `normalizedPath` is derived from the absolutised path while `normalizedOriginal` is derived from the literal input. In our port both inputs are the same string (we don't resolve relative paths to cwd — we receive pre-formed paths), so `original_norm = normalized.clone()`. Named separately to preserve the structural parity with npkill for future audits.

### No `regex` crate
All checks use `str::contains`, `str::starts_with`, `str::split`, and `str::find` — decision C5 preserved exactly.

### No new Cargo.toml dependencies
No crates added.

## Deviations from npkill Source

| # | npkill behaviour | Rust port | Rationale |
|---|---|---|---|
| 1 | `path.resolve(process.cwd(), originalPath)` for relative paths | Skipped — port receives absolute paths from scanner | Scanner always emits absolute paths; resolution at caller boundary is cleaner |
| 2 | `os.homedir()` fallback if HOME/USERPROFILE both missing | Not implemented — treated as no home | `os.homedir()` on Unix reads `/etc/passwd`; that's FS I/O, violating the purity constraint. Acceptable since HOME is always set in practice |
| 3 | UNC detection: `originalPath.startsWith('\\\\')` checked BEFORE absolutisation | `original_norm.starts_with("//")` checked on normalised string | After `normalize_str`, `\\` → `//`, so semantically identical; no behavioral difference |

All reason strings match npkill verbatim (verified case-by-case against `files.service.ts`):
- `"Contains user configuration data (~/.config)"`
- `"User data folder (~/.local/share)"`
- `"System-wide cache directory (~/.cache)"`
- `"Contains unsafe hidden folder"`
- `"Inside macOS .app package"`
- `"Hidden path in network share"`
- `"Inside Windows AppData Roaming folder"`
- `"Inside Windows AppData Local folder"`
- `"Inside Program Files folder"`

## Unresolved Questions

None.

## Next Steps

- Phase 02: wire `risk::analyze` into scanner output (`ScanFoundFolder.risk_analysis`)
- Phase 05: wire `safe_delete::is_safe_to_delete` into delete guard before `fs::remove_dir_all`
- Pre-existing clippy issue in `tests/size_smoke.rs` (3x `useless_vec`) should be fixed by Phase 03 owner before the repo reaches a clean `--all-targets` clippy gate
