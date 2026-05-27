//! ratatui draw functions. Stateless: take `AppState`, write to a Frame.
//!
//! Layout (top to bottom):
//! ```text
//! ┌────────────────────────────────────────────────────────────┐
//! │ nmk  scan /root   N found · 12.3 GB total · scanning…  [dry-run] │  header (3 lines)
//! ├────────────────────────────────────────────────────────────┤
//! │ ⚠ /root/proj-a/node_modules                        1.2 GB │  table (flex)
//! │   /root/proj-b/node_modules                          250 MB │
//! │ ► /root/proj-c/node_modules                          120 MB │   ← cursor row
//! ├────────────────────────────────────────────────────────────┤
//! │ ↑↓ navigate  d delete  q quit                              │  status (1 line)
//! └────────────────────────────────────────────────────────────┘
//! ```
//!
//! When the user requests a delete, a centred modal overlays the screen.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState};

use crate::core::types::SortBy;
use crate::tui::app::{AppState, Mode};

pub fn draw(frame: &mut Frame<'_>, state: &AppState) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    draw_header(frame, chunks[0], state);
    draw_table(frame, chunks[1], state);
    draw_status(frame, chunks[2], state);

    if let Mode::Confirm(idx) = &state.mode {
        let row_path = state
            .results
            .get(*idx)
            .map(|r| r.path.display().to_string())
            .unwrap_or_else(|| "<missing>".into());
        draw_confirm_modal(frame, area, &row_path, state.dry_run);
    }
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let scan_state = if state.scan_finished { "done" } else { "scanning…" };
    let dry_badge = if state.dry_run { " [dry-run]" } else { "" };
    let title = Line::from(vec![
        Span::styled("nmk", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::raw(state.root.display().to_string()),
    ]);
    let sort_label = match state.sort {
        SortBy::Path => "name",
        SortBy::Size => "size",
        SortBy::Age => "last-used",
    };
    let stats = Line::from(vec![
        Span::raw(format!("{} found", state.results.len())),
        Span::raw(" · "),
        Span::raw(human_bytes(state.total_size())),
        Span::raw(" total · "),
        Span::styled(scan_state, Style::default().fg(Color::Yellow)),
        Span::raw(" · "),
        Span::styled("sort: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{sort_label} {}", state.sort_direction.indicator()),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(dry_badge, Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
    ]);

    let p = Paragraph::new(vec![title, stats]).block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(p, area);
}

fn draw_table(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let header = Row::new(vec![Cell::from("!"), Cell::from("path"), Cell::from("size")])
        .style(Style::default().fg(Color::DarkGray));

    let rows: Vec<Row> = state
        .results
        .iter()
        .map(|r| {
            let risk_marker = match &r.risk {
                Some(a) if a.is_sensitive => Span::styled(
                    "⚠",
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
                _ => Span::raw(" "),
            };
            let path = if r.deleted {
                Span::styled(
                    r.path.display().to_string(),
                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::CROSSED_OUT),
                )
            } else {
                Span::raw(r.path.display().to_string())
            };
            let size = match r.size_bytes {
                Some(b) => human_bytes(b),
                None => "…".to_string(),
            };
            Row::new(vec![Cell::from(risk_marker), Cell::from(path), Cell::from(size)])
        })
        .collect();

    let widths = [Constraint::Length(2), Constraint::Min(20), Constraint::Length(12)];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::NONE))
        .row_highlight_style(
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("► ");

    let mut t_state = TableState::default();
    if !state.results.is_empty() {
        t_state.select(Some(state.cursor.min(state.results.len() - 1)));
    }

    frame.render_stateful_widget(table, area, &mut t_state);
}

fn draw_status(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let hint = match state.mode {
        Mode::Browse => "↑↓/jk navigate · d/Enter delete · s size · n name · m last-used · q quit",
        Mode::Confirm(_) => "y delete · n / Esc cancel",
    };
    let mut spans = vec![Span::styled(hint, Style::default().fg(Color::DarkGray))];
    if let Some(msg) = &state.last_message {
        spans.push(Span::raw("    "));
        spans.push(Span::styled(msg.clone(), Style::default().fg(Color::Green)));
    }
    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Left), area);
}

fn draw_confirm_modal(frame: &mut Frame<'_>, area: Rect, target_path: &str, dry_run: bool) {
    let width = area.width.saturating_mul(2) / 3;
    let height = 7u16.min(area.height);
    let x = area.x + (area.width - width) / 2;
    let y = area.y + (area.height - height) / 2;
    let modal = Rect { x, y, width, height };

    frame.render_widget(Clear, modal);

    let body = vec![
        Line::from(""),
        Line::from(Span::styled(
            target_path.to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(if dry_run {
            Span::styled(
                "Dry-run: nothing will actually be deleted",
                Style::default().fg(Color::Magenta),
            )
        } else {
            Span::styled(
                "This will be permanently deleted",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )
        }),
        Line::from(""),
        Line::from("press  y  to confirm   ·   n / Esc  to cancel"),
    ];
    let p = Paragraph::new(body)
        .alignment(Alignment::Center)
        .block(Block::default().title(" delete? ").borders(Borders::ALL));
    frame.render_widget(p, modal);
}

/// Format bytes with a 2-decimal MB or GB suffix, npkill-style.
fn human_bytes(b: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = b as f64;
    if b < MB {
        format!("{:.0} KB", b / KB)
    } else if b < GB {
        format!("{:.1} MB", b / MB)
    } else {
        format!("{:.2} GB", b / GB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_kb_range() {
        assert!(human_bytes(0).ends_with("KB"));
        assert!(human_bytes(50 * 1024).ends_with("KB"));
    }

    #[test]
    fn human_bytes_mb_range() {
        let s = human_bytes(5 * 1024 * 1024);
        assert!(s.ends_with("MB"), "got {s}");
    }

    #[test]
    fn human_bytes_gb_range() {
        let s = human_bytes(3 * 1024 * 1024 * 1024);
        assert!(s.ends_with("GB"), "got {s}");
    }
}
