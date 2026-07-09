//! Pure geometry for the renderer: how tall each panel is, how text wraps, where the popup sits.
//!
//! Nothing here touches a [`Frame`](ratatui::Frame) or an [`App`](crate::app::App); it is
//! arithmetic over [`Rect`] and `&str`, saturating throughout so a one-column terminal
//! degrades instead of panicking.

use ratatui::layout::Rect;

/// Shortest scramble panel: two border rows plus two text rows, which is what a single-line scramble needs.
pub(super) const HEADER_MIN_H: u16 = 4;
/// Tallest scramble panel. Megaminx needs seven text rows; past that the scramble truncates rather than eat the screen.
pub(super) const HEADER_MAX_H: u16 = 10;
/// Rows the timer panel needs to keep its block font: [`GLYPH_H`](super::GLYPH_H) glyph rows plus borders.
pub(super) const TIMER_MIN_H: u16 = super::GLYPH_H as u16 + 2;

/// Popup width; the help text is packed to fit it and the popup shrinks to the terminal.
pub(super) const HELP_W: u16 = 62;
/// Width of the key column, indent excluded.
pub(super) const HELP_KEY_W: usize = 16;
/// Columns a help row leaves for its description: [`HELP_W`] less borders, indent and key column.
const HELP_DESC_W: usize = HELP_W as usize - 2 - 2 - HELP_KEY_W;

/// Inner area of a bordered block, guarding against rects too small to have one.
pub(super) fn inner_of(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

/// Height of the status panel; under six rows the screen has nothing to spare for it.
pub(super) fn footer_height(area: Rect) -> u16 {
    if area.height >= 6 {
        3
    } else {
        0
    }
}

/// Rows one line occupies at `width` under greedy word wrap, matching `Wrap { trim: true }`.
pub(super) fn wrapped_rows(line: &str, width: u16) -> usize {
    let width = width as usize;
    if width == 0 {
        return 1;
    }
    let mut rows = 1usize;
    let mut used = 0usize;
    for word in line.split_whitespace() {
        let w = word.chars().count();
        if used > 0 {
            if used.saturating_add(1).saturating_add(w) > width {
                rows = rows.saturating_add(1);
                used = 0;
            } else {
                used = used.saturating_add(1);
            }
        }
        // A word wider than the line itself spills onto further rows.
        let end = used.saturating_add(w);
        rows = rows.saturating_add(end.saturating_sub(1) / width);
        used = (end.saturating_sub(1) % width).saturating_add(1);
    }
    rows
}

/// Text rows a scramble needs: its explicit lines (Megaminx has seven) plus the wrapping of each.
pub(super) fn scramble_rows(scramble: &str, width: u16) -> usize {
    if scramble.is_empty() {
        return 1;
    }
    scramble
        .split('\n')
        .map(|line| wrapped_rows(line, width))
        .fold(0usize, |acc, n| acc.saturating_add(n))
}

/// Height of the scramble panel: it grows to fit the text, then yields to the timer.
///
/// The cap is what keeps the block font alive. Once the remaining rows would drop the
/// timer below [`TIMER_MIN_H`], the header stops growing and the scramble truncates
/// instead, which is the cheaper loss of the two.
pub(super) fn header_height(scramble: &str, area: Rect, footer_h: u16) -> u16 {
    if area.height < 9 {
        return 3;
    }
    let wanted = scramble_rows(scramble, area.width.saturating_sub(2))
        .saturating_add(2)
        .clamp(HEADER_MIN_H as usize, HEADER_MAX_H as usize) as u16;
    let budget = area
        .height
        .saturating_sub(footer_h)
        .saturating_sub(TIMER_MIN_H)
        .max(HEADER_MIN_H);
    budget.min(wanted)
}

/// A centered rect of at most `w` x `h`, always inside `area`.
pub(super) fn centered(w: u16, h: u16, area: Rect) -> Rect {
    let width = w.min(area.width);
    let height = h.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

/// The `/<puzzle>` description, packed into as many rows as [`HELP_DESC_W`] allows.
///
/// Eleven puzzle names do not fit on one row, and the popup does not wrap, so the list
/// is broken here instead of being clipped. Rows after the first are drawn with an empty
/// key so they line up under the text.
pub(super) fn puzzle_help_rows(names: &[&str]) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    let mut row = "switch puzzle:".to_string();
    for name in names {
        let used = row.chars().count();
        let needed = used.saturating_add(1).saturating_add(name.chars().count());
        if used > 0 && needed > HELP_DESC_W {
            rows.push(std::mem::take(&mut row));
        }
        if !row.is_empty() {
            row.push(' ');
        }
        row.push_str(name);
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Puzzle;

    /// One Megaminx line: eleven tokens, 41 columns.
    const MEGA_LINE: &str = "R-- D++ R-- D-- R++ D++ R++ D++ R++ D++ U";

    fn mega() -> String {
        [MEGA_LINE; 7].join("\n")
    }

    fn rect(width: u16, height: u16) -> Rect {
        Rect::new(0, 0, width, height)
    }

    fn header_of(scramble: &str, width: u16, height: u16) -> u16 {
        let area = rect(width, height);
        header_height(scramble, area, footer_height(area))
    }

    // ---- wrapping

    #[test]
    fn wrapped_rows_counts_a_line_that_fits_as_one() {
        assert_eq!(wrapped_rows(MEGA_LINE, 78), 1);
        assert_eq!(wrapped_rows(MEGA_LINE, 41), 1);
        assert_eq!(wrapped_rows("", 40), 1);
    }

    #[test]
    fn wrapped_rows_breaks_on_word_boundaries() {
        // 20 columns fits "R-- D++ R-- D-- R++", then five more tokens, then the trailing U.
        assert_eq!(wrapped_rows(MEGA_LINE, 20), 3);
        assert_eq!(wrapped_rows(MEGA_LINE, 40), 2);
    }

    #[test]
    fn wrapped_rows_splits_a_word_wider_than_the_line() {
        assert_eq!(wrapped_rows("(0,-1)/", 4), 2);
        assert_eq!(wrapped_rows("(0,-1)/", 3), 3);
        assert_eq!(wrapped_rows("UR4+", 4), 1);
    }

    #[test]
    fn wrapped_rows_survives_zero_width() {
        assert_eq!(wrapped_rows(MEGA_LINE, 0), 1);
    }

    // ---- scramble rows

    #[test]
    fn scramble_rows_counts_every_explicit_line() {
        assert_eq!(scramble_rows(&mega(), 78), 7);
        assert_eq!(scramble_rows(&mega(), 20), 21);
    }

    #[test]
    fn scramble_rows_of_a_single_line_or_nothing_is_at_least_one() {
        assert_eq!(scramble_rows("R U R' U'", 60), 1);
        assert_eq!(scramble_rows("", 60), 1);
        assert_eq!(scramble_rows("", 0), 1);
    }

    // ---- header height

    #[test]
    fn header_stays_at_the_minimum_for_a_one_line_scramble() {
        assert_eq!(header_of("R U2 F' L B2 D R' U F2 L'", 80, 30), HEADER_MIN_H);
        assert_eq!(header_of("", 80, 30), HEADER_MIN_H);
    }

    #[test]
    fn header_grows_to_fit_a_megaminx_scramble() {
        // Seven text rows plus two borders, and the timer still has room to spare.
        assert_eq!(header_of(&mega(), 80, 30), 9);
    }

    #[test]
    fn header_yields_to_the_timer_before_it_finishes_growing() {
        let area = rect(80, 18);
        let footer = footer_height(area);
        let header = header_height(&mega(), area, footer);
        assert_eq!(
            header, 8,
            "the scramble truncates instead of squeezing the timer"
        );
        assert!(
            area.height.saturating_sub(header).saturating_sub(footer) >= TIMER_MIN_H,
            "the block font must keep its {} rows, got {}",
            TIMER_MIN_H,
            area.height.saturating_sub(header).saturating_sub(footer)
        );
    }

    #[test]
    fn header_never_exceeds_its_cap() {
        let many = [MEGA_LINE; 40].join("\n");
        assert_eq!(header_of(&many, 80, 200), HEADER_MAX_H);
        // A single line long enough to wrap past the cap is capped the same way.
        assert_eq!(header_of(&"R U R' U' ".repeat(60), 24, 200), HEADER_MAX_H);
    }

    #[test]
    fn header_degrades_on_a_tiny_terminal() {
        for height in 0..9u16 {
            for width in [0u16, 1, 2, 3, 40] {
                assert_eq!(header_of(&mega(), width, height), 3);
            }
        }
        // Nine rows is where the old fixed header started, and it still starts there.
        assert_eq!(header_of(&mega(), 80, 9), HEADER_MIN_H);
    }

    // ---- help overlay

    #[test]
    fn puzzle_help_rows_fit_the_popup() {
        let names: Vec<&str> = Puzzle::ALL.iter().map(|p| p.name()).collect();
        let rows = puzzle_help_rows(&names);
        for row in &rows {
            assert!(
                row.chars().count() <= HELP_DESC_W,
                "help row {:?} is {} columns, over the {} the popup has",
                row,
                row.chars().count(),
                HELP_DESC_W
            );
        }
        let joined = rows.join(" ");
        for name in &names {
            assert!(joined.contains(*name), "{} missing from the help", name);
        }
    }

    #[test]
    fn puzzle_help_rows_keep_one_row_while_the_names_fit() {
        assert_eq!(
            puzzle_help_rows(&["3x3", "2x2"]),
            ["switch puzzle: 3x3 2x2"]
        );
        assert_eq!(
            puzzle_help_rows(&[]),
            ["switch puzzle:"],
            "an empty list still labels the row"
        );
    }

    #[test]
    fn puzzle_help_rows_wrap_once_the_names_overflow() {
        let names = [
            "2x2", "3x3", "4x4", "5x5", "6x6", "7x7", "pyraminx", "skewb", "megaminx", "sq1",
            "clock",
        ];
        let rows = puzzle_help_rows(&names);
        assert_eq!(rows.len(), 2, "eleven names take two rows, got {:?}", rows);
        assert_eq!(rows[0], "switch puzzle: 2x2 3x3 4x4 5x5 6x6 7x7");
        assert_eq!(rows[1], "pyraminx skewb megaminx sq1 clock");
    }

    // ---- geometry

    #[test]
    fn inner_of_strips_the_border_and_bottoms_out_at_zero() {
        assert_eq!(inner_of(Rect::new(0, 0, 10, 6)), Rect::new(1, 1, 8, 4));
        let tiny = inner_of(Rect::new(4, 7, 1, 1));
        assert_eq!((tiny.width, tiny.height), (0, 0), "no room for an inside");
    }

    #[test]
    fn centered_shrinks_to_the_area_it_is_given() {
        assert_eq!(centered(20, 10, Rect::new(0, 0, 80, 30)), Rect::new(30, 10, 20, 10));
        let clamped = centered(HELP_W, 40, Rect::new(0, 0, 10, 4));
        assert_eq!(clamped, Rect::new(0, 0, 10, 4), "the popup never exceeds the screen");
    }
}
