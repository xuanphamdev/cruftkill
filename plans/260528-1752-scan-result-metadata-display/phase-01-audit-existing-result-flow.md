---
phase: 1
title: Audit existing result flow
status: completed
priority: P2
effort: 1h
dependencies: []
---

# Phase 1: Audit existing result flow

## Context Links

- Plan: [Display scan result metadata](./plan.md)
- README: [Project README](../../README.md)
- Source: [types](../../src/core/types.rs), [profiles](../../src/core/profiles.rs), [risk](../../src/core/risk.rs), [TUI](../../src/tui/render.rs), [no-tui](../../src/main.rs)

## Overview

Confirm the exact data path from CLI profiles to scanner results, TUI rendering, and NDJSON output before changing types.

## Requirements

- Functional: identify all result creation points and output surfaces.
- Non-functional: avoid scanner regression; no behavior change in phase 1.

## Architecture

Current flow:

```text
CliArgs -> resolved_targets -> ScanOptions
  -> scanner::start_scan -> ScanFoundFolder
  -> FolderResult::from_scan
  -> TUI table / no-tui JSON
```

Metadata should be attached after target match is known, before UI/output rendering.

## Related Code Files

- Modify later: `src/core/types.rs`
- Modify later: `src/core/profiles.rs`
- Modify later: `src/tui/render.rs`
- Modify later: `src/main.rs`
- Read only in this phase: `tests/*.rs`

## Implementation Steps

1. Trace every `ScanFoundFolder::new` call and every `FolderResult::from_scan` call.
2. Confirm whether scanner has enough context to classify target basename.
3. Confirm `--target` custom basenames behavior.
4. Confirm TUI row height constraints on narrow terminals.
5. Confirm NDJSON output shape and backward-compatible add-only fields.

## Success Criteria

- [ ] Data-flow map is complete.
- [ ] All files needing modification are listed.
- [ ] No unresolved output surface.

## Risk Assessment

Low. Read-only audit. Main risk is missing a secondary result path; mitigate with `rg "ScanFoundFolder|FolderResult|json!"`.

## Security Considerations

Do not weaken delete guards. This phase only locates read/display paths.

## Next Steps

Proceed to metadata model once result flow is confirmed.
