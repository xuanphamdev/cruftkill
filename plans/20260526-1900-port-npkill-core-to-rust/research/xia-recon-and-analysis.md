# xia Phase 1–3: Recon, Map, Analyze

## Source manifest

- repo: `voidcosmos/npkill`
- commit: `2dad63647fdd6887e9022c8d22887fe5606eb92f` (main, 2026-05-16)
- license: MIT (compatible with port)
- language: TypeScript (Node.js, ESM, worker_threads + RxJS)
- core code analyzed: ~2,170 LoC across 19 files in `src/core/**`, `src/utils/**`, `src/constants/**`, `src/cli/services/scan.service.ts`
- features in scope: scan, delete, size, risky detection, sort/filter
- features out of scope (TUI presentation, update checker, CLI argv parser — re-implemented Rust-side)

## Source map — core boundaries

```
src/core/                           <-- library surface (port target)
├── npkill.ts                       facade implementing NpkillInterface
├── interfaces/
│   ├── npkill.interface.ts         public API contract
│   ├── folder.interface.ts         ScanFoundFolder, ScanOptions, DeleteResult, RiskAnalysis, SortBy
│   ├── file-service.interface.ts   IFileService (internal)
│   └── search-status.model.ts      ScanStatus (counters)
├── services/files/
│   ├── files.service.ts            abstract FileService — risk analysis, validation, getRecentModification
│   ├── files.worker.service.ts     thread pool manager — round-robin job dispatch, lifecycle
│   ├── files.worker.ts             walker logic running INSIDE each worker thread
│   ├── unix-files.service.ts       deleteDir → execFile('rm', ['-rf', path])
│   └── windows-files.service.ts    deleteDir → fs.rm(path, {recursive: true, force: true})
├── services/stream.service.ts      thin RxJS Subject wrapper
└── constants/
    ├── global-ignored.constants.ts GLOBAL_IGNORE set (skip .git, .Trash, node_modules-as-dir, etc.)
    └── profiles.constants.ts       BASE_PROFILES (node, python, rust, java, …)
src/utils/
├── is-safe-to-delete.ts            basename ∈ targets check
└── unit-conversions.ts             bytes ↔ KB/MB/GB, formatSize
src/constants/sort.result.ts        FOLDER_SORT comparators (path|size|age)
src/constants/workers.constants.ts  MAX_WORKERS=8, MAX_PROCS=100, EVENTS enum
```

Clean boundary: `src/core/` is a self-contained library. `src/cli/` is purely UI consumer. Port can target core only.

## Component inventory

| # | Component | Purpose | Source file | LoC |
|---|---|---|---|---|
| 1 | Facade API | streams of results, getSize, delete, validate | npkill.ts | 201 |
| 2 | Scan options + result types | data contracts | folder.interface.ts | 100 |
| 3 | Walker pool orchestrator | spawn workers, round-robin dispatch, lifecycle, getFolderSize timeout | files.worker.service.ts | 315 |
| 4 | Walker (per-thread) | readdir loop, target match, size collector w/ refcount | files.worker.ts | 379 |
| 5 | Risk analyzer | platform-aware path classification | files.service.ts:isDangerous | ~115 |
| 6 | Root validation | exists + is_dir + readable | files.service.ts:isValidRootFolder | ~25 |
| 7 | Recent modification | recursive readdir ignoring node_modules/.git/coverage/dist | files.service.ts:getFileStatsInDir | ~40 |
| 8 | Delete (Unix) | execs `rm -rf` | unix-files.service.ts | 26 |
| 9 | Delete (Windows) | `fs.rm` recursive | windows-files.service.ts | 20 |
| 10 | Global ignore | hardcoded set of dirs never recursed into | global-ignored.constants.ts | 49 |
| 11 | Profiles | predefined target groups (node, python, rust, etc.) | profiles.constants.ts | 160 |
| 12 | Sort comparators | path / size / age (with null handling) | sort.result.ts | 27 |
| 13 | Size formatting | bytes → MB/GB string | unit-conversions.ts | 70 |
| 14 | Safe-delete guard | basename must equal a target | is-safe-to-delete.ts | 10 |
| 15 | Worker constants | MAX_WORKERS=8, MAX_PROCS=100 | workers.constants.ts | 15 |

## Dependency matrix — TS → Rust

| Capability | Node/TS impl | Rust crate / std | Notes |
|---|---|---|---|
| Recursive parallel walk | worker_threads × N + per-worker async readdir queue (MAX_PROCS=100) | `ignore::WalkBuilder` (parallel) **or** `walkdir` + `rayon` | `ignore` already does parallel walking + gitignore filtering used by ripgrep. Recommended. |
| Thread pool | manual Worker[] + MessageChannel + round-robin | `rayon` global pool **or** `tokio::runtime` workers | `rayon` for CPU/FS-bound; `tokio` for async I/O streaming |
| Reactive stream → consumer | RxJS `Subject<T>` | `tokio::sync::mpsc::channel::<T>` → wrap as `Stream` | one-to-one mapping |
| Cancellation | `shouldStop` bool + `worker.terminate()` | `tokio_util::sync::CancellationToken` **or** `Arc<AtomicBool>` |  |
| FS metadata | `fs.stat`, `fs.lstat`, `dirent` | `std::fs::metadata`, `symlink_metadata`, `read_dir` |  |
| Disk usage (Unix true blocks) | `stats.blocks * 512` | `std::os::unix::fs::MetadataExt::blocks() * 512` | gated by `#[cfg(unix)]` |
| Disk usage (Windows logical) | `stats.size` | `metadata.len()` | gated by `#[cfg(windows)]` |
| Delete (Unix) | `execFile('rm','-rf',path)` | `std::fs::remove_dir_all(path)` | std works everywhere; no shell-out needed |
| Delete (Windows) | `fs.rm(path, {recursive,force})` | `std::fs::remove_dir_all(path)` | same as Unix |
| Symlink check | `dirent.isSymbolicLink()` | `Metadata::file_type().is_symlink()` |  |
| Path normalize | `path.resolve`, manual lowercase | `std::path::Path` + `dunce` (Windows UNC) | `path-clean` crate optional |
| TUI render | (the user's choice was ratatui) | `ratatui` + `crossterm` | ratatui = de facto Rust TUI |
| Args parser | `commander`/manual | `clap` (derive) | standard |
| Config (npkillrc) | JSON via fs.readFile | `serde_json` or `toml` + `serde` | TOML more idiomatic for Rust binaries |
| Logger | custom in-mem + getLog$ | `tracing` + `tracing-subscriber` + ring-buffer layer for in-UI tail |  |
| Sort | `Array.sort(cmp)` | `Vec::sort_by` | direct |
| Date/time | `Date.now()` | `std::time::Instant` / `SystemTime` |  |

### Cargo deps (proposed)

```toml
[dependencies]
ratatui = "0.29"
crossterm = "0.28"
clap = { version = "4", features = ["derive"] }
ignore = "0.4"           # parallel walker (used by ripgrep)
tokio = { version = "1", features = ["rt-multi-thread","macros","sync","time"] }
tokio-util = "0.7"       # CancellationToken
crossbeam-channel = "0.5" # non-async channel for walker → UI thread
serde = { version = "1", features = ["derive"] }
toml = "0.8"             # user config (~/.config/nmk/config.toml)
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow = "1"
thiserror = "1"
humansize = "2"          # bytes → human readable
dunce = "1"              # Windows path canonicalization
chrono = { version = "0.4", features = ["clock"] } # mtime formatting (optional)

[target.'cfg(unix)'.dependencies]
# nothing extra — std::os::unix is enough

[dev-dependencies]
tempfile = "3"
assert_fs = "1"
predicates = "3"
```

## Core algorithm analysis

### 1. Target detection (the heart)

`files.worker.ts:newDirEntry`:

```
for each dirent in dir:
  skip if symlink or not directory
  isTarget = searchConfig.targets.includes(entry.name)   // exact basename match
  if GLOBAL_IGNORE has entry.name AND !isTarget: skip    // don't recurse into ignored unless they ARE the target
  subpath = join(dir, entry.name)
  if exclude.some(ex => subpath.includes(ex)): skip      // substring match!
  emit { path: subpath, isTarget }
```

Implicit contracts:
- **Targets are matched by basename only** (`node_modules` not `**/node_modules`)
- **Exclusions are substring matches** (`/work/` excludes anything under `/work/`)
- **GLOBAL_IGNORE wins unless the ignored name is itself a target** (so `node_modules` inside `node_modules` would still be reported — but only if it appears at all; in practice ripple effect because walker never descends into matched targets either)
- Walker does NOT descend into targets (target found → emit, do not enqueue subpath as new explore task — looking at orchestrator code: `if isTarget: stream$.next(path)` (emit) `else if !shouldStop: addJob(explore, path)`). **This is the most important invariant**: matched node_modules is reported but its contents are not walked.

### 2. Parallel walk with backpressure

`files.worker.service.ts`:
- `MAX_WORKERS = 8` threads, `MAX_PROCS = 100` concurrent dir reads per worker.
- Round-robin dispatch: `tunnel[index].postMessage(job); index = (index+1) % workers`
- Per-worker pending count tracked → `getPendingJobs()` returns sum across workers
- Complete = pendingJobs reaches 0

Rust translation: `rayon::ThreadPoolBuilder::num_threads(8).build()` + `crossbeam_channel::bounded(...)` for jobs; OR more idiomatic: `ignore::WalkBuilder::new(root).threads(8).build_parallel()` — that already handles all the orchestration including backpressure.

### 3. Size calculation (parallel + refcounted collector)

`files.worker.ts:runGetFolderSizeChild`:

```
collector = { total: 0, pending: 1, onComplete: callback }
for chunk of 100 entries:
  parallel:
    if symlink: skip
    elif dir: total += 4096; enqueue child (collector++)
    else: lstat; total += (blocks*512 || size)
collector.pending--; if pending==0: onComplete(total)
```

Implicit contracts:
- **Refcounted termination**: `pending` starts at 1 (the root task) and increments by N children before that root completes; only when all reach pending=0 does onComplete fire.
- **Symlinks excluded** (avoid infinite loops + double-counting)
- **Dir adds flat 4096 bytes** (approximate inode block) so empty dirs aren't free
- **Unix: blocks × 512** for real on-disk size; **Windows: stats.size** for logical
- 60s timeout per top-level folder size request (orchestrator-level)

Rust translation: `Arc<AtomicU64>` total + `Arc<AtomicUsize>` pending + `tokio::sync::Notify` or oneshot when pending==0. **Or simpler**: synchronous `rayon::iter::par_iter` recursive sum returning eagerly. The async refcounted approach is needed only because npkill streams. Since size is called per-result on demand, a blocking parallel walk per folder is perfectly fine in Rust.

### 4. Risk analysis (`isDangerous`)

Pure function. Branches:
1. Normalize path (lower, `\` → `/`, strip drive letter)
2. Compute `isInHome = path == HOME or starts with HOME/`
3. If inside HOME:
   - `.config/*` → sensitive (user config)
   - `.local/share/*` → sensitive
   - `.cache/*` → sensitive
   - `.npm` / `.pnpm` → safe (whitelist)
   - Any other top-level `.*` (hidden dotdir) → sensitive
4. Inside `/Applications/*.app/` → sensitive (macOS bundles)
5. UNC paths `\\server\share` with hidden segment → sensitive
6. Windows `AppData\Roaming` → sensitive
7. Windows `AppData\Local` → sensitive unless in `.cache|.npm|.pnpm`
8. `Program Files [ (x86) ]\` → sensitive
9. Else → `{ isSensitive: false }`

**Port verbatim**. Pure string manipulation. No FS access. Easy to unit-test (table-driven).

### 5. Sort comparators

`sort.result.ts`:
- `path`: lexicographic asc
- `size`: size desc, tiebreak by path asc
- `age`: modificationTime asc, **null-aware** (null mtime sorted last), tiebreak by path asc

Direct Rust translation in a `cmp::Ordering` chain.

### 6. Delete

Unix: `rm -rf` (subprocess) — chose execFile for speed on huge trees.
Windows: `fs.rm({recursive, force})`.

**Rust**: `std::fs::remove_dir_all` is the same level of force on both platforms. Some benchmarks show shelling out to `rm` is faster than walking via syscalls, but `remove_dir_all` is portable and avoids `rm` injection risks.

Safety contract: caller MUST verify path is contained in target root. npkill enforces this in the facade (`delete$`) — Rust port should match: `path.canonical().starts_with(target.canonical())` BEFORE delete.

### 7. Recent modification (`getFileStatsInDir`)

Recursive readdir; for each file collect mtime; ignore subdirs named `node_modules .git coverage dist` (hardcoded). Return file with max mtime.

Implicit: this is called per-result by the UI to show "Last modified X days ago" → must be fast. Currently sequential recursive. For Rust: use `ignore::WalkBuilder::new(path).filter_entry(skip-ignored).build_parallel()` and reduce-max on mtime.

## Cross-cutting concerns

| Concern | Source behavior | Rust note |
|---|---|---|
| Symlink loops | excluded via `isSymbolicLink()` check | same approach via `Metadata::file_type().is_symlink()` |
| Permission errors | swallowed (return [] / continue) | use `.ok()` or `match` and log via `tracing::debug!` |
| Path containment for delete | facade-level check that path ∈ targets root | enforce same in Rust facade; canonicalize both sides |
| Stop scan mid-run | shouldStop flag + worker.terminate() | `CancellationToken` checked in walker filter_entry + at result boundaries |
| Resource limits | hardcoded MAX_WORKERS/PROCS, no RAM heuristic (TODO in source) | parameterize via CLI flag, default = `num_cpus::get().min(8)` |
| Logging surface | in-memory `LogEntry[]` exposed via `getLog$()` for TUI tail | `tracing-subscriber` with custom layer pushing to `Arc<Mutex<VecDeque<String>>>` for TUI consumption |

## Behavioral invariants to preserve in port

1. **Targets matched by exact basename**, not glob (preserve simplicity, matches npkill semantics).
2. **Exclude is substring match**, not glob (intentional UX — paste-a-path UX).
3. **Walker does NOT descend into matched targets** (most important: stops at first `node_modules` and doesn't recurse into its `node_modules/foo/node_modules/`).
4. **GLOBAL_IGNORE excludes recursion but allows target match** (so a profile target named `.cache` is still detected).
5. **Symlinks are never followed**.
6. **Permission errors are silently skipped**, not propagated.
7. **Size on Unix uses blocks×512** (true disk usage), not logical file size.
8. **Directories themselves count 4096 bytes** in size calc.
9. **Risky detection is best-effort** — false positives acceptable; false negatives less so for `.config`, AppData, Program Files.
10. **Delete safety guard**: deletion path must be inside scan root (or the path itself was emitted by the scan).

## Risks and gaps for Rust port

| Risk | Severity | Mitigation |
|---|---|---|
| `ignore` crate skips hidden dirs by default | M | configure `.hidden(false)` and disable gitignore filters; provide own filter via `filter_entry` |
| Windows path comparison case-sensitivity | M | lower-case both sides for compare; preserve original for display |
| Permission denied on macOS Spotlight / SIP dirs | L | already in GLOBAL_IGNORE |
| `remove_dir_all` slower than `rm -rf` on huge node_modules | L–M | acceptable for v1; can add platform fast-path later |
| Risk analyzer regex behavior diverges in Rust | M | port literal Rust string ops, no regex unless necessary; table-driven tests |
| TUI flicker / non-Unicode terminals on Windows | M | crossterm handles this; test on Windows Terminal + cmd.exe |
| Cancellation latency (in-flight syscall can't be interrupted) | L | accept — same limitation as Node version |
| Recent-mod scan can be slow on large projects | M | parallelize via rayon + cap depth or file count |

## Files-to-create estimate (Rust crate)

```
nodemoduleskiller/
├── Cargo.toml
├── Cargo.lock                       (generated)
├── src/
│   ├── main.rs                      bin entry → cli::run
│   ├── cli.rs                       clap definitions, dispatch
│   ├── lib.rs                       crate exports
│   ├── core/
│   │   ├── mod.rs                   facade `Npkill` struct
│   │   ├── types.rs                 ScanOptions, ScanFoundFolder, RiskAnalysis, SortBy, DeleteResult
│   │   ├── scanner.rs               parallel walker → mpsc::Receiver<ScanFoundFolder>
│   │   ├── size.rs                  parallel folder size (blocks*512 on unix)
│   │   ├── delete.rs                safe delete with containment guard
│   │   ├── risk.rs                  isDangerous port (pure)
│   │   ├── recent.rs                getRecentModification
│   │   ├── profiles.rs              BASE_PROFILES const + ProfilesService
│   │   ├── ignore.rs                GLOBAL_IGNORE const set
│   │   ├── sort.rs                  comparators
│   │   └── log_buf.rs               in-memory ring buffer log subscriber for TUI
│   ├── tui/
│   │   ├── mod.rs                   ratatui app loop
│   │   ├── state.rs                 AppState (results, cursor, filter, sort, status)
│   │   ├── events.rs                key handling
│   │   ├── render/
│   │   │   ├── results.rs           results table
│   │   │   ├── header.rs            stats + path
│   │   │   ├── help.rs              keybindings panel
│   │   │   └── details.rs           selected result detail pane
│   │   └── theme.rs                 colors/style
│   └── config.rs                    optional ~/.config/nmk/config.toml
└── tests/
    ├── risk_table.rs                table-driven isDangerous tests
    ├── scanner_smoke.rs             scan a tempdir tree
    ├── size_smoke.rs                
    └── delete_guard.rs              containment guard tests
```

~20 source files, ~2,500–3,500 LoC Rust expected.

## Open questions for Phase 4 (Challenge)

See `challenge-decisions.md`.
