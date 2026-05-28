---
phase: 2
title: Design metadata model
status: completed
priority: P2
effort: 2h
dependencies:
  - 1
---

# Phase 2: Design metadata model

## Context Links

- Phase 1: [Audit existing result flow](./phase-01-audit-existing-result-flow.md)
- Existing profile registry: [profiles](../../src/core/profiles.rs)
- Existing risk analyzer: [risk](../../src/core/risk.rs)

## Overview

Define a small core metadata model that answers: ecosystem, target category, delete-risk verdict, and cleanup hint.

## Requirements

- Functional: classify each matched directory basename into one or more ecosystems.
- Functional: support custom `--target` entries as `custom`/unknown metadata.
- Functional: combine target metadata with existing path-sensitive risk.
- Non-functional: pure functions, deterministic, table-tested, no filesystem I/O.

## Architecture

Add a core module, likely `src/core/metadata.rs`, exposed from `src/core/mod.rs`.

Suggested types:

```rust
pub struct CruftMetadata {
    pub target_name: String,
    pub ecosystems: Vec<String>,
    pub category: CruftCategory,
    pub delete_risk: DeleteRiskLevel,
    pub delete_risk_reason: String,
    pub rebuild_hint: Option<String>,
}

pub enum CruftCategory {
    DependencyTree,
    BuildOutput,
    TestCache,
    ToolCache,
    VirtualEnvironment,
    EditorCache,
    DeploymentCache,
    Unknown,
}

pub enum DeleteRiskLevel {
    Low,
    Medium,
    High,
}
```

Classification rules:

- Reverse-map `base_profiles()` from target basename to profile names.
- Use explicit target category table for important basenames.
- If `RiskAnalysis.is_sensitive`, final risk is `High` regardless of target category.
- Known generated caches/build outputs default `Low`.
- Recreate-cost folders (`node_modules`, `.venv`, `venv`, `.gradle`, `DerivedData`, `target`) can be `Low` or `Medium`; decide by message clarity, not fear.
- Unknown custom targets use `Medium` with "custom target; review before delete".

## Related Code Files

- Create: `src/core/metadata.rs`
- Modify: `src/core/mod.rs`
- Modify: `src/core/types.rs`
- Modify: `src/core/profiles.rs` if reverse lookup belongs there instead.

## Implementation Steps

1. Pick final type names and derive traits needed by tests/debug display.
2. Add profile reverse lookup helper: target basename -> sorted ecosystem names.
3. Add category mapping for existing profile targets.
4. Add final risk function accepting basename, ecosystems, and `Option<&RiskAnalysis>`.
5. Decide display labels: short ASCII text, stable for JSON consumers.

## Success Criteria

- [ ] Model covers every existing profile target.
- [ ] Overlapping targets return multiple ecosystems, not arbitrary first match.
- [ ] Sensitive-path risk overrides normal cleanup safety.
- [ ] Custom target behavior is explicit.

## Risk Assessment

Risk: overclaiming "safe" for sensitive paths. Mitigation: path risk always wins and text says "review before delete".

Risk: too much metadata in `profiles.rs`. Mitigation: keep classification in `metadata.rs`; leave profile registry as source of truth for names/targets.

## Security Considerations

Metadata is advisory only. Delete confirmation and safe-delete guards remain authoritative.

## Next Steps

Implement model and wire it into scan results.
