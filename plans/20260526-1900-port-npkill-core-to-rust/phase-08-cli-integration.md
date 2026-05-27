# Phase 08 — CLI args + integration glue

## Context

Wire up the binary `nmk` with `clap`. Translate args into `ScanOptions` and `AppState` then call `tui::run`. Add `--no-tui` for scriptable mode that prints found paths as JSON lines.

## Priority

P1.

## Status

completed (2026-05-27, no-tui mode only; full TUI lands in Phase 07)

## Requirements

- args:
  - `[root]` positional, default = current dir
  - `-p, --profile <name>` (repeatable, default = `["node"]`)
  - `-t, --target <name>` (repeatable, extra targets beyond profile)
  - `-e, --exclude <substr>` (repeatable)
  - `-s, --sort <path|size|age>` default `size`
  - `--no-risk-analysis`
  - `--dry-run`
  - `--no-tui` (JSON output mode)
  - `--json` (force JSON even in TTY)
  - `-v, --verbose` (tracing debug)
  - `-V, --version`
- `--no-tui` prints one JSON object per result to stdout and exits when scan completes.

## Architecture

```rust
// src/cli.rs

#[derive(clap::Parser, Debug)]
#[command(name = "nmk", version, about = "Find and delete node_modules and friends")]
pub struct CliArgs {
    /// Root directory to scan (default: current dir)
    pub root: Option<PathBuf>,

    #[arg(short, long = "profile", action = ArgAction::Append)]
    pub profile: Vec<String>,

    #[arg(short, long = "target", action = ArgAction::Append)]
    pub target: Vec<String>,

    #[arg(short, long = "exclude", action = ArgAction::Append)]
    pub exclude: Vec<String>,

    #[arg(short = 's', long, default_value = "size")]
    pub sort: SortArg,

    #[arg(long = "no-risk-analysis", default_value_t = false)]
    pub no_risk: bool,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long = "no-tui")]
    pub no_tui: bool,

    #[arg(long)]
    pub json: bool,

    #[arg(short, long, action = ArgAction::Count)]
    pub verbose: u8,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum SortArg { Path, Size, Age }
```

```rust
// src/main.rs

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    init_logging(args.verbose)?;
    let root = args.root.clone().unwrap_or_else(|| std::env::current_dir().unwrap());
    let scan_opts = ScanOptions {
        targets: resolve_all_targets(&args),
        exclude: args.exclude.clone(),
        sort_by: args.sort.into(),
        perform_risk_analysis: !args.no_risk,
    };

    if args.no_tui || !is_terminal::is_terminal(&std::io::stdout()) {
        run_no_tui(root, scan_opts, args.dry_run).await
    } else {
        tui::run(args, root, scan_opts).await
    }
}

fn resolve_all_targets(args: &CliArgs) -> Vec<String> {
    let profiles = if args.profile.is_empty() { vec!["node".to_string()] } else { args.profile.clone() };
    let names: Vec<&str> = profiles.iter().map(|s| s.as_str()).collect();
    let mut t = profiles::resolve_targets(&names);
    t.extend(args.target.iter().cloned());
    t.sort();
    t.dedup();
    t
}
```

### `--no-tui` mode

```rust
async fn run_no_tui(root: PathBuf, opts: ScanOptions, dry_run: bool) -> anyhow::Result<()> {
    let mut handle = scanner::start_scan(root.clone(), opts);
    while let Some(found) = handle.results.recv().await {
        let size = size::get_folder_size(&handle, found.path.clone()).await.unwrap_or(0);
        let mtime = std::fs::metadata(&found.path).and_then(|m| m.modified()).ok();
        let line = serde_json::json!({
            "path": found.path,
            "size_bytes": size,
            "is_sensitive": found.risk_analysis.as_ref().map(|r| r.is_sensitive).unwrap_or(false),
            "risk_reason": found.risk_analysis.and_then(|r| r.reason),
            "modified": mtime.map(|t| t.duration_since(UNIX_EPOCH).ok()).flatten().map(|d| d.as_secs()),
            "dry_run": dry_run,
        });
        println!("{line}");
    }
    Ok(())
}
```

## Files to create

- (only modify existing `cli.rs` + `main.rs` from Phase 01)

## Files to modify

- `src/cli.rs` — full clap impl
- `src/main.rs` — entry routing TUI vs no-TUI
- `Cargo.toml` add `is-terminal = "0.4"` and `serde_json = "1"`

## Implementation steps

1. Expand `CliArgs` with all flags.
2. Implement `resolve_all_targets`.
3. Implement `init_logging` using `tracing-subscriber` with `EnvFilter` (RUST_LOG override) — log to stderr in TUI mode, to file or stderr in no-TUI.
4. Implement `run_no_tui`.
5. Test `--help` output is readable.
6. Test `--no-tui --json /tmp/tree` outputs valid NDJSON.

## Todo

- [ ] Full clap struct with all flags + value_enum
- [ ] `init_logging` w/ tracing-subscriber + EnvFilter
- [ ] `resolve_all_targets` (profile + extra targets + dedup)
- [ ] TUI vs no-TUI dispatch
- [ ] `run_no_tui` streams NDJSON
- [ ] `cargo run -- --help` readable
- [ ] `cargo run -- --no-tui --dry-run /tmp/seed` outputs NDJSON

## Success criteria

- `nmk --help` shows all flags clearly
- `nmk --no-tui` works without a TTY (CI-friendly)
- `nmk --version` returns crate version
- Default behavior `nmk` in a directory with node_modules launches TUI and finds them

## Risks

| Risk | Mitigation |
|---|---|
| Profile name collision with `--target` | dedup post-merge |
| `is-terminal` detection wrong in some CI | force via `--no-tui` flag exists |
| `tracing` logs polluting TUI screen | redirect to file in TUI mode (use `tracing_appender`) |

## Security considerations

None new.

## Next steps

Phase 09 — tests + docs.
