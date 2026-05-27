//! TUI application state and pure-function reducer.
//!
//! Keeps render code and event handling decoupled: the main loop
//! ([`super::mod::run`]) translates `KeyEvent` / scan-result / size-result
//! signals into [`Action`]s; this module applies them to [`AppState`] and
//! returns a small set of side effects that the loop performs (e.g. "delete
//! folder at index N").
//!
//! Why a reducer-style design? It makes the UI logic unit-testable without a
//! real terminal — see the `tests` module below.

use std::path::PathBuf;

use crate::core::types::{FolderResult, ScanFoundFolder, SortBy};

/// What the TUI is currently doing. Most of the time it's `Browse`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Browse,
    /// Awaiting Y/N on deleting the row at this cursor index.
    Confirm(usize),
}

/// All UI-visible state.
#[derive(Debug)]
pub struct AppState {
    pub root: PathBuf,
    pub targets: Vec<String>,
    pub dry_run: bool,

    pub results: Vec<FolderResult>,
    pub cursor: usize,
    pub mode: Mode,
    pub sort: SortBy,

    /// Set true when the scanner channel closes — used in the status bar.
    pub scan_finished: bool,

    /// Last status / error message shown to the user. Cleared on next action.
    pub last_message: Option<String>,
}

impl AppState {
    pub fn new(root: PathBuf, targets: Vec<String>, dry_run: bool, sort: SortBy) -> Self {
        Self {
            root,
            targets,
            dry_run,
            results: Vec::new(),
            cursor: 0,
            mode: Mode::Browse,
            sort,
            scan_finished: false,
            last_message: None,
        }
    }

    /// Total size across all rows that have a known size.
    pub fn total_size(&self) -> u64 {
        self.results.iter().filter_map(|r| r.size_bytes).sum()
    }

    /// Currently-highlighted row, if any.
    pub fn selected(&self) -> Option<&FolderResult> {
        self.results.get(self.cursor)
    }
}

/// User intent. Translated by `apply` into state changes + optional side effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    /// Open the confirm prompt for the currently-selected row.
    RequestDelete,
    ConfirmYes,
    ConfirmNo,
    Quit,
    Noop,
}

/// Things the reducer asks the main loop to do that aren't pure state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Perform the actual delete on the FS, then call back into the reducer
    /// via `record_delete_outcome`.
    DeleteFolder {
        index: usize,
        path: PathBuf,
    },
    /// Tear down the TUI and exit.
    Quit,
    None,
}

impl AppState {
    /// Apply an action, returning any side effect for the main loop to execute.
    pub fn apply(&mut self, action: Action) -> Effect {
        self.last_message = None;
        match action {
            Action::Up => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                Effect::None
            }
            Action::Down => {
                if !self.results.is_empty() && self.cursor + 1 < self.results.len() {
                    self.cursor += 1;
                }
                Effect::None
            }
            Action::RequestDelete => {
                if self.mode == Mode::Browse
                    && let Some(r) = self.results.get(self.cursor)
                    && !r.deleted
                {
                    self.mode = Mode::Confirm(self.cursor);
                }
                Effect::None
            }
            Action::ConfirmYes => match self.mode.clone() {
                Mode::Confirm(idx) => {
                    self.mode = Mode::Browse;
                    if let Some(r) = self.results.get(idx) {
                        Effect::DeleteFolder { index: idx, path: r.path.clone() }
                    } else {
                        Effect::None
                    }
                }
                _ => Effect::None,
            },
            Action::ConfirmNo => {
                if matches!(self.mode, Mode::Confirm(_)) {
                    self.mode = Mode::Browse;
                }
                Effect::None
            }
            Action::Quit => Effect::Quit,
            Action::Noop => Effect::None,
        }
    }

    /// Push a result coming off the scanner channel.
    pub fn push_result(&mut self, found: ScanFoundFolder) {
        self.results.push(FolderResult::from_scan(found));
    }

    /// Update a row's size after the size-calc task completes.
    pub fn record_size(&mut self, path: &std::path::Path, size: u64) {
        if let Some(row) = self.results.iter_mut().find(|r| r.path == path) {
            row.size_bytes = Some(size);
        }
    }

    /// Update a row after a delete attempt completes.
    pub fn record_delete_outcome(&mut self, index: usize, success: bool, error: Option<String>) {
        if let Some(row) = self.results.get_mut(index) {
            row.deleted = success;
            if let Some(e) = error {
                self.last_message = Some(format!("delete failed: {e}"));
            } else if self.dry_run {
                self.last_message = Some("(dry-run) would have deleted".into());
            } else {
                self.last_message = Some("deleted".into());
            }
        }
    }

    pub fn mark_scan_finished(&mut self) {
        self.scan_finished = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fresh_state() -> AppState {
        AppState::new(PathBuf::from("/root"), vec!["node_modules".into()], false, SortBy::Size)
    }

    fn push(state: &mut AppState, p: &str) {
        state.push_result(ScanFoundFolder::new(PathBuf::from(p), None));
    }

    #[test]
    fn navigation_stays_in_bounds() {
        let mut s = fresh_state();
        push(&mut s, "/a");
        push(&mut s, "/b");
        push(&mut s, "/c");
        assert_eq!(s.cursor, 0);
        s.apply(Action::Up);
        assert_eq!(s.cursor, 0); // can't go below 0
        s.apply(Action::Down);
        s.apply(Action::Down);
        s.apply(Action::Down); // tries to go past last
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn delete_flow_yes_emits_effect_and_returns_to_browse() {
        let mut s = fresh_state();
        push(&mut s, "/a/node_modules");
        s.apply(Action::RequestDelete);
        assert_eq!(s.mode, Mode::Confirm(0));
        let eff = s.apply(Action::ConfirmYes);
        match eff {
            Effect::DeleteFolder { index, path } => {
                assert_eq!(index, 0);
                assert_eq!(path, PathBuf::from("/a/node_modules"));
            }
            other => panic!("expected DeleteFolder, got {other:?}"),
        }
        assert_eq!(s.mode, Mode::Browse);
    }

    #[test]
    fn delete_flow_no_returns_to_browse_without_effect() {
        let mut s = fresh_state();
        push(&mut s, "/a/node_modules");
        s.apply(Action::RequestDelete);
        assert_eq!(s.mode, Mode::Confirm(0));
        let eff = s.apply(Action::ConfirmNo);
        assert_eq!(eff, Effect::None);
        assert_eq!(s.mode, Mode::Browse);
    }

    #[test]
    fn cannot_request_delete_when_no_rows() {
        let mut s = fresh_state();
        s.apply(Action::RequestDelete);
        assert_eq!(s.mode, Mode::Browse);
    }

    #[test]
    fn cannot_redelete_a_deleted_row() {
        let mut s = fresh_state();
        push(&mut s, "/a/node_modules");
        s.record_delete_outcome(0, true, None);
        s.apply(Action::RequestDelete);
        assert_eq!(s.mode, Mode::Browse, "should not open confirm for deleted row");
    }

    #[test]
    fn record_size_updates_matching_row() {
        let mut s = fresh_state();
        push(&mut s, "/a/node_modules");
        push(&mut s, "/b/node_modules");
        s.record_size(Path::new("/a/node_modules"), 42_000);
        assert_eq!(s.results[0].size_bytes, Some(42_000));
        assert_eq!(s.results[1].size_bytes, None);
    }

    #[test]
    fn total_size_sums_known_sizes() {
        let mut s = fresh_state();
        push(&mut s, "/a");
        push(&mut s, "/b");
        s.record_size(Path::new("/a"), 1_000);
        s.record_size(Path::new("/b"), 2_500);
        assert_eq!(s.total_size(), 3_500);
    }

    #[test]
    fn quit_returns_quit_effect() {
        let mut s = fresh_state();
        assert_eq!(s.apply(Action::Quit), Effect::Quit);
    }

    #[test]
    fn dry_run_message_after_delete() {
        let mut s = fresh_state();
        s.dry_run = true;
        push(&mut s, "/a/node_modules");
        s.record_delete_outcome(0, true, None);
        assert!(s.last_message.as_deref().unwrap().contains("dry-run"));
    }
}
