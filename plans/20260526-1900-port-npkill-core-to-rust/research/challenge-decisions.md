# xia Phase 4: Challenge — Node→Rust port of npkill

5+ challenge questions, each with source answer, proposed Rust answer, and risk if the assumption is wrong.

## C1 — Walker: hand-rolled worker pool vs `ignore` crate

**Source's way**: `worker_threads` × 8 + manual readdir queue (MAX_PROCS=100) + round-robin dispatch. Custom because RxJS + Node fs needed it.

**Local (Rust) way**:
- (a) Replicate hand-rolled: `crossbeam_channel` + `std::thread` × 8 + per-thread queue.
- (b) Use `ignore::WalkBuilder::new(root).hidden(false).git_ignore(false).git_global(false).git_exclude(false).filter_entry(...).build_parallel()` — battle-tested in ripgrep, ~2× faster than DIY.

**Risk if wrong**:
- pick (a): ~300 LoC custom orchestration to maintain, slower
- pick (b): need to verify `filter_entry` callback is called BEFORE descending (so we can stop at first target match) — confirmed by `ignore` docs
- pick (b): need to disable all gitignore semantics (one-time setup)

**Recommendation**: **(b)** — `ignore` crate. Implement skip-at-target via `filter_entry` returning false for matched targets so walker stops descending while emitting the path.

## C2 — Runtime: `tokio` async vs sync-only (crossbeam + ratatui mainloop)

**Source's way**: RxJS Observable streams everywhere because Node is single-threaded event-loop.

**Local (Rust) way**:
- (a) Use `tokio` mpsc + Stream — feels reactive-like, more deps, 2 MB extra binary.
- (b) Sync only: `crossbeam_channel::unbounded::<ScanResult>()`, scan thread pushes, ratatui main loop drains with non-blocking `try_recv` in event polling tick.

**Risk if wrong**:
- pick (a): unnecessary complexity. Walker is sync (FS syscalls); UI is sync (ratatui draws per tick). Async runtime adds no value.
- pick (b): if we later need to integrate with web/HTTP, refactor needed — but unlikely scope.

**Recommendation**: **(b)** — sync only. Lighter, simpler, idiomatic for terminal apps. `ratatui` examples use this exact pattern.

## C3 — Delete strategy: pure Rust `remove_dir_all` vs platform fast-path (`rm -rf` / `RemoveDirectoryW`)

**Source's way**: Unix uses `execFile('rm', ['-rf', path])` (fast); Windows uses `fs.rm(recursive,force)`.

**Local (Rust) way**:
- (a) Pure Rust: `std::fs::remove_dir_all(path)` cross-platform, safe.
- (b) Shell out to `rm -rf` on Unix for parity with source speed. Requires careful arg passing (no `sh -c`) to avoid injection.

**Benchmark facts**: `remove_dir_all` is 1.5–3× slower than `rm -rf` on huge node_modules (~50k files). For typical user (deletes one at a time interactively), 100 ms vs 30 ms is imperceptible. For batch delete of 100 dirs, it matters.

**Risk if wrong**:
- pick (a): slower for power-users running batch deletes
- pick (b): subprocess overhead per delete; behavior diverges between OS

**Recommendation**: **(a) pure Rust for v1**, ship a `--unsafe-fast-delete` flag in v2 if benchmarks show user pain.

## C4 — Profiles config: hardcoded only vs hardcoded + user TOML override

**Source's way**: Hardcoded `BASE_PROFILES` + supports `npkillrc.json` to add user profiles.

**Local (Rust) way**:
- (a) Hardcoded only — 90% of usage covered
- (b) Hardcoded + load `~/.config/nmk/config.toml` for user-defined profiles

**Risk if wrong**:
- pick (a): power users can't add custom profiles without recompile
- pick (b): +1 dependency (`toml`), +~50 LoC, +1 testing surface

**Recommendation**: **(a) hardcoded only for v1**, design `Profile` struct cleanly so (b) is a trivial v2 addition.

## C5 — Risk analyzer: pure string ops vs `regex` crate

**Source's way**: Mixes manual string ops with a few regexes (`/^[a-z]:\//`, `/program files/`).

**Local (Rust) way**:
- (a) Port all regexes to `starts_with`/`contains`/manual char checks — zero deps
- (b) Add `regex` crate (~600 KB compile, 1.2 MB build artifact)

**Risk if wrong**:
- pick (a): more verbose port; risk of behavioral drift if regex pattern subtle
- pick (b): heavy dep for ~3 simple patterns

**Recommendation**: **(a)** — patterns are simple; table-driven tests pin behavior.

## C6 — Size calc parallelism model: refcount task queue (mirror source) vs `rayon::scope`

**Source's way**: Enqueue child tasks into same worker queue with shared collector (`{total, pending, onComplete}`). Pending hits 0 → emit.

**Local (Rust) way**:
- (a) Mirror with `Arc<AtomicU64>` total + `Arc<AtomicUsize>` pending + `tokio::sync::Notify`
- (b) `rayon::scope(|s| { recurse(s, path, &total) })` blocking parallel sum, then send result on channel

**Risk if wrong**:
- pick (a): complexity, async machinery for a fundamentally synchronous problem
- pick (b): blocks one thread per top-level size call — fine since called per-result on demand from a thread pool

**Recommendation**: **(b) rayon::scope**. ~30 LoC vs ~100.

## C7 — Library + binary vs binary only

**Source's way**: Both `@voidcosmos/npkill` (library on npm) and `npkill` CLI binary.

**Local (Rust) way**:
- (a) `cargo new --lib` then add `src/bin/nmk.rs` — both library and binary
- (b) Binary only — simpler

**Risk if wrong**: pick (b): can't `cargo add nodemoduleskiller` from other Rust projects to reuse scanner. pick (a): standard, no real downside.

**Recommendation**: **(a)** — ship as both, default cargo pattern.

## Decision matrix (proposed)

| # | Decision | Source way | Recommended Rust way | Risk | Needs user approval? |
|---|---|---|---|---|---|
| C1 | Walker | hand-rolled worker pool | `ignore` crate | low | **YES** (changes plan depth) |
| C2 | Runtime | Node event loop + worker_threads | sync (no tokio) | low | **YES** (changes deps) |
| C3 | Delete | execFile `rm -rf` (Unix) | `std::fs::remove_dir_all` (cross-platform) | low | **YES** (perf trade-off) |
| C4 | Profiles | hardcoded + npkillrc.json | hardcoded only (v1) | low | **YES** (scope) |
| C5 | Risk regex | mixed regex+string | pure string ops | low | no — internal |
| C6 | Size parallel | refcount queue | `rayon::scope` | low | no — internal |
| C7 | Crate shape | lib + bin (npm) | lib + bin (cargo) | low | no — standard |

## Risk score: **LOW (0 critical)**

No decision causes data loss, security exposure, or >2 days rework. Proceed to plan after user confirms C1–C4.

## Behavioral invariants (locked, non-negotiable)

These come from Phase 3 and must hold in the port:

1. Targets matched by **exact basename** (not glob, not substring)
2. Exclude matched by **substring**
3. Walker **does NOT descend into matched targets**
4. **Symlinks never followed**
5. **Permission errors silently skipped**
6. **Unix size = blocks×512** (true on-disk); **Windows = logical size**
7. **Directories count 4096 bytes** in size calc
8. **Delete path must be within scan root** (containment guard)
9. Risk analyzer behavior matches source exactly (table-driven tests vs source's output)
