//! Risk analyzer — pure path classification, no FS access.
//!
//! Port of `isDangerous` from npkill's `src/core/services/files/files.service.ts`.
//! Branch order is intentionally identical to the source so behavior is auditable.
//!
//! All decision logic lives in `analyze_with_home` (takes an explicit `Option<&Path>`
//! for hermetic, parallel-safe testing). The public `analyze` calls it with the real
//! env-derived home directory.

use crate::core::types::RiskAnalysis;
use std::path::Path;

// ─── Public API ──────────────────────────────────────────────────────────────

/// Classify a path as sensitive or safe.
///
/// Reads `HOME` (Unix) / `USERPROFILE` (Windows) from the environment to
/// determine the user's home directory. If neither is set, the home-relative
/// branch is skipped entirely.
///
/// This function performs **no filesystem I/O** — it is a pure string analysis.
pub fn analyze(path: &Path) -> RiskAnalysis {
    let home_str = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).ok();
    let home_path = home_str.as_deref().map(Path::new);
    analyze_with_home(path, home_path)
}

/// Pure, hermetic version — pass the home directory explicitly.
///
/// Use this in tests so they are not affected by the process environment and
/// can run safely in parallel.
pub fn analyze_with_home(path: &Path, home: Option<&Path>) -> RiskAnalysis {
    // Normalise to absolute if the caller hands us a relative path.
    // For UNC paths (\\server\share) we keep them as-is (same as npkill).
    let path_str = path.to_string_lossy();

    // `normalized` / `original_norm` = lowercase, backslashes → forward-slashes,
    // drive letter stripped. Both are derived from the same source string; kept as
    // separate bindings to mirror npkill's variable naming (`normalizedPath` vs
    // `normalizedOriginal`). After normalization they are identical because we
    // always feed the original string — the distinction is meaningful in npkill
    // only when the original is relative (it resolves to cwd). We always work
    // with the literal string, so one normalize pass suffices.
    let normalized = normalize_str(&path_str);
    let original_norm = normalized.clone();

    // ── HOME-relative branch ─────────────────────────────────────────────────
    if let Some(home_path) = home {
        let home_norm = normalize_str(&home_path.to_string_lossy());

        if !home_norm.is_empty()
            && (normalized == home_norm || normalized.starts_with(&format!("{home_norm}/")))
        {
            // Strip the home prefix and the leading slash.
            let rel = normalized.strip_prefix(&home_norm).unwrap_or("").trim_start_matches('/');

            // ~/.config — user configuration data
            if rel == ".config" || rel.starts_with(".config/") {
                return RiskAnalysis::sensitive("Contains user configuration data (~/.config)");
            }

            // ~/.local/share — user data folder
            if rel == ".local/share" || rel.starts_with(".local/share/") {
                return RiskAnalysis::sensitive("User data folder (~/.local/share)");
            }

            // ~/.cache — system-wide cache directory
            if rel == ".cache" || rel.starts_with(".cache/") {
                return RiskAnalysis::sensitive("System-wide cache directory (~/.cache)");
            }

            // Whitelisted package-manager caches inside HOME — safe to delete
            // Mirrors: /^\.(npm|pnpm)(\/|$)/.test(rel)
            if is_npm_or_pnpm_prefix(rel) {
                return RiskAnalysis::safe();
            }

            // Any other top-level dotdir inside HOME → sensitive
            let top = rel.split('/').next().unwrap_or("");
            if top.starts_with('.')
                && top != "."
                && top != ".."
                && top != ".npm"
                && top != ".pnpm"
                && !top.is_empty()
            {
                return RiskAnalysis::sensitive("Contains unsafe hidden folder");
            }
        }
    }

    // ── macOS .app bundles ───────────────────────────────────────────────────
    // Mirrors: /\/applications\/[^/]+\.app\//.test(normalizedPath)
    if contains_app_bundle(&normalized) {
        return RiskAnalysis::sensitive("Inside macOS .app package");
    }

    // ── Windows UNC network paths ────────────────────────────────────────────
    // After normalisation "\\server\share" → "//server/share"
    // Mirrors: normalizedOriginal.startsWith("//") && /\/\.[^/]+(\/|$)/.test(normalizedOriginal)
    if original_norm.starts_with("//") && contains_hidden_segment(&original_norm) {
        return RiskAnalysis::sensitive("Hidden path in network share");
    }

    // ── Windows AppData Roaming ──────────────────────────────────────────────
    if normalized.contains("/appdata/roaming") {
        return RiskAnalysis::sensitive("Inside Windows AppData Roaming folder");
    }

    // ── Windows AppData Local ────────────────────────────────────────────────
    if normalized.contains("/appdata/local") {
        // Whitelist: /\.(cache|npm|pnpm)(\/|$)/.test(normalizedPath)
        if contains_safe_appdata_local(&normalized) {
            return RiskAnalysis::safe();
        }
        return RiskAnalysis::sensitive("Inside Windows AppData Local folder");
    }

    // ── Windows Program Files ────────────────────────────────────────────────
    // Mirrors: /program files( \(x86\))?\//.test(normalizedPath)
    if is_program_files(&normalized) {
        return RiskAnalysis::sensitive("Inside Program Files folder");
    }

    RiskAnalysis::safe()
}

// ─── Private helpers (pure string ops, no regex crate) ───────────────────────

/// Lowercase, backslash → forward-slash, strip Windows drive letter prefix
/// (e.g. `c:/foo` → `/foo`).
fn normalize_str(s: &str) -> String {
    let lowered = s.to_lowercase().replace('\\', "/");

    // Strip "c:/" → "/"  (drive letter a-z followed by `:`)
    // After lowercasing the drive letter is always a-z.
    if lowered.len() >= 3 {
        let b = lowered.as_bytes();
        if b[0].is_ascii_lowercase() && b[1] == b':' && b[2] == b'/' {
            return lowered[2..].to_string();
        }
    }

    lowered
}

/// Returns `true` if `rel` (the path relative to HOME) begins with `.npm` or
/// `.pnpm` followed by `/` or end-of-string.
///
/// Mirrors: `/^\.(npm|pnpm)(\/|$)/.test(rel)`
fn is_npm_or_pnpm_prefix(rel: &str) -> bool {
    for prefix in [".npm", ".pnpm"] {
        if rel == prefix || rel.starts_with(&format!("{prefix}/")) {
            return true;
        }
    }
    false
}

/// Returns `true` when the path contains a `/applications/<name>.app/` segment.
///
/// Mirrors: `/\/applications\/[^/]+\.app\//.test(normalizedPath)`
fn contains_app_bundle(s: &str) -> bool {
    let needle = "/applications/";
    if let Some(i) = s.find(needle) {
        let rest = &s[i + needle.len()..];
        if let Some(slash) = rest.find('/') {
            return rest[..slash].ends_with(".app");
        }
    }
    false
}

/// Returns `true` when the path contains a hidden segment (starts with `.`,
/// more than one char, not `..`).
///
/// Mirrors: `/\/\.[^/]+(\/|$)/.test(normalizedOriginal)`
fn contains_hidden_segment(s: &str) -> bool {
    s.split('/').any(|seg| seg.starts_with('.') && seg.len() > 1 && seg != "..")
}

/// Returns `true` when a Windows `AppData/Local` path is whitelisted-safe.
///
/// Mirrors: `/\/\.(cache|npm|pnpm)(\/|$)/.test(normalizedPath)`
fn contains_safe_appdata_local(s: &str) -> bool {
    for name in ["/.cache", "/.npm", "/.pnpm"] {
        if s.contains(&format!("{name}/")) || s.ends_with(name) {
            return true;
        }
    }
    false
}

/// Returns `true` for paths inside `Program Files` or `Program Files (x86)`.
///
/// Mirrors: `/program files( \(x86\))?\//.test(normalizedPath)`
fn is_program_files(s: &str) -> bool {
    s.contains("program files/") || s.contains("program files (x86)/")
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_drive_letter() {
        assert_eq!(normalize_str("C:\\Users\\foo"), "/users/foo");
        assert_eq!(normalize_str("c:/Windows"), "/windows");
    }

    #[test]
    fn normalize_lowercases_and_converts_backslash() {
        assert_eq!(normalize_str("C:\\Foo\\Bar"), "/foo/bar");
    }

    #[test]
    fn npm_pnpm_prefix_exact_match() {
        assert!(is_npm_or_pnpm_prefix(".npm"));
        assert!(is_npm_or_pnpm_prefix(".pnpm"));
        assert!(is_npm_or_pnpm_prefix(".npm/cache"));
        assert!(is_npm_or_pnpm_prefix(".pnpm/store"));
        assert!(!is_npm_or_pnpm_prefix(".npmrc"));
        assert!(!is_npm_or_pnpm_prefix(".pnpmrc"));
    }

    #[test]
    fn app_bundle_detection() {
        assert!(contains_app_bundle("/applications/foo.app/contents/resources"));
        assert!(!contains_app_bundle("/applications/foo/bar"));
        assert!(!contains_app_bundle("/applications/foo.app")); // no trailing slash
    }

    #[test]
    fn safe_appdata_local_whitelist() {
        assert!(contains_safe_appdata_local("/appdata/local/.cache/x"));
        assert!(contains_safe_appdata_local("/appdata/local/.npm"));
        assert!(!contains_safe_appdata_local("/appdata/local/programs"));
    }
}
