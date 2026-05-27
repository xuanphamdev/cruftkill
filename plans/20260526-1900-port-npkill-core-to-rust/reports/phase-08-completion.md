# Phase 08 — completion report

## Status

**Completed** 2026-05-27 (no-tui mode only). Full interactive TUI is Phase 07.

## Delivered

- `src/cli.rs` expanded: `--profile`/`--target`/`--exclude` (repeatable), `--sort {path|size|age}`, `--no-risk-analysis`, `--dry-run`, `--no-tui`, `--version`. `--help` reads well. Unknown profile names produce a clean error before scan.
- `src/main.rs` dispatches: tty + no `--no-tui` → Phase 07 TUI stub; otherwise → `run_no_tui` streaming NDJSON.
- `run_no_tui`:
  - resolves HOME once and passes to `risk::analyze_with_home` per result (addresses Phase 04 reviewer MEDIUM #1)
  - streams `{ path, size_bytes, is_sensitive, risk_reason, modified_unix, dry_run }` one JSON object per line
- Cargo: added `serde_json = "1"` and `is-terminal = "0.4"`.

## End-to-end smoke (verified live)

```
$ mkdir -p /tmp/nmk-e2e/a/node_modules /tmp/nmk-e2e/b/c/node_modules
$ cargo run -- /tmp/nmk-e2e --no-tui --no-risk-analysis
{"dry_run":false,"is_sensitive":false,"modified_unix":1779867789,"path":"/tmp/nmk-e2e/a/node_modules","risk_reason":null,"size_bytes":0}
{"dry_run":false,"is_sensitive":false,"modified_unix":1779867789,"path":"/tmp/nmk-e2e/b/c/node_modules","risk_reason":null,"size_bytes":0}
```

Scanner + size + (when not `--no-risk-analysis`) risk analyzer all wired through the live binary.

## Gates

| Gate | Result |
|---|---|
| `cargo test` | **122 passed** (Phase 06: 115 → +7 CLI tests) |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| `nmk --help` | renders all flags |
| `nmk --version` | `nmk 0.1.0` |
| End-to-end NDJSON | works |

## Deviation from plan

Plan said add `--json` flag in addition to `--no-tui`. v0.1 collapses them into one — `--no-tui` always emits NDJSON. Simpler UX; a future `--format text` could split them.

`--verbose / -v` flag deferred — tracing not wired in v0.1 (no logger initialization). Add in Phase 09 (docs/CI) if needed.

## Open for Phase 07 (TUI)

- TUI consumes the same `ScanOptions` + `start_scan` API and adds `delete::delete` on keypress.
- Interactive flow needs to render `is_sensitive` badge, prompt-confirm before delete, show progress (pending/completed from `ScanStats`).
- Wire `--dry-run` flag through delete keybind UX.

## Next

Phase 07 (TUI) is the last functional phase; Phase 09 is docs/CI/release.
