# Phase 04 Review — Risk Analyzer + Safe-Delete Guard

Reviewer: code-reviewer
Date: 2026-05-27
Phase file: `plans/20260526-1900-port-npkill-core-to-rust/phase-04-risk-analyzer.md`
Subagent completion: `plans/20260526-1900-port-npkill-core-to-rust/reports/phase-04-completion.md`

## Scope

| File | LoC | Status |
|---|---|---|
| `src/core/risk.rs` | 241 | Reviewed |
| `src/core/safe_delete.rs` | 65 | Reviewed |
| `tests/risk_table.rs` | 280 | Reviewed |
| `src/core/mod.rs` | 15 | Wiring confirmed (`pub mod risk; pub mod safe_delete;`) |

## Gate Verification (independent)

| Command | Result |
|---|---|
| `cargo test` | 84/84 passed (6 suites) |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean (no warnings) |
| `cargo fmt --check` | clean |
| `cargo test --test risk_table` | 35/35 passed |

All three claimed gates reproduce. Pre-existing `useless_vec` clippy issue mentioned in
the completion report is no longer present — clippy is fully clean. The completion
report is slightly out of date on that point only.

## Behavioral Parity With npkill — Branch-by-Branch

| npkill branch (`files.service.ts`) | Rust analogue | Order | Reason verbatim | Status |
|---|---|---|---|---|
| `rel === '.config' \|\| rel.startsWith('.config/')` | same | 1 in HOME block | `"Contains user configuration data (~/.config)"` | match |
| `rel === '.local/share' \|\| rel.startsWith('.local/share/')` | same | 2 | `"User data folder (~/.local/share)"` | match |
| `rel === '.cache' \|\| rel.startsWith('.cache/')` | same | 3 | `"System-wide cache directory (~/.cache)"` | match |
| `/^\.(npm\|pnpm)(\/\|$)/.test(rel)` → safe | `is_npm_or_pnpm_prefix(rel)` | 4 | n/a (safe) | match |
| top-level dotdir except `.`/`..`/whitelist → sensitive | `top.starts_with('.') && top != "." && top != ".." && top != ".npm" && top != ".pnpm" && !top.is_empty()` | 5 | `"Contains unsafe hidden folder"` | match |
| `/\/applications\/[^/]+\.app\//.test(normalizedPath)` | `contains_app_bundle` | 6 | `"Inside macOS .app package"` | match |
| `normalizedOriginal.startsWith('//') && /\/\.[^/]+(\/\|$)/.test(...)` | `original_norm.starts_with("//") && contains_hidden_segment(...)` | 7 | `"Hidden path in network share"` | match (verified against Node regex on 7 edge inputs — identical) |
| `normalizedPath.includes('/appdata/roaming')` | same | 8 | `"Inside Windows AppData Roaming folder"` | match |
| `normalizedPath.includes('/appdata/local')` then `/\/\.(cache\|npm\|pnpm)(\/\|$)/` whitelist | same with `contains_safe_appdata_local` | 9 | `"Inside Windows AppData Local folder"` | match |
| `/program files( \(x86\))?\//.test(normalizedPath)` | `is_program_files` | 10 | `"Inside Program Files folder"` | match |

**Branch order matches the source line-for-line.** Short-circuit semantics are
preserved: `.config` fires before generic top-level dotdir, app-bundle fires before
UNC, AppData Local whitelist takes precedence over the parent sensitive branch.

### Reason-string diff vs source

All nine reason strings copied from `files.service.ts:128-193` are byte-identical
to the strings in `risk.rs:59,64,69,87,95,102,107,116,122`. No drift.

## Test Coverage vs Plan

Plan listed 18 specific cases. Rust port delivers **35**. Mapping (plan case → test):

| Plan case | Test | Present |
|---|---|---|
| `~/foo/node_modules` → safe | `home_project_folder_is_safe` | yes |
| `~/.config/node_modules` → sensitive | `home_config_is_sensitive` | yes |
| `~/.local/share/node_modules` → sensitive | `home_local_share_is_sensitive` | yes |
| `~/.cache/node_modules` → sensitive | `home_cache_is_sensitive` | yes |
| `~/.npm/foo/node_modules` → safe | `home_npm_whitelist_is_safe` | yes |
| `~/.pnpm/foo/node_modules` → safe | `home_pnpm_whitelist_is_safe` | yes |
| `~/.local/foo/node_modules` → sensitive | `home_local_non_share_is_sensitive` | yes |
| `/Applications/Foo.app/Contents/node_modules` → sensitive | `macos_app_bundle_is_sensitive` | yes |
| `C:\Users\X\AppData\Roaming\node_modules` → sensitive | `windows_appdata_roaming_is_sensitive` | yes |
| `C:\Users\X\AppData\Local\node_modules` → sensitive | `windows_appdata_local_is_sensitive` | yes |
| `C:\Users\X\AppData\Local\.cache\node_modules` → safe | `windows_appdata_local_cache_is_safe` | yes |
| `C:\Program Files\App\node_modules` → sensitive | `windows_program_files_is_sensitive` | yes |
| `C:\Program Files (x86)\App\node_modules` → sensitive | `windows_program_files_x86_is_sensitive` | yes |
| `\\server\share\.config\node_modules` → sensitive | `unc_hidden_segment_is_sensitive` | yes |
| `~/projects/foo/node_modules` → safe | `home_projects_folder_is_safe` | yes |
| `is_safe_to_delete("/x/node_modules", ["node_modules"])` → true | `safe_delete_matching_target_returns_true` | yes |
| `is_safe_to_delete("/x/.cache", ["node_modules"])` → false | `safe_delete_non_matching_basename_returns_false` | yes |
| `is_safe_to_delete("", ["node_modules"])` → false | `safe_delete_empty_path_returns_false` | yes |

All 18 plan cases are present. The additional 17 cover hardening:

- whitelist boundary cases (`~/.npm` exact, `~/.pnpm` exact, `.npmrc/...`)
- macOS deep paths and case-insensitivity (`/APPLICATIONS/...`)
- `analyze_with_home(_, None)` branch (HOME absent)
- root-only safe-delete (`/`)
- multiple-target safe-delete

## Edge Cases — Specifically Requested

| Case | Behaviour | Pass? |
|---|---|---|
| empty path `""` | `analyze_with_home(Path::new(""), Some(home))` → `is_sensitive=false`, no FS panic. `is_safe_to_delete(Path::new(""), …)` → false (test 26). | yes |
| root `/` | normalizes to `/`. Not under HOME `/home/user`. No appdata/applications match. → safe. Matches npkill. | yes |
| Windows drive root `C:\` | normalizes to `/`. Same as above. | yes |
| UNC root `\\server` | normalizes to `//server`. `starts_with("//")` true; `contains_hidden_segment` → false (no `.X` segment). → safe. Matches npkill regex. | yes |
| mixed slashes `C:\Users/X\AppData\Roaming` | backslash→slash gives unified path, drive stripped, lowercased, contains `/appdata/roaming` → sensitive. | yes |
| `..` segments | `contains_hidden_segment` explicitly excludes `..`. Top-level dotdir check also excludes `..`. → safe (no false positive). | yes |

I verified UNC hidden-segment behavior empirically by running both the JS regex
(`/\/\.[^/]+(\/\|$)/`) and the Rust `contains_hidden_segment` against 7 inputs —
they agree on every case (including `//.config/node_modules`, `//server/.hidden`,
`//server/share`, `//`).

## Findings

### CRITICAL
None.

### HIGH
None.

### MEDIUM

1. **HOME variable purity violation breaks hermeticity for `analyze()`.** The
   public `analyze()` reads `HOME`/`USERPROFILE` from `std::env::var`, which is
   process-global and not thread-safe to mutate. If the scanner runs multi-threaded
   and any other crate mutates env at runtime, results are non-deterministic.
   `analyze_with_home` (used by tests) is the pure form. Recommendation for
   Phase 02 wiring: the scanner should resolve HOME **once** at construction and
   pass it into `analyze_with_home` for every result, not call `analyze()`
   per-folder. This also avoids 1 env lookup per emitted folder. Not blocking.

2. **`.npm`/`.pnpm` whitelist matches BEFORE the AppData branches but not inside
   AppData Local.** This is intentional and matches npkill (the home-relative
   block returns early). Worth a sentence in the doc comment so a future reader
   doesn't try to "consolidate" the two whitelists. Style/maintenance.

### LOW

3. **`safe_delete` accepts `&[String]` instead of `&[impl AsRef<str>]`.** Forces
   callers to allocate `String`s even when they have `&str` config slices. Phase 05
   will be the first caller — it's worth checking the call site there. Not urgent.

4. **`tests/risk_table.rs` uses one `#[test]` function per row instead of a
   single parameterised `rstest` table.** 35 functions is more boilerplate but
   gives better failure isolation. Acceptable; the plan didn't mandate rstest.

### NIT

5. **Doc comment on `analyze`** says "no filesystem I/O" but the function does
   `std::env::var` which is technically a syscall. Tighten to "no path I/O" or
   "no FS open/read/stat calls". Cosmetic.

6. **`is_program_files`** uses two separate `contains(...)` calls — the second
   is a substring of the first (`"program files (x86)/"` contains `"program files"`
   but not `"program files/"`). Current code is correct; a single `s.contains("program files")
   && s.contains("/")` check would change semantics. Keep as is. No action.

## Security Implications

The primary failure mode is **false-negative on sensitive paths** (we classify
a sensitive folder as safe → user can delete real data). I walked every npkill
branch and confirmed every transition produces an identical Rust outcome. No
false-negative gap identified.

The reverse risk (**false-positive** flagging a normal folder as sensitive)
just shows a warning prompt — annoying but not destructive. The whitelist
boundary check (`is_npm_or_pnpm_prefix`) correctly rejects `.npmrc` and `.pnpmrc`
prefix matches (tests 33), preventing the most likely user-facing false-positive.

`is_safe_to_delete` is conservative by construction: empty basename, empty
targets, and root path all return false. Phase 05 must call this **before**
`fs::remove_dir_all` and abort on `false`.

## Phase 05 Readiness

No blockers. Phase 05 can wire:

```rust
if !is_safe_to_delete(&path, &options.targets) {
    return DeleteResult::fail(path, "refused: basename not in target list");
}
```

before `fs::remove_dir_all`. Recommend Phase 05 also assert the path is absolute
and contains no `..` after canonicalization, since `is_safe_to_delete` checks
basename only — a path like `/home/user/safe_target/../../../etc` has basename
`etc` which is not a target, so the current guard happens to catch it, but
defense-in-depth is cheap.

## Idiomatic Rust

- Module organisation by domain: ✓
- `let`-default immutability: ✓ (no `mut` outside loops)
- Borrowing over ownership in helpers (`&str`, `&Path`): ✓
- `Result`/`Option` over panics: ✓ (no `unwrap`/`expect` in production paths)
- Doc comments on every public item: ✓
- No `unsafe`: ✓
- No `regex` crate (decision C5): ✓
- Unit tests co-located with `#[cfg(test)] mod tests`: ✓

One nit: `for prefix in [".npm", ".pnpm"] { … }` in `is_npm_or_pnpm_prefix`
could be written more idiomatically with `["..."].iter().any(...)` but the
current form is fine and arguably clearer.

## Plan Task Status

All 5 todo items in `phase-04-risk-analyzer.md` are complete and verified:

- [x] `normalize` + helpers
- [x] `analyze` with branch order matching source
- [x] `is_safe_to_delete`
- [x] Table-driven tests ≥20 cases (35 delivered)
- [x] All cases pass with reason strings matching source

Recommend marking Phase 04 as **done** in `plan.md`.

## Recommended Actions

1. **Phase 05**: call `is_safe_to_delete` before `fs::remove_dir_all`; also gate on
   risk analysis if user hasn't explicitly confirmed override (per npkill UX).
2. **Phase 02 wiring**: resolve HOME once at scanner construction, pass to
   `analyze_with_home` per folder (MEDIUM #1).
3. **Optional**: relax `is_safe_to_delete` signature to `&[impl AsRef<str>]`
   (LOW #3) before Phase 05 cements the API.

## Unresolved Questions

None.

## Status

DONE
