# nodemoduleskiller (`nmk`)

[![CI](https://img.shields.io/badge/CI-pending-lightgrey)](.github/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](Cargo.toml)

A Rust port of [voidcosmos/npkill](https://github.com/voidcosmos/npkill) — find
and delete `node_modules` (and a few dozen other build-cache directories) from
your filesystem to free up disk space.

> ⚠️ **`nmk` deletes recursively without a recycle bin.** Always review the list
> before pressing `d`. Run with `--dry-run` first if you want to preview.

## Features (v0.1)

- 🔍 Parallel async directory scanner — finds matches as it walks
- 📏 Per-folder size calculation (true on-disk size on Unix via `blocks × 512`)
- 🛡️ Risk analyzer — flags paths inside `~/.config`, AppData, `/Applications/*.app`, etc.
- 🗑️ Safe delete with two-layer guard (basename + canonicalize containment)
- 🎯 17 hardcoded profiles: `node`, `python`, `rust`, `java`, `swift`, `dotnet`, … or pass `--profile all`
- 🖥️ Two UX modes:
  - **Interactive TUI** (ratatui): navigable list, delete-with-confirmation, live size updates
  - **`--no-tui` mode**: streams NDJSON for scripting / CI pipelines

## Install (from source)

```bash
git clone <this-repo>
cd nodemoduleskiller
cargo install --path .
nmk --help
```

Requires Rust 1.85+ (edition 2024).

## Usage

### Interactive TUI

```bash
nmk                            # scan current dir with `node` profile
nmk ~/Projects                 # scan a specific directory
nmk -p rust ~/code             # use the `rust` profile (matches `target/`)
nmk -p node -p python ~/code   # combine profiles
nmk --dry-run ~/Projects       # preview what would be deleted (modal shows badge)
```

In the TUI:

| Key | Action |
|---|---|
| `↑` / `k` | move cursor up |
| `↓` / `j` | move cursor down |
| `d` / `Space` / `Enter` | open delete confirm |
| `y` / `Y` | confirm delete |
| `n` / `N` / `Esc` | cancel modal |
| `q`, `Ctrl-C` | quit |

### Scriptable JSON output

```bash
nmk --no-tui ~/Projects | jq '. | select(.size_bytes > 100000000)'
```

Each line is a JSON object:

```json
{
  "path": "/home/me/proj-a/node_modules",
  "size_bytes": 314572800,
  "is_sensitive": false,
  "risk_reason": null,
  "modified_unix": 1779867789,
  "dry_run": false
}
```

When stdout is not a TTY (e.g. piped or in CI), `nmk` auto-falls-back to
`--no-tui` mode.

### Profiles

Run `nmk --help` or read [`src/core/profiles.rs`](src/core/profiles.rs) for the
full target list. Highlights:

| Profile | Matches |
|---|---|
| `node` (default) | `node_modules`, `.npm`, `.pnpm-store`, `.next`, `.nuxt`, `.turbo`, `.cache`, `coverage`, … |
| `python` | `__pycache__`, `.pytest_cache`, `.venv`, `.tox`, `.mypy_cache`, … |
| `rust` | `target` |
| `java` | `target`, `.gradle`, `out` |
| `swift` | `DerivedData`, `.swiftpm` |
| `dotnet` | `obj`, `TestResults`, `.vs` |
| `cpp` | `CMakeFiles`, `cmake-build-debug`, `cmake-build-release` |
| `infra` | `.serverless`, `.vercel`, `.netlify`, `.terraform`, … |
| `all` | union of every profile |

Add ad-hoc targets with `-t`:

```bash
nmk -p node -t my_custom_cache ~/code
```

## Safety model

Two layered guards run before any FS mutation:

1. **Basename guard** — the path's basename must appear in the resolved target
   list. Catches the "wrong path" mistake.
2. **Containment guard** — both the scan root and the target path are
   canonicalized (symlinks resolved). The canonical target must
   `starts_with` the canonical root. Catches symlink escape attacks even if
   the link is named like a target.

`std::fs::remove_dir_all` is hardened against symlink-traversal
([CVE-2022-21658](https://blog.rust-lang.org/2022/01/20/cve-2022-21658.html))
— Rust does not follow symlinks when removing a directory.

## Architecture

```
nmk binary (src/main.rs)
   │
   ├── TUI mode  ──►  src/tui/  (ratatui + crossterm + tokio::select!)
   │                     │
   └── --no-tui  ──►  src/main.rs::run_no_tui → NDJSON
                         │
                         ▼
                  src/core/
                     ├── scanner    (parallel tokio worker pool)
                     ├── size       (refcounted async sum, 60s timeout)
                     ├── risk       (pure string analyzer)
                     ├── safe_delete (basename guard)
                     ├── delete     (canonicalize + remove_dir_all)
                     ├── profiles   (17 profiles, resolve_targets)
                     ├── sort       (path / size / age comparators)
                     ├── filter     (case-insensitive substring)
                     ├── ignore     (GLOBAL_IGNORE set)
                     ├── types      (ScanOptions, FolderResult, …)
                     └── error      (NpkillError)
```

Full design walkthrough:
[`plans/20260526-1900-port-npkill-core-to-rust/research/xia-recon-and-analysis.md`](plans/20260526-1900-port-npkill-core-to-rust/research/xia-recon-and-analysis.md)

## Development

```bash
cargo test                                    # unit + integration tests (~140)
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo build --release                          # ./target/release/nmk
```

## Roadmap (post-v0.1)

- v0.2 ideas: live filter input (`/`), help overlay (`?`), in-UI sort cycle
  (`s`), detail pane, user-defined profiles via TOML, multi-select + bulk
  delete, fast-delete shell-out flag, recent-modification heuristic.

## Attribution

This project is a port of [voidcosmos/npkill](https://github.com/voidcosmos/npkill)
(© voidcosmos, MIT license). Many design decisions — target detection rules,
risk analysis heuristics, profile definitions, behavioural invariants — are
preserved verbatim. See
[`research/xia-recon-and-analysis.md`](plans/20260526-1900-port-npkill-core-to-rust/research/xia-recon-and-analysis.md)
for the full mapping and
[`research/challenge-decisions.md`](plans/20260526-1900-port-npkill-core-to-rust/research/challenge-decisions.md)
for documented deviations and their rationale.

## License

MIT — see [`LICENSE`](LICENSE).
