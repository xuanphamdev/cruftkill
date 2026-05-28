//! Table-driven tests for `risk::analyze_with_home` and `safe_delete::is_safe_to_delete`.
//!
//! Every `analyze` case uses `analyze_with_home` with a fixed synthetic home
//! (`/home/user`) so tests are hermetic and safe to run in parallel — no
//! process-env mutation required.
//!
//! Reason strings are asserted verbatim against npkill's TypeScript source to
//! catch any future drift between implementations.

use cruftkill::core::risk::analyze_with_home;
use cruftkill::core::safe_delete::is_safe_to_delete;
use std::path::Path;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Synthetic home directory used in all `analyze_with_home` calls.
fn home() -> &'static Path {
    Path::new("/home/user")
}

/// Assert that a path is classified as safe (not sensitive).
fn assert_safe(path: &str) {
    let result = analyze_with_home(Path::new(path), Some(home()));
    assert!(
        !result.is_sensitive,
        "expected SAFE for {path:?}, got sensitive with reason: {:?}",
        result.reason
    );
}

/// Assert that a path is classified as sensitive with the exact reason string.
fn assert_sensitive(path: &str, expected_reason: &str) {
    let result = analyze_with_home(Path::new(path), Some(home()));
    assert!(
        result.is_sensitive,
        "expected SENSITIVE for {path:?} with reason {expected_reason:?}, but got safe"
    );
    assert_eq!(result.reason.as_deref(), Some(expected_reason), "reason mismatch for {path:?}");
}

// ─── Table: HOME-relative paths ──────────────────────────────────────────────

/// 1. Normal project folder inside home → safe.
#[test]
fn home_project_folder_is_safe() {
    assert_safe("/home/user/foo/node_modules");
}

/// 2. ~/.config/node_modules → sensitive (user configuration data).
#[test]
fn home_config_is_sensitive() {
    assert_sensitive(
        "/home/user/.config/node_modules",
        "Contains user configuration data (~/.config)",
    );
}

/// 3. ~/.config itself → sensitive.
#[test]
fn home_config_exact_is_sensitive() {
    assert_sensitive("/home/user/.config", "Contains user configuration data (~/.config)");
}

/// 4. ~/.local/share/node_modules → sensitive (user data folder).
#[test]
fn home_local_share_is_sensitive() {
    assert_sensitive("/home/user/.local/share/node_modules", "User data folder (~/.local/share)");
}

/// 5. ~/.local/share exact → sensitive.
#[test]
fn home_local_share_exact_is_sensitive() {
    assert_sensitive("/home/user/.local/share", "User data folder (~/.local/share)");
}

/// 6. ~/.cache/node_modules → sensitive (system-wide cache).
#[test]
fn home_cache_is_sensitive() {
    assert_sensitive("/home/user/.cache/node_modules", "System-wide cache directory (~/.cache)");
}

/// 7. ~/.cache exact → sensitive.
#[test]
fn home_cache_exact_is_sensitive() {
    assert_sensitive("/home/user/.cache", "System-wide cache directory (~/.cache)");
}

/// 8. ~/.npm/foo/node_modules → safe (whitelisted package-manager cache).
#[test]
fn home_npm_whitelist_is_safe() {
    assert_safe("/home/user/.npm/foo/node_modules");
}

/// 9. ~/.pnpm/foo/node_modules → safe (whitelisted package-manager cache).
#[test]
fn home_pnpm_whitelist_is_safe() {
    assert_safe("/home/user/.pnpm/foo/node_modules");
}

/// 10. ~/.local/foo/node_modules → sensitive (top-level hidden dir other than whitelisted).
#[test]
fn home_local_non_share_is_sensitive() {
    assert_sensitive("/home/user/.local/foo/node_modules", "Contains unsafe hidden folder");
}

/// 11. ~/projects/foo/node_modules → safe (non-hidden subdirectory).
#[test]
fn home_projects_folder_is_safe() {
    assert_safe("/home/user/projects/foo/node_modules");
}

/// 12. A dotdir directly inside home that is not whitelisted → sensitive.
#[test]
fn home_arbitrary_dotdir_is_sensitive() {
    assert_sensitive("/home/user/.vscode/extensions/node_modules", "Contains unsafe hidden folder");
}

/// 13. ~/.npm exact (no trailing slash) → safe (whitelist boundary check).
#[test]
fn home_npm_exact_is_safe() {
    assert_safe("/home/user/.npm");
}

/// 14. ~/.pnpm exact → safe.
#[test]
fn home_pnpm_exact_is_safe() {
    assert_safe("/home/user/.pnpm");
}

// ─── Table: macOS .app bundles ────────────────────────────────────────────────

/// 15. /Applications/Foo.app/Contents/node_modules → sensitive.
#[test]
fn macos_app_bundle_is_sensitive() {
    assert_sensitive("/Applications/Foo.app/Contents/node_modules", "Inside macOS .app package");
}

/// 16. /Applications/Foo.app/Contents/Resources/node_modules → sensitive.
#[test]
fn macos_app_bundle_deep_is_sensitive() {
    assert_sensitive(
        "/Applications/Foo.app/Contents/Resources/node_modules",
        "Inside macOS .app package",
    );
}

// ─── Table: Windows AppData ───────────────────────────────────────────────────

/// 17. C:\Users\X\AppData\Roaming\node_modules → sensitive.
#[test]
fn windows_appdata_roaming_is_sensitive() {
    assert_sensitive(
        r"C:\Users\X\AppData\Roaming\node_modules",
        "Inside Windows AppData Roaming folder",
    );
}

/// 18. C:\Users\X\AppData\Local\node_modules → sensitive.
#[test]
fn windows_appdata_local_is_sensitive() {
    assert_sensitive(
        r"C:\Users\X\AppData\Local\node_modules",
        "Inside Windows AppData Local folder",
    );
}

/// 19. C:\Users\X\AppData\Local\.cache\node_modules → safe (whitelisted).
#[test]
fn windows_appdata_local_cache_is_safe() {
    assert_safe(r"C:\Users\X\AppData\Local\.cache\node_modules");
}

/// 20. C:\Program Files\App\node_modules → sensitive.
#[test]
fn windows_program_files_is_sensitive() {
    assert_sensitive(r"C:\Program Files\App\node_modules", "Inside Program Files folder");
}

/// 21. C:\Program Files (x86)\App\node_modules → sensitive.
#[test]
fn windows_program_files_x86_is_sensitive() {
    assert_sensitive(r"C:\Program Files (x86)\App\node_modules", "Inside Program Files folder");
}

// ─── Table: UNC (network share) paths ────────────────────────────────────────

/// 22. \\server\share\.config\node_modules → sensitive (hidden segment on UNC).
#[test]
fn unc_hidden_segment_is_sensitive() {
    assert_sensitive(r"\\server\share\.config\node_modules", "Hidden path in network share");
}

/// 23. \\server\share\projects\node_modules → safe (no hidden segment).
#[test]
fn unc_non_hidden_is_safe() {
    assert_safe(r"\\server\share\projects\node_modules");
}

// ─── Table: is_safe_to_delete ─────────────────────────────────────────────────

/// 24. Basename matches the single target → true.
#[test]
fn safe_delete_matching_target_returns_true() {
    assert!(is_safe_to_delete(Path::new("/x/node_modules"), &["node_modules".to_string()]));
}

/// 25. Basename does not match the target → false.
#[test]
fn safe_delete_non_matching_basename_returns_false() {
    assert!(!is_safe_to_delete(Path::new("/x/.cache"), &["node_modules".to_string()]));
}

/// 26. Empty path has no basename → false.
#[test]
fn safe_delete_empty_path_returns_false() {
    assert!(!is_safe_to_delete(Path::new(""), &["node_modules".to_string()]));
}

/// 27. Empty targets list → false even when path looks valid.
#[test]
fn safe_delete_empty_targets_returns_false() {
    assert!(!is_safe_to_delete(Path::new("/x/node_modules"), &[]));
}

/// 28. Basename matches one of several targets → true.
#[test]
fn safe_delete_matches_one_of_multiple_targets() {
    let targets = vec!["venv".to_string(), "node_modules".to_string(), "__pycache__".to_string()];
    assert!(is_safe_to_delete(Path::new("/x/__pycache__"), &targets));
}

/// 29. Root path has no meaningful basename → false.
#[test]
fn safe_delete_root_path_returns_false() {
    assert!(!is_safe_to_delete(Path::new("/"), &["node_modules".to_string()]));
}

// ─── Additional edge cases ────────────────────────────────────────────────────

/// 30. No home provided → home branch skipped entirely; normal project is safe.
#[test]
fn no_home_normal_path_is_safe() {
    let result = analyze_with_home(Path::new("/projects/foo/node_modules"), None);
    assert!(!result.is_sensitive);
}

/// 31. No home provided → AppData Roaming still detected (non-home branch).
#[test]
fn no_home_appdata_roaming_still_sensitive() {
    let result = analyze_with_home(Path::new(r"C:\Users\X\AppData\Roaming\node_modules"), None);
    assert!(result.is_sensitive);
    assert_eq!(result.reason.as_deref(), Some("Inside Windows AppData Roaming folder"));
}

/// 32. Path exactly equal to HOME → not sensitive (home itself is the boundary,
///     rel becomes "" which has no leading dot).
#[test]
fn path_equal_to_home_is_safe() {
    assert_safe("/home/user");
}

/// 33. ~/.npmrc is NOT under .npm (dot-segment is ".npmrc") → sensitive hidden folder.
#[test]
fn home_npmrc_dotfile_is_sensitive() {
    // .npmrc is a file-like name at top level of home; treated as hidden top-level entry.
    assert_sensitive("/home/user/.npmrc/node_modules", "Contains unsafe hidden folder");
}

/// 34. Windows AppData Local with .npm subdir → safe (whitelisted).
#[test]
fn windows_appdata_local_npm_is_safe() {
    assert_safe(r"C:\Users\X\AppData\Local\.npm\node_modules");
}

/// 35. Case-insensitive: APPLICATIONS on macOS (capital A) → still sensitive.
#[test]
fn macos_app_bundle_case_insensitive() {
    // normalize_str lowercases before matching.
    assert_sensitive("/APPLICATIONS/MyApp.app/Contents/node_modules", "Inside macOS .app package");
}
