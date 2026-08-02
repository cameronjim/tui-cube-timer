//! The two popups drawn over the frame: the key and command reference, and one solve in full.
//!
//! Both are read-only over [`App`] like the rest of [`ui`](super), and both clamp themselves
//! to the terminal instead of assuming there is room.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use super::layout::{centered, detail_popup, puzzle_help_rows, HELP_KEY_W, HELP_W};
use super::{dim, C_ACCENT, C_IDLE, C_INSPECT, C_TIMING, C_WORST};
use crate::app::App;
use crate::types::{format_solve, format_timestamp, Penalty, Puzzle};

/// The border both popups share, titled and accented so they read as one layer.
fn popup_block(title: String) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_ACCENT))
        .title(title)
}

// ----------------------------------------------------------- help overlay

fn help_row<'a>(key: &'a str, desc: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(
            format!("  {:<width$}", key, width = HELP_KEY_W),
            Style::default().fg(C_TIMING).add_modifier(Modifier::BOLD),
        ),
        Span::styled(desc, Style::default().fg(C_IDLE)),
    ])
}

fn help_head(text: &str) -> Line<'_> {
    Line::styled(
        text,
        Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD),
    )
}

pub(super) fn draw_help(frame: &mut Frame, area: Rect) {
    // Kept in sync with the supported puzzles rather than hard-coded.
    let names: Vec<&str> = Puzzle::ALL.iter().map(|p| p.name()).collect();
    let puzzle_rows = puzzle_help_rows(&names);

    let mut lines: Vec<Line> = vec![
        help_head(" keys"),
        help_row("space", "hold until green, release to start"),
        help_row("any key", "stop the running timer"),
        help_row("esc", "cancel inspection / leave command mode"),
        help_row("n", "new scramble"),
        help_row("↑ / ↓", "move the selection in the times list"),
        help_row("enter", "details for the selected solve"),
        help_row("r / esc", "in details: load its scramble / close"),
        help_row("h / ?", "toggle this help"),
        help_row("/", "command mode"),
        help_row("q", "quit"),
        Line::from(""),
        help_head(" commands"),
    ];
    for (i, row) in puzzle_rows.iter().enumerate() {
        lines.push(help_row(if i == 0 { "/<puzzle>" } else { "" }, row));
    }
    lines.extend([
        help_row("/new [name]", "new session for the current puzzle"),
        help_row("/sessions", "list all sessions"),
        help_row("/session <id>", "switch to session by id"),
        help_row("/rename <name>", "rename the current session"),
        help_row("/delsession", "delete a session by id (default: current)"),
        help_row("/del [n]", "delete solve n, newest by default"),
        help_row("/dnf  /+2  /ok", "set the last solve's penalty"),
        help_row("/inspect", "toggle 15s inspection (off by default)"),
        help_row("/hidetime", "toggle hiding the time while solving"),
        help_row("/help", "toggle this help"),
        help_row("/quit  /q", "quit"),
        Line::from(""),
        Line::styled("  press h, ? or esc to close", dim()),
    ]);

    // The popup follows the content, which grows with the number of puzzles.
    let content_h = lines.len().min(u16::MAX as usize) as u16;
    let popup = centered(HELP_W, content_h.saturating_add(2), area);
    if popup.width < 4 || popup.height < 4 {
        return;
    }

    frame.render_widget(Clear, popup);
    let p = Paragraph::new(Text::from(lines)).block(popup_block(" help ".to_string()));
    frame.render_widget(p, popup);
}

// --------------------------------------------------- solve detail overlay

/// One solve in full: its time, when it was recorded, and the scramble it was recorded on.
///
/// `index` counts from the newest solve, matching the times list, and is clamped rather than
/// trusted: the solve it points at can be deleted between the keypress and the frame.
pub(super) fn draw_detail(frame: &mut Frame, app: &App, index: usize, area: Rect) {
    let solves = &app.current_session().solves;
    let total = solves.len();
    if total == 0 {
        return;
    }
    let index = index.min(total - 1);
    let solve = &solves[total - 1 - index];

    let time_color = match solve.penalty {
        Penalty::Dnf => C_WORST,
        Penalty::Plus2 => C_INSPECT,
        Penalty::None => C_IDLE,
    };
    let mut lines: Vec<Line> = vec![
        Line::styled(
            format_solve(solve),
            Style::default().fg(time_color).add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::styled(format_timestamp(solve.timestamp), dim()),
        Line::from(""),
    ];
    if solve.scramble.is_empty() {
        lines.push(Line::styled("(no scramble recorded)", dim()));
    } else {
        let style = Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD);
        for line in solve.scramble.split('\n') {
            lines.push(Line::styled(line.to_string(), style));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::styled("r: load scramble   esc: close", dim()));

    let popup = detail_popup(&solve.scramble, area);
    if popup.width < 4 || popup.height < 4 {
        return;
    }

    frame.render_widget(Clear, popup);
    let p = Paragraph::new(Text::from(lines))
        .block(popup_block(format!(" solve {} ", total - index)))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(p, popup);
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::super::testkit::{app_with, mega, render, render_all};
    use crate::app::TimerState;
    use crate::types::Puzzle;

    #[test]
    fn the_solve_detail_overlay_renders_a_megaminx_scramble_at_every_size() {
        let mut app = app_with(Puzzle::Megaminx, 6);
        let last = app.current_session().solves.len() - 1;
        app.current_session_mut().solves[last].scramble = mega();
        app.solve_detail = Some(0);
        render_all(&app);

        let text = render(&app, 80, 30);
        assert!(text.contains(" solve 6 "), "the popup is titled with the number");
        assert!(text.contains("D++"), "the solve's own scramble is shown");
        assert!(text.contains("UTC"), "the date line is shown");
        assert!(text.contains("esc: close"), "the hint line is shown");

        // A small terminal clamps the popup instead of panicking or hiding it.
        assert!(render(&app, 30, 8).contains("D++"));
    }

    #[test]
    fn the_solve_detail_overlay_wins_over_the_help_overlay() {
        let mut app = app_with(Puzzle::Cube3, 3);
        app.show_help = true;
        app.solve_detail = Some(1);
        let text = render(&app, 80, 40);
        assert!(text.contains("esc: close"), "the detail popup is drawn");
        assert!(!text.contains("switch puzzle"), "the help popup is not");
        assert!(text.contains(" solve 2 "), "index 1 is the second newest");
    }

    #[test]
    fn a_solve_detail_index_with_nothing_behind_it_renders_at_every_size() {
        // The solve can be deleted between the keypress that opened the popup and the frame.
        let mut app = app_with(Puzzle::Cube3, 0);
        app.solve_detail = Some(0);
        render_all(&app);
        let mut app = app_with(Puzzle::Cube3, 3);
        app.solve_detail = Some(usize::MAX);
        render_all(&app);
        assert!(render(&app, 80, 30).contains(" solve 1 "), "clamped to the oldest");
    }

    #[test]
    fn the_help_overlay_renders_at_every_size() {
        let mut app = app_with(Puzzle::Clock, 7);
        app.show_help = true;
        render_all(&app);
        let text = render(&app, 80, 40);
        assert!(text.contains("switch puzzle"));
        for row in [
            "details for the selected solve",
            "in details: load its scramble / close",
            "delete solve n, newest by default",
            "toggle hiding the time while solving",
        ] {
            assert!(text.contains(row), "the help is missing {:?}", row);
        }
        assert!(
            text.contains("/hidetime") && text.contains("/del [n]"),
            "the help is missing a command name"
        );
    }

    /// The overlays sit on top of the frame, so an open one must survive every timer state.
    #[test]
    fn an_overlay_renders_over_a_running_timer() {
        let mut app = app_with(Puzzle::Cube3, 4);
        app.state = TimerState::Timing {
            started: std::time::Instant::now(),
        };
        app.show_help = true;
        render_all(&app);
        app.solve_detail = Some(0);
        render_all(&app);
    }
}
