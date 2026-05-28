---
title: Display scan result metadata
description: >-
  Show ecosystem, delete-risk verdict, and practical cleanup hints under each
  scanned folder path.
status: completed
priority: P2
effort: 8h
branch: main
tags:
  - feature
  - tui
  - cli
  - rust
blockedBy: []
blocks: []
created: '2026-05-28T10:52:37.068Z'
createdBy: 'ck:plan'
source: skill
---

# Display scan result metadata

## Overview

Add per-result cleanup context after scanning. Each found folder should show the likely ecosystem/framework
(`node`, `python`, `rust`, etc.), whether deleting it is risky, and short useful hints like rebuild/reinstall
cost. The feature must reuse existing profile and risk-analysis logic, not invent a second scanner.

Mode: fast/local plan. Research skipped because the codebase already contains the relevant profile,
risk, scanner, TUI, and NDJSON surfaces.

## Current Findings

- `src/core/types.rs` owns `ScanFoundFolder` and `FolderResult`; results currently carry path, risk, size, age.
- `src/core/profiles.rs` maps profile -> targets but lacks reverse lookup target -> ecosystem.
- `src/core/risk.rs` only flags sensitive path locations; it does not explain target-specific cleanup impact.
- `src/tui/render.rs` renders one-line table rows: path, size, age, risk icon.
- `src/main.rs::run_no_tui` emits NDJSON and can add fields without breaking existing consumers.

## Target UX

For each result, show:

```text
/work/app/.venv                         1.4 GB   3w
  python | virtual env | risk: low | safe to delete; recreate with installer
```

Sensitive paths should be clearer:

```text
~/.config/some-app/node_modules         200 MB   2d
  node | dependency tree | risk: high | inside user config; review before delete
```

## Cross-Plan Dependencies

| Relationship | Plan | Status |
|-------------|------|--------|
| None | `20260526-1900-port-npkill-core-to-rust` | done |

## Phases

| Phase | Name | Status |
|-------|------|--------|
| 1 | [Audit existing result flow](./phase-01-audit-existing-result-flow.md) | Completed |
| 2 | [Design metadata model](./phase-02-design-metadata-model.md) | Completed |
| 3 | [Implement metadata enrichment](./phase-03-implement-metadata-enrichment.md) | Completed |
| 4 | [Update TUI and JSON output](./phase-04-update-tui-and-json-output.md) | Completed |
| 5 | [Add tests and docs](./phase-05-add-tests-and-docs.md) | Completed |

## Dependencies

- Rust 1.85+, edition 2024.
- Existing `ratatui`, `serde_json`, `tokio`, and test stack.
- No new crate unless implementation proves current std/core code cannot express metadata cleanly.

## Success Criteria

- TUI displays metadata directly under each folder path.
- NDJSON includes equivalent metadata fields while keeping existing keys.
- Risk verdict combines existing path-sensitive analyzer with target cleanup category.
- Overlapping targets (`target`, `.venv`, etc.) list all likely ecosystems.
- Tests cover known targets, custom targets, sensitive paths, and JSON/TUI-visible fields.
