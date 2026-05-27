# Phase 01 — Project bootstrap

## Context

Empty directory. Need a fresh Rust crate that is BOTH a library (`nodemoduleskiller`) AND a binary (`nmk`). All later phases depend on this scaffold.

Related research: [`research/xia-recon-and-analysis.md`](research/xia-recon-and-analysis.md) §Dependency matrix.

## Priority

P0 — blocks all other phases.

## Status

completed (2026-05-26)

## Requirements

- Functional: `cargo build` and `cargo run -- --version` succeed.
- Non-functional: edition 2021 (or 2024 if stable on toolchain), MIT license, clippy clean, rustfmt config in repo.

## Architecture

```
nodemoduleskiller/
├── Cargo.toml
├── Cargo.lock                (gitignored: no — bin crate keeps lock)
├── rustfmt.toml
├── clippy.toml               (optional)
├── LICENSE                   (MIT)
├── README.md
├── .gitignore
└── src/
    ├── lib.rs                pub mod core; pub mod tui; pub mod config;
    ├── main.rs               nmk binary entry → cli::run()
    ├── cli.rs                clap::Parser definitions (stub)
    ├── core/
    │   ├── mod.rs            re-exports
    │   ├── types.rs          ScanOptions, ScanFoundFolder, RiskAnalysis, SortBy, DeleteResult, FolderResult
    │   └── error.rs          NpkillError (thiserror)
    └── tui/
        └── mod.rs            stub `pub async fn run(args: CliArgs) -> Result<()>`
```

## Files to create

- `Cargo.toml` (see plan.md for full dep list)
- `rustfmt.toml`: `edition = "2021"\nmax_width = 100\nuse_small_heuristics = "Max"`
- `.gitignore`: `target/\n*.swp\n.DS_Store\n`
- `LICENSE` (MIT, copyright current user)
- `README.md` (one paragraph)
- `src/lib.rs`, `src/main.rs`, `src/cli.rs`
- `src/core/{mod.rs, types.rs, error.rs}`
- `src/tui/mod.rs` (stub)

## Files to modify

None — greenfield.

## Implementation steps

1. `cargo init --lib` then add `[[bin]] name = "nmk" path = "src/main.rs"` to Cargo.toml.
2. Add full dep block from plan.md.
3. Implement `types.rs` mirroring npkill `folder.interface.ts`:
   ```rust
   pub enum SortBy { Path, Size, Age }
   pub struct ScanOptions {
       pub targets: Vec<String>,
       pub exclude: Vec<String>,
       pub sort_by: Option<SortBy>,
       pub perform_risk_analysis: bool,  // default true
   }
   pub struct RiskAnalysis { pub is_sensitive: bool, pub reason: Option<String> }
   pub struct ScanFoundFolder { pub path: PathBuf, pub risk_analysis: Option<RiskAnalysis> }
   pub struct FolderResult {  // enriched for UI
       pub path: PathBuf,
       pub risk: Option<RiskAnalysis>,
       pub size_bytes: Option<u64>,
       pub last_modified: Option<SystemTime>,
       pub selected: bool,
       pub deleted: bool,
   }
   pub struct DeleteResult { pub path: PathBuf, pub success: bool, pub error: Option<String> }
   ```
4. `error.rs`:
   ```rust
   #[derive(thiserror::Error, Debug)]
   pub enum NpkillError {
       #[error("path not within scan root: {0}")]
       PathEscape(PathBuf),
       #[error("io error: {0}")]
       Io(#[from] std::io::Error),
       #[error("invalid root: {0}")]
       InvalidRoot(String),
   }
   ```
5. Stub `cli.rs` with `#[derive(clap::Parser)] struct CliArgs { target_dir: Option<PathBuf> }`.
6. Stub `main.rs`: `#[tokio::main] async fn main() -> Result<()> { let args = CliArgs::parse(); tui::run(args).await }`.
7. Verify: `cargo build`, `cargo clippy -- -D warnings`, `cargo run -- --help`.

## Todo

- [ ] `cargo init --lib`
- [ ] Cargo.toml with full deps + `[[bin]]`
- [ ] rustfmt.toml + .gitignore + LICENSE + README
- [ ] `core/types.rs` with all data structs
- [ ] `core/error.rs` with NpkillError
- [ ] `cli.rs` skeleton with clap
- [ ] `main.rs` tokio entry
- [ ] `tui/mod.rs` stub returning Ok
- [ ] `cargo build` passes
- [ ] `cargo clippy -- -D warnings` clean

## Success criteria

- `cargo build --release` succeeds
- `cargo run -- --help` prints help
- `cargo clippy --all-targets -- -D warnings` is clean

## Risks

- Picking incompatible crate versions: pin to versions in plan.md.
- Edition / MSRV mismatch with newer crates: declare `rust-version = "1.75"` (or current stable) in Cargo.toml.

## Security considerations

- License (MIT) compatible with npkill source license.
- README must credit `voidcosmos/npkill` as the inspiration and link the upstream repo.

## Next steps

Phases 02 (scanner) and 04 (risk analyzer) can both start in parallel once this lands.
