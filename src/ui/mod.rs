//! All rendering (entry point [`draw`]), read-only over [`App`].
//!
//! Every value shown is already on `App`, including the statistics: this runs on the 15 ms
//! tick, so it reads and never computes. The geometry lives in [`layout`], and is saturating
//! throughout so tiny terminals degrade instead of panicking. The big countdown in the middle
//! is [`timer`], and the popups drawn on top of the frame live in [`overlay`].

mod layout;
mod overlay;
mod timer;

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, InputMode};
use crate::types::{format_millis, format_solve, Penalty};
use layout::{
    fit_count, footer_height, header_height, inner_of, list_window, stat_budget, stats_height,
    STAT_PREFIX_W, STAT_ROWS, STAT_SEP,
};
use timer::draw_timer;

// ---------------------------------------------------------------- palette

const C_IDLE: Color = Color::White;
const C_INSPECT: Color = Color::Yellow;
const C_ARMED: Color = Color::Red;
const C_READY: Color = Color::Green;
const C_TIMING: Color = Color::Cyan;
const C_LABEL: Color = Color::DarkGray;
const C_ACCENT: Color = Color::Magenta;
const C_BEST: Color = Color::Green;
const C_WORST: Color = Color::Red;
/// A personal best just set: the banner over the digits and the digits under it.
const C_PB: Color = Color::LightGreen;
/// Inspection past eight seconds.
const C_STAGE1: Color = Color::LightMagenta;
/// Inspection past twelve seconds.
const C_STAGE2: Color = Color::LightRed;

fn dim() -> Style {
    Style::default().fg(C_LABEL)
}

fn panel(title: &str) -> Block<'static> {
    let b = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(dim());
    if title.is_empty() {
        b
    } else {
        b.title(format!(" {} ", title))
    }
}

// ------------------------------------------------------------ entry point

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }

    // Header / body / status; on short terminals the body collapses to zero rows and clips.
    let footer_h = footer_height(area);
    let header_h = header_height(&app.scramble, area, footer_h);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_h),
            Constraint::Min(0),
            Constraint::Length(footer_h),
        ])
        .split(area);

    draw_scramble(frame, app, rows[0]);
    draw_body(frame, app, rows[1]);
    if footer_h > 0 {
        draw_status(frame, app, rows[2]);
    }

    // One overlay at a time, and the detail popup is the one the user just asked for.
    if let Some(index) = app.solve_detail {
        overlay::draw_detail(frame, app, index, area);
    } else if let Some(cursor) = app.sessions_overlay {
        overlay::draw_sessions(frame, app, cursor, area);
    } else if app.show_trend {
        overlay::draw_trend(frame, app, area);
    } else if app.show_help {
        overlay::draw_help(frame, area);
    }
}

// ------------------------------------------------- header: title + scramble

fn draw_scramble(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let session = app.current_session();
    let title = format!(
        "cubetimer ─ {} ─ session: {} (#{}) ─ inspection: {}",
        session.puzzle.name(),
        session.name,
        session.id,
        if app.save.settings.inspection {
            "on"
        } else {
            "off"
        }
    );

    let scramble_style = Style::default()
        .fg(C_ACCENT)
        .add_modifier(Modifier::BOLD);

    // Megaminx scrambles arrive as seven newline-separated lines; every other puzzle is one.
    let text = if app.scramble.is_empty() {
        Text::from(Line::styled("(no scramble)", dim()))
    } else {
        Text::from(
            app.scramble
                .split('\n')
                .map(|line| Line::styled(line, scramble_style))
                .collect::<Vec<Line>>(),
        )
    };

    let p = Paragraph::new(text)
        .block(panel(&title))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(p, area);
}

// ------------------------ body: timer + stats left, times list right

fn draw_body(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    // Side column only when there is room for it to be useful.
    let side_w: u16 = if area.width >= 60 {
        26
    } else if area.width >= 44 {
        20
    } else {
        0
    };

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(side_w)])
        .split(area);

    // Stats strip is 3 text rows and its two borders, or nothing at all.
    let stats_h = stats_height(cols[0].height);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(stats_h)])
        .split(cols[0]);

    draw_timer(frame, app, left[0]);
    if stats_h > 0 {
        draw_stats(frame, app, left[1]);
    }
    if side_w > 0 {
        draw_times(frame, app, cols[1]);
    }
}

// ------------------------------------------------- stats + personal bests

fn opt_time(v: Option<u64>) -> String {
    v.map(format_millis).unwrap_or_else(|| "-".to_string())
}

/// One row of the stats strip: a prefix, then `label value` entries packed into `width` columns.
///
/// The prefix names what the whole row is, and an empty one still holds its column so the three
/// rows line up. The strip has three rows and no more, so a row that cannot hold everything it
/// was given drops entries from the right rather than wrapping into the row below.
fn stat_row(
    prefix: &'static str,
    entries: Vec<(&'static str, String, Color)>,
    width: u16,
) -> Line<'static> {
    let widths: Vec<usize> = entries
        .iter()
        .map(|(label, value, _)| label.chars().count() + 1 + value.chars().count())
        .collect();
    let keep = fit_count(&widths, stat_budget(width));

    let mut spans: Vec<Span> = vec![Span::styled(
        format!("{:<width$} ", prefix, width = STAT_PREFIX_W),
        dim(),
    )];
    for (i, (label, value, color)) in entries.into_iter().take(keep).enumerate() {
        if i > 0 {
            spans.push(Span::raw(" ".repeat(STAT_SEP)));
        }
        spans.push(Span::styled(label, dim()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(value, Style::default().fg(color)));
    }
    Line::from(spans)
}

fn draw_stats(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let inner = inner_of(area);
    frame.render_widget(panel("stats"), area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let st = &app.stats;
    let pb = &app.pbs;
    let width = inner.width;

    // The first and last rows carry the same five windows, so a rolling average sits directly
    // above the best that window has ever been. The session's own spread goes between them.
    let averages = stat_row(
        "current",
        vec![
            ("mo3", st.mo3.display(), C_TIMING),
            ("ao5", st.ao5.display(), C_TIMING),
            ("ao12", st.ao12.display(), C_TIMING),
            ("ao100", st.ao100.display(), C_TIMING),
            ("ao1000", st.ao1000.display(), C_TIMING),
        ],
        width,
    );
    let session = stat_row(
        "",
        vec![
            // Two-word labels, because `best` and `worst` alone would read as the row prefixes.
            ("best single", opt_time(st.best), C_BEST),
            ("worst single", opt_time(st.worst), C_WORST),
            ("mean", opt_time(st.mean), C_IDLE),
            (
                "solves",
                format!("{} ({} ok)", st.count, st.valid_count),
                C_IDLE,
            ),
        ],
        width,
    );
    let bests = stat_row(
        "best",
        vec![
            ("mo3", opt_time(pb.mo3), C_ACCENT),
            ("ao5", opt_time(pb.ao5), C_ACCENT),
            ("ao12", opt_time(pb.ao12), C_ACCENT),
            ("ao100", opt_time(pb.ao100), C_ACCENT),
            ("ao1000", opt_time(pb.ao1000), C_ACCENT),
        ],
        width,
    );

    // The block is sized to hold exactly these three, but clip rather than trust that.
    let rows = Rect {
        height: inner.height.min(STAT_ROWS),
        ..inner
    };
    frame.render_widget(Paragraph::new(Text::from(vec![averages, session, bests])), rows);
}

// ---------------------------- times list (newest first, with a selection)

fn draw_times(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let solves = &app.current_session().solves;
    let total = solves.len();
    let best = app.stats.best;
    let worst = app.stats.worst;
    let selected = app.times_selected.min(total.saturating_sub(1));

    let title = if total == 0 {
        "times".to_string()
    } else if selected > 0 {
        // The selected solve's own number over the session total, so the depth reads at a glance.
        format!("times {}/{}", total - selected, total)
    } else {
        format!("times ({})", total)
    };

    let block = panel(&title);
    let inner = inner_of(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if total == 0 {
        let p = Paragraph::new(Line::styled("no solves yet", dim())).alignment(Alignment::Center);
        frame.render_widget(p, inner);
        return;
    }

    // Newest first, so both ends of the window count from the newest solve.
    let (start, len) = list_window(selected, total, inner.height as usize);
    let mut lines: Vec<Line> = Vec::with_capacity(len);

    for (offset, solve) in solves.iter().rev().skip(start).take(len).enumerate() {
        let index = start + offset;
        let number = total.saturating_sub(index);
        let eff = solve.effective_millis();
        let mut style = if solve.penalty == Penalty::Dnf {
            Style::default().fg(C_WORST).add_modifier(Modifier::DIM)
        } else if total > 1 && eff.is_some() && eff == best {
            Style::default().fg(C_BEST).add_modifier(Modifier::BOLD)
        } else if total > 1 && eff.is_some() && eff == worst {
            Style::default().fg(C_WORST)
        } else {
            Style::default().fg(C_IDLE)
        };
        let mut number_style = dim();
        let marker = if index == selected {
            // Reversed rather than recoloured, so best-green and worst-red still read on it;
            // DIM loses its foreground once the colours swap, so it comes off first.
            style = style
                .remove_modifier(Modifier::DIM)
                .add_modifier(Modifier::REVERSED);
            number_style = Style::default().fg(C_IDLE).add_modifier(Modifier::REVERSED);
            '>'
        } else {
            ' '
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{}{:>3} ", marker, number), number_style),
            Span::styled(format_solve(solve), style),
        ]));
    }

    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

// --------------------------- bottom line: command buffer / status / hints

const HINT: &str =
    "space hold+release: start · /: commands · n: new scramble · h: help · q: quit";

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let line = match app.input_mode {
        InputMode::Command => Line::from(vec![
            Span::styled(
                app.command_buf.clone(),
                Style::default()
                    .fg(C_TIMING)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("█", Style::default().fg(C_TIMING)),
        ]),
        InputMode::Normal => match &app.status_msg {
            Some(msg) => Line::styled(msg.clone(), Style::default().fg(C_INSPECT)),
            None => Line::styled(HINT, dim()),
        },
    };

    let p = Paragraph::new(line)
        .block(panel(""))
        .wrap(Wrap { trim: true });
    frame.render_widget(p, area);
}

// ------------------------------------------------------- test scaffolding

/// Shared by the render smoke tests here and in [`overlay`].
#[cfg(test)]
mod testkit {
    use super::{draw, App};
    use crate::types::{Penalty, Puzzle, SaveFile, Solve};
    use ratatui::buffer::{Buffer, Cell};
    use ratatui::style::Color;

    /// The sizes every smoke test sweeps: comfortable, narrow, short, and absurd.
    const SIZES: [(u16, u16); 4] = [(80, 30), (44, 12), (30, 8), (10, 4)];

    /// One Megaminx scramble line, and the seven-line block generators emit.
    const MEGA_LINE: &str = "R-- D++ R-- D-- R++ D++ R++ D++ R++ D++ U";

    pub(super) fn mega() -> String {
        [MEGA_LINE; 7].join("\n")
    }

    /// An app on `puzzle` with `count` solves recorded, at a path nothing in these tests writes.
    pub(super) fn app_with(puzzle: Puzzle, count: usize) -> App {
        let mut save = SaveFile::default();
        let id = puzzle.default_session_id();
        let session = save
            .sessions
            .iter_mut()
            .find(|s| s.id == id)
            .expect("every puzzle has a default session");
        session.solves = (0..count)
            .map(|i| Solve {
                millis: 8_000 + (i as u64 % 37) * 500,
                penalty: match i % 7 {
                    3 => Penalty::Plus2,
                    5 => Penalty::Dnf,
                    _ => Penalty::None,
                },
                scramble: "R U R' U'".to_string(),
                timestamp: 1_700_000_000_000 + i as u64,
            })
            .collect();
        save.active_session_id = id;
        App::new(
            save,
            std::env::temp_dir().join("cubetimer-ui-render-test.json"),
        )
    }

    /// Draw one frame and return the cell buffer. Not panicking is most of the assertion.
    pub(super) fn render_buffer(app: &App, w: u16, h: u16) -> Buffer {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("test terminal");
        terminal
            .draw(|frame| draw(frame, app))
            .expect("draw must not fail");
        terminal.backend().buffer().clone()
    }

    /// Draw one frame and return every cell's symbol, row by row.
    pub(super) fn render(app: &App, w: u16, h: u16) -> String {
        render_buffer(app, w, h)
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    /// Cells drawn in `color`, the only way to assert a colour that carries meaning.
    pub(super) fn cells_colored(buffer: &Buffer, color: Color) -> usize {
        buffer.content().iter().filter(|c| c.fg == color).count()
    }

    /// The frame split into one string per row, which is how alignment and cursors read.
    pub(super) fn rows_of(buffer: &Buffer) -> Vec<String> {
        let width = buffer.area.width as usize;
        if width == 0 {
            return Vec::new();
        }
        buffer
            .content()
            .chunks(width)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect())
            .collect()
    }

    /// Index of the first row containing `needle`, to assert how that row is styled.
    pub(super) fn row_with(buffer: &Buffer, needle: &str) -> usize {
        rows_of(buffer)
            .iter()
            .position(|row| row.contains(needle))
            .unwrap_or_else(|| panic!("no row of the frame contains {:?}", needle))
    }

    /// The cells of row `y`, in order.
    pub(super) fn row_cells(buffer: &Buffer, y: usize) -> Vec<&Cell> {
        let width = buffer.area.width as usize;
        buffer.content().iter().skip(y * width).take(width).collect()
    }

    /// Draw at all four sizes.
    pub(super) fn render_all(app: &App) {
        for (w, h) in SIZES {
            render(app, w, h);
        }
    }
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::testkit::{app_with, mega, render, render_all, render_buffer, rows_of};
    use super::*;
    use crate::app::TimerState;
    use crate::types::Puzzle;
    use std::time::Instant;

    /// The text of a [`Line`], span by span, for the rows built outside a [`Frame`].
    fn line_text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    /// The three stats rows as drawn, stripped of the panel border and of trailing padding.
    fn stats_lines(app: &App, w: u16, h: u16) -> Vec<String> {
        let buffer = render_buffer(app, w, h);
        let rows = rows_of(&buffer);
        let top = rows
            .iter()
            .position(|row| row.contains("current "))
            .expect("the stats strip is drawn");
        rows[top..top + 3]
            .iter()
            .map(|row| {
                let inner: String = row.chars().skip(1).take_while(|c| *c != '│').collect();
                inner.trim_end().to_string()
            })
            .collect()
    }

    /// The column `label` starts at, counted in characters so the box drawing does not skew it.
    fn label_col(row: &str, label: &str) -> usize {
        let byte = row
            .find(label)
            .unwrap_or_else(|| panic!("{:?} is missing from {:?}", label, row));
        row[..byte].chars().count()
    }

    #[test]
    fn a_normal_frame_actually_draws_its_chrome() {
        // The other tests only assert "no panic", so one of them has to prove draw wrote something.
        let text = render(&app_with(Puzzle::Cube3, 25), 80, 30);
        assert!(text.contains("cubetimer"), "the header title is missing");
        assert!(text.contains("3x3"), "the puzzle name is missing");
        assert!(text.contains("stats"), "the stats panel is missing");
        assert!(text.contains("times"), "the times panel is missing");
    }

    #[test]
    fn every_puzzle_renders_empty_and_populated_at_every_size() {
        for puzzle in Puzzle::ALL {
            for count in [0usize, 1, 25, 120] {
                let app = app_with(puzzle, count);
                assert!(
                    !app.scramble.is_empty(),
                    "{} produced no scramble",
                    puzzle.name()
                );
                render_all(&app);
            }
        }
    }

    #[test]
    fn a_seven_line_megaminx_scramble_renders_at_every_size() {
        let mut app = app_with(Puzzle::Megaminx, 30);
        app.scramble = mega();
        render_all(&app);
        assert!(render(&app, 80, 30).contains("D++"), "the scramble is drawn");
    }

    #[test]
    fn a_sixty_character_session_name_renders_at_every_size() {
        let mut app = app_with(Puzzle::Cube3, 5);
        let long: String = "long session name ".repeat(4).chars().take(60).collect();
        assert_eq!(long.chars().count(), 60);
        app.save.sessions[0].name = long;
        render_all(&app);
    }

    #[test]
    fn a_times_selection_past_the_end_renders_at_every_size() {
        let mut app = app_with(Puzzle::Cube3, 12);
        app.times_selected = usize::MAX;
        render_all(&app);
        app.times_selected = 11;
        render_all(&app);
    }

    #[test]
    fn the_selected_row_is_marked_highlighted_and_counted_in_the_title() {
        let mut app = app_with(Puzzle::Cube3, 20);
        app.times_selected = 5;
        let buffer = render_buffer(&app, 80, 30);
        let text: String = buffer.content().iter().map(|c| c.symbol()).collect();

        assert!(
            text.contains("times 15/20"),
            "the times panel title must count the selection"
        );
        assert!(text.contains("> 15 "), "the selected row carries the marker");
        assert!(
            buffer
                .content()
                .iter()
                .any(|c| c.modifier.contains(Modifier::REVERSED)),
            "the selected row is highlighted"
        );

        // The newest solve is the resting position and reads as a plain count.
        app.times_selected = 0;
        let text = render(&app, 80, 30);
        assert!(text.contains("times (20)"), "no position while on the newest");
        assert!(!text.contains("> 15 "), "the marker moved with the selection");
    }

    #[test]
    fn a_stats_row_pays_for_its_prefix_column_before_packing_entries() {
        let entries = || {
            vec![
                ("ao5", "10.00".to_string(), C_TIMING),
                ("ao12", "11.00".to_string(), C_TIMING),
            ]
        };
        // Eight columns of prefix, "ao5 10.00" in nine, three between, "ao12 11.00" in ten.
        assert_eq!(
            line_text(&stat_row("current", entries(), 30)),
            "current ao5 10.00   ao12 11.00"
        );
        assert_eq!(
            line_text(&stat_row("current", entries(), 29)),
            "current ao5 10.00",
            "one column short and the second entry goes"
        );
        assert_eq!(
            line_text(&stat_row("", entries(), 30)),
            "        ao5 10.00   ao12 11.00",
            "an unlabelled row still holds the column"
        );
        assert_eq!(
            line_text(&stat_row("best", entries(), 0)),
            "best    ao5 10.00",
            "a row with no room left still shows one entry"
        );

        // A two-word label is measured whole, space included, or the row would overpack.
        let session = || {
            vec![
                ("best single", "10.00".to_string(), C_BEST),
                ("worst single", "11.00".to_string(), C_WORST),
            ]
        };
        assert_eq!(
            line_text(&stat_row("", session(), 46)),
            "        best single 10.00   worst single 11.00",
            "seventeen columns, three between, and eighteen more"
        );
        assert_eq!(
            line_text(&stat_row("", session(), 45)),
            "        best single 10.00",
            "one column short and the second two-word entry goes"
        );
    }

    #[test]
    fn the_stats_rows_name_themselves_and_line_up_under_each_other() {
        let app = app_with(Puzzle::Cube3, 30);
        let buffer = render_buffer(&app, 100, 30);
        let rows = rows_of(&buffer);
        let find = |needle: &str| {
            rows.iter()
                .find(|row| row.contains(needle))
                .unwrap_or_else(|| panic!("no stats row holds {:?}", needle))
                .clone()
        };

        let current = find("current ");
        let session = find("best single");
        let bests = find("best    ");
        assert!(
            current.contains("mo3"),
            "the rolling averages are the labelled row"
        );
        assert!(
            !bests.contains("single"),
            "the personal bests row is averages only: {:?}",
            bests
        );
        assert!(
            session.contains("worst single"),
            "the session spread names both of its singles in full: {:?}",
            session
        );

        let first = label_col(&current, "mo3");
        assert_eq!(
            first,
            label_col(&current, "current") + STAT_PREFIX_W + 1,
            "the widest prefix and one space, and no ragged offset after it"
        );
        assert_eq!(
            first,
            label_col(&session, "best single"),
            "the unlabelled row lines up with the labelled ones"
        );
        assert_eq!(
            first,
            label_col(&bests, "mo3"),
            "and the PB of a window sits under the rolling one"
        );
    }

    #[test]
    fn the_stats_strip_drops_entries_from_the_right_as_the_terminal_narrows() {
        let app = app_with(Puzzle::Cube3, 30);

        assert_eq!(
            stats_lines(&app, 80, 30),
            [
                "current mo3 22.00   ao5 22.00   ao12 DNF   ao100 -",
                "        best single 8.00   worst single 22.50",
                "best    mo3 8.50   ao5 9.16   ao12 11.45   ao100 -",
            ],
            "at 80 columns each row keeps what its 44 columns of budget hold"
        );

        assert_eq!(
            stats_lines(&app, 44, 30),
            [
                "current mo3 22.00",
                // Sixteen columns of `best single 8.00` against fourteen of budget: the row keeps
                // the entry anyway, because one clipped number reads better than a blank row.
                "        best single 8.",
                "best    mo3 8.50",
            ],
            "at 44 columns every row is down to the one entry it may never drop"
        );
    }

    #[test]
    fn command_mode_renders_at_every_size() {
        let mut app = app_with(Puzzle::Square1, 3);
        app.input_mode = InputMode::Command;
        app.command_buf = "/session 12345".to_string();
        render_all(&app);
        assert!(render(&app, 80, 30).contains("/session 12345"));
    }

    #[test]
    fn a_status_message_and_an_inspection_countdown_render() {
        let mut app = app_with(Puzzle::Pyraminx, 2);
        app.status_msg = Some("no solves to delete".repeat(6));
        render_all(&app);

        app.status_msg = None;
        app.state = TimerState::Inspecting {
            started: Instant::now(),
        };
        for remaining in [Some(15i64), Some(1), Some(0), Some(-2), None] {
            app.inspection_remaining = remaining;
            render_all(&app);
        }
    }
}
