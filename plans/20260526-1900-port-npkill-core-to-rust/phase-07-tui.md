# Phase 07 — Interactive TUI with ratatui

## Context

Build a terminal UI that mirrors npkill's interactive feel: a results table that fills in live as the scanner finds matches, with size and risk badge per row, keyboard navigation, and delete-with-confirmation.

Decision C2 (locked): tokio runtime + crossbeam_channel? — actually **C2 picked tokio**, so use `tokio::sync::mpsc` for in-process eventing. Use `tokio::select!` in the main loop to drive UI ticks + scan results + keyboard events together.

## Priority

P1 — primary UX.

## Status

completed (2026-05-27, minimal v0.1 — header + table + status + delete modal. Filter / help / sort-cycle / details pane deferred to v0.2)

## Requirements

- Live updates: rows appear as scanner emits, sizes fill in as size-calc finishes
- Keys (npkill-equivalent):
  - `↑/↓` `j/k`: navigate
  - `Space` / `Enter` / `d`: delete selected
  - `/`: filter input mode (esc to clear)
  - `s`: cycle sort (path → size → age → path)
  - `?` / `h`: toggle help panel
  - `q` / `Ctrl-C`: quit (cancel scan first)
- Risk badge: ⚠ for sensitive, blank otherwise
- Status row: total found, total size, scan state (scanning/done)
- Confirmation prompt before delete; dry-run mode shown in title

## Architecture

```
src/tui/
├── mod.rs             pub async fn run(args, core) — owns the runtime loop
├── app.rs             AppState (results: Vec<FolderResult>, cursor, sort, filter, mode)
├── events.rs          map KeyEvent → AppAction
├── actions.rs         AppAction enum + reducer
├── theme.rs           Style helpers
└── render/
    ├── mod.rs         draw(frame, &state)
    ├── header.rs      title, scan path, stats line
    ├── table.rs       results table with sticky cursor
    ├── details.rs     selected row detail pane (path, size, mtime, risk reason)
    ├── help.rs        keybinding overlay
    └── filter_bar.rs  active filter input
```

### Main loop sketch

```rust
pub async fn run(opts: CliArgs) -> anyhow::Result<()> {
    let mut term = init_terminal()?;
    let scan_root = opts.root_path()?;
    let scan_opts = build_scan_opts(&opts)?;
    let mut handle = scanner::start_scan(scan_root.clone(), scan_opts.clone());
    let mut state = AppState::new(scan_root.clone(), scan_opts.targets.clone(), opts.dry_run);
    let mut tick = tokio::time::interval(Duration::from_millis(120));
    let mut input = EventStream::new();           // crossterm async events

    loop {
        tokio::select! {
            _ = tick.tick() => {
                term.draw(|f| render::draw(f, &state))?;
                state.maybe_request_sizes(&handle);    // kicks off get_folder_size for new rows
            }
            Some(ev) = input.next() => {
                let act = events::map(ev?);
                if matches!(act, Action::Quit) { handle.cancel.cancel(); break; }
                state.apply(act, &handle).await;
            }
            Some(found) = handle.results.recv() => {
                state.push_result(found);
            }
        }
    }
    restore_terminal(term)
}
```

### State + actions

```rust
pub enum Mode { Browse, Filter, Confirm(usize), Help }

pub struct AppState {
    pub root: PathBuf,
    pub targets: Vec<String>,
    pub results: Vec<FolderResult>,
    pub cursor: usize,
    pub sort: SortBy,
    pub filter: String,
    pub mode: Mode,
    pub dry_run: bool,
    pub size_in_flight: HashSet<PathBuf>,
    pub size_results: mpsc::Receiver<(PathBuf, u64)>,
    pub size_tx: mpsc::Sender<(PathBuf, u64)>,
}

pub enum Action { Up, Down, Confirm, Delete, ToggleSort, Filter(char), Backspace, EnterFilter, EscapeMode, Quit, Help }
```

## Files to create

- everything under `src/tui/` (~600–800 LoC across files)

## Files to modify

- `src/main.rs` actually wires real `tui::run`
- `src/lib.rs` keeps tui re-export if needed for integration tests

## Implementation steps

1. **Init/restore terminal**: `crossterm::terminal::enable_raw_mode`, `EnterAlternateScreen`, `Hide cursor`. Restore on drop via guard.
2. **AppState** + initial seeding.
3. **Render functions**:
   - `header`: 2 rows — title with mode (`SCAN ~/projects [dry-run]`), stats (`12 found · 1.2 GB total · scanning`).
   - `table`: list of (selected?) (risk?) (path) (size) (mtime). Use `ratatui::widgets::Table` + `TableState` for cursor.
   - `details`: bottom pane with selected row details + risk reason.
   - `help`: overlay listing keybindings (toggled).
   - `filter_bar`: appears when in `Mode::Filter`.
4. **Events**: `EventStream` from `crossterm` async events.
5. **Size dispatch**: after each tick, for visible rows without `size_bytes` and not in `size_in_flight`, spawn `tokio::spawn(async { let s = get_folder_size(...).await; size_tx.send((path,s)).await })`. On size_results.recv, update the row.
6. **Delete action**: enters `Mode::Confirm(idx)`. Y/Enter → calls `delete::delete(path, &root, &targets, dry_run)` → marks row `deleted=true`. N/Esc → back to Browse.
7. **Sort + filter**: re-derive view from results + filter + sort on every state change.
8. **Manual integration test**: run on a small fake tree and visually verify.

## Todo

- [ ] Terminal init/restore guard
- [ ] AppState + Action + reducer
- [ ] Header render
- [ ] Table render with cursor state
- [ ] Details pane render
- [ ] Help overlay render
- [ ] Filter bar render + input mode
- [ ] Main loop with `tokio::select!`
- [ ] Live size dispatch
- [ ] Delete confirmation flow
- [ ] Sort cycle + filter
- [ ] Manual test on tempfile tree
- [ ] Quit cancels scanner within 200 ms

## Success criteria

- TUI launches on a small directory and finds known node_modules
- Sizes populate live as scan runs
- Delete with confirmation removes the folder and marks row deleted
- `q` exits cleanly and restores terminal (no garbled output)
- Window resize handled (ratatui handles automatically via layout)

## Risks

| Risk | Mitigation |
|---|---|
| Terminal not restored on panic | wrap in `Drop` guard for raw mode; install panic hook |
| Channels backpressure when many results | result channel size 1024; size channel size 256 |
| Flicker on tick | ratatui double-buffers automatically; 120 ms tick is fine |
| Long paths overflow table | use `Cell::from(Text::raw(path).style(...))` with column constraint to truncate from left |
| `EventStream` not available in `crossterm`: yes it is via `event-stream` feature | enable `crossterm = { version = "0.28", features = ["event-stream"] }` |

## Security considerations

Delete confirmation is mandatory (no "are you sure" bypass). Dry-run badge always visible.

## Next steps

Phase 08 wires CLI flags into AppState (`--target`, `--exclude`, `--profile`, `--dry-run`, `--sort`).
