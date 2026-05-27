//! Directory names the scanner refuses to descend into.
//!
//! Ported verbatim from npkill's `src/core/constants/global-ignored.constants.ts`.
//! Rule: if a directory name appears in this set AND is NOT itself a target,
//! the scanner skips recursion into it. A name in this set CAN still be a
//! target — that case wins (so a profile that lists `node_modules` will still
//! match `node_modules` directories).

use std::collections::HashSet;
use std::sync::OnceLock;

/// Returns the shared global-ignore set. Lazily initialized.
pub fn global_ignore() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        let entries: &[&'static str] = &[
            // Version controls
            ".git",
            ".svn",
            ".hg",
            ".fossil",
            // System folders
            ".Trash",
            ".Trashes",
            "System Volume Information",
            ".Spotlight-V100",
            ".fseventsd",
            // Tools and environment
            ".nvm",
            ".rvm",
            ".rustup",
            ".pyenv",
            ".rbenv",
            ".asdf",
            ".deno",
            // IDEs
            ".vscode",
            ".idea",
            ".vs",
            ".settings",
            // Other
            "snap",
            ".flatpak-info",
            // Heavy
            "node_modules",
            "__pycache__",
            "target",
            "build",
            "dist",
            ".cache",
            ".venv",
            "venv",
        ];
        entries.iter().copied().collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_common_ignores() {
        let s = global_ignore();
        assert!(s.contains("node_modules"));
        assert!(s.contains(".git"));
        assert!(s.contains("__pycache__"));
        assert!(s.contains("target"));
    }

    #[test]
    fn does_not_contain_arbitrary_name() {
        let s = global_ignore();
        assert!(!s.contains("my-project"));
        assert!(!s.contains("README.md"));
    }

    #[test]
    fn instance_is_stable_across_calls() {
        let a = global_ignore() as *const _;
        let b = global_ignore() as *const _;
        assert_eq!(a, b);
    }
}
