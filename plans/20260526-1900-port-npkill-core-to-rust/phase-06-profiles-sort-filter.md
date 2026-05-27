# Phase 06 — Profiles + sort + filter

## Context

Three small but UX-critical features:
- **Profiles**: port `BASE_PROFILES` from `src/core/constants/profiles.constants.ts` (17 profiles: node, python, rust, java, android, swift, dotnet, ruby, elixir, haskell, scala, cpp, unity, unreal, godot, infra, data-science, plus auto-generated `all`).
- **Sort**: port `FOLDER_SORT` from `src/constants/sort.result.ts` (path, size, age).
- **Filter**: live substring filter on result path (TUI feature, not in source explicitly — small addition).

Decision C4 (locked): hardcoded only in v1.

## Priority

P1 — required for usability and decision-quality from user perspective.

## Status

completed (2026-05-27)

## Requirements

- `pub static BASE_PROFILES: phf::Map<&str, Profile>` — content identical to source
- `pub fn resolve_targets(profile_names: &[&str]) -> Vec<String>` — deduped union
- `pub fn sort_results(items: &mut [FolderResult], by: SortBy)` — comparator matching source semantics
- `pub fn matches_filter(item: &FolderResult, query: &str) -> bool` — case-insensitive substring on path

## Architecture

```rust
// src/core/profiles.rs
pub struct Profile {
    pub description: &'static str,
    pub targets: &'static [&'static str],
}

pub const DEFAULT_PROFILE: &str = "node";

pub fn base_profiles() -> &'static HashMap<&'static str, Profile> {
    static M: OnceLock<HashMap<&'static str, Profile>> = OnceLock::new();
    M.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert("node", Profile { description: "...", targets: &["node_modules", ".npm", ".pnpm-store", ".next", ...] });
        m.insert("python", Profile { description: "...", targets: &["__pycache__", ".pytest_cache", ...] });
        // ... all 17 from source
        m
    })
}

pub fn resolve_targets(names: &[&str]) -> Vec<String> {
    let profiles = base_profiles();
    let mut set = BTreeSet::new();
    for n in names {
        if *n == "all" {
            for p in profiles.values() {
                for t in p.targets { set.insert(t.to_string()); }
            }
        } else if let Some(p) = profiles.get(n) {
            for t in p.targets { set.insert(t.to_string()); }
        }
    }
    set.into_iter().collect()
}
```

```rust
// src/core/sort.rs
pub fn sort_results(items: &mut [FolderResult], by: SortBy) {
    items.sort_by(|a, b| match by {
        SortBy::Path => a.path.cmp(&b.path),
        SortBy::Size => match (a.size_bytes, b.size_bytes) {
            (Some(x), Some(y)) if x != y => y.cmp(&x),     // desc
            _ => a.path.cmp(&b.path),                       // tiebreak
        },
        SortBy::Age => match (a.last_modified, b.last_modified) {
            (None, Some(_)) => Ordering::Greater,           // null last
            (Some(_), None) => Ordering::Less,
            (None, None) => a.path.cmp(&b.path),
            (Some(x), Some(y)) if x == y => a.path.cmp(&b.path),
            (Some(x), Some(y)) => x.cmp(&y),                // older first
        },
    });
}
```

```rust
// src/core/filter.rs
pub fn matches_filter(item: &FolderResult, query: &str) -> bool {
    if query.is_empty() { return true; }
    item.path.to_string_lossy().to_lowercase().contains(&query.to_lowercase())
}
```

## Files to create

- `src/core/profiles.rs` (~200 LoC of mostly data)
- `src/core/sort.rs` (~50 LoC)
- `src/core/filter.rs` (~15 LoC)
- `tests/profiles_sort_filter.rs`

## Files to modify

- `src/core/mod.rs` — `pub mod profiles; pub mod sort; pub mod filter;`

## Implementation steps

1. Copy all 17 profile blocks from `profiles.constants.ts` verbatim into `profiles.rs`. Preserve target order from source.
2. Implement `resolve_targets` with `BTreeSet` dedup. Handle `"all"` specially.
3. Implement `sort_results` for the three modes. **Match source comparator semantics exactly**:
   - Path: ascending
   - Size: **descending**, tiebreak by path asc
   - Age: ascending (older first), nulls last, tiebreak by path asc
4. Implement `matches_filter` (case-insensitive substring).
5. Tests:
   - `resolve_targets(&["node"])` returns >= `["node_modules", ".npm", ...]`
   - `resolve_targets(&["all"])` returns deduped union of all profiles
   - sort by size: [10MB, 5MB, 20MB] → [20MB, 10MB, 5MB]
   - sort by age: items with `None` mtime sorted to end
   - filter: `"foo"` matches `/x/foo/node_modules` and `/x/FOO/node_modules`

## Todo

- [ ] All 17 profiles ported from source
- [ ] `resolve_targets` with dedup + `"all"` handling
- [ ] `sort_results` for path/size/age with null handling
- [ ] `matches_filter` case-insensitive
- [ ] Tests: profile resolution
- [ ] Tests: sort comparators (3 modes, with null mtime)
- [ ] Tests: filter case-insensitive

## Success criteria

- Profile content matches source line-for-line
- Sort comparators behave identically to npkill (verified by parallel test vectors)
- All tests pass

## Risks

| Risk | Mitigation |
|---|---|
| Typo in profile target lists causes silent miss | copy via clipboard, run grep diff against source after |
| Sort null-handling drift | explicit unit test per branch of `match` |

## Security considerations

None.

## Next steps

Phase 07 (TUI) reads from these modules. CLI in Phase 08 lets user pick profiles via `--profile <name>` (repeatable).
