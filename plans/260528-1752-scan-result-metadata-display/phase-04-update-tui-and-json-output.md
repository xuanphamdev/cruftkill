---
phase: 4
title: Update TUI and JSON output
status: completed
priority: P2
effort: 2h
dependencies:
  - 3
---

# Phase 4: Update TUI and JSON output

## Context Links

- TUI render: [render](../../src/tui/render.rs)
- TUI state: [app](../../src/tui/app.rs)
- no-tui output: [main](../../src/main.rs)

## Overview

Display metadata under each folder path in TUI and add equivalent add-only fields to NDJSON.

## Requirements

- Functional: metadata appears directly below the path in each TUI row.
- Functional: risk text is visible, not only an icon.
- Functional: NDJSON includes metadata keys for scripts.
- Non-functional: readable at narrow terminal widths; no panics on long paths.

## Architecture

TUI table rows become height 2:

```text
PATH / size / age
  ecosystems | category | risk: level | hint
```

Suggested NDJSON additions:

```json
{
  "target_name": ".venv",
  "ecosystems": ["python", "data-science"],
  "category": "virtual-environment",
  "delete_risk": "low",
  "delete_risk_reason": "Regenerable virtual environment outside sensitive path",
  "rebuild_hint": "Recreate with your package manager or project setup command"
}
```

Keep existing keys: `path`, `size_bytes`, `is_sensitive`, `risk_reason`, `modified_unix`, `dry_run`.

## Related Code Files

- Modify: `src/tui/render.rs`
- Modify: `src/tui/app.rs` only if state helpers are needed.
- Modify: `src/main.rs`
- Modify: `README.md`

## Implementation Steps

1. Add formatter helpers for metadata display strings.
2. Render path row plus metadata row. Use muted style for metadata; highlight `risk: high`.
3. Keep table width constraints stable. Avoid horizontal overflow where practical.
4. Confirm selected/highlighted row still highlights both lines or remains visually coherent.
5. Add NDJSON metadata fields using the same core metadata object.
6. Update README JSON example and feature list.

## Success Criteria

- [ ] TUI displays framework/ecosystem, category, risk verdict, and hint under each path.
- [ ] High-risk rows remain obvious in table and confirm modal.
- [ ] `cft --no-tui` emits added metadata fields.
- [ ] Existing JSON consumers remain compatible because old keys stay.

## Risk Assessment

Risk: TUI becomes cramped. Mitigation: concise labels, row height 2, truncate long hint if needed.

Risk: `render.rs` grows further. Mitigation: extract small helper functions in same module first; create a new module only if necessary.

## Security Considerations

High-risk text must not be softened. Use "review before delete" for sensitive paths and custom targets.

## Next Steps

Add tests and docs validation.
