//! Public data types for the nodemoduleskiller core.
//!
//! Ported from npkill's `src/core/interfaces/folder.interface.ts` and
//! `npkill.interface.ts`. Names follow Rust conventions (snake_case fields,
//! `PascalCase` enums). Behavioral invariants documented at each type.

use std::path::PathBuf;
use std::time::SystemTime;

/// Sort criteria for displayed scan results.
///
/// Mirrors npkill's `SortBy = 'path' | 'size' | 'age'`. Comparators in
/// [`crate::core::sort`] (Phase 06) preserve npkill's null-aware,
/// path-tiebreaking semantics.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortBy {
    /// Lexicographic path ascending.
    Path,
    /// On-disk size descending; tiebreak by path ascending.
    #[default]
    Size,
    /// Last-modified ascending (older first); nulls last; tiebreak by path.
    Age,
}

/// Risk classification for a found folder.
///
/// `is_sensitive == true` means deleting this folder may break user-level
/// applications or configuration (e.g., inside `~/.config`, AppData,
/// `/Applications/X.app`). Detection is best-effort and intentionally
/// errs on the side of marking sensitive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskAnalysis {
    pub is_sensitive: bool,
    pub reason: Option<String>,
}

impl RiskAnalysis {
    pub fn safe() -> Self {
        Self { is_sensitive: false, reason: None }
    }

    pub fn sensitive(reason: impl Into<String>) -> Self {
        Self { is_sensitive: true, reason: Some(reason.into()) }
    }
}

/// Options controlling a scan.
///
/// `targets` are exact basenames to match (e.g., `node_modules`).
/// `exclude` paths are substring-matched against the full path.
/// `perform_risk_analysis` defaults to `true`, mirroring npkill.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub targets: Vec<String>,
    pub exclude: Vec<String>,
    pub sort_by: Option<SortBy>,
    pub perform_risk_analysis: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            targets: Vec::new(),
            exclude: Vec::new(),
            sort_by: None,
            perform_risk_analysis: true,
        }
    }
}

/// A folder emitted by the scanner.
///
/// Lean type: just the path plus optional risk classification. Size and
/// modification time are computed later (on demand) and surface via
/// [`FolderResult`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanFoundFolder {
    pub path: PathBuf,
    pub risk_analysis: Option<RiskAnalysis>,
}

impl ScanFoundFolder {
    pub fn new(path: impl Into<PathBuf>, risk_analysis: Option<RiskAnalysis>) -> Self {
        Self { path: path.into(), risk_analysis }
    }
}

/// UI-enriched view of a scan result.
///
/// Owned by the TUI layer (Phase 07) and updated as auxiliary data
/// (folder size, mtime) becomes available.
#[derive(Debug, Clone)]
pub struct FolderResult {
    pub path: PathBuf,
    pub risk: Option<RiskAnalysis>,
    pub size_bytes: Option<u64>,
    pub last_modified: Option<SystemTime>,
    pub selected: bool,
    pub deleted: bool,
}

impl FolderResult {
    pub fn from_scan(found: ScanFoundFolder) -> Self {
        Self {
            path: found.path,
            risk: found.risk_analysis,
            size_bytes: None,
            last_modified: None,
            selected: false,
            deleted: false,
        }
    }
}

/// Outcome of a single delete operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteResult {
    pub path: PathBuf,
    pub success: bool,
    pub error: Option<String>,
}

impl DeleteResult {
    pub fn ok(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), success: true, error: None }
    }

    pub fn fail(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self { path: path.into(), success: false, error: Some(message.into()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_by_default_is_size() {
        assert_eq!(SortBy::default(), SortBy::Size);
    }

    #[test]
    fn risk_analysis_safe_has_no_reason() {
        let r = RiskAnalysis::safe();
        assert!(!r.is_sensitive);
        assert!(r.reason.is_none());
    }

    #[test]
    fn risk_analysis_sensitive_carries_reason() {
        let r = RiskAnalysis::sensitive("inside .config");
        assert!(r.is_sensitive);
        assert_eq!(r.reason.as_deref(), Some("inside .config"));
    }

    #[test]
    fn scan_options_default_performs_risk() {
        let o = ScanOptions::default();
        assert!(o.perform_risk_analysis);
        assert!(o.targets.is_empty());
        assert!(o.exclude.is_empty());
        assert!(o.sort_by.is_none());
    }

    #[test]
    fn scan_found_folder_constructs_with_pathbuf_or_str() {
        let a = ScanFoundFolder::new("/x/node_modules", None);
        let b = ScanFoundFolder::new(PathBuf::from("/x/node_modules"), None);
        assert_eq!(a, b);
    }

    #[test]
    fn folder_result_from_scan_starts_empty() {
        let s = ScanFoundFolder::new("/x", Some(RiskAnalysis::sensitive("test")));
        let r = FolderResult::from_scan(s);
        assert_eq!(r.path, PathBuf::from("/x"));
        assert!(r.risk.is_some());
        assert!(r.size_bytes.is_none());
        assert!(r.last_modified.is_none());
        assert!(!r.selected);
        assert!(!r.deleted);
    }

    #[test]
    fn delete_result_ok_has_no_error() {
        let r = DeleteResult::ok("/x");
        assert!(r.success);
        assert!(r.error.is_none());
    }

    #[test]
    fn delete_result_fail_carries_message() {
        let r = DeleteResult::fail("/x", "boom");
        assert!(!r.success);
        assert_eq!(r.error.as_deref(), Some("boom"));
    }
}
