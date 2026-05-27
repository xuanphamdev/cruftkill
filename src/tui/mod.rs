//! Interactive ratatui-based UI for the `nmk` binary.
//!
//! v0.1 deliberately ships a minimal feel: header, results table, status hint,
//! delete-with-confirm modal. Filter, help overlay, sort cycle, and live
//! progress bars are deferred.
//!
//! The main loop drives three event sources at the same time:
//! 1. periodic tick (so the scan-status spinner and pending sizes refresh),
//! 2. crossterm `EventStream` (key + resize),
//! 3. the scanner's result channel + the size-result channel.
//!
//! Terminal state is owned by [`TerminalGuard`] — a RAII handle that always
//! restores the terminal on drop, including panic unwind.

pub mod app;
pub mod render;

use std::collections::HashSet;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc;

use crate::cli::CliArgs;
use crate::core::types::{ScanOptions, SortBy};
use crate::core::{delete, risk, scanner, size};

use self::app::{Action, AppState, Effect};

/// RAII terminal-mode guard: restores the terminal on drop, even during panic.
struct TerminalGuard {
    term: Option<Terminal<CrosstermBackend<Stdout>>>,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut out = io::stdout();
        execute!(out, EnterAlternateScreen)?;
        let term = Terminal::new(CrosstermBackend::new(out))?;
        Ok(Self { term: Some(term) })
    }

    fn terminal(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        self.term.as_mut().expect("terminal taken")
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if let Some(mut term) = self.term.take() {
            let _ = disable_raw_mode();
            let _ = execute!(term.backend_mut(), LeaveAlternateScreen);
            let _ = term.show_cursor();
        }
    }
}

/// Entry point invoked by `main.rs` when stdout is a TTY and `--no-tui` was not passed.
pub async fn run(args: CliArgs) -> anyhow::Result<()> {
    let root = args.root_path()?;
    let targets = args.resolved_targets();
    let opts = ScanOptions {
        targets: targets.clone(),
        exclude: args.exclude.clone(),
        sort_by: Some(args.sort.into()),
        perform_risk_analysis: !args.no_risk,
    };
    let sort: SortBy = args.sort.into();
    let dry_run = args.dry_run;

    let mut state = AppState::new(root.clone(), targets.clone(), dry_run, sort);

    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).ok();
    let home_path = home.as_deref().map(std::path::PathBuf::from);

    // Re-wrap scanner result risk to use the centralised home (instead of
    // re-reading env per result). The scanner already computed a placeholder
    // via Phase 02; we recompute here so reasons are accurate.
    let mut handle = scanner::start_scan(root.clone(), opts);

    let mut term = TerminalGuard::enter().context("failed to enter TUI mode")?;
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(120));

    // Size-calculation pipeline: rows we've already requested size for, plus
    // a channel for completed sizes.
    let mut size_requested: HashSet<PathBuf> = HashSet::new();
    let (size_tx, mut size_rx) = mpsc::channel::<(PathBuf, u64)>(256);

    // Delete-outcome pipeline.
    let (del_tx, mut del_rx) = mpsc::channel::<(usize, bool, Option<String>)>(32);

    loop {
        // Draw current state.
        term.terminal().draw(|f| render::draw(f, &state))?;

        tokio::select! {
            _ = tick.tick() => {
                // Kick off size requests for any new rows we haven't asked about yet.
                for r in &state.results {
                    if r.size_bytes.is_none() && size_requested.insert(r.path.clone()) {
                        let p = r.path.clone();
                        let tx = size_tx.clone();
                        tokio::spawn(async move {
                            let s = size::get_folder_size(p.clone()).await.unwrap_or(0);
                            let _ = tx.send((p, s)).await;
                        });
                    }
                }
            }

            Some(ev) = events.next() => {
                let Ok(ev) = ev else { continue };
                if let Event::Key(k) = ev {
                    let action = map_key(k);
                    // Recompute risk for the latest results before quitting / etc.
                    let effect = state.apply(action);
                    match effect {
                        Effect::Quit => {
                            handle.cancel.cancel();
                            break;
                        }
                        Effect::DeleteFolder { index, path } => {
                            let scan_root = state.root.clone();
                            let targets = state.targets.clone();
                            let tx = del_tx.clone();
                            tokio::spawn(async move {
                                let res = delete::delete(&path, &scan_root, &targets, dry_run).await;
                                let _ = tx.send((index, res.success, res.error)).await;
                            });
                        }
                        Effect::None => {}
                    }
                }
            }

            Some(found) = handle.results.recv() => {
                let mut found = found;
                // Replace the scanner's placeholder safe-by-default with a real
                // risk analysis using the cached home dir.
                if !args.no_risk {
                    found.risk_analysis = Some(risk::analyze_with_home(&found.path, home_path.as_deref()));
                }
                state.push_result(found);
            }

            Some((path, sz)) = size_rx.recv() => {
                state.record_size(&path, sz);
            }

            Some((idx, ok, err)) = del_rx.recv() => {
                state.record_delete_outcome(idx, ok, err);
            }

            else => {
                // All channels closed and no events — mark scan done. The user
                // can still navigate / delete already-discovered rows.
                if !state.scan_finished {
                    state.mark_scan_finished();
                }
                tokio::time::sleep(Duration::from_millis(120)).await;
            }
        }

        // After each event, detect "scanner done" so the header can show it.
        // The receiver returns None only after all senders drop.
        if !state.scan_finished && handle.results.is_closed() {
            state.mark_scan_finished();
        }
    }

    Ok(())
}

fn map_key(k: KeyEvent) -> Action {
    if k.kind == KeyEventKind::Release {
        return Action::Noop;
    }
    // Ctrl-C always quits, regardless of modal state.
    if k.modifiers.contains(KeyModifiers::CONTROL) && matches!(k.code, KeyCode::Char('c')) {
        return Action::Quit;
    }
    match k.code {
        KeyCode::Up | KeyCode::Char('k') => Action::Up,
        KeyCode::Down | KeyCode::Char('j') => Action::Down,
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('d') | KeyCode::Char(' ') | KeyCode::Enter => Action::RequestDelete,
        KeyCode::Char('y') | KeyCode::Char('Y') => Action::ConfirmYes,
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => Action::ConfirmNo,
        _ => Action::Noop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn arrows_and_letters_map_correctly() {
        assert_eq!(map_key(key(KeyCode::Up)), Action::Up);
        assert_eq!(map_key(key(KeyCode::Char('k'))), Action::Up);
        assert_eq!(map_key(key(KeyCode::Down)), Action::Down);
        assert_eq!(map_key(key(KeyCode::Char('j'))), Action::Down);
        assert_eq!(map_key(key(KeyCode::Char('q'))), Action::Quit);
        assert_eq!(map_key(key(KeyCode::Char('d'))), Action::RequestDelete);
        assert_eq!(map_key(key(KeyCode::Enter)), Action::RequestDelete);
        assert_eq!(map_key(key(KeyCode::Char(' '))), Action::RequestDelete);
        assert_eq!(map_key(key(KeyCode::Char('y'))), Action::ConfirmYes);
        assert_eq!(map_key(key(KeyCode::Char('Y'))), Action::ConfirmYes);
        assert_eq!(map_key(key(KeyCode::Char('n'))), Action::ConfirmNo);
        assert_eq!(map_key(key(KeyCode::Esc)), Action::ConfirmNo);
    }

    #[test]
    fn ctrl_c_quits() {
        let k = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(map_key(k), Action::Quit);
    }

    #[test]
    fn unknown_keys_are_noop() {
        assert_eq!(map_key(key(KeyCode::F(1))), Action::Noop);
        assert_eq!(map_key(key(KeyCode::Char('x'))), Action::Noop);
    }

    #[test]
    fn key_release_is_noop() {
        let mut k = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        k.kind = KeyEventKind::Release;
        assert_eq!(map_key(k), Action::Noop);
    }
}
