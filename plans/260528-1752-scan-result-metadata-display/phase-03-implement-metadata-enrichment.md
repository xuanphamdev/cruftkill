---
phase: 3
title: Implement metadata enrichment
status: completed
priority: P2
effort: 2h
dependencies:
  - 2
---

# Phase 3: Implement metadata enrichment

## Context Links

- Phase 2: [Design metadata model](./phase-02-design-metadata-model.md)
- Core result types: [types](../../src/core/types.rs)
- Scanner: [scanner](../../src/core/scanner.rs)

## Overview

Attach metadata to every found folder without changing scanner matching semantics.

## Requirements

- Functional: each `FolderResult` has metadata ready for TUI rendering.
- Functional: no-tui output can compute the same metadata.
- Non-functional: no extra filesystem traversal; metadata is O(number of profile targets).
- Compatibility: do not remove or rename existing fields.

## Architecture

Preferred shape:

```text
scanner emits ScanFoundFolder { path, risk_analysis, metadata }
FolderResult copies metadata
TUI/no-tui render from the same model
```

If carrying metadata through scanner makes tests harder, compute it at `FolderResult::from_scan`
and in `run_no_tui` via a shared `metadata::classify_path(&path, risk)`.

## Related Code Files

- Modify: `src/core/types.rs`
- Modify: `src/core/scanner.rs`
- Modify: `src/core/mod.rs`
- Create/modify: `src/core/metadata.rs`
- Modify tests near `src/core/types.rs` and scanner smoke tests as needed.

## Implementation Steps

1. Add `metadata: CruftMetadata` to `ScanFoundFolder` or `FolderResult`.
2. Extract basename with safe fallback for non-UTF-8 paths using existing lossy behavior.
3. Classify known target with profile reverse lookup.
4. Preserve existing risk-analysis behavior; do not rely on scanner placeholder `RiskAnalysis::safe()` as final truth in TUI.
5. Ensure rescan and sort paths keep metadata stable.
6. Keep clone/copy costs acceptable; metadata strings are small.

## Success Criteria

- [ ] All scan results have metadata.
- [ ] Existing scanner tests still pass.
- [ ] Unknown/custom targets do not panic.
- [ ] Non-UTF-8 path behavior remains lossy but safe, matching scanner conventions.

## Risk Assessment

Risk: duplicating classification between TUI and no-tui. Mitigation: one public core function used by both.

Risk: changing scanner channel payload affects tests. Mitigation: update constructors and keep `ScanFoundFolder::new` ergonomic.

## Security Considerations

Do not let "low risk" text bypass confirmation. Confirm modal should still warn for sensitive paths.

## Next Steps

Render the metadata in TUI and emit it in NDJSON.
