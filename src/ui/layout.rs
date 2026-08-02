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
/// Rows the timer panel needs to keep its block font: [`GLYPH_H`](super::timer::GLYPH_H) glyph rows plus borders.
pub(super) const TIMER_MIN_H: u16 = super::timer::GLYPH_H as u16 + 2;

/// Popup width; the help text is packed to fit it and the popup shrinks to the terminal.
pub(super) const HELP_W: u16 = 62;
/// Width of the key column, indent excluded.
pub(super) const HELP_KEY_W: usize = 16;
/// Columns a help row leaves for its description: [`HELP_W`] less borders, indent and key column.
const HELP_DESC_W: usize = HELP_W as usize - 2 - 2 - HELP_KEY_W;

/// Width of the solve-detail popup, which shrinks to the terminal exactly as the help overlay does.
const DETAIL_W: u16 = 52;
/// Tallest the solve-detail popup grows; past this the scramble truncates inside it.
const DETAIL_MAX_H: u16 = 20;
/// Rows the detail popup spends on everything but the scramble: the time, the date, the hint and the blanks between them.
const DETAIL_FIXED_ROWS: usize = 6;

/// Width of the sessions popup: a marker, an id, a name column, a puzzle and a solve count.
pub(super) const SESSIONS_W: u16 = 44;
/// Columns the name column of the sessions popup gets, padded or cut to exactly this.
pub(super) const SESSIONS_NAME_W: usize = 18;
/// Rows the sessions popup spends on something other than a session: two borders and the hint.
const SESSIONS_FIXED_ROWS: usize = 3;

/// Entries kept above the cursor in a scrolling list while it moves down.
const LIST_LEAD: usize = 2;
/// Columns between two entries of a stats row; the renderer inserts exactly this many.
pub(super) const STAT_SEP: usize = 3;
/// Columns the prefix column of a stats row is padded to, so all three rows line up under it.
///
/// Seven is the width of `current`, the longest of the three prefixes; the space after the
/// column is added on top, so every row's first entry starts at the same column.
pub(super) const STAT_PREFIX_W: usize = 7;

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

/// Where the solve-detail popup sits: as tall as its scramble needs, capped and clamped to `area`.
pub(super) fn detail_popup(scramble: &str, area: Rect) -> Rect {
    let width = DETAIL_W.min(area.width);
    let wanted = scramble_rows(scramble, width.saturating_sub(2))
        .saturating_add(DETAIL_FIXED_ROWS)
        .saturating_add(2)
        .min(DETAIL_MAX_H as usize) as u16;
    centered(width, wanted, area)
}

/// Where the sessions popup sits, and how many of `count` sessions have a row inside it.
///
/// The popup is content-driven and clamped to `area` like the other two. The bottom inner row
/// always belongs to the hint, so a list too tall for the popup gets one row fewer and scrolls
/// under the cursor instead: the returned count is a window size, not the whole list.
pub(super) fn sessions_popup(count: usize, area: Rect) -> (Rect, usize) {
    let wanted = count
        .saturating_add(SESSIONS_FIXED_ROWS)
        .min(u16::MAX as usize) as u16;
    let popup = centered(SESSIONS_W, wanted, area);
    let rows = (popup.height.saturating_sub(2) as usize).saturating_sub(1);
    (popup, rows.min(count))
}

/// The slice of a scrolling list to draw, as `(entries hidden above, entries visible)`.
///
/// Both counts index from the top of the list as drawn, which is the newest solve in the
/// times panel and the first session in the sessions popup. The window keeps [`LIST_LEAD`]
/// entries above the cursor where it can and never runs past the end. Those two rules alone
/// can scroll the cursor out of view in a panel one or two rows tall, which is what the guard
/// at the end is for.
pub(super) fn list_window(selected: usize, total: usize, rows: usize) -> (usize, usize) {
    if total == 0 || rows == 0 {
        return (0, 0);
    }
    let selected = selected.min(total - 1);
    let mut start = selected.saturating_sub(LIST_LEAD);
    start = start.min(total.saturating_sub(rows));
    // That clamp can push the selection off the bottom of a panel one or two rows tall.
    if selected.saturating_sub(start) >= rows {
        start = selected.saturating_sub(rows - 1);
    }
    (start, rows.min(total - start))
}

/// Columns a stats row has left for its entries once the prefix column and its space are taken.
///
/// Every row pays for the column, labelled or not, because the three rows only read as a block
/// if their values start in the same place.
pub(super) fn stat_budget(width: u16) -> u16 {
    width.saturating_sub(STAT_PREFIX_W as u16).saturating_sub(1)
}

/// How many leading entries of `widths` fit in `width` columns with [`STAT_SEP`] between them.
///
/// Entries are dropped from the right, which is the order the stats strip ranks them in. A
/// non-empty list never packs down to nothing: one clipped entry reads better than a blank row.
pub(super) fn fit_count(widths: &[usize], width: u16) -> usize {
    if widths.is_empty() {
        return 0;
    }
    let width = width as usize;
    let mut used = 0usize;
    let mut kept = 0usize;
    for w in widths {
        let needed = if kept == 0 {
            *w
        } else {
            STAT_SEP.saturating_add(*w)
        };
        let end = used.saturating_add(needed);
        if end > width {
            break;
        }
        used = end;
        kept = kept.saturating_add(1);
    }
    kept.max(1)
}

/// The `/<puzzle>` description, packed into as many rows as [`HELP_DESC_W`] allows.
///
/// Twelve puzzle names do not fit on one row, and the popup does not wrap, so the list
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
            "clock", "oh",
        ];
        let rows = puzzle_help_rows(&names);
        assert_eq!(rows.len(), 2, "twelve names take two rows, got {:?}", rows);
        assert_eq!(rows[0], "switch puzzle: 2x2 3x3 4x4 5x5 6x6 7x7");
        assert_eq!(rows[1], "pyraminx skewb megaminx sq1 clock oh");
    }

    // ---- list window

    #[test]
    fn list_window_shows_the_top_of_the_list_first() {
        assert_eq!(list_window(0, 87, 10), (0, 10));
        assert_eq!(list_window(1, 87, 10), (0, 10));
        assert_eq!(list_window(2, 87, 10), (0, 10), "the lead is used up first");
    }

    #[test]
    fn list_window_keeps_two_entries_above_a_cursor_in_the_middle() {
        assert_eq!(list_window(3, 87, 10), (1, 10));
        assert_eq!(list_window(40, 87, 10), (38, 10));
    }

    #[test]
    fn list_window_stops_at_the_end_of_the_list() {
        // The last ten of 87 start at 77, and the window does not scroll past them.
        assert_eq!(list_window(80, 87, 10), (77, 10));
        assert_eq!(list_window(86, 87, 10), (77, 10));
    }

    #[test]
    fn list_window_shows_everything_when_the_list_is_shorter_than_the_panel() {
        assert_eq!(list_window(0, 4, 10), (0, 4));
        assert_eq!(list_window(3, 4, 10), (0, 4));
    }

    #[test]
    fn list_window_keeps_the_cursor_visible_in_a_one_or_two_row_panel() {
        for rows in [1usize, 2] {
            for selected in 0..20usize {
                let (start, len) = list_window(selected, 20, rows);
                assert!(
                    (start..start + len).contains(&selected),
                    "selection {} fell outside {}..{} at {} rows",
                    selected,
                    start,
                    start + len,
                    rows
                );
            }
        }
        assert_eq!(list_window(7, 20, 1), (7, 1));
        assert_eq!(list_window(7, 20, 2), (6, 2));
    }

    #[test]
    fn list_window_of_an_empty_or_invisible_list_is_empty() {
        assert_eq!(list_window(0, 0, 10), (0, 0));
        assert_eq!(list_window(usize::MAX, 0, 10), (0, 0));
        assert_eq!(list_window(0, 20, 0), (0, 0));
    }

    #[test]
    fn a_cursor_past_the_end_is_clamped_to_the_last_entry() {
        assert_eq!(list_window(usize::MAX, 87, 10), (77, 10));
        assert_eq!(list_window(500, 4, 10), (0, 4));
    }

    /// The sessions popup windows the same way, with the rows [`sessions_popup`] leaves it.
    #[test]
    fn list_window_scrolls_a_sessions_list_deep_under_its_cursor() {
        let (_, rows) = sessions_popup(15, rect(80, 10));
        assert_eq!(rows, 7, "seven sessions, then the hint");
        assert_eq!(list_window(0, 15, rows), (0, 7), "it opens on the first");
        assert_eq!(list_window(14, 15, rows), (8, 7), "and follows the cursor down");
        assert_eq!(list_window(9, 15, rows), (7, 7));
    }

    // ---- stats packing

    #[test]
    fn stat_budget_charges_every_row_for_the_prefix_column() {
        // Seven columns of prefix and the space after it, whether the row labels itself or not.
        assert_eq!(STAT_PREFIX_W, "current".len(), "the column fits the widest prefix");
        assert_eq!(stat_budget(52), 44);
        assert_eq!(stat_budget(9), 1);
        for width in 0..=8u16 {
            assert_eq!(stat_budget(width), 0, "a row this narrow has nothing left");
        }
    }

    #[test]
    fn fit_count_keeps_what_the_row_holds_and_drops_the_rest() {
        // Three entries of 10 columns need 10 + 3 + 10 + 3 + 10 = 36.
        let widths = [10usize, 10, 10];
        assert_eq!(fit_count(&widths, 36), 3);
        assert_eq!(fit_count(&widths, 35), 2);
        assert_eq!(fit_count(&widths, 23), 2);
        assert_eq!(fit_count(&widths, 22), 1);
    }

    #[test]
    fn fit_count_never_empties_a_row_it_could_only_clip() {
        assert_eq!(fit_count(&[10, 10], 0), 1);
        assert_eq!(fit_count(&[10, 10], 3), 1);
        assert_eq!(fit_count(&[], 80), 0, "nothing to pack is still nothing");
    }

    #[test]
    fn fit_count_survives_absurd_widths() {
        assert_eq!(fit_count(&[usize::MAX, 1], u16::MAX), 1);
        assert_eq!(fit_count(&[1, usize::MAX], u16::MAX), 1);
    }

    // ---- solve detail popup

    #[test]
    fn the_detail_popup_grows_to_fit_a_megaminx_scramble() {
        // Seven scramble rows, six fixed rows and two borders.
        let popup = detail_popup(&mega(), rect(80, 30));
        assert_eq!((popup.width, popup.height), (DETAIL_W, 15));
        assert_eq!((popup.x, popup.y), (14, 7), "and it is centered");
    }

    #[test]
    fn the_detail_popup_is_shortest_for_a_one_line_scramble() {
        let popup = detail_popup("R U2 F' L B2 D R' U F2 L'", rect(80, 30));
        assert_eq!(popup.height, 9);
    }

    #[test]
    fn the_detail_popup_is_capped_and_clamped() {
        let many = [MEGA_LINE; 40].join("\n");
        assert_eq!(detail_popup(&many, rect(80, 200)).height, DETAIL_MAX_H);
        for (w, h) in [(80u16, 30u16), (44, 12), (30, 8), (10, 4), (1, 1), (0, 0)] {
            let popup = detail_popup(&mega(), rect(w, h));
            assert!(popup.width <= w && popup.height <= h, "{:?} escapes {}x{}", popup, w, h);
        }
    }

    // ---- sessions popup

    #[test]
    fn the_sessions_popup_grows_to_hold_the_whole_list() {
        // Twelve defaults and three of your own: fifteen rows, a hint row and two borders.
        let (popup, rows) = sessions_popup(15, rect(80, 30));
        assert_eq!((popup.width, popup.height), (SESSIONS_W, 18));
        assert_eq!(rows, 15, "everything fits, so everything is drawn");
        assert_eq!((popup.x, popup.y), (18, 6), "and it is centered");
    }

    #[test]
    fn the_sessions_popup_keeps_its_last_row_for_the_hint() {
        // Eight rows leaves six inside the border: five sessions and the hint.
        let (popup, rows) = sessions_popup(15, rect(80, 8));
        assert_eq!(popup.height, 8);
        assert_eq!(rows, 5);

        // One row short of the whole list costs a session, which the window then scrolls.
        let (popup, rows) = sessions_popup(15, rect(80, 17));
        assert_eq!(popup.height, 17);
        assert_eq!(rows, 14);
    }

    #[test]
    fn the_sessions_popup_is_clamped_to_the_terminal() {
        for (w, h) in [(80u16, 30u16), (44, 12), (30, 8), (10, 4), (1, 1), (0, 0)] {
            let (popup, rows) = sessions_popup(15, rect(w, h));
            assert!(popup.width <= w && popup.height <= h, "{:?} escapes {}x{}", popup, w, h);
            assert!(rows <= 15, "the popup never claims to draw more than it has");
            assert!(
                rows + 3 <= popup.height.max(3) as usize,
                "{} rows and a hint do not fit a popup {} tall",
                rows,
                popup.height
            );
        }
    }

    #[test]
    fn the_sessions_popup_of_a_short_list_has_no_hidden_rows() {
        let (popup, rows) = sessions_popup(2, rect(80, 30));
        assert_eq!(popup.height, 5, "two sessions, the hint and two borders");
        assert_eq!(rows, 2);
        assert_eq!(sessions_popup(0, rect(80, 30)).1, 0, "an empty list draws no rows");
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
