//! The five popups drawn over the frame: the key and command reference, the session picker,
//! the trend graph, the scramble as a cube, and one solve in full.
//!
//! All are read-only over [`App`] like the rest of [`ui`](super), and all clamp themselves
//! to the terminal instead of assuming there is room.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Axis, Block, BorderType, Borders, Chart, Clear, Dataset, GraphType, Paragraph, Wrap,
};
use ratatui::Frame;

use super::layout::{
    centered, detail_popup, inner_of, list_window, puzzle_help_rows, sessions_popup, trend_popup,
    HELP_KEY_W, HELP_W, SESSIONS_NAME_W,
};
use super::{dim, net, C_ACCENT, C_IDLE, C_INSPECT, C_TIMING, C_WORST};
use crate::app::App;
use crate::cube::Cube;
use crate::types::{format_millis, format_solve, format_timestamp, Penalty, Puzzle};

/// The border every popup shares, titled and accented so they read as one layer.
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
        help_row("/sessions", "every session in a popup"),
        help_row("/session <id>", "switch to session by id"),
        help_row("/rename <name>", "rename the current session"),
        help_row("/delsession", "delete a session by id (default: current)"),
        help_row("/delsession a-b", "delete every session in the id range"),
        help_row("/del [n]", "delete solve n, newest by default"),
        help_row("/dnf  /+2  /ok", "set the last solve's penalty"),
        help_row("/inspect", "toggle 15s inspection (off by default)"),
        help_row("/hidetime", "toggle hiding the time while solving"),
        help_row("/export [path]", "write your times as a csTimer .txt file"),
        help_row("/import <path>", "bring csTimer sessions in as new sessions"),
        help_row("/trend", "graph the last 50 solves"),
        help_row("/preview", "show the cube the scramble makes"),
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

// ------------------------------------------------------- sessions overlay

/// `name` in exactly `width` columns, so the columns beside it line up whatever it holds.
///
/// A name too long for the column ends in an ellipsis, which is the only thing that tells
/// two sessions sharing a long prefix apart from one that simply ends there.
fn clip(name: &str, width: usize) -> String {
    let len = name.chars().count();
    if len > width {
        let mut out: String = name.chars().take(width.saturating_sub(1)).collect();
        out.push('…');
        return out;
    }
    let mut out = name.to_string();
    for _ in len..width {
        out.push(' ');
    }
    out
}

/// Every session in one popup: id, name, puzzle and solve count, under a moving cursor.
///
/// `cursor` indexes `app.save.sessions` in the order they are drawn and is clamped rather than
/// trusted. The two states a row can be in are deliberately different marks: the active session
/// keeps the `>` in the marker column, the cursor reverses its whole row, and the row that is
/// both reads as a reversed row with a marker on it. A list taller than the popup scrolls under
/// the cursor, and the title counts the position so the rows outside the window are accounted for.
pub(super) fn draw_sessions(frame: &mut Frame, app: &App, cursor: usize, area: Rect) {
    let sessions = &app.save.sessions;
    let total = sessions.len();
    let (popup, rows) = sessions_popup(total, area);
    if popup.width < 4 || popup.height < 4 {
        return;
    }
    let cursor = cursor.min(total.saturating_sub(1));
    let (start, len) = list_window(cursor, total, rows);

    let mut lines: Vec<Line> = Vec::with_capacity(len.saturating_add(1));
    for (offset, session) in sessions.iter().skip(start).take(len).enumerate() {
        // Twelve of these are named `default`, so dimming that name is what makes the
        // sessions someone made themselves findable in the list.
        let name_style = if session.is_default() {
            dim()
        } else {
            Style::default().fg(C_IDLE)
        };
        let active = session.id == app.save.active_session_id;
        let marker_style = if active {
            Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD)
        } else {
            dim()
        };
        let line = Line::from(vec![
            Span::styled(if active { ">" } else { " " }, marker_style),
            Span::styled(format!("{:>3}  ", session.id), dim()),
            Span::styled(clip(&session.name, SESSIONS_NAME_W), name_style),
            Span::styled(
                format!(" {:<8} ", session.puzzle.name()),
                Style::default().fg(C_ACCENT),
            ),
            Span::styled(format!("[{}]", session.solves.len()), dim()),
        ]);
        // Reversed rather than recoloured, so the dim default names still read on it.
        lines.push(if start + offset == cursor {
            line.patch_style(Style::default().add_modifier(Modifier::REVERSED))
        } else {
            line
        });
    }

    lines.push(Line::styled("  enter: switch   esc: close", dim()));

    // The position stands in for the rows the window left out, and only appears when it did.
    let title = if len < total {
        format!(" sessions {}/{} ", cursor.saturating_add(1), total)
    } else {
        " sessions ".to_string()
    };

    frame.render_widget(Clear, popup);
    let p = Paragraph::new(Text::from(lines)).block(popup_block(title));
    frame.render_widget(p, popup);
}

// ---------------------------------------------------------- trend overlay

/// Percentile of the window the top of the y axis is pinned to, as a percentage.
const TREND_PERCENTILE: usize = 95;
/// Shortest window whose slowest solve may be left outside the y axis.
///
/// Under this there are too few solves to call any of them an outlier, so the axis tops out at
/// the window maximum. At or above it the slowest is always clipped, which costs the single
/// distinction between it and the next slowest and buys the whole of the range below.
const TREND_TRIM_MIN: usize = 5;
/// Half the y range a window with nothing to separate is drawn against, so its line sits mid height.
const TREND_FLAT_PAD: f64 = 1.0;

/// One trend graph: the points to plot and the axis bounds they are plotted against.
#[derive(Debug, PartialEq)]
struct TrendPlot {
    points: Vec<(f64, f64)>,
    x_bounds: [f64; 2],
    y_bounds: [f64; 2],
}

/// The top of the y axis: the [`TREND_PERCENTILE`] of `sorted` by nearest rank.
///
/// `sorted` is ascending and non-empty. Nearest rank alone is the window maximum until twenty
/// entries, which is the length at which one wrecked solve does the most damage, so from
/// [`TREND_TRIM_MIN`] the rank is also held below the last entry.
fn trend_top(sorted: &[u64]) -> u64 {
    let n = sorted.len();
    let mut rank = (n * TREND_PERCENTILE).div_ceil(100).max(1);
    if n >= TREND_TRIM_MIN {
        rank = rank.min(n - 1);
    }
    sorted[rank - 1]
}

/// The window as a line: x is a solve's place in it, y is the time that solve cost.
///
/// Solve times cluster in a band far away from zero, so the axis spans the window's own range
/// rather than starting at zero, which draws every session as one flat line near the top. The
/// ceiling is [`trend_top`] rather than the window maximum, and anything above it is clamped
/// onto it, so a single 60 second solve among twelve second ones pins to the top edge instead
/// of crushing every other point onto the bottom row. A window with nothing to separate draws
/// flat at mid height, and a lone solve is the flat line it is rather than a dot in the corner.
fn trend_plot(window: &[u64]) -> TrendPlot {
    let mut sorted: Vec<u64> = window.to_vec();
    sorted.sort_unstable();
    let (min, top) = match sorted.first() {
        Some(min) => (*min, trend_top(&sorted)),
        None => (0, 0),
    };

    let mut points: Vec<(f64, f64)> = window
        .iter()
        .enumerate()
        .map(|(i, v)| (i as f64, (*v).min(top) as f64))
        .collect();
    if points.len() == 1 {
        points.push((1.0, points[0].1));
    }

    let (lo, hi) = if top > min {
        (min as f64, top as f64)
    } else {
        (min as f64 - TREND_FLAT_PAD, min as f64 + TREND_FLAT_PAD)
    };
    let right = points.len().saturating_sub(1).max(1) as f64;
    TrendPlot {
        points,
        x_bounds: [0.0, right],
        y_bounds: [lo, hi],
    }
}

/// The three y ticks, bottom first, which is the order ratatui stacks them in.
fn trend_ticks(bounds: [f64; 2]) -> Vec<Line<'static>> {
    let mid = (bounds[0] + bounds[1]) / 2.0;
    [bounds[0], mid, bounds[1]]
        .into_iter()
        .map(|v| Line::styled(format_millis(v.max(0.0) as u64), dim()))
        .collect()
}

/// The last fifty solves of the session as a line graph, time up the side, solve number along.
///
/// The whole window is plotted whatever the popup's width, because a line stays a line when two
/// solves share a column, and dropping the oldest to avoid that would make the axis lie. The
/// marker is `HalfBlock`, whose only glyphs are `▀`, `▄` and `█`: ratatui's default braille dots
/// and the eighth blocks the trend was drawn with before are both missing from the classic
/// Windows console fonts and render as tofu, while all three of these are CP437 characters every
/// console font has. The axis lines are `─`, `│` and `└`, which are CP437 as well.
pub(super) fn draw_trend(frame: &mut Frame, app: &App, area: Rect) {
    let Some(popup) = trend_popup(area) else {
        return;
    };
    frame.render_widget(Clear, popup);
    frame.render_widget(popup_block(" trend ".to_string()), popup);

    let inner = inner_of(popup);
    if inner.width == 0 || inner.height < 2 {
        return;
    }
    // The hint keeps the bottom row, as it does in the other popups that carry one.
    let graph = Rect {
        height: inner.height.saturating_sub(1),
        ..inner
    };
    let hint = Rect {
        y: inner.y.saturating_add(graph.height),
        height: 1,
        ..inner
    };
    frame.render_widget(Paragraph::new(Line::styled("  esc: close", dim())), hint);

    if app.trend.is_empty() {
        let empty = Paragraph::new(Line::styled("no solves to plot yet", dim()))
            .alignment(Alignment::Center);
        frame.render_widget(empty, graph);
        return;
    }

    let plot = trend_plot(&app.trend);
    let chart = Chart::new(vec![Dataset::default()
        .marker(Marker::HalfBlock)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(C_TIMING))
        .data(&plot.points)])
    .x_axis(
        Axis::default()
            .style(dim())
            .bounds(plot.x_bounds)
            .labels([
                Line::styled("1", dim()),
                Line::styled(app.trend.len().to_string(), dim()),
            ]),
    )
    .y_axis(
        Axis::default()
            .style(dim())
            .bounds(plot.y_bounds)
            .labels(trend_ticks(plot.y_bounds))
            .labels_alignment(Alignment::Right),
    )
    .legend_position(None);
    frame.render_widget(chart, graph);
}

// -------------------------------------------------------- preview overlay

/// Columns the preview popup falls back to when it has a sentence to show instead of a net.
const PREVIEW_MSG_W: u16 = 30;

/// Where the preview popup sits, and which net mode fits inside it.
///
/// `n` is the cube's size, or None for an event with no model. The popup is sized to the net it
/// holds, so the mode is settled before the border is drawn: full size first, the half block
/// compact net second, and the message box last. Unlike the trend graph the popup is never
/// skipped outright, because "too small" is worth saying when the user just asked for it.
fn preview_popup(n: Option<u8>, area: Rect) -> (Rect, Option<bool>) {
    if let Some(n) = n {
        for compact in [false, true] {
            let (w, h) = net::size(n, compact);
            let (want_w, want_h) = (w.saturating_add(2), h.saturating_add(2));
            let popup = centered(want_w, want_h, area);
            if popup.width == want_w && popup.height == want_h {
                return (popup, Some(compact));
            }
        }
    }
    (centered(PREVIEW_MSG_W, 3, area), None)
}

/// The scramble on screen as the cube it produces: six faces unfolded into a flat net.
///
/// Every sticker is read from [`App::preview`], which `app` rebuilt when the scramble changed,
/// so this draws a cube it never turns. Seven of the twelve events have a model; the other five
/// get the line saying so, as does a terminal with no room for even the compact net.
pub(super) fn draw_preview(frame: &mut Frame, app: &App, area: Rect) {
    let (popup, mode) = preview_popup(app.preview.as_ref().map(Cube::n), area);
    if popup.width < 4 || popup.height < 3 {
        return;
    }

    let title = format!(" {} preview ", app.current_session().puzzle.name());
    frame.render_widget(Clear, popup);
    frame.render_widget(popup_block(title), popup);

    let inner = inner_of(popup);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    match (app.preview.as_ref(), mode) {
        (Some(cube), Some(compact)) => {
            let net_view = Paragraph::new(Text::from(net::lines(cube, compact)))
                .alignment(Alignment::Center);
            frame.render_widget(net_view, inner);
        }
        // A cube with nowhere to draw it, then an event whose cube Cubetimer cannot build yet.
        (cube, _) => {
            let text = if cube.is_some() {
                "terminal too small"
            } else {
                "no preview for this event"
            };
            let msg = Paragraph::new(Line::styled(text, dim())).alignment(Alignment::Center);
            frame.render_widget(msg, inner);
        }
    }
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
    use super::super::testkit::{
        app_with, mega, render, render_all, render_buffer, row_cells, row_with, rows_of,
    };
    use super::*;
    use crate::app::{App, TimerState};
    use crate::types::{Puzzle, Session, FIRST_USER_ID};
    use ratatui::buffer::Buffer;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::style::{Color, Modifier};

    /// Cells of row `y` drawn reversed, which is how the cursor marks the row it is on.
    fn reversed_in(buffer: &Buffer, y: usize) -> usize {
        row_cells(buffer, y)
            .iter()
            .filter(|c| c.modifier.contains(Modifier::REVERSED))
            .count()
    }

    /// Add sessions of your own on top of the twelve permanent defaults.
    fn with_user_sessions(app: &mut App, names: &[&str]) {
        for (i, name) in names.iter().enumerate() {
            app.save.sessions.push(Session {
                id: FIRST_USER_ID + i as u64,
                name: (*name).to_string(),
                puzzle: Puzzle::Cube3,
                solves: Vec::new(),
                created_at: 1_700_000_000_000,
            });
        }
    }

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
            "delete every session in the id range",
            "toggle hiding the time while solving",
            "write your times as a csTimer .txt file",
            "bring csTimer sessions in as new sessions",
        ] {
            assert!(text.contains(row), "the help is missing {:?}", row);
        }
        assert!(
            text.contains("/hidetime") && text.contains("/del [n]"),
            "the help is missing a command name"
        );
        assert!(
            text.contains("/export [path]") && text.contains("/import <path>"),
            "the help is missing the csTimer commands"
        );
        assert!(
            text.contains("press h, ? or esc to close"),
            "the popup still fits its last row at forty rows"
        );
    }

    #[test]
    fn the_sessions_overlay_lists_every_session_at_every_size() {
        let mut app = app_with(Puzzle::Cube3, 5);
        with_user_sessions(&mut app, &["morning", "evening", "one-handed drills, sub-20 push"]);
        app.sessions_overlay = Some(0);
        render_all(&app);

        let buffer = render_buffer(&app, 80, 30);
        let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
        assert!(text.contains(" sessions "), "the popup is titled");
        assert!(
            !text.contains("sessions 1/15"),
            "the title counts the position only when a row was left out"
        );
        assert!(
            text.contains(">  1  default"),
            "the active session carries the marker"
        );
        assert!(text.contains("[5]"), "its solve count is shown");
        for name in ["morning", "evening"] {
            assert!(text.contains(name), "the listing is missing {:?}", name);
        }
        assert!(
            text.contains("one-handed drills…"),
            "a name past the column ends in an ellipsis"
        );
        assert!(text.contains("megaminx"), "every default is listed too");
        assert!(
            text.contains("enter: switch   esc: close"),
            "the hint names both keys the overlay answers to"
        );

        // A small terminal clamps the popup instead of panicking or hiding it.
        assert!(render(&app, 30, 8).contains(" sessions "));
    }

    #[test]
    fn the_cursor_reverses_its_row_and_the_active_session_keeps_its_marker() {
        let mut app = app_with(Puzzle::Cube3, 5);
        with_user_sessions(&mut app, &["morning", "evening", "night"]);

        // On the active session the two marks stack: a reversed row with the marker on it.
        app.sessions_overlay = Some(0);
        let buffer = render_buffer(&app, 80, 30);
        let active = row_with(&buffer, ">  1  default");
        assert!(
            reversed_in(&buffer, active) > 0,
            "the cursor reverses the row it sits on"
        );

        // Away from it the marker is the only thing saying which session is live.
        app.sessions_overlay = Some(12);
        let buffer = render_buffer(&app, 80, 30);
        let active = row_with(&buffer, ">  1  default");
        let cursor = row_with(&buffer, "morning");
        assert_ne!(active, cursor, "the two rows are different rows");
        assert_eq!(
            reversed_in(&buffer, active),
            0,
            "the active row is not the cursor row"
        );
        assert!(
            reversed_in(&buffer, cursor) > 0,
            "the cursor took its own row with it"
        );
        assert_eq!(
            reversed_in(&buffer, row_with(&buffer, "evening")),
            0,
            "and no other session row is reversed"
        );
    }

    #[test]
    fn a_sessions_list_taller_than_the_popup_scrolls_under_the_cursor() {
        let mut app = app_with(Puzzle::Cube3, 1);
        with_user_sessions(&mut app, &["morning", "evening", "night"]);

        // Ten rows leaves eight inside the border: seven sessions and the hint.
        app.sessions_overlay = Some(0);
        let text = render(&app, 80, 10);
        assert!(
            text.contains(" sessions 1/15 "),
            "the title counts the rows outside the window"
        );
        assert!(text.contains(">  1  default"), "the window opens on the top");
        assert!(!text.contains("morning"), "and the end of the list is not in it");
        assert!(text.contains("enter: switch"), "the hint keeps its row");

        // The cursor at the far end pulls the window with it.
        app.sessions_overlay = Some(14);
        let buffer = render_buffer(&app, 80, 10);
        let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
        assert!(text.contains(" sessions 15/15 "));
        for name in ["morning", "evening", "night"] {
            assert!(text.contains(name), "the window is missing {:?}", name);
        }
        assert!(reversed_in(&buffer, row_with(&buffer, "night")) > 0);

        // A cursor past the end of the list is clamped, not trusted.
        app.sessions_overlay = Some(usize::MAX);
        assert!(render(&app, 80, 10).contains(" sessions 15/15 "));
        render_all(&app);
    }

    #[test]
    fn the_solve_detail_overlay_wins_over_the_sessions_overlay() {
        let mut app = app_with(Puzzle::Cube3, 3);
        app.sessions_overlay = Some(0);
        app.solve_detail = Some(0);
        let text = render(&app, 80, 40);
        assert!(text.contains("r: load scramble"), "the detail popup is drawn");
        assert!(!text.contains(" sessions "), "the sessions popup is not");
    }

    // ----------------------------------------------------------- trend overlay

    /// Every non-ASCII glyph the trend popup draws inside its border, all six of them CP437.
    ///
    /// Three half blocks for the line and three box characters for the axes. Ratatui's default
    /// braille marker and the eighth blocks the trend used to be drawn with are both absent from
    /// the classic Windows console fonts, so anything outside this set is the tofu regression.
    const TREND_CP437: [char; 6] = ['▀', '▄', '█', '─', '│', '└'];

    /// A window with a real spread in it, oldest first.
    fn seeded_trend() -> Vec<u64> {
        (0..30u64).map(|i| 11_000 + (i * 7_919) % 4_000).collect()
    }

    /// An app with `window` as its trend and the popup open over it.
    fn trend_app(window: Vec<u64>) -> App {
        let mut app = app_with(Puzzle::Cube3, 30);
        app.trend = window;
        app.show_trend = true;
        app
    }

    /// The rows inside the trend popup's border, which is everything the graph may write on.
    fn trend_inside(app: &App, w: u16, h: u16) -> Vec<String> {
        let popup = trend_popup(Rect::new(0, 0, w, h)).expect("the terminal holds the popup");
        rows_of(&render_buffer(app, w, h))[(popup.y + 1) as usize..(popup.y + popup.height - 1) as usize]
            .iter()
            .map(|row| {
                row.chars()
                    .skip(popup.x as usize + 1)
                    .take(popup.width as usize - 2)
                    .collect()
            })
            .collect()
    }

    /// Indexes of the rows carrying line ink, top first.
    fn ink_rows(inside: &[String]) -> Vec<usize> {
        inside
            .iter()
            .enumerate()
            .filter(|(_, row)| row.chars().any(|c| "▀▄█".contains(c)))
            .map(|(i, _)| i)
            .collect()
    }

    #[test]
    fn the_trend_overlay_draws_a_line_graph_between_two_labelled_axes() {
        let app = trend_app(seeded_trend());
        let text = render(&app, 100, 34);
        assert!(text.contains(" trend "), "the popup is titled");
        assert!(text.contains("esc: close"), "and keeps its bottom row for the hint");

        // Three y ticks from the plot's own bounds, bottom to top, as times.
        let plot = trend_plot(&app.trend);
        for tick in trend_ticks(plot.y_bounds) {
            let label: String = tick.spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(text.contains(&label), "the y axis is missing the tick {:?}", label);
        }
        assert_eq!(
            plot.y_bounds,
            [11_000.0, 14_838.0],
            "the axis spans the window and stops at its 95th percentile"
        );

        let inside = trend_inside(&app, 100, 34);
        let last = app.trend.len().to_string();
        assert!(
            inside.iter().any(|row| row.contains(" 1 ") || row.trim_start().starts_with('1')),
            "the x axis numbers its first solve: {:?}",
            inside.last()
        );
        assert!(
            inside.iter().any(|row| row.ends_with(&last)),
            "and its last: {:?}",
            inside
        );
        assert!(
            inside.iter().any(|row| row.contains('└')),
            "the two axis lines meet in a corner"
        );
        assert!(ink_rows(&inside).len() > 4, "and a line is drawn between them");
        render_all(&app);
    }

    #[test]
    fn the_trend_overlay_draws_nothing_a_cp437_console_cannot_render() {
        // The regression the user hit twice: eighth blocks and braille are tofu on Windows.
        for window in [
            seeded_trend(),
            vec![100, 1_180, 640, 100, 1_180, 200],
            vec![12_000; 6],
            vec![9_999],
            vec![],
            (0..50u64).map(|i| 5_000 + i * 137).collect(),
        ] {
            let app = trend_app(window.clone());
            for (w, h) in [(100u16, 34u16), (80, 30), (46, 17)] {
                for row in trend_inside(&app, w, h) {
                    for c in row.chars() {
                        assert!(
                            c.is_ascii() || TREND_CP437.contains(&c),
                            "{:?} drew {:?} at {}x{}, which no CP437 font carries: {:?}",
                            window,
                            c,
                            w,
                            h,
                            row
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn one_ruined_solve_no_longer_flattens_the_rest_of_the_window() {
        // Ten solves half a second apart and one sixty second disaster. Scaled to the maximum
        // the cluster collapses onto the bottom row, which is the graph the user complained about.
        let mut window: Vec<u64> = (0..10).map(|i| 480 + i * 8).collect();
        window.push(60_000);
        let plot = trend_plot(&window);
        assert_eq!(plot.y_bounds, [480.0, 552.0], "the axis tops out below the disaster");
        assert_eq!(
            plot.points.last(), Some(&(10.0, 552.0)),
            "which the disaster is pinned to rather than owning"
        );

        let inside = trend_inside(&trend_app(window), 100, 34);
        assert!(
            ink_rows(&inside).len() > 6,
            "the cluster keeps its own spread over the height: {:?}",
            inside
        );
    }

    #[test]
    fn a_window_with_nothing_to_separate_draws_one_flat_line() {
        for window in [vec![12_000; 20], vec![9_999]] {
            let inside = trend_inside(&trend_app(window.clone()), 100, 34);
            assert_eq!(
                ink_rows(&inside).len(),
                1,
                "{:?} is one height, not a slope: {:?}",
                window,
                inside
            );
        }
    }

    #[test]
    fn an_empty_trend_says_so_instead_of_drawing_empty_axes() {
        let app = trend_app(Vec::new());
        let text = render(&app, 100, 34);
        assert!(text.contains("no solves to plot yet"));
        assert!(text.contains("esc: close"), "the hint is drawn either way");
        assert!(
            !text.contains('└'),
            "and no axis is drawn for a window with nothing in it"
        );
    }

    #[test]
    fn the_trend_overlay_is_skipped_on_a_terminal_too_small_to_read_it() {
        let app = trend_app(seeded_trend());
        render_all(&app);
        for (w, h) in [(43u16, 40u16), (80, 15), (30, 8), (10, 4), (1, 1)] {
            let text = render(&app, w, h);
            assert!(
                !text.contains(" trend "),
                "{}x{} has no room for a readable graph",
                w,
                h
            );
        }
        assert!(render(&app, 44, 16).contains(" trend "), "and 44x16 is where it starts");
    }

    #[test]
    fn the_solve_detail_overlay_wins_over_the_trend_overlay() {
        let mut app = trend_app(seeded_trend());
        app.solve_detail = Some(0);
        let text = render(&app, 100, 40);
        assert!(text.contains("r: load scramble"), "the detail popup is drawn");
        assert!(!text.contains(" trend "), "the trend popup is not");
    }

    // ---- the y domain, as a pure function

    #[test]
    fn the_axis_ceiling_is_the_ninety_fifth_percentile_by_nearest_rank() {
        let sorted: Vec<u64> = (1..=50).collect();
        assert_eq!(trend_top(&sorted), 48, "of fifty, the two slowest are outside");
        assert_eq!(trend_top(&(1..=20).collect::<Vec<u64>>()), 19);
        assert_eq!(trend_top(&(1..=11).collect::<Vec<u64>>()), 10);
        assert_eq!(trend_top(&[1, 2, 3, 4, 900]), 4, "five is where trimming starts");

        // Under that there is nothing to call an outlier and the maximum is the ceiling.
        assert_eq!(trend_top(&[1, 2, 3, 900]), 900);
        assert_eq!(trend_top(&[1, 900]), 900, "a two-value window still shows its rise");
        assert_eq!(trend_top(&[7]), 7);
    }

    #[test]
    fn a_flat_or_lone_window_is_plotted_as_a_line_across_the_middle() {
        let flat = trend_plot(&[12_000; 4]);
        assert_eq!(flat.y_bounds, [11_999.0, 12_001.0], "the line lands mid height");
        assert!(flat.points.iter().all(|(_, y)| *y == 12_000.0));

        let lone = trend_plot(&[9_999]);
        assert_eq!(lone.points, [(0.0, 9_999.0), (1.0, 9_999.0)], "one solve is a flat line");
        assert_eq!(lone.x_bounds, [0.0, 1.0]);

        // Nothing at all never panics and never divides by zero.
        let empty = trend_plot(&[]);
        assert!(empty.points.is_empty());
        assert_eq!(empty.x_bounds, [0.0, 1.0]);
        assert_eq!(empty.y_bounds, [-1.0, 1.0]);
    }

    #[test]
    fn a_two_value_window_spans_both_of_its_values() {
        let plot = trend_plot(&[10_000, 12_000]);
        assert_eq!(plot.y_bounds, [10_000.0, 12_000.0]);
        assert_eq!(plot.points, [(0.0, 10_000.0), (1.0, 12_000.0)]);
    }

    // ---------------------------------------------------------- preview overlay

    #[test]
    fn the_preview_overlay_says_so_for_an_event_with_no_cube_model() {
        let mut app = app_with(Puzzle::Megaminx, 3);
        assert!(
            app.preview.is_none(),
            "megaminx has no model to preview yet"
        );
        app.show_preview = true;
        render_all(&app);

        let text = render(&app, 80, 30);
        assert!(
            text.contains(" megaminx preview "),
            "the popup is titled with the event"
        );
        assert!(
            text.contains("no preview for this event"),
            "and says why it is empty rather than framing nothing"
        );
    }

    #[test]
    fn the_solve_detail_overlay_wins_over_the_preview_overlay() {
        let mut app = app_with(Puzzle::Megaminx, 3);
        app.show_preview = true;
        app.solve_detail = Some(0);
        let text = render(&app, 80, 40);
        assert!(text.contains("r: load scramble"), "the detail popup is drawn");
        assert!(!text.contains(" preview "), "the preview popup is not");
    }

    /// The message box is the popup's floor: it is placed and clamped like every other popup.
    #[test]
    fn the_preview_popup_without_a_cube_fits_the_terminal_it_is_drawn_on() {
        for w in 0..80u16 {
            for h in 0..40u16 {
                let (popup, mode) = preview_popup(None, Rect::new(0, 0, w, h));
                assert_eq!(mode, None, "no cube, no net mode");
                assert!(
                    popup.x + popup.width <= w && popup.y + popup.height <= h,
                    "{:?} escapes {}x{}",
                    popup,
                    w,
                    h
                );
            }
        }
    }

    /// Every non-ASCII glyph the preview popup draws inside its border, both of them CP437.
    ///
    /// The full net's whole block and the compact net's upper half. The sentence the popup falls
    /// back to is ASCII, so anything outside this set is the tofu regression the trend chart
    /// already guards against.
    const PREVIEW_CP437: [char; 2] = ['█', '▀'];

    /// One key press, built the way `app`'s own tests build them.
    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// The preview open on `puzzle` over the scramble the generator handed it.
    fn open_preview(puzzle: Puzzle) -> App {
        let mut app = app_with(puzzle, 3);
        app.show_preview = true;
        app
    }

    /// The preview open on `puzzle` over exactly `scramble`.
    ///
    /// The scramble arrives by recalling it from a solve through the real key path, so the cube
    /// behind the net is the one `app` built from it rather than one this test planted.
    fn preview_of(puzzle: Puzzle, scramble: &str) -> App {
        let mut app = app_with(puzzle, 1);
        app.current_session_mut().solves[0].scramble = scramble.to_string();
        app.on_key(press(KeyCode::Enter));
        app.on_key(press(KeyCode::Char('r')));
        assert_eq!(app.scramble, scramble, "the recall put the scramble on screen");
        app.show_preview = true;
        app
    }

    /// The rows inside the preview popup's border, which is everything the net may write on.
    fn preview_inside(app: &App, w: u16, h: u16) -> Vec<String> {
        let (popup, _) = preview_popup(app.preview.as_ref().map(Cube::n), Rect::new(0, 0, w, h));
        if popup.width < 3 || popup.height < 3 {
            return Vec::new();
        }
        rows_of(&render_buffer(app, w, h))
            [(popup.y + 1) as usize..(popup.y + popup.height - 1) as usize]
            .iter()
            .map(|row| {
                row.chars()
                    .skip(popup.x as usize + 1)
                    .take(popup.width as usize - 2)
                    .collect()
            })
            .collect()
    }

    #[test]
    fn the_preview_draws_the_cube_the_scramble_actually_makes() {
        let app = preview_of(Puzzle::Cube3, "R");
        let (popup, mode) = preview_popup(Some(3), Rect::new(0, 0, 80, 30));
        assert_eq!(mode, Some(false), "eighty by thirty holds the full sized net");

        let buffer = render_buffer(&app, 80, 30);
        // The net fills the popup's inside exactly, so its own top left is the inside's.
        let inner = inner_of(popup);
        let at = |row: u16, col: u16| {
            row_cells(&buffer, (inner.y + row) as usize)[(inner.x + col) as usize]
        };

        // U and D are indented by the L face and its gutter, which is seven cells on a 3x3, and
        // every sticker is two cells wide: U's third column is cells 11 and 12 of rows 0 to 2.
        // The middle band starts three rows below, so F's third column is cells 11 and 12 of
        // rows 4 to 6. One R turn stains both, so this pins `cube`, `net` and the geometry here
        // against each other rather than any one of them against itself.
        for r in 0..3u16 {
            for c in [11u16, 12] {
                let drawn = at(r, c);
                assert_eq!(drawn.symbol(), "█", "full mode draws whole blocks");
                assert_eq!(
                    drawn.fg,
                    Color::Green,
                    "R lifts the front onto U's right column, and the front is green"
                );
                assert_eq!(
                    at(4 + r, c).fg,
                    Color::Yellow,
                    "and the bottom onto F's, which is yellow"
                );
            }
            for c in 7..11u16 {
                assert_eq!(at(r, c).fg, Color::White, "the rest of U keeps its own colour");
            }
        }

        let text = render(&app, 80, 30);
        assert!(text.contains(" 3x3 preview "), "the popup is titled with the event");
        render_all(&app);
    }

    #[test]
    fn a_frame_too_short_for_the_full_net_falls_back_to_the_compact_one() {
        let app = open_preview(Puzzle::Cube7);
        // A 7x7 net is 23 rows tall in full mode and 14 in compact, plus a border row each side.
        assert_eq!(preview_popup(Some(7), Rect::new(0, 0, 80, 30)).1, Some(false));
        assert_eq!(preview_popup(Some(7), Rect::new(0, 0, 80, 20)).1, Some(true));

        let full = preview_inside(&app, 80, 30).concat();
        assert!(full.contains('█'), "the full net draws whole blocks");
        assert!(!full.contains('▀'), "and no half ones");

        let compact = preview_inside(&app, 80, 20).concat();
        assert!(
            compact.contains('▀'),
            "the compact net pairs two sticker rows into a half block"
        );
        assert!(!compact.contains('█'), "and draws no whole ones");

        // Too small for either, and the popup says so rather than framing a clipped cube.
        let text = render(&app, 40, 10);
        assert!(text.contains(" 7x7 preview "), "the popup is still titled");
        assert!(text.contains("terminal too small"));
    }

    #[test]
    fn the_preview_overlay_draws_nothing_a_cp437_console_cannot_render() {
        for puzzle in Puzzle::ALL {
            let app = open_preview(puzzle);
            for (w, h) in [(120u16, 60u16), (80, 30), (80, 20), (60, 18), (40, 10), (20, 6)] {
                for row in preview_inside(&app, w, h) {
                    for c in row.chars() {
                        assert!(
                            c.is_ascii() || PREVIEW_CP437.contains(&c),
                            "{} drew {:?} at {}x{}, which no CP437 font carries: {:?}",
                            puzzle.name(),
                            c,
                            w,
                            h,
                            row
                        );
                    }
                }
            }
        }
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

