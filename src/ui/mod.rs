//! All rendering (entry point [`draw`]), read-only over [`App`].
//!
//! Every value shown is already on `App`, including the statistics: this runs on the 15 ms
//! tick, so it reads and never computes. The geometry lives in [`layout`], and is saturating
//! throughout so tiny terminals degrade instead of panicking. The popups drawn on top of the
//! frame live in [`overlay`].

mod layout;
mod overlay;

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, InputMode, TimerState};
use crate::types::{format_millis, format_solve, Penalty};
use layout::{fit_count, footer_height, header_height, inner_of, times_window, STAT_SEP};

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
/// Inspection past eight seconds.
const C_STAGE1: Color = Color::LightMagenta;
/// Inspection past twelve seconds.
const C_STAGE2: Color = Color::LightRed;

/// Stands in for the running time when `hide_time` is on; the block font has a `'.'` glyph.
const HIDDEN_TIME: &str = "...";

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

    // Stats strip is 3 text rows + borders; drop it when the body is short.
    let stats_h: u16 = if cols[0].height >= 12 { 5 } else { 0 };
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

// -------------------------------------------------------------- big timer

/// What the big area shows now: glyph string, colour, optional penalty caption, state caption.
fn timer_view(app: &App) -> (String, Color, Option<String>, &'static str) {
    match app.state {
        TimerState::Idle => (
            format_millis(app.display_millis),
            C_IDLE,
            None,
            "ready, hold space",
        ),
        TimerState::Inspecting { .. } => {
            let remaining = app.inspection_remaining.unwrap_or(15);
            // The stage is decided by `app`, so the two warning colours cannot drift from it.
            let stage_color = match app.inspection_stage {
                0 => C_INSPECT,
                1 => C_STAGE1,
                _ => C_STAGE2,
            };
            // `remaining` counts 15..0 then negative: <= 0 is past 15s (+2), <= -2 is past 17s (DNF).
            let (penalty, color) = if remaining <= -2 {
                (Some("DNF".to_string()), Color::Red)
            } else if remaining <= 0 {
                (Some("+2".to_string()), Color::Red)
            } else {
                (None, stage_color)
            };
            let shown = if remaining > 0 { remaining } else { 0 };
            (shown.to_string(), color, penalty, "inspecting")
        }
        TimerState::Armed { .. } => {
            if app.armed_ready() {
                (
                    format_millis(app.display_millis),
                    C_READY,
                    None,
                    "release to start",
                )
            } else {
                (
                    format_millis(app.display_millis),
                    C_ARMED,
                    None,
                    "keep holding…",
                )
            }
        }
        TimerState::Timing { .. } => {
            // Hiding the running time is a practice aid, so only the run itself is masked:
            // the result is on screen the moment the timer stops.
            let shown = if app.save.settings.hide_time {
                HIDDEN_TIME.to_string()
            } else {
                format_millis(app.display_millis)
            };
            (shown, C_TIMING, None, "solving, any key stops")
        }
    }
}

fn draw_timer(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let block = panel("");
    let inner = inner_of(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let (text, color, penalty, caption) = timer_view(app);
    let style = Style::default().fg(color).add_modifier(Modifier::BOLD);

    let mut body: Vec<Line> = Vec::new();
    let big = big_text(&text);
    let big_w = big
        .first()
        .map(|r| r.chars().count())
        .unwrap_or(0)
        .min(u16::MAX as usize) as u16;

    if inner.height >= GLYPH_H as u16 && big_w <= inner.width && big_w > 0 {
        for row in big {
            body.push(Line::styled(row, style));
        }
    } else {
        // Not enough room for the block font: plain (still bold/coloured) text.
        body.push(Line::styled(text, style));
    }

    if let Some(p) = penalty {
        body.push(Line::from(""));
        body.push(Line::styled(
            p,
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }
    if inner.height as usize >= body.len() + 2 {
        body.push(Line::from(""));
        body.push(Line::styled(caption, dim()));
    }

    // Vertical centering via leading blank lines.
    let pad = (inner.height as usize).saturating_sub(body.len()) / 2;
    let mut lines: Vec<Line> = Vec::with_capacity(body.len() + pad);
    for _ in 0..pad {
        lines.push(Line::from(""));
    }
    lines.extend(body);

    let p = Paragraph::new(Text::from(lines)).alignment(Alignment::Center);
    frame.render_widget(p, inner);
}

// ------------------------------------------------------- 5-row block font

const GLYPH_H: usize = 5;

/// Rows of one glyph; unknown chars render blank so odd input can never mis-align rows.
fn glyph(c: char) -> [&'static str; GLYPH_H] {
    match c {
        '0' => ["████", "█  █", "█  █", "█  █", "████"],
        '1' => ["   █", "   █", "   █", "   █", "   █"],
        '2' => ["████", "   █", "████", "█   ", "████"],
        '3' => ["████", "   █", "████", "   █", "████"],
        '4' => ["█  █", "█  █", "████", "   █", "   █"],
        '5' => ["████", "█   ", "████", "   █", "████"],
        '6' => ["████", "█   ", "████", "█  █", "████"],
        '7' => ["████", "   █", "   █", "   █", "   █"],
        '8' => ["████", "█  █", "████", "█  █", "████"],
        '9' => ["████", "█  █", "████", "   █", "████"],
        ':' => ["  ", "██", "  ", "██", "  "],
        '.' => ["  ", "  ", "  ", "  ", "██"],
        '-' => ["    ", "    ", "████", "    ", "    "],
        '+' => ["    ", " ██ ", "████", " ██ ", "    "],
        _ => ["  ", "  ", "  ", "  ", "  "],
    }
}

/// Render `s` into `GLYPH_H` equal-width rows, one space between glyphs.
fn big_text(s: &str) -> Vec<String> {
    let mut rows: Vec<String> = vec![String::new(); GLYPH_H];
    for (i, c) in s.chars().enumerate() {
        let g = glyph(c);
        for (r, cell) in rows.iter_mut().zip(g.iter()) {
            if i > 0 {
                r.push(' ');
            }
            r.push_str(cell);
        }
    }
    rows
}

// ------------------------------------------------- stats + personal bests

fn opt_time(v: Option<u64>) -> String {
    v.map(format_millis).unwrap_or_else(|| "-".to_string())
}

/// One row of the stats strip: `label value` entries packed left to right into `width` columns.
///
/// The strip has three rows and no more, so a row that cannot hold everything it was given
/// drops entries from the right rather than wrapping into the row below.
fn stat_row(
    prefix: Option<&'static str>,
    entries: Vec<(&'static str, String, Color)>,
    width: u16,
) -> Line<'static> {
    let lead = prefix.map_or(0u16, |p| p.chars().count().saturating_add(1) as u16);
    let widths: Vec<usize> = entries
        .iter()
        .map(|(label, value, _)| label.chars().count() + 1 + value.chars().count())
        .collect();
    let keep = fit_count(&widths, width.saturating_sub(lead));

    let mut spans: Vec<Span> = Vec::new();
    if let Some(p) = prefix {
        spans.push(Span::styled(p, dim()));
        spans.push(Span::raw(" "));
    }
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
    let st = &app.stats;
    let pb = &app.pbs;
    let width = area.width.saturating_sub(2);

    let averages = stat_row(
        None,
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
        None,
        vec![
            ("best", opt_time(st.best), C_BEST),
            ("worst", opt_time(st.worst), C_WORST),
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
        Some("pb"),
        vec![
            ("single", opt_time(pb.single), C_ACCENT),
            ("mo3", opt_time(pb.mo3), C_ACCENT),
            ("ao5", opt_time(pb.ao5), C_ACCENT),
            ("ao12", opt_time(pb.ao12), C_ACCENT),
            ("ao100", opt_time(pb.ao100), C_ACCENT),
            ("ao1000", opt_time(pb.ao1000), C_ACCENT),
        ],
        width,
    );

    let p = Paragraph::new(Text::from(vec![averages, session, bests])).block(panel("stats"));
    frame.render_widget(p, area);
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
    let (start, len) = times_window(selected, total, inner.height as usize);
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
    use ratatui::buffer::Buffer;
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
    use super::testkit::{app_with, cells_colored, mega, render, render_all, render_buffer};
    use super::*;
    use crate::types::Puzzle;
    use std::time::Instant;

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
    fn hiding_the_time_masks_the_running_solve_but_not_the_result() {
        let mut app = app_with(Puzzle::Cube3, 4);
        app.state = TimerState::Timing {
            started: Instant::now(),
        };
        app.display_millis = 12_340;

        let visible = render(&app, 80, 30).matches('█').count();
        app.save.settings.hide_time = true;
        let hidden = render(&app, 80, 30).matches('█').count();
        // Three dots are one glyph row of two cells each, and nothing else in the frame is a block.
        assert_eq!(hidden, 6, "only the three dots survive, got {} blocks", hidden);
        assert!(visible > hidden, "the digits were drawn before, got {}", visible);
        render_all(&app);

        // Back in Idle the finished time is on screen as usual.
        app.state = TimerState::Idle;
        assert!(render(&app, 80, 30).matches('█').count() > 6);
    }

    #[test]
    fn the_inspection_stages_recolour_the_countdown() {
        let mut app = app_with(Puzzle::Pyraminx, 2);
        app.state = TimerState::Inspecting {
            started: Instant::now(),
        };
        app.inspection_remaining = Some(5);

        app.inspection_stage = 0;
        assert_eq!(cells_colored(&render_buffer(&app, 80, 30), C_STAGE1), 0);
        app.inspection_stage = 1;
        assert!(cells_colored(&render_buffer(&app, 80, 30), C_STAGE1) > 0);
        render_all(&app);
        app.inspection_stage = 2;
        assert!(cells_colored(&render_buffer(&app, 80, 30), C_STAGE2) > 0);
        render_all(&app);
    }

    #[test]
    fn a_penalty_overrides_the_inspection_stage_colour() {
        let mut app = app_with(Puzzle::Pyraminx, 2);
        app.state = TimerState::Inspecting {
            started: Instant::now(),
        };
        app.inspection_stage = 2;

        for (remaining, caption) in [(0i64, "+2"), (-3, "DNF")] {
            app.inspection_remaining = Some(remaining);
            let buffer = render_buffer(&app, 80, 30);
            let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
            assert!(text.contains(caption), "the {} caption is drawn", caption);
            assert_eq!(
                cells_colored(&buffer, C_STAGE2),
                0,
                "the penalty red must win over the stage colour"
            );
        }
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
    fn a_solve_over_an_hour_renders_at_every_size() {
        let mut app = app_with(Puzzle::Cube7, 4);
        app.state = TimerState::Timing {
            started: Instant::now(),
        };
        app.display_millis = 3_723_450;
        render_all(&app);
        // 62:03.45 is nine glyphs wide, so 80 columns still gets the block font.
        assert!(render(&app, 80, 30).contains('█'));
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
