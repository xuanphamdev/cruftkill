//! Case-insensitive substring filter for displayed scan results.
//!
//! Not strictly a port from npkill (their TUI does live filtering inline),
//! but ports the user-visible semantics: match if the query is empty, else
//! the lowercased path contains the lowercased query as a substring.

use crate::core::types::FolderResult;

/// Returns `true` if `item.path` matches `query` (case-insensitive substring).
/// An empty `query` matches everything.
pub fn matches_filter(item: &FolderResult, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    item.path.to_string_lossy().to_lowercase().contains(&query.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::ScanFoundFolder;
    use std::path::PathBuf;

    fn r(path: &str) -> FolderResult {
        FolderResult::from_scan(ScanFoundFolder::new(PathBuf::from(path), None))
    }

    #[test]
    fn empty_query_matches_anything() {
        assert!(matches_filter(&r("/x/node_modules"), ""));
    }

    #[test]
    fn substring_match_is_case_insensitive() {
        assert!(matches_filter(&r("/x/FOO/node_modules"), "foo"));
        assert!(matches_filter(&r("/x/foo/node_modules"), "FOO"));
    }

    #[test]
    fn substring_match_at_any_position() {
        assert!(matches_filter(&r("/projects/aaa/node_modules"), "aaa"));
        assert!(matches_filter(&r("/projects/aaa/node_modules"), "projects"));
    }

    #[test]
    fn no_match_returns_false() {
        assert!(!matches_filter(&r("/x/node_modules"), "rust-toolchain"));
    }
}
