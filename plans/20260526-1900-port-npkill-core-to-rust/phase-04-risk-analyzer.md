# Phase 04 — Risk analyzer + safe-delete guard

## Context

Port two pure functions: `isDangerous` from `src/core/services/files/files.service.ts` (~115 LoC) and `isSafeToDelete` from `src/utils/is-safe-to-delete.ts`. No FS access — pure string and path logic. **No `regex` crate** (decision C5).

Can run in parallel with Phase 02.

## Priority

P0 — required by scanner (emits risk per result) and delete (containment guard).

## Status

completed

## Requirements

- `pub fn analyze(path: &Path) -> RiskAnalysis` — pure, no FS calls
- `pub fn is_safe_to_delete(path: &Path, targets: &[String]) -> bool` — basename ∈ targets
- Behavior IDENTICAL to npkill — pinned by table-driven test against npkill outputs

## Architecture

```rust
// src/core/risk.rs

pub fn analyze(original: &Path) -> RiskAnalysis {
    let abs = absolute_path(original);
    let normalized = normalize(&abs);
    let original_norm = normalize_str(&original.to_string_lossy());

    if let Some(home) = home_dir() {
        let home_norm = normalize(&home);
        if normalized == home_norm || normalized.starts_with(&format!("{}/", home_norm)) {
            let rel = normalized.strip_prefix(&home_norm).unwrap_or("");
            let rel = rel.trim_start_matches('/');

            if rel == ".config" || rel.starts_with(".config/") {
                return RiskAnalysis::sensitive("Contains user configuration data (~/.config)");
            }
            if rel == ".local/share" || rel.starts_with(".local/share/") {
                return RiskAnalysis::sensitive("User data folder (~/.local/share)");
            }
            if rel == ".cache" || rel.starts_with(".cache/") {
                return RiskAnalysis::sensitive("System-wide cache directory (~/.cache)");
            }
            // whitelist
            if rel.starts_with(".npm") && (rel.len() == 4 || rel.as_bytes()[4] == b'/') { return RiskAnalysis::safe(); }
            if rel.starts_with(".pnpm") && (rel.len() == 5 || rel.as_bytes()[5] == b'/') { return RiskAnalysis::safe(); }
            // top-level dotdir → sensitive
            let top = rel.split('/').next().unwrap_or("");
            if top.starts_with('.') && top != "." && top != ".." && !["", ".npm", ".pnpm"].contains(&top) {
                return RiskAnalysis::sensitive("Contains unsafe hidden folder");
            }
        }
    }

    // macOS .app bundles: /applications/<x>.app/
    if contains_app_bundle(&normalized) {
        return RiskAnalysis::sensitive("Inside macOS .app package");
    }
    // UNC hidden segment
    if original_norm.starts_with("//") && contains_hidden_segment(&original_norm) {
        return RiskAnalysis::sensitive("Hidden path in network share");
    }
    if normalized.contains("/appdata/roaming") {
        return RiskAnalysis::sensitive("Inside Windows AppData Roaming folder");
    }
    if normalized.contains("/appdata/local") {
        if contains_safe_appdata_local(&normalized) { return RiskAnalysis::safe(); }
        return RiskAnalysis::sensitive("Inside Windows AppData Local folder");
    }
    if is_program_files(&normalized) {
        return RiskAnalysis::sensitive("Inside Program Files folder");
    }
    RiskAnalysis::safe()
}
```

### Helper functions (pure, no regex)

```rust
fn normalize(p: &Path) -> String {
    let s = p.to_string_lossy().to_lowercase().replace('\\', "/");
    // strip drive letter "c:/" -> "/"
    if s.len() >= 3 && s.as_bytes()[1] == b':' && s.as_bytes()[2] == b'/'
       && s.as_bytes()[0].is_ascii_lowercase() {
        s[2..].to_string()
    } else { s }
}

fn contains_app_bundle(s: &str) -> bool {
    // matches /applications/<something>.app/
    let needle = "/applications/";
    if let Some(i) = s.find(needle) {
        let rest = &s[i + needle.len()..];
        if let Some(slash) = rest.find('/') {
            return rest[..slash].ends_with(".app");
        }
    }
    false
}

fn is_program_files(s: &str) -> bool {
    s.contains("program files/") || s.contains("program files (x86)/")
}

fn contains_safe_appdata_local(s: &str) -> bool {
    s.contains("/.cache/") || s.ends_with("/.cache")
    || s.contains("/.npm/")   || s.ends_with("/.npm")
    || s.contains("/.pnpm/")  || s.ends_with("/.pnpm")
}

fn contains_hidden_segment(s: &str) -> bool {
    // matches "/.X" segment
    s.split('/').any(|seg| seg.starts_with('.') && seg.len() > 1 && seg != "..")
}
```

### Safe-delete guard

```rust
// src/core/safe_delete.rs

pub fn is_safe_to_delete(path: &Path, targets: &[String]) -> bool {
    let Some(base) = path.file_name().and_then(|n| n.to_str()) else { return false };
    targets.iter().any(|t| t == base)
}
```

## Files to create

- `src/core/risk.rs` (~120 LoC)
- `src/core/safe_delete.rs` (~15 LoC)
- `tests/risk_table.rs` — table-driven tests with ≥20 cases

## Files to modify

- `src/core/mod.rs` — `pub mod risk; pub mod safe_delete;`

## Implementation steps

1. Implement `normalize`, `normalize_str` helpers — verify with unit tests.
2. Implement `analyze` with the branch order **identical to source** (order matters for short-circuit).
3. Implement `is_safe_to_delete`.
4. Build a table of test cases capturing npkill's exact output for:
   - `~/foo/node_modules` → safe
   - `~/.config/node_modules` → sensitive (user config)
   - `~/.local/share/node_modules` → sensitive
   - `~/.cache/node_modules` → sensitive
   - `~/.npm/foo/node_modules` → safe (whitelist)
   - `~/.pnpm/foo/node_modules` → safe
   - `~/.local/foo/node_modules` → sensitive (top-level dotdir)
   - `/Applications/Foo.app/Contents/node_modules` → sensitive
   - `C:\\Users\\X\\AppData\\Roaming\\node_modules` → sensitive
   - `C:\\Users\\X\\AppData\\Local\\node_modules` → sensitive
   - `C:\\Users\\X\\AppData\\Local\\.cache\\node_modules` → safe
   - `C:\\Program Files\\App\\node_modules` → sensitive
   - `C:\\Program Files (x86)\\App\\node_modules` → sensitive
   - `\\\\server\\share\\.config\\node_modules` → sensitive (UNC hidden)
   - `~/projects/foo/node_modules` → safe
   - `is_safe_to_delete("/x/node_modules", ["node_modules"])` → true
   - `is_safe_to_delete("/x/.cache", ["node_modules"])` → false
   - empty targets → false
   - `is_safe_to_delete("", &["node_modules"])` → false (empty basename)
5. Each test must `assert_eq!` BOTH `is_sensitive` AND `reason` text — text matches source verbatim.

## Todo

- [x] `normalize` + helpers
- [x] `analyze` with branch order matching source
- [x] `is_safe_to_delete`
- [x] Table-driven tests ≥ 20 cases (35 delivered)
- [x] All cases pass with reason strings matching source

## Success criteria

- All table tests pass
- `cargo clippy` clean
- No `regex` dependency added

## Risks

| Risk | Mitigation |
|---|---|
| Subtle string-op divergence from source regex behavior | for each case, also run the original npkill code (Node + ts-node) and capture output; pin to that output in test |
| Path separator differences across OS | both branches kept (forward + back slash normalized to `/`) |
| Drive letter handling | `c:` stripped only if lowercase a-z; uppercase already handled by `to_lowercase` |

## Security considerations

False negative for sensitive paths is the real risk (we'd delete user data). The order of checks matters — keep identical to source so behavior is auditable.

## Next steps

Phase 02 wires `risk::analyze` into scanner output. Phase 05 wires `is_safe_to_delete` into delete guard.
