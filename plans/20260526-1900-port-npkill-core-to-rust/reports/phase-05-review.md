# Phase 05 Review — Safe Directory Deletion

Reviewer: code-reviewer
Date: 2026-05-27
Phase file: `plans/20260526-1900-port-npkill-core-to-rust/phase-05-delete.md`
Dependency: Phase 04 review at `reports/phase-04-review.md`

## Scope

| File | LoC | Status |
|---|---|---|
| `src/core/delete.rs` | 111 | Reviewed |
| `tests/delete_guard.rs` | 137 | Reviewed |
| `src/core/mod.rs` | 15 | Wiring confirmed (`pub mod delete;`) |

## Gate Verification (independent)

| Command | Result |
|---|---|
| `cargo test` | 94 / 94 passed (7 suites, 2.28s) |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| `cargo test --test delete_guard` | 7 / 7 passed |

All three gates reproduce.

## Plan Acceptance Criteria — Test-Case Map

| Plan case | Test | Present |
|---|---|---|
| delete a tempdir → success, dir gone | `deletes_a_real_node_modules_inside_root` | yes |
| `dry_run=true` → success, dir still exists | `dry_run_reports_success_but_does_not_touch_fs` | yes |
| basename NOT in targets → rejected | `rejects_basename_not_in_targets` | yes |
| path OUTSIDE scan_root → rejected | `rejects_path_outside_scan_root` | yes |
| nonexistent path → failure | `nonexistent_path_returns_failure_not_panic` | yes |
| symlink-traversal `root/escape -> /etc` → rejected | `symlink_escape_attempt_is_rejected` | yes |

All 6 plan cases present. The 7th test
(`symlink_pointing_outside_with_target_basename_rejected_by_containment`) is a
**critical hardening case** — it exercises the *only* path that exists in this
design where guard 1 passes and guard 2 is the sole barrier. Without it the
suite would silently accept a refactor that drops the containment check. Strongly
recommend keeping.

## Security Walkthrough — Guard-by-Guard

### Guard 1 (basename in targets)

Delegates to Phase 04 `is_safe_to_delete`. Behaviour verified empirically:

| Input | `file_name()` | Outcome |
|---|---|---|
| `/` | `None` | rejected |
| `.`, `..` | `None` | rejected |
| `/x/node_modules` | `Some("node_modules")` | accepted if in targets |
| `/x/node_modules/` (trailing slash) | `Some("node_modules")` | accepted |
| empty `""` | `None` | rejected |

No bypass found. Empty `targets` slice also rejects (verified by inline unit
test `empty_targets_always_rejects`).

### Guard 2 (containment via canonicalize + `starts_with`)

Empirically probed on this macOS host:

- **`..` segments**: a path like `tmp_root/../outside` correctly canonicalizes
  to the resolved outside dir; `starts_with(canon_root)` returns `false`.
  Rejected.
- **Symlink escape (basename matches target)**: `root/node_modules -> outside`
  canonicalizes to `outside`; `starts_with` returns false. Rejected by test 7.
- **`/private` prefix on macOS**: `canonicalize("/var/folders/X")` returns
  `/private/var/folders/X`. Because **both** `path` and `scan_root` are
  canonicalized, the prefixes always agree. No false-negative on macOS.
- **Nonexistent path**: canonicalize returns `Err`; mapped to
  `DeleteResult::fail("canonicalize failed: ...")`. No FS mutation.

### Guard 3 (the implicit one) — `remove_dir_all` symlink semantics

**Empirically confirmed on Rust 1.89 / macOS**: a symlink that lives **inside**
the canonical scan tree but **points outside** is treated as a regular dir
entry — `std::fs::remove_dir_all` unlinks the symlink itself; the target tree
and its files remain. Probe code created `/tmp/rm_probe_dir` containing
`evil_link -> /tmp/rm_probe_outside/{treasure.txt}` and confirmed
`treasure.txt` survived after `remove_dir_all`.

This matches Rust stdlib documentation since [Rust 1.51 / CVE-2022-21658
hardening] and is the correct behaviour. The doc comment on `delete.rs:14–18`
states this clearly.

## Attack Vectors Considered

| Vector | Status | Notes |
|---|---|---|
| symlink at `root/node_modules` → `/etc` | **caught** by guard 2 (test 7) | canonicalize resolves the link to `/etc`; `/etc` doesn't start with `<root>` |
| relative path with `..` (`root/foo/../../etc`) | **caught** by guard 2 | empirically verified; canonicalize resolves through `..`; outside path fails `starts_with`. *Missing as an explicit test* — see MEDIUM #1. |
| TOCTOU: symlink swapped between canonicalize and `remove_dir_all` | **theoretical race**, same in npkill | `remove_dir_all` re-traverses by name. An attacker with write access to the parent of `path` between the two calls *could* swap the leaf. But that attacker already has the privileges to do worse. Documented in plan risk table. Acceptable. |
| Windows `\\?\` extended-path prefix from canonicalize | **OK** | `Path::starts_with` compares component-wise, and both ends get the prefix from `canonicalize`, so they agree. Plan also calls out `dunce` — code uses bare `std::fs::canonicalize` instead (see MEDIUM #2). |
| Empty basename / root-only path / drive-root | **caught** by guard 1 | `file_name()` returns `None` for `/`, `.`, `..`, `""`, `C:\`; `is_safe_to_delete` rejects |
| Hardlink inside scan tree pointing to outside file | **N/A for dirs** | Unix hardlinks to directories are forbidden by the kernel. File-level hardlinks are harmless: `remove_dir_all` unlinks the in-tree name; outside name still references the same inode. |
| Symlink at `scan_root` itself | **OK** | `canon_root` is the resolved real path; `path` resolves to the same realpath subtree; `starts_with` works |
| Race: user deletes `scan_root` between root-canonicalize and path-canonicalize | benign | path-canonicalize will likely fail and return `DeleteResult::fail` |

No bypass identified.

## Findings

### CRITICAL

None.

### HIGH

None.

### MEDIUM

1. **Explicit `..`-traversal test is missing.** The plan asks for "relative
   path with `..` components" coverage (vector 2 above). The current symlink
   tests exercise guard 2 via a different mechanism. The behaviour is
   correct (empirically verified by the reviewer), but a regression in
   canonicalize handling, in a future Rust stdlib change, or in a refactor
   to a different canonicalizer would slip past CI. Suggested test:

   ```rust
   #[tokio::test(flavor = "multi_thread")]
   async fn rejects_dotdot_escape_to_outside_target() {
       let tmp_root = tempfile::tempdir().unwrap();
       let outside = tempfile::tempdir().unwrap();
       let _outside_target = make_tree(outside.path(), "node_modules");
       // path that uses `..` to escape root while keeping the right basename
       let escape = tmp_root.path().join("../").join(
           outside.path().file_name().unwrap()).join("node_modules");
       let res = delete::delete(&escape, tmp_root.path(), &targets(), false).await;
       assert!(!res.success);
   }
   ```

   Not blocking — file under "harden before Phase 07".

2. **Plan said `dunce::canonicalize`; code uses `std::fs::canonicalize`.**
   The phase file calls out `dunce` explicitly (lines 40, 44) "to avoid
   Windows `\\?\` extended-path surprises." The actual implementation uses
   bare `std::fs::canonicalize`. On macOS / Linux this is identical. On
   Windows the resulting `\\?\C:\…` prefix is **identical for both `path`
   and `scan_root`**, so `starts_with` still works — but downstream callers
   (Phase 07 TUI display) will see the `\\?\` prefix in error messages and
   may print ugly paths. Either:
   - swap to `dunce::canonicalize` (the plan's intent), or
   - drop `dunce` from `Cargo.toml` and update the plan to reflect the
     decision.

   Pick one and document. Currently the plan and code disagree.

3. **`DeleteResult::path` is `&Path` (caller-supplied) — not the canonical
   path that was actually deleted.** On success, `DeleteResult::ok(path)`
   echoes the caller's path. If the caller passes a symlinked path, the
   user has no record of *what* was actually unlinked vs what remained.
   For audit / UI display this is fine, but worth documenting. Consider
   recording `canon_path` instead, at least on success.

### LOW

4. **Dry-run randomness uses `subsec_nanos()` only.** That's <1 ms of
   entropy — fine for the TUI fake-feel, but a sequence of dry-runs fired
   in tight loop (e.g., in tests) will produce near-identical sleeps. The
   doc comment correctly calls it "not cryptographic." No action.

5. **`empty_targets_always_rejects` unit test asserts only `!success`** —
   doesn't assert the error message. Hardening: also assert the message
   contains `"target"` so a future refactor that fails earlier (e.g., at
   canonicalize, because `/tmp/foo` doesn't exist) doesn't silently flip
   the test from validating guard 1 to validating guard 2. Cosmetic.

6. **`spawn_blocking` result branches all map to string errors.** Lossy:
   `e.to_string()` on `std::io::Error` loses the `ErrorKind`, which a
   caller (Phase 07) might want for "is this a permission-denied?" UX.
   Consider returning a typed error in a future iteration. Acceptable now
   since `DeleteResult` is the published shape.

### NIT

7. **The `dry_run_against_nonexistent_path_still_rejected_at_canonicalize`
   inline test name is long.** Could be shortened to `dry_run_still_runs_guards`.

8. **`pb: PathBuf = canon_path;` annotation is redundant** — `canon_path`
   is already `PathBuf`. Pure style.

## Behavioral Questions Answered

### Q: Does `std::fs::remove_dir_all` follow symlinks?

**No.** Empirically confirmed on Rust 1.89 / macOS. The `evil_link -> outside`
probe left `outside/treasure.txt` untouched. The Rust docs since 1.51 specify
this behaviour as a hardening against CVE-2022-21658 (TOCTOU race in
`remove_dir_all`). Phase 05's safety depends on this guarantee being stable
across Rust versions — it is, but worth noting in the doc comment.

### Q: What does `DeleteResult` say on partial deletion mid-walk?

`remove_dir_all` propagates the **first** I/O error. Files deleted before the
error are gone; files after are not. `DeleteResult` returns `success=false`
with `error=Some("remove_dir_all failed: <io error>")`. The caller cannot
distinguish "rejected at guard" from "partially deleted before error". For
Phase 07 UX this matters — a partial delete should refresh the scan view,
a rejection should not. Recommend Phase 07 either:
- check `path.exists()` after a failed delete, or
- enrich `DeleteResult` with a `phase: enum { GuardRejected, PartialDelete,
  ... }` discriminant.

Not blocking for Phase 05. Flag for Phase 07.

### Q: Idiomatic Rust?

- Ownership: ✓ borrows `&Path`/`&[String]` in the public API; only allocates
  `PathBuf` when crossing the `spawn_blocking` boundary
- Error mapping: ✓ no panics, no `unwrap` (except inside the helper that's
  documented as best-effort)
- Naming: ✓ `snake_case` throughout; `delete` matches the npkill name
- No `unsafe`: ✓
- Doc comments on every public item: ✓
- `match` arms exhaustive: ✓
- Async correctness: ✓ `spawn_blocking` is the correct primitive for a
  potentially long-running sync FS call

One stylistic nit: `format!("canonicalize failed: {e}")` is repeated twice
(lines 51, 55). Could be DRY'd to a helper. Not worth the abstraction.

## Plan Task Status

All 6 todo items in `phase-05-delete.md` are complete:

- [x] `DeleteResult::ok/fail` constructors (in `types.rs`, Phase 01)
- [x] Guard 1: basename in targets
- [x] Guard 2: containment via canonicalize + starts_with
- [x] `dry_run` branch
- [x] real delete via `spawn_blocking`
- [x] All 6 test cases pass (+1 hardening test)

Recommend marking Phase 05 as **done** in `plan.md`.

## Phase 07 Readiness / Blocking Concerns

No blockers for Phase 07 (TUI delete UX). For Phase 07 to provide good UX,
consider these soft asks:

1. Resolve MEDIUM #2 (dunce vs std::fs::canonicalize) before Phase 07 so the
   TUI doesn't render `\\?\` paths to users on Windows.
2. Decide whether `DeleteResult` needs a phase discriminant (see partial-delete
   answer above). If yes, change it in Phase 05 — it's a public type and
   churning it from Phase 07 is more disruptive.
3. Phase 07 should call `is_safe_to_delete` itself before showing the "press
   d to delete" prompt, so the keybind is hidden when the basename is wrong.
   Defense in depth: the actual delete will re-check, but the UI should not
   *invite* a guaranteed-fail action.

## Recommended Actions

1. **Add the explicit `..`-traversal test** (MEDIUM #1).
2. **Pick `dunce` or `std::fs::canonicalize` and align plan + code + Cargo.toml**
   (MEDIUM #2). If keeping `std::fs::canonicalize`, also remove `dunce` from
   the dependency list to avoid dead deps.
3. **Annotate the doc comment** with the Rust 1.51 / CVE-2022-21658 reference
   to make the safety contract auditable by future readers.
4. **Defer**: typed `DeleteResult` error kind (LOW #6) — wait for Phase 07
   to drive the requirement.

## Unresolved Questions

- Should `DeleteResult::path` reflect the *requested* path (current) or the
  *canonical* path (more auditable)? Defer to Phase 07 UX feedback.

## Status

DONE_WITH_CONCERNS

Concerns: MEDIUM #1 (missing explicit `..`-traversal test) and MEDIUM #2
(plan vs code diverge on `dunce`). Neither is a security bug — behaviour is
correct on macOS and Linux today — but both should be resolved before Phase 07
seals the public delete API.
