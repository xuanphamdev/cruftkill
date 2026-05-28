//! TUI application state and pure-function reducer.
//!
//! Keeps render code and event handling decoupled: the main loop
//! ([`super::run`]) translates `KeyEvent` / scan-result / size-result
//! signals into [`Action`]s; this module applies them to [`AppState`] and
//! returns a small set of side effects that the loop performs (e.g. "delete
//! folder at index N").
//!
//! Why a reducer-style design? It makes the UI logic unit-testable without a
//! real terminal — see the `tests` module below.

use std::path::PathBuf;
use std::time::SystemTime;

use crate::core::sort::sort_results;
use crate::core::types::{FolderResult, ScanFoundFolder, SortBy, SortDirection};

/// What the TUI is currently doing. Most of the time it's `Browse`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Browse,
    /// Awaiting Y/N on deleting one-or-more folders. Holds the path list so
    /// the operation is index-stable even if the results vec re-sorts.
    Confirm(Vec<PathBuf>),
    /// Delete confirmed; waiting for filesystem operations to complete.
    /// The modal stays open with a spinner + live progress bar so the user
    /// knows the app is working — a batch of large `node_modules` trees
    /// can take many seconds.
    Deleting(DeleteProgress),
}

/// Live progress for an in-flight batch delete. Updated on each
/// `record_delete_outcome` call until `done()` flips true.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteProgress {
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    /// Sum of `size_bytes` of every successfully deleted row.
    pub bytes_done: u64,
}

impl DeleteProgress {
    pub fn new(total: usize) -> Self {
        Self { total, completed: 0, failed: 0, bytes_done: 0 }
    }
    pub fn done(&self) -> bool {
        self.completed + self.failed >= self.total
    }
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
    pub sort_direction: SortDirection,

    /// Set true when the scanner channel closes — used in the status bar.
    pub scan_finished: bool,

    /// `true` once the user has pressed ↑/↓ at least once. Until then we
    /// keep the cursor pinned to row 0 across re-sorts so the "top hit"
    /// stays visible while results stream in. After the user moves the
    /// cursor we switch to "preserve by path" behaviour.
    pub user_navigated: bool,

    /// Live progress counter — number of directories whose contents have
    /// been read. Sourced from `ScanStats::completed` and refreshed by the
    /// main loop on each tick.
    pub dirs_scanned: u64,

    /// Last status / error message shown to the user. Cleared on next action.
    pub last_message: Option<String>,
}

impl AppState {
    /// Create a new state with the default sort (`Size` desc).
    pub fn new(root: PathBuf, targets: Vec<String>, dry_run: bool, sort: SortBy) -> Self {
        Self::with_sort(root, targets, dry_run, sort, SortDirection::default())
    }

    pub fn with_sort(
        root: PathBuf,
        targets: Vec<String>,
        dry_run: bool,
        sort: SortBy,
        sort_direction: SortDirection,
    ) -> Self {
        Self {
            root,
            targets,
            dry_run,
            results: Vec::new(),
            cursor: 0,
            mode: Mode::Browse,
            sort,
            sort_direction,
            scan_finished: false,
            user_navigated: false,
            dirs_scanned: 0,
            last_message: None,
        }
    }

    /// Total size across all rows that have a known size — backwards compat
    /// alias for [`releasable_bytes`] + [`saved_bytes`].
    pub fn total_size(&self) -> u64 {
        self.results.iter().filter_map(|r| r.size_bytes).sum()
    }

    /// Bytes still on disk that the user could reclaim by deleting them.
    /// Excludes rows already deleted in this session.
    pub fn releasable_bytes(&self) -> u64 {
        self.results.iter().filter(|r| !r.deleted).filter_map(|r| r.size_bytes).sum()
    }

    /// Bytes the user has actually reclaimed (or simulated reclaiming, in
    /// dry-run mode) during this session.
    pub fn saved_bytes(&self) -> u64 {
        self.results.iter().filter(|r| r.deleted).filter_map(|r| r.size_bytes).sum()
    }

    /// Currently-highlighted row, if any.
    pub fn selected(&self) -> Option<&FolderResult> {
        self.results.get(self.cursor)
    }

    /// Number of rows the user has multi-selected via Space.
    pub fn selection_count(&self) -> usize {
        self.results.iter().filter(|r| r.selected && !r.deleted).count()
    }

    /// Total size of the multi-selected rows (rows missing size contribute 0).
    pub fn selection_bytes(&self) -> u64 {
        self.results.iter().filter(|r| r.selected && !r.deleted).filter_map(|r| r.size_bytes).sum()
    }

    /// Resolve "what does the next delete operate on?".
    ///
    /// - if any rows are selected → those rows
    /// - otherwise → the cursor row (single-row delete, classic behaviour)
    /// - already-deleted rows are filtered out
    pub fn pending_delete_targets(&self) -> Vec<PathBuf> {
        if self.selection_count() > 0 {
            self.results
                .iter()
                .filter(|r| r.selected && !r.deleted)
                .map(|r| r.path.clone())
                .collect()
        } else {
            self.selected().filter(|r| !r.deleted).map(|r| vec![r.path.clone()]).unwrap_or_default()
        }
    }
}

/// User intent. Translated by `apply` into state changes + optional side effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    /// Toggle selection on the row under the cursor (multi-select).
    ToggleSelect,
    /// Clear every selection. Useful as an Esc-out without leaving the app.
    ClearSelection,
    /// Open the delete confirm modal. Targets the current selection if any,
    /// else just the cursor row.
    RequestDelete,
    ConfirmYes,
    ConfirmNo,
    /// Toggle sort by size. Pressing again flips direction. When switching
    /// FROM another sort, the direction resets to the default for that key
    /// (Size→Desc, Name→Asc, LastUsed→Desc).
    ToggleSortBySize,
    ToggleSortByName,
    ToggleSortByLastUsed,
    /// Cancel the current scan, clear results, and start a fresh scan with
    /// the same options. Useful after the user has just deleted folders and
    /// wants a clean state.
    Rescan,
    Quit,
    Noop,
}

/// Things the reducer asks the main loop to do that aren't pure state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Perform the actual delete on the FS, then call back into the reducer
    /// via `record_delete_outcome`. Path-based (not index) so the operation
    /// is stable under concurrent removal / re-sort.
    DeleteBatch(Vec<PathBuf>),
    /// Tear down the TUI and exit.
    Quit,
    /// Cancel the current scan and start a new one with the same options.
    /// Main loop is responsible for the actual scanner lifecycle.
    Rescan,
    None,
}

impl AppState {
    /// Apply an action, returning any side effect for the main loop to execute.
    pub fn apply(&mut self, action: Action) -> Effect {
        self.last_message = None;
        match action {
            Action::Up => {
                self.user_navigated = true;
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                Effect::None
            }
            Action::Down => {
                self.user_navigated = true;
                if !self.results.is_empty() && self.cursor + 1 < self.results.len() {
                    self.cursor += 1;
                }
                Effect::None
            }
            Action::ToggleSelect => {
                if self.mode == Mode::Browse
                    && let Some(row) = self.results.get_mut(self.cursor)
                    && !row.deleted
                {
                    row.selected = !row.selected;
                }
                Effect::None
            }
            Action::ClearSelection => {
                if self.mode == Mode::Browse {
                    for r in &mut self.results {
                        r.selected = false;
                    }
                }
                Effect::None
            }
            Action::RequestDelete => {
                if self.mode == Mode::Browse {
                    let targets = self.pending_delete_targets();
                    if !targets.is_empty() {
                        self.mode = Mode::Confirm(targets);
                    }
                }
                Effect::None
            }
            Action::ConfirmYes => match self.mode.clone() {
                Mode::Confirm(paths) if !paths.is_empty() => {
                    self.mode = Mode::Deleting(DeleteProgress::new(paths.len()));
                    Effect::DeleteBatch(paths)
                }
                _ => {
                    self.mode = Mode::Browse;
                    Effect::None
                }
            },
            Action::ConfirmNo => {
                if matches!(self.mode, Mode::Confirm(_)) {
                    self.mode = Mode::Browse;
                }
                Effect::None
            }
            Action::ToggleSortBySize => {
                self.toggle_or_switch_sort(SortBy::Size, SortDirection::Desc);
                Effect::None
            }
            Action::ToggleSortByName => {
                self.toggle_or_switch_sort(SortBy::Path, SortDirection::Asc);
                Effect::None
            }
            Action::ToggleSortByLastUsed => {
                self.toggle_or_switch_sort(SortBy::Age, SortDirection::Desc);
                Effect::None
            }
            Action::Rescan => {
                if matches!(self.mode, Mode::Browse) {
                    self.clear_for_rescan();
                    Effect::Rescan
                } else {
                    Effect::None
                }
            }
            Action::Quit => Effect::Quit,
            Action::Noop => Effect::None,
        }
    }

    /// Reset everything that depends on a specific scan run. Keeps the
    /// user's chosen sort + direction + targets + root + dry_run flag.
    pub fn clear_for_rescan(&mut self) {
        self.results.clear();
        self.cursor = 0;
        self.user_navigated = false;
        self.scan_finished = false;
        self.dirs_scanned = 0;
        self.last_message = Some("rescanning…".into());
    }

    fn toggle_or_switch_sort(&mut self, by: SortBy, default_direction: SortDirection) {
        if self.sort == by {
            self.sort_direction = self.sort_direction.toggle();
        } else {
            self.sort = by;
            self.sort_direction = default_direction;
        }
        self.resort();
    }

    /// Sort `results` by the current `sort` + `sort_direction`.
    ///
    /// Cursor policy:
    /// - If the user has not navigated yet (`!user_navigated`), pin to row 0
    ///   so the top hit stays visible while results stream in or get re-sorted.
    /// - Otherwise, preserve the cursor on whichever row was selected
    ///   (by path), so a sort-toggle keeps your item in view.
    /// - Final clamp to keep the cursor in range.
    pub fn resort(&mut self) {
        let selected_path =
            if self.user_navigated { self.selected().map(|r| r.path.clone()) } else { None };
        sort_results(&mut self.results, self.sort, self.sort_direction);
        if let Some(p) = selected_path
            && let Some(idx) = self.results.iter().position(|r| r.path == p)
        {
            self.cursor = idx;
        } else if !self.user_navigated {
            self.cursor = 0;
        } else if self.cursor >= self.results.len() && !self.results.is_empty() {
            self.cursor = self.results.len() - 1;
        }
    }

    /// Push a result coming off the scanner channel. Caller can preset
    /// `last_modified` if they have it (the TUI does this synchronously
    /// from `fs::metadata` so the `Age` sort is meaningful immediately).
    pub fn push_result(&mut self, found: ScanFoundFolder) {
        self.results.push(FolderResult::from_scan(found));
        self.resort();
    }

    /// Push a result and seed its `last_modified` in one call.
    pub fn push_result_with_mtime(
        &mut self,
        found: ScanFoundFolder,
        last_modified: Option<SystemTime>,
    ) {
        let mut row = FolderResult::from_scan(found);
        row.last_modified = last_modified;
        self.results.push(row);
        self.resort();
    }

    /// Update a row's size after the size-calc task completes.
    pub fn record_size(&mut self, path: &std::path::Path, size: u64) {
        let changed = if let Some(row) = self.results.iter_mut().find(|r| r.path == path) {
            row.size_bytes = Some(size);
            true
        } else {
            false
        };
        // Only re-sort if the change can affect ordering.
        if changed && self.sort == SortBy::Size {
            self.resort();
        }
    }

    /// Record the outcome of a single path's delete attempt and update the
    /// modal-stage progress counter if we're mid-batch.
    ///
    /// - **Real-delete success**: remove the row entirely so the user sees
    ///   the freed space drop off the list. Cursor clamps if needed.
    /// - **Dry-run success**: keep the row but mark `deleted` so the ✗
    ///   strike-through icon shows; the user can still see what would
    ///   have been removed.
    /// - **Failure**: keep the row exactly as it was; the failing path is
    ///   collected into the status message.
    ///
    /// Closes the modal once every batched delete has reported back.
    pub fn record_delete_outcome(
        &mut self,
        path: &std::path::Path,
        success: bool,
        error: Option<String>,
    ) {
        let idx = self.results.iter().position(|r| r.path == path);
        let size = idx.and_then(|i| self.results[i].size_bytes);

        if let Some(i) = idx {
            if success {
                if self.dry_run {
                    self.results[i].deleted = true;
                    self.results[i].selected = false;
                } else {
                    self.results.remove(i);
                    if !self.results.is_empty() && self.cursor >= self.results.len() {
                        self.cursor = self.results.len() - 1;
                    }
                }
            } else {
                // Keep the row but drop its selected flag — the user
                // probably doesn't want to retry the same failure with
                // one more Enter press.
                self.results[i].selected = false;
            }
        }

        // Track first error to surface to the user later.
        if !success && error.is_some() {
            // Stash on last_message right now so single-shot deletes still
            // see the error even though the batch only has one item.
            let msg = error.clone().unwrap_or_else(|| "unknown error".into());
            self.last_message = Some(format!("delete failed: {msg}"));
        }

        // Update the batch progress and decide whether to close the modal.
        if let Mode::Deleting(prog) = &mut self.mode {
            if success {
                prog.completed += 1;
                if let Some(sz) = size {
                    prog.bytes_done = prog.bytes_done.saturating_add(sz);
                }
            } else {
                prog.failed += 1;
            }
            if prog.done() {
                let (completed, failed, total) = (prog.completed, prog.failed, prog.total);
                self.mode = Mode::Browse;
                // Only overwrite `last_message` with a batch summary when we
                // have something more interesting to say than a single-item
                // error. Single-item failure keeps the per-item message we
                // already set above.
                if total > 1 {
                    self.last_message = Some(if failed == 0 {
                        format!("deleted {completed} folders")
                    } else {
                        format!("{completed}/{total} deleted · {failed} failed")
                    });
                } else if failed == 0 {
                    self.last_message = Some(if self.dry_run {
                        "(dry-run) would have deleted".into()
                    } else {
                        "deleted".into()
                    });
                }
            }
        }
    }

    /// Called when the scanner channel closes. Per user request: always
    /// snap the cursor to the first row when the scan completes — even if
    /// the user has been navigating — so the "top hit" is immediately
    /// visible. The user can still scroll afterwards.
    pub fn mark_scan_finished(&mut self) {
        let was_already_finished = self.scan_finished;
        self.scan_finished = true;
        if !was_already_finished && !self.results.is_empty() {
            self.cursor = 0;
            self.user_navigated = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::Duration;

    fn fresh_state() -> AppState {
        AppState::new(PathBuf::from("/root"), vec!["node_modules".into()], false, SortBy::Size)
    }

    fn push(state: &mut AppState, p: &str) {
        state.push_result(ScanFoundFolder::new(PathBuf::from(p), None));
    }

    #[test]
    fn default_sort_is_size_desc() {
        let s = fresh_state();
        assert_eq!(s.sort, SortBy::Size);
        assert_eq!(s.sort_direction, SortDirection::Desc);
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
    fn single_delete_emits_one_path_batch_and_stays_in_deleting() {
        let mut s = fresh_state();
        push(&mut s, "/a/node_modules");
        s.apply(Action::RequestDelete);
        assert_eq!(s.mode, Mode::Confirm(vec![PathBuf::from("/a/node_modules")]));
        let eff = s.apply(Action::ConfirmYes);
        match eff {
            Effect::DeleteBatch(paths) => {
                assert_eq!(paths, vec![PathBuf::from("/a/node_modules")]);
            }
            other => panic!("expected DeleteBatch, got {other:?}"),
        }
        // Modal stays open with the spinner while the FS op runs.
        assert!(matches!(s.mode, Mode::Deleting(_)));
        s.record_delete_outcome(Path::new("/a/node_modules"), true, None);
        assert_eq!(s.mode, Mode::Browse);
        assert!(s.results.is_empty(), "real-delete success should drop the row");
        assert_eq!(s.last_message.as_deref(), Some("deleted"));
    }

    #[test]
    fn delete_failure_keeps_row_and_surfaces_error() {
        let mut s = fresh_state();
        push(&mut s, "/a/node_modules");
        s.apply(Action::RequestDelete);
        s.apply(Action::ConfirmYes);
        assert!(matches!(s.mode, Mode::Deleting(_)));
        s.record_delete_outcome(Path::new("/a/node_modules"), false, Some("perm denied".into()));
        assert_eq!(s.mode, Mode::Browse);
        assert_eq!(s.results.len(), 1, "failed delete must leave the row visible");
        assert!(
            s.last_message.as_deref().unwrap().contains("perm denied"),
            "got {:?}",
            s.last_message
        );
    }

    #[test]
    fn dry_run_keeps_row_and_marks_deleted_visually() {
        let mut s = fresh_state();
        s.dry_run = true;
        push(&mut s, "/a/node_modules");
        s.apply(Action::RequestDelete);
        s.apply(Action::ConfirmYes);
        s.record_delete_outcome(Path::new("/a/node_modules"), true, None);
        assert_eq!(s.results.len(), 1, "dry-run never deletes; row stays");
        assert!(s.results[0].deleted, "dry-run still marks the row visually");
        assert!(s.last_message.as_deref().unwrap().contains("dry-run"));
    }

    #[test]
    fn cursor_clamps_after_removing_last_row() {
        let mut s = fresh_state();
        push(&mut s, "/a/node_modules");
        push(&mut s, "/b/node_modules");
        push(&mut s, "/c/node_modules");
        s.apply(Action::Down);
        s.apply(Action::Down);
        assert_eq!(s.cursor, 2);
        // Delete the row under the cursor (path c).
        s.apply(Action::RequestDelete);
        s.apply(Action::ConfirmYes);
        s.record_delete_outcome(Path::new("/c/node_modules"), true, None);
        assert_eq!(s.results.len(), 2);
        assert_eq!(s.cursor, 1, "cursor should clamp to the new last row");
    }

    #[test]
    fn delete_flow_no_returns_to_browse_without_effect() {
        let mut s = fresh_state();
        push(&mut s, "/a/node_modules");
        s.apply(Action::RequestDelete);
        assert!(matches!(s.mode, Mode::Confirm(_)));
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
    fn cannot_redelete_a_dryrun_deleted_row() {
        let mut s = fresh_state();
        s.dry_run = true;
        push(&mut s, "/a/node_modules");
        s.apply(Action::RequestDelete);
        s.apply(Action::ConfirmYes);
        s.record_delete_outcome(Path::new("/a/node_modules"), true, None);
        assert!(s.results[0].deleted, "precondition: dry-run leaves row marked");
        s.apply(Action::RequestDelete);
        assert_eq!(s.mode, Mode::Browse, "no targets to delete; modal stays closed");
    }

    // ─── Multi-select + batch ───────────────────────────────────────────────

    #[test]
    fn space_toggles_selection_on_cursor_row() {
        let mut s = fresh_state();
        push(&mut s, "/a/node_modules");
        push(&mut s, "/b/node_modules");
        assert_eq!(s.selection_count(), 0);
        s.apply(Action::ToggleSelect);
        assert_eq!(s.selection_count(), 1);
        s.apply(Action::Down);
        s.apply(Action::ToggleSelect);
        assert_eq!(s.selection_count(), 2);
        s.apply(Action::ToggleSelect); // unselect b
        assert_eq!(s.selection_count(), 1);
    }

    #[test]
    fn clear_selection_unsets_every_row() {
        let mut s = fresh_state();
        push(&mut s, "/a");
        push(&mut s, "/b");
        s.apply(Action::ToggleSelect);
        s.apply(Action::Down);
        s.apply(Action::ToggleSelect);
        assert_eq!(s.selection_count(), 2);
        s.apply(Action::ClearSelection);
        assert_eq!(s.selection_count(), 0);
    }

    #[test]
    fn request_delete_uses_selection_when_present() {
        let mut s = fresh_state();
        push(&mut s, "/a/node_modules");
        push(&mut s, "/b/node_modules");
        push(&mut s, "/c/node_modules");
        // Select a and c, cursor still on a (index 0) — Selection wins over cursor.
        s.apply(Action::ToggleSelect); // a
        s.apply(Action::Down);
        s.apply(Action::Down);
        s.apply(Action::ToggleSelect); // c
        s.apply(Action::Up); // cursor on b (unselected)
        s.apply(Action::RequestDelete);
        match &s.mode {
            Mode::Confirm(paths) => {
                let strs: Vec<_> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
                assert!(strs.contains(&"/a/node_modules".to_string()));
                assert!(strs.contains(&"/c/node_modules".to_string()));
                assert!(!strs.contains(&"/b/node_modules".to_string()));
                assert_eq!(strs.len(), 2);
            }
            other => panic!("expected Confirm with 2 paths, got {other:?}"),
        }
    }

    #[test]
    fn batch_delete_progress_closes_modal_when_all_done() {
        let mut s = fresh_state();
        push(&mut s, "/a/node_modules");
        push(&mut s, "/b/node_modules");
        push(&mut s, "/c/node_modules");
        s.apply(Action::ToggleSelect);
        s.apply(Action::Down);
        s.apply(Action::ToggleSelect);
        s.apply(Action::Down);
        s.apply(Action::ToggleSelect);
        assert_eq!(s.selection_count(), 3);
        s.apply(Action::RequestDelete);
        let eff = s.apply(Action::ConfirmYes);
        let Effect::DeleteBatch(paths) = eff else { panic!("expected DeleteBatch") };
        assert_eq!(paths.len(), 3);
        // Modal should reflect 3 in flight.
        match &s.mode {
            Mode::Deleting(p) => {
                assert_eq!(p.total, 3);
                assert_eq!(p.completed, 0);
            }
            other => panic!("expected Deleting, got {other:?}"),
        }
        // Stream outcomes back one at a time.
        s.record_delete_outcome(Path::new("/a/node_modules"), true, None);
        assert!(matches!(s.mode, Mode::Deleting(_)), "modal stays open until all done");
        s.record_delete_outcome(Path::new("/b/node_modules"), true, None);
        assert!(matches!(s.mode, Mode::Deleting(_)));
        s.record_delete_outcome(Path::new("/c/node_modules"), true, None);
        assert_eq!(s.mode, Mode::Browse);
        assert!(s.results.is_empty());
        assert_eq!(s.last_message.as_deref(), Some("deleted 3 folders"));
    }

    #[test]
    fn batch_delete_summary_counts_failures() {
        let mut s = fresh_state();
        push(&mut s, "/a/node_modules");
        push(&mut s, "/b/node_modules");
        s.apply(Action::ToggleSelect);
        s.apply(Action::Down);
        s.apply(Action::ToggleSelect);
        s.apply(Action::RequestDelete);
        s.apply(Action::ConfirmYes);
        s.record_delete_outcome(Path::new("/a/node_modules"), true, None);
        s.record_delete_outcome(Path::new("/b/node_modules"), false, Some("perm".into()));
        assert_eq!(s.mode, Mode::Browse);
        assert!(s.last_message.as_deref().unwrap().contains("1/2 deleted"));
        assert!(s.last_message.as_deref().unwrap().contains("1 failed"));
    }

    #[test]
    fn record_size_updates_matching_row() {
        let mut s = fresh_state();
        push(&mut s, "/a/node_modules");
        push(&mut s, "/b/node_modules");
        s.record_size(Path::new("/a/node_modules"), 42_000);
        let row_a = s.results.iter().find(|r| r.path == Path::new("/a/node_modules")).unwrap();
        let row_b = s.results.iter().find(|r| r.path == Path::new("/b/node_modules")).unwrap();
        assert_eq!(row_a.size_bytes, Some(42_000));
        assert_eq!(row_b.size_bytes, None);
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

    // (Removed `dry_run_message_after_delete` — covered by the path-based
    // `dry_run_keeps_row_and_marks_deleted_visually`.)

    // ─── Sort toggle behaviour ──────────────────────────────────────────────

    #[test]
    fn pressing_size_again_flips_direction() {
        let mut s = fresh_state();
        assert_eq!(s.sort_direction, SortDirection::Desc);
        s.apply(Action::ToggleSortBySize);
        assert_eq!(s.sort, SortBy::Size);
        assert_eq!(s.sort_direction, SortDirection::Asc);
        s.apply(Action::ToggleSortBySize);
        assert_eq!(s.sort_direction, SortDirection::Desc);
    }

    #[test]
    fn switching_from_size_to_name_uses_default_asc() {
        let mut s = fresh_state(); // Size + Desc
        s.apply(Action::ToggleSortByName);
        assert_eq!(s.sort, SortBy::Path);
        assert_eq!(s.sort_direction, SortDirection::Asc);
    }

    #[test]
    fn switching_to_last_used_uses_default_desc() {
        let mut s = fresh_state();
        s.apply(Action::ToggleSortByName); // somewhere else
        s.apply(Action::ToggleSortByLastUsed);
        assert_eq!(s.sort, SortBy::Age);
        assert_eq!(s.sort_direction, SortDirection::Desc);
        // Toggling again flips.
        s.apply(Action::ToggleSortByLastUsed);
        assert_eq!(s.sort_direction, SortDirection::Asc);
    }

    #[test]
    fn resort_keeps_cursor_on_same_row_after_user_navigates() {
        let mut s = fresh_state(); // sort=Size desc
        push(&mut s, "/aaa");
        push(&mut s, "/bbb");
        push(&mut s, "/ccc");
        s.record_size(Path::new("/aaa"), 100);
        s.record_size(Path::new("/bbb"), 999);
        s.record_size(Path::new("/ccc"), 500);
        // Order under Size+Desc: bbb(999), ccc(500), aaa(100).
        // Move cursor down to the middle (ccc) — this also flips user_navigated.
        s.apply(Action::Down);
        assert!(s.user_navigated);
        let ccc_idx = s.results.iter().position(|r| r.path == Path::new("/ccc")).unwrap();
        assert_eq!(s.cursor, ccc_idx);
        // Flip to Asc.
        s.apply(Action::ToggleSortBySize);
        // New order: aaa(100), ccc(500), bbb(999). Cursor should still point to ccc.
        let new_idx = s.results.iter().position(|r| r.path == Path::new("/ccc")).unwrap();
        assert_eq!(s.cursor, new_idx);
    }

    // ─── Auto-top + rescan behaviour ────────────────────────────────────────

    #[test]
    fn cursor_pinned_at_top_during_streaming_until_user_navigates() {
        // Simulate live streaming: rows arrive one-by-one with sizes filling in.
        let mut s = fresh_state();
        let scan = |p: &str| ScanFoundFolder::new(PathBuf::from(p), None);
        s.push_result(scan("/small"));
        s.record_size(Path::new("/small"), 100);
        // Now a bigger row arrives — it should sort to the top under Size+Desc,
        // and because user hasn't navigated, the cursor must move to the new top row.
        s.push_result(scan("/big"));
        s.record_size(Path::new("/big"), 10_000);
        assert_eq!(s.cursor, 0);
        assert_eq!(s.selected().unwrap().path, PathBuf::from("/big"));

        // User presses Down → user_navigated flips → cursor follows the row.
        s.apply(Action::Down);
        assert!(s.user_navigated);
        let small_idx = s.results.iter().position(|r| r.path == Path::new("/small")).unwrap();
        assert_eq!(s.cursor, small_idx);

        // Another row arrives mid-stream; cursor should STAY on /small now.
        s.push_result(scan("/medium"));
        s.record_size(Path::new("/medium"), 1_000);
        assert_eq!(s.selected().unwrap().path, PathBuf::from("/small"));
    }

    #[test]
    fn scan_finished_snaps_cursor_to_top_even_if_user_navigated() {
        let mut s = fresh_state();
        push(&mut s, "/a");
        push(&mut s, "/b");
        push(&mut s, "/c");
        // User moved around mid-scan.
        s.apply(Action::Down);
        s.apply(Action::Down);
        assert!(s.cursor > 0);
        // Scan completes.
        s.mark_scan_finished();
        assert_eq!(s.cursor, 0, "post-scan cursor must snap to top");
        assert!(s.scan_finished);
        // user_navigated also resets so further streaming-style pushes pin to top
        // (in case the user later rescans).
        assert!(!s.user_navigated);
    }

    #[test]
    fn scan_finished_is_idempotent_does_not_reset_cursor_on_redundant_calls() {
        let mut s = fresh_state();
        push(&mut s, "/a");
        push(&mut s, "/b");
        push(&mut s, "/c");
        s.mark_scan_finished();
        assert_eq!(s.cursor, 0);
        // Now user navigates AFTER scan finished.
        s.apply(Action::Down);
        assert_eq!(s.cursor, 1);
        // A second mark_scan_finished call (e.g., main loop double-checks) must
        // NOT yank the cursor back to 0 again.
        s.mark_scan_finished();
        assert_eq!(s.cursor, 1, "redundant mark_scan_finished should be a no-op");
    }

    #[test]
    fn rescan_clears_results_and_emits_effect() {
        let mut s = fresh_state();
        push(&mut s, "/a");
        push(&mut s, "/b");
        s.apply(Action::Down);
        s.mark_scan_finished();
        // Sanity.
        assert_eq!(s.results.len(), 2);
        assert!(s.scan_finished);

        let eff = s.apply(Action::Rescan);
        assert_eq!(eff, Effect::Rescan);
        assert!(s.results.is_empty());
        assert_eq!(s.cursor, 0);
        assert!(!s.scan_finished);
        assert!(!s.user_navigated);
        // status message acknowledges the action
        assert!(s.last_message.as_deref().unwrap_or("").contains("rescan"));
    }

    #[test]
    fn rescan_ignored_in_confirm_mode() {
        let mut s = fresh_state();
        push(&mut s, "/a/node_modules");
        s.apply(Action::RequestDelete);
        assert!(matches!(s.mode, Mode::Confirm(_)));
        let eff = s.apply(Action::Rescan);
        assert_eq!(eff, Effect::None);
        // Modal is still active and results unchanged.
        assert!(matches!(s.mode, Mode::Confirm(_)));
        assert_eq!(s.results.len(), 1);
    }

    #[test]
    fn push_keeps_results_sorted() {
        let mut s = fresh_state();
        // Push in random order with sizes preset via push then record_size.
        let scan = |p: &str| ScanFoundFolder::new(PathBuf::from(p), None);
        s.push_result(scan("/small"));
        s.record_size(Path::new("/small"), 100);
        s.push_result(scan("/big"));
        s.record_size(Path::new("/big"), 10_000);
        s.push_result(scan("/medium"));
        s.record_size(Path::new("/medium"), 1_000);
        // Default Size + Desc → big, medium, small.
        let paths: Vec<_> =
            s.results.iter().map(|r| r.path.to_string_lossy().into_owned()).collect();
        assert_eq!(paths, vec!["/big", "/medium", "/small"]);
    }

    #[test]
    fn age_sort_uses_last_modified() {
        let mut s = fresh_state();
        let now = SystemTime::now();
        s.push_result_with_mtime(
            ScanFoundFolder::new(PathBuf::from("/recent"), None),
            Some(now - Duration::from_secs(10)),
        );
        s.push_result_with_mtime(
            ScanFoundFolder::new(PathBuf::from("/ancient"), None),
            Some(now - Duration::from_secs(10_000)),
        );
        s.push_result_with_mtime(ScanFoundFolder::new(PathBuf::from("/no-mtime"), None), None);
        s.apply(Action::ToggleSortByLastUsed); // Age + Desc (newest first)
        let paths: Vec<_> =
            s.results.iter().map(|r| r.path.to_string_lossy().into_owned()).collect();
        assert_eq!(paths, vec!["/recent", "/ancient", "/no-mtime"]);
    }
}
