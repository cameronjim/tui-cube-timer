//! The big timer: what the centre of the frame shows, and the block font it shows it in.
//!
//! Read-only over [`App`] like the rest of [`ui`](super). The glyph rows are fixed width, so
//! the caller can measure the string before deciding whether the block font fits at all.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::layout::inner_of;
use super::{
    dim, panel, C_ARMED, C_IDLE, C_INSPECT, C_NEW_BEST, C_READY, C_STAGE1, C_STAGE2, C_TIMING,
};
use crate::app::{App, TimerState};
use crate::types::format_millis;

/// Stands in for the running time when `hide_time` is on; the block font has a `'.'` glyph.
const HIDDEN_TIME: &str = "...";

/// What the big area shows now: glyph string, colour, the note under it, state caption.
///
/// The note is the slot the inspection judge calls and the inspection penalties share, so it
/// carries its own colour: the calls take the stage colour, a penalty is always red.
type TimerView = (String, Color, Option<(String, Color)>, &'static str);

fn timer_view(app: &App) -> TimerView {
    match app.state {
        TimerState::Idle => {
            // A session best recolours the result it was set on, until `app` drops the banner.
            let color = if app.best_banner.is_some() {
                C_NEW_BEST
            } else {
                C_IDLE
            };
            (
                format_millis(app.display_millis),
                color,
                None,
                "ready, hold space",
            )
        }
        TimerState::Inspecting { .. } => {
            let remaining = app.inspection_remaining.unwrap_or(15);
            // The stage is decided by `app`, so neither the warning colour nor the call it
            // announces can drift from it.
            let (stage_color, call) = match app.inspection_stage {
                0 => (C_INSPECT, None),
                1 => (C_STAGE1, Some("8s")),
                _ => (C_STAGE2, Some("12s")),
            };
            // `remaining` counts 15..0 then negative: <= 0 is past 15s (+2), <= -2 is past 17s (DNF).
            let (note, color) = if remaining <= -2 {
                (Some(("DNF".to_string(), Color::Red)), Color::Red)
            } else if remaining <= 0 {
                (Some(("+2".to_string(), Color::Red)), Color::Red)
            } else {
                // Until a penalty claims the slot it carries the judge call for the stage.
                (call.map(|c| (c.to_string(), stage_color)), stage_color)
            };
            let shown = if remaining > 0 { remaining } else { 0 };
            (shown.to_string(), color, note, "inspecting")
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

pub(super) fn draw_timer(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let block = panel("");
    let inner = inner_of(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let (text, color, note, caption) = timer_view(app);
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

    // The celebration sits directly over the digits, and is the first line the panel gives up.
    if let Some(banner) = app.best_banner.as_deref() {
        if (inner.height as usize) > body.len() {
            body.insert(
                0,
                Line::styled(
                    banner.to_string(),
                    Style::default()
                        .fg(Color::Black)
                        .bg(C_NEW_BEST)
                        .add_modifier(Modifier::BOLD),
                ),
            );
        }
    }

    if let Some((text, color)) = note {
        body.push(Line::from(""));
        body.push(Line::styled(
            text,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
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

pub(super) const GLYPH_H: usize = 5;

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

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::super::testkit::{
        app_with, cells_colored, render, render_all, render_buffer, row_cells, row_with, rows_of,
    };
    use super::*;
    use crate::types::Puzzle;
    use std::time::Instant;

    /// Block cells above the stats strip, which is the big timer and nothing else.
    ///
    /// The trend sparkline draws full blocks of its own, so a count over the whole frame would
    /// stop being a count of the digits.
    fn timer_blocks(app: &App, w: u16, h: u16) -> usize {
        let buffer = render_buffer(app, w, h);
        let rows = rows_of(&buffer);
        let stats = rows
            .iter()
            .position(|row| row.contains(" stats "))
            .unwrap_or(rows.len());
        rows[..stats]
            .iter()
            .map(|row| row.matches('█').count())
            .sum()
    }

    #[test]
    fn hiding_the_time_masks_the_running_solve_but_not_the_result() {
        let mut app = app_with(Puzzle::Cube3, 4);
        app.state = TimerState::Timing {
            started: Instant::now(),
        };
        app.display_millis = 12_340;

        let visible = timer_blocks(&app, 80, 30);
        app.save.settings.hide_time = true;
        let hidden = timer_blocks(&app, 80, 30);
        // Three dots are one glyph row of two cells each, and nothing else up there is a block.
        assert_eq!(hidden, 6, "only the three dots survive, got {} blocks", hidden);
        assert!(visible > hidden, "the digits were drawn before, got {}", visible);
        render_all(&app);

        // Back in Idle the finished time is on screen as usual.
        app.state = TimerState::Idle;
        assert!(timer_blocks(&app, 80, 30) > 6);
    }

    #[test]
    fn the_inspection_stages_recolour_the_countdown_and_announce_the_call() {
        let mut app = app_with(Puzzle::Pyraminx, 2);
        app.state = TimerState::Inspecting {
            started: Instant::now(),
        };
        app.inspection_remaining = Some(5);

        app.inspection_stage = 0;
        let buffer = render_buffer(&app, 80, 30);
        assert_eq!(cells_colored(&buffer, C_STAGE1), 0);
        let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
        assert!(!text.contains("8s"), "under eight seconds there is no call yet");

        for (stage, call, color) in [(1u8, "8s", C_STAGE1), (2, "12s", C_STAGE2)] {
            app.inspection_stage = stage;
            let buffer = render_buffer(&app, 80, 30);
            assert!(
                cells_colored(&buffer, color) > 0,
                "stage {} recolours the countdown",
                stage
            );
            let row = row_with(&buffer, call);
            assert!(
                row_cells(&buffer, row).iter().any(|c| c.fg == color),
                "the {} caption is drawn in the stage colour",
                call
            );
            render_all(&app);
        }
    }

    #[test]
    fn a_penalty_overrides_the_inspection_stage_colour_and_its_call() {
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
            assert!(!text.contains("12s"), "and it takes the slot from the call");
            assert_eq!(
                cells_colored(&buffer, C_STAGE2),
                0,
                "the penalty red must win over the stage colour"
            );
        }
    }

    #[test]
    fn a_session_best_banner_takes_the_row_over_the_digits_and_turns_them_green() {
        let mut app = app_with(Puzzle::Cube3, 5);
        app.display_millis = 12_340;
        app.best_banner = Some("new best single: 12.34".to_string());
        let buffer = render_buffer(&app, 80, 30);

        let banner = row_with(&buffer, "new best single: 12.34");
        assert!(
            row_cells(&buffer, banner).iter().any(|c| c.bg == C_NEW_BEST
                && c.fg == Color::Black
                && c.modifier.contains(Modifier::BOLD)),
            "the banner is bold black on light green"
        );

        let digits = row_with(&buffer, "█");
        assert!(digits > banner, "and it sits directly over the digits");
        assert!(
            row_cells(&buffer, digits)
                .iter()
                .filter(|c| c.symbol() == "█")
                .all(|c| c.fg == C_NEW_BEST),
            "the result celebrates in the same green as the banner"
        );
        render_all(&app);
    }

    #[test]
    fn without_a_banner_the_idle_digits_are_the_white_they_always_were() {
        let mut app = app_with(Puzzle::Cube3, 5);
        app.display_millis = 12_340;
        let buffer = render_buffer(&app, 80, 30);

        assert_eq!(
            cells_colored(&buffer, C_NEW_BEST),
            0,
            "an ordinary solve celebrates nothing"
        );
        let digits = row_with(&buffer, "█");
        assert!(
            row_cells(&buffer, digits)
                .iter()
                .filter(|c| c.symbol() == "█")
                .all(|c| c.fg == C_IDLE)
        );
    }

    #[test]
    fn the_banner_is_the_first_line_a_short_panel_drops() {
        let mut app = app_with(Puzzle::Cube3, 3);
        app.best_banner = Some("new best ao5: 13.07".to_string());
        render_all(&app);

        // Fifteen rows leave the timer six inside its border: the five glyph rows and the banner.
        assert!(render(&app, 80, 15).contains("new best ao5"));

        // Fourteen leave it exactly the glyph rows, and the digits outrank the celebration.
        let text = render(&app, 80, 14);
        assert!(text.contains('█'), "the block font keeps its rows");
        assert!(!text.contains("new best ao5"), "the banner is what goes");
    }

    #[test]
    fn a_banner_never_takes_the_colour_of_a_running_inspection() {
        let mut app = app_with(Puzzle::Cube3, 5);
        app.best_banner = Some("new best single: 9.87".to_string());
        app.state = TimerState::Inspecting {
            started: Instant::now(),
        };
        app.inspection_stage = 2;
        app.inspection_remaining = Some(2);

        let buffer = render_buffer(&app, 80, 30);
        assert!(
            cells_colored(&buffer, C_STAGE2) > 0,
            "the countdown keeps the stage it is in"
        );
        assert_eq!(
            cells_colored(&buffer, C_NEW_BEST),
            0,
            "the celebration recolours an idle result and nothing else"
        );
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
}
