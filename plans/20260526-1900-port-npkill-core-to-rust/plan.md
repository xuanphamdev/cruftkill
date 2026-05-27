# Plan: Port npkill core to Rust (`nodemoduleskiller` / `nmk`)

## Goal

Rewrite the core scanning, sizing, risky-detection, sort/filter, and delete features of [voidcosmos/npkill](https://github.com/voidcosmos/npkill) (TypeScript) idiomatically in Rust, plus an interactive TUI. License: MIT (compatible).

## Source

- repo: `voidcosmos/npkill` @ `2dad63647fdd6887e9022c8d22887fe5606eb92f` (2026-05-16)
- ~2,170 LoC core analyzed
- detailed recon: [`research/xia-recon-and-analysis.md`](research/xia-recon-and-analysis.md)
- challenge gate: [`research/challenge-decisions.md`](research/challenge-decisions.md)

## Approved decisions

| # | Decision | Choice |
|---|---|---|
| C1 | Walker engine | hand-rolled tokio worker pool (mirror npkill design) |
| C2 | Runtime | `tokio` (async streams, mpsc, CancellationToken) |
| C3 | Delete | `std::fs::remove_dir_all` cross-platform |
| C4 | Profiles | hardcoded only in v1 |
| C5 | Risk analyzer | pure string ops, no `regex` crate |
| C6 | Size calc | refcounted async collector (mirror npkill — fits tokio model) |
| C7 | Crate shape | lib + bin (`nmk` binary, `nodemoduleskiller` library) |

Risk score: **LOW** (0 critical).

## Behavioral invariants (locked)

1. Targets = exact basename match
2. Exclude = substring match
3. Walker stops descending at matched targets
4. Symlinks never followed
5. Permission errors silently skipped
6. Unix size = `blocks × 512`; Windows = logical size
7. Directories count 4096 bytes in size
8. Delete path must be contained in scan root
9. Risk analyzer behavior identical to npkill (table-driven tests)

## Phases

| # | Phase | Status | Owner |
|---|---|---|---|
| 01 | Project bootstrap (Cargo, scaffold, types) | **completed** | claude |
| 02 | Core scanner (worker pool + walker + ignore rules) | **completed** | claude |
| 03 | Folder size calculation (refcounted async sum) | **completed** | claude |
| 04 | Risk analyzer + safe-delete guard | **completed** | fullstack-developer subagent |
| 05 | Delete operation | **completed** | claude |
| 06 | Profiles + sort + filter | **completed** | claude |
| 07 | TUI with ratatui (results table, header, help, details) | **completed (minimal v0.1)** | claude |
| 08 | CLI args + integration glue | **completed (no-tui mode)** | claude |
| 09 | Tests + docs + manual-test checklist | **completed** | claude |

Dependencies: 02 ← 01; 03 ← 02; 04 indep of 02/03; 05 ← 04; 06 ← 02; 07 ← 02,03,06; 08 ← 07; 09 ← all.

Phases 02 and 04 can run in parallel after 01. Phase 03 needs 02's worker abstraction.

## Cargo dependencies (locked)

```toml
[dependencies]
ratatui      = "0.29"
crossterm    = "0.28"
clap         = { version = "4", features = ["derive"] }
tokio        = { version = "1", features = ["rt-multi-thread","macros","sync","time","fs"] }
tokio-util   = { version = "0.7", features = ["rt"] }   # CancellationToken
futures      = "0.3"
num_cpus     = "1"
tracing      = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow       = "1"
thiserror    = "1"
humansize    = "2"
dunce        = "1"

[dev-dependencies]
tempfile     = "3"
assert_fs    = "1"
predicates   = "3"
tokio        = { version = "1", features = ["test-util","macros","rt"] }
```

## Handoff

Implementation: `/ck:cook ./plans/20260526-1900-port-npkill-core-to-rust/plan.md`

Each phase file is self-contained with context, todo list, success criteria, risks, and code-shape hints.
