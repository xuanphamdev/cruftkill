//! Safe-delete guard — prevents deleting a path whose basename is not in the
//! user-configured target list.
//!
//! Port of `isSafeToDelete` from npkill's `src/utils/is-safe-to-delete.ts`.

use std::path::Path;

/// Returns `true` only when the basename of `path` exactly matches one of the
/// `targets` strings.
///
/// Returns `false` when:
/// - `path` has no basename component (empty or root-only path).
/// - `targets` is empty.
/// - The basename is not in `targets`.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use cruftkill::core::safe_delete::is_safe_to_delete;
///
/// assert!(is_safe_to_delete(Path::new("/x/node_modules"), &["node_modules".to_string()]));
/// assert!(!is_safe_to_delete(Path::new("/x/.cache"), &["node_modules".to_string()]));
/// ```
pub fn is_safe_to_delete(path: &Path, targets: &[String]) -> bool {
    let Some(base) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    targets.iter().any(|t| t == base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_exact_target() {
        let targets = vec!["node_modules".to_string()];
        assert!(is_safe_to_delete(Path::new("/x/node_modules"), &targets));
    }

    #[test]
    fn rejects_non_target_basename() {
        let targets = vec!["node_modules".to_string()];
        assert!(!is_safe_to_delete(Path::new("/x/.cache"), &targets));
    }

    #[test]
    fn rejects_empty_path() {
        let targets = vec!["node_modules".to_string()];
        assert!(!is_safe_to_delete(Path::new(""), &targets));
    }

    #[test]
    fn rejects_empty_targets() {
        assert!(!is_safe_to_delete(Path::new("/x/node_modules"), &[]));
    }

    #[test]
    fn matches_one_of_multiple_targets() {
        let targets = vec!["venv".to_string(), "node_modules".to_string(), ".gradle".to_string()];
        assert!(is_safe_to_delete(Path::new("/x/venv"), &targets));
        assert!(is_safe_to_delete(Path::new("/x/.gradle"), &targets));
    }
}
