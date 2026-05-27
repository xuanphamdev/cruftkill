# Phase 07 — completion report

## Status

**Completed** 2026-05-27 (minimal viable TUI for v0.1).

## Delivered

- `src/tui/app.rs` (~260 LoC) — `AppState`, `Action`, `Effect`, `Mode { Browse, Confirm(idx) }`, reducer `apply`, helpers `push_result` / `record_size` / `record_delete_outcome` / `mark_scan_finished`. **9 unit tests** for the reducer.
- `src/tui/render.rs` (~200 LoC) — ratatui draw funcs: header (title + stats + dry-run badge), results table (cursor, risk ⚠, deleted strike-through), status hint, centred confirm modal. `human_bytes` formatter. **3 inline tests**.
- `src/tui/mod.rs` (~250 LoC) — `run()` main loop with `tokio::select!` over tick / `EventStream` / scanner results / size results / delete outcomes. `TerminalGuard` RAII restores raw mode + leaves alt screen on drop (panic-safe). `map_key` keybind table. **4 inline tests** for key mapping.
- Cargo: added `ratatui = "0.29"`, `crossterm = "0.28" + ["event-stream"]`, `futures = "0.3"`.

## Keybinds (v0.1)

| Key | Action |
|---|---|
| `↑` / `k` | move cursor up |
| `↓` / `j` | move cursor down |
| `d` / `Space` / `Enter` | open delete confirm |
| `y` / `Y` | confirm delete |
| `n` / `N` / `Esc` | cancel modal |
| `q` | quit (cancels scanner) |
| `Ctrl-C` | quit (always wins) |

## Behavioural highlights

- **Risk re-analyzed in the main loop** using a cached `home_path`, replacing the scanner's safe-by-default placeholder. Phase 04 reviewer MEDIUM #1 addressed.
- Size requests fire lazily on tick — only for rows still missing a size, deduped via `HashSet<PathBuf>`.
- Delete runs in a tokio task; outcome is funneled back through an mpsc and updates `last_message` + the row's `deleted` flag (strike-through render).
- Dry-run badge in the header AND in the confirm modal — never silently destructive.
- Scanner cancel fires on `q` / `Ctrl-C`; `TerminalGuard::drop` always restores the terminal.

## Deferred to v0.2 (explicit scope cut for KISS / YAGNI)

- Filter input (`/` key) and live substring filtering of displayed rows
- Help overlay (`?` / `h`)
- In-UI sort cycle (`s`) — sort can be set via CLI `-s path|size|age`
- Detail pane / drawer for the selected row
- Multi-select + bulk delete
- Live progress bars / pending-jobs counters from `ScanStats`

## Gates

| Gate | Result |
|---|---|
| `cargo test` | **138 passed** (Phase 08: 122 → +16: 9 reducer + 3 render + 4 keybind) |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| `cargo build --release` | clean |
| Build size (`nmk` release) | not measured here — Phase 09 will benchmark |
| Code review | pending (will be Phase 07.5) |

## Open questions

1. Want a real interactive smoke test in this session, or rely on unit tests + Phase 09 CI for verification?
2. Should TUI also stream `delete::delete` failures into the status bar as red text (vs current green "delete failed: …")? Cosmetic.

## Next

Phase 09 (docs, CI, release prep) is the only remaining functional phase.
