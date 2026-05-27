//! CLI argument parsing for the `nmk` binary.
//!
//! Phase 08 expands Phase 01's stub into the full surface: profile + target +
//! exclude lists, sort criterion, risk-analysis toggle, dry-run, no-tui JSON
//! output, version. The TUI itself is still a Phase 01 stub — Phase 07
//! lands it — so for v0.1 `--no-tui` is the only practically useful mode.

use std::path::PathBuf;

use clap::{ArgAction, Parser, ValueEnum};

use crate::core::profiles::{DEFAULT_PROFILE, profile_names};
use crate::core::types::SortBy;

/// `nmk` — find and delete `node_modules` (and other build-cache folders).
#[derive(Debug, Parser)]
#[command(
    name = "nmk",
    version,
    about = "Find and delete node_modules and friends",
    long_about = None,
)]
pub struct CliArgs {
    /// Root directory to scan. Defaults to the current working directory.
    pub root: Option<PathBuf>,

    /// Scan profile to use. Repeatable. Defaults to `node`. Pass `--profile all`
    /// to combine every base profile.
    #[arg(short, long = "profile", action = ArgAction::Append)]
    pub profile: Vec<String>,

    /// Extra target basename(s) to match beyond what the profiles provide.
    #[arg(short, long = "target", action = ArgAction::Append)]
    pub target: Vec<String>,

    /// Substrings that, if found anywhere in a candidate path, skip it.
    #[arg(short, long = "exclude", action = ArgAction::Append)]
    pub exclude: Vec<String>,

    /// Sort criterion for the displayed / streamed results.
    #[arg(short = 's', long, value_enum, default_value_t = SortArg::Size)]
    pub sort: SortArg,

    /// Skip per-result risk analysis (slightly faster scans).
    #[arg(long = "no-risk-analysis", default_value_t = false)]
    pub no_risk: bool,

    /// Simulate deletes without touching the filesystem.
    #[arg(long)]
    pub dry_run: bool,

    /// Print one JSON object per result to stdout instead of launching the TUI.
    #[arg(long = "no-tui")]
    pub no_tui: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SortArg {
    Path,
    Size,
    Age,
}

impl From<SortArg> for SortBy {
    fn from(s: SortArg) -> Self {
        match s {
            SortArg::Path => SortBy::Path,
            SortArg::Size => SortBy::Size,
            SortArg::Age => SortBy::Age,
        }
    }
}

impl CliArgs {
    /// Return the user-supplied root path, falling back to the current working
    /// directory. NOT canonicalized — the scanner / delete guard handle that.
    pub fn root_path(&self) -> std::io::Result<PathBuf> {
        match &self.root {
            Some(p) => Ok(p.clone()),
            None => std::env::current_dir(),
        }
    }

    /// Resolve `--profile` + `--target` into a final, deduped target list.
    /// Defaults to `[DEFAULT_PROFILE]` when no profile is given.
    pub fn resolved_targets(&self) -> Vec<String> {
        let profile_strs: Vec<&str> = if self.profile.is_empty() {
            vec![DEFAULT_PROFILE]
        } else {
            self.profile.iter().map(String::as_str).collect()
        };
        let mut targets = crate::core::profiles::resolve_targets(&profile_strs);
        for t in &self.target {
            targets.push(t.clone());
        }
        targets.sort();
        targets.dedup();
        targets
    }

    /// True if any user-supplied profile name is not in the registry.
    pub fn unknown_profile(&self) -> Option<&str> {
        let known = profile_names();
        self.profile
            .iter()
            .find(|p| p.as_str() != "all" && !known.contains(&p.as_str()))
            .map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_with_no_args() {
        let args = CliArgs::try_parse_from(["nmk"]).unwrap();
        assert!(args.root.is_none());
        assert!(!args.dry_run);
        assert!(!args.no_tui);
        assert!(matches!(args.sort, SortArg::Size));
    }

    #[test]
    fn parses_root_positional() {
        let args = CliArgs::try_parse_from(["nmk", "/tmp/scan"]).unwrap();
        assert_eq!(args.root, Some(PathBuf::from("/tmp/scan")));
    }

    #[test]
    fn parses_dry_run_flag() {
        let args = CliArgs::try_parse_from(["nmk", "--dry-run"]).unwrap();
        assert!(args.dry_run);
    }

    #[test]
    fn parses_no_tui_flag() {
        let args = CliArgs::try_parse_from(["nmk", "--no-tui"]).unwrap();
        assert!(args.no_tui);
    }

    #[test]
    fn parses_multiple_profiles() {
        let args = CliArgs::try_parse_from(["nmk", "-p", "node", "-p", "python"]).unwrap();
        assert_eq!(args.profile, vec!["node", "python"]);
    }

    #[test]
    fn parses_sort_age() {
        let args = CliArgs::try_parse_from(["nmk", "-s", "age"]).unwrap();
        assert!(matches!(args.sort, SortArg::Age));
    }

    #[test]
    fn default_profile_when_none_passed() {
        let args = CliArgs::try_parse_from(["nmk"]).unwrap();
        let t = args.resolved_targets();
        assert!(t.contains(&"node_modules".to_string()));
    }

    #[test]
    fn extra_targets_merged() {
        let args = CliArgs::try_parse_from(["nmk", "-p", "node", "-t", "extra_dir"]).unwrap();
        let t = args.resolved_targets();
        assert!(t.contains(&"extra_dir".to_string()));
        assert!(t.contains(&"node_modules".to_string()));
    }

    #[test]
    fn unknown_profile_detected() {
        let args = CliArgs::try_parse_from(["nmk", "-p", "node", "-p", "this-is-fake"]).unwrap();
        assert_eq!(args.unknown_profile(), Some("this-is-fake"));
    }

    #[test]
    fn all_profile_is_recognised() {
        let args = CliArgs::try_parse_from(["nmk", "-p", "all"]).unwrap();
        assert!(args.unknown_profile().is_none());
    }

    #[test]
    fn root_path_falls_back_to_cwd() {
        let args = CliArgs {
            root: None,
            profile: vec![],
            target: vec![],
            exclude: vec![],
            sort: SortArg::Size,
            no_risk: false,
            dry_run: false,
            no_tui: false,
        };
        let cwd = args.root_path().unwrap();
        // Just confirm it doesn't error and returns something we can use.
        assert!(cwd.exists());
    }
}
