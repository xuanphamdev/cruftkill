//! `nmk` binary entry point.
//!
//! Phase 08 dispatches between the (Phase 07) interactive TUI and the
//! scriptable `--no-tui` JSON output mode. For v0.1 the TUI is still a stub,
//! so `--no-tui` is the practically useful path.

use std::io::IsTerminal;

use anyhow::Context;
use clap::Parser;

use nodemoduleskiller::cli::CliArgs;
use nodemoduleskiller::tui;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();

    if let Some(unknown) = args.unknown_profile() {
        anyhow::bail!("unknown profile: {unknown}. Run `nmk --help` for the list.");
    }

    // Auto-fallback to no-tui when stdout is not a real terminal (e.g., piped or CI).
    let no_tui = args.no_tui || !std::io::stdout().is_terminal();

    if no_tui { run_no_tui(args).await.context("no-tui mode failed") } else { tui::run(args).await }
}

async fn run_no_tui(args: CliArgs) -> anyhow::Result<()> {
    use nodemoduleskiller::core::{risk, scanner, size, types::ScanOptions};

    let root = args.root_path()?;
    let targets = args.resolved_targets();
    let opts = ScanOptions {
        targets: targets.clone(),
        exclude: args.exclude.clone(),
        sort_by: Some(args.sort.into()),
        perform_risk_analysis: !args.no_risk,
    };
    // Resolve HOME once instead of re-reading the env per result.
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).ok();
    let home_path = home.as_deref().map(std::path::Path::new);

    let mut handle = scanner::start_scan(root.clone(), opts);

    while let Some(found) = handle.results.recv().await {
        let size_bytes = size::get_folder_size(found.path.clone()).await.unwrap_or(0);
        let risk_analysis =
            if args.no_risk { None } else { Some(risk::analyze_with_home(&found.path, home_path)) };
        let modified = std::fs::metadata(&found.path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        let line = serde_json::json!({
            "path": found.path,
            "size_bytes": size_bytes,
            "is_sensitive": risk_analysis.as_ref().map(|r| r.is_sensitive).unwrap_or(false),
            "risk_reason": risk_analysis.and_then(|r| r.reason),
            "modified_unix": modified,
            "dry_run": args.dry_run,
        });
        println!("{line}");
    }

    let _ = targets;
    Ok(())
}
