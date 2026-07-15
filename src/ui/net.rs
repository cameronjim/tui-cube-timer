//! Pure net geometry: an unfolded cube as colored block-glyph lines. No `Frame`, no `App`.
//!
//! The cross is the U face over the F column, then the L F R B strip, then D under the F
//! column, with one blank row or column wherever two faces meet. Three characters are ever
//! drawn, the space and the two blocks below, because the classic Windows console fonts carry
//! CP437 and render anything outside it as tofu.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::cube::{Cube, Face};

/// Cells one sticker spans across, which squares a face up against the terminal's tall cells.
const CELL_W: u16 = 2;
/// Blank cells between two faces, and blank rows between two bands.
const GUTTER: u16 = 1;
/// Bands of faces on the cross: U, the L F R B strip, D.
const BANDS: u16 = 3;

/// One whole sticker, which is what full mode draws.
const FULL_BLOCK: char = '█';
/// Two stickers stacked in one cell: foreground the upper sticker row, background the lower.
const HALF_BLOCK: char = '▀';

/// Cell footprint of the net, `(width, height)`, borders excluded.
///
/// Four faces of `n` two-cell stickers with three gutters between them put the width at 8n+3
/// in both modes. Height is the three bands plus the two gutter rows: 3n+2 in full mode, and
/// 3*ceil(n/2)+2 in compact, where one text row carries two sticker rows.
pub(super) fn size(n: u8, compact: bool) -> (u16, u16) {
    let n = u16::from(n);
    let width = 4 * (n * CELL_W) + 3 * GUTTER;
    let band_rows = if compact { n.div_ceil(2) } else { n };
    (width, BANDS * band_rows + 2 * GUTTER)
}

/// The net as styled lines, one per text row, every character CP437.
///
/// Adjacent cells sharing a style collapse into one span, so a row of a solved face arrives as
/// a single span rather than `n` of them.
pub(super) fn lines(cube: &Cube, compact: bool) -> Vec<Line<'static>> {
    let (width, _) = size(cube.n(), compact);
    let width = usize::from(width);
    // Left edge of the F column, which U and D are centered over: the L face and its gutter.
    let indent = usize::from(cube.n()) * usize::from(CELL_W) + usize::from(GUTTER);

    let mut out = band(cube, compact, width, indent, &[Face::U]);
    out.push(blank(width));
    out.extend(band(
        cube,
        compact,
        width,
        0,
        &[Face::L, Face::F, Face::R, Face::B],
    ));
    out.push(blank(width));
    out.extend(band(cube, compact, width, indent, &[Face::D]));
    out
}

/// One band of the cross: `faces` side by side from `indent`, every row padded out to `width`.
///
/// Compact mode steps two sticker rows at a time, which is what halves the band's height.
fn band(
    cube: &Cube,
    compact: bool,
    width: usize,
    indent: usize,
    faces: &[Face],
) -> Vec<Line<'static>> {
    let n = usize::from(cube.n());
    let step = if compact { 2 } else { 1 };
    (0..n)
        .step_by(step)
        .map(|row| {
            let mut cells = vec![blank_cell(); indent];
            for (i, face) in faces.iter().enumerate() {
                if i > 0 {
                    cells.extend(std::iter::repeat_n(blank_cell(), usize::from(GUTTER)));
                }
                for col in 0..n {
                    let sticker = cell(cube, *face, row, col, compact);
                    cells.extend(std::iter::repeat_n(sticker, usize::from(CELL_W)));
                }
            }
            cells.resize(width, blank_cell());
            merge(cells)
        })
        .collect()
}

/// The glyph and style of one sticker cell.
///
/// Full mode is a block in the sticker's own color. Compact mode hangs the next sticker row
/// off the same cell as the background, which an odd cube's last row has none of, so there the
/// background is left unset and the terminal's own shows through.
fn cell(cube: &Cube, face: Face, row: usize, col: usize, compact: bool) -> (char, Style) {
    let upper = Style::default().fg(color_of(cube.sticker(face, row, col)));
    if !compact {
        return (FULL_BLOCK, upper);
    }
    let lower = row + 1;
    if lower < usize::from(cube.n()) {
        (
            HALF_BLOCK,
            upper.bg(color_of(cube.sticker(face, lower, col))),
        )
    } else {
        (HALF_BLOCK, upper)
    }
}

/// An empty cell: a plain space carrying no style, which is what gutters and the corners draw.
fn blank_cell() -> (char, Style) {
    (' ', Style::default())
}

/// A gutter row, nothing but unstyled spaces across the full width.
fn blank(width: usize) -> Line<'static> {
    Line::from(Span::raw(" ".repeat(width)))
}

/// Cells into a line, runs of one style becoming one span.
fn merge(cells: Vec<(char, Style)>) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run = String::new();
    let mut style = Style::default();
    for (glyph, cell_style) in cells {
        if !run.is_empty() && cell_style != style {
            spans.push(Span::styled(std::mem::take(&mut run), style));
        }
        style = cell_style;
        run.push(glyph);
    }
    if !run.is_empty() {
        spans.push(Span::styled(run, style));
    }
    Line::from(spans)
}

/// The terminal color a face's stickers wear.
///
/// Magenta stands in for orange, which the 16-color palette every console font agrees on does
/// not carry. This map lives here and never in `cube`, which knows faces and not colors.
fn color_of(face: Face) -> Color {
    match face {
        Face::U => Color::White,
        Face::D => Color::Yellow,
        Face::F => Color::Green,
        Face::B => Color::Blue,
        Face::R => Color::Red,
        Face::L => Color::Magenta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character the net is allowed to draw, all three of them CP437.
    ///
    /// Eighth blocks and braille are absent from the classic Windows console fonts, so
    /// anything outside this set is the tofu regression the trend chart already guards.
    const NET_CP437: [char; 3] = [' ', FULL_BLOCK, HALF_BLOCK];

    /// Every character of a line, its spans concatenated.
    fn text_of(line: &Line<'static>) -> String {
        line.spans.iter().map(|span| span.content.as_ref()).collect()
    }

    /// Every line as its individual cells, spans expanded, for asserting one position at a time.
    fn cells_of(rows: &[Line<'static>]) -> Vec<Vec<(char, Style)>> {
        rows.iter()
            .map(|line| {
                line.spans
                    .iter()
                    .flat_map(|span| span.content.chars().map(move |c| (c, span.style)))
                    .collect()
            })
            .collect()
    }

    /// Left edge of the F column on an `n` cube, which is also the U and D indent.
    fn indent_of(n: usize) -> usize {
        2 * n + 1
    }

    // ---- footprint

    #[test]
    fn size_follows_the_plan_formulas_at_every_cube_size() {
        for n in 2..=7u8 {
            let nn = u16::from(n);
            assert_eq!(size(n, false), (8 * nn + 3, 3 * nn + 2), "full {n}x{n}");
            assert_eq!(
                size(n, true),
                (8 * nn + 3, 3 * nn.div_ceil(2) + 2),
                "compact {n}x{n}"
            );
        }
    }

    #[test]
    fn the_width_is_the_same_in_both_modes_and_only_the_height_shrinks() {
        for n in 2..=7u8 {
            let (full_w, full_h) = size(n, false);
            let (compact_w, compact_h) = size(n, true);
            assert_eq!(full_w, compact_w, "{n}x{n} keeps its width");
            assert!(compact_h <= full_h, "{n}x{n} compact is never taller");
        }
    }

    #[test]
    fn every_line_matches_the_footprint_size_reports() {
        for n in 2..=7u8 {
            for compact in [false, true] {
                let (width, height) = size(n, compact);
                let rows = lines(&Cube::solved(n), compact);
                assert_eq!(
                    rows.len(),
                    usize::from(height),
                    "{n}x{n} compact={compact} row count"
                );
                for (i, line) in rows.iter().enumerate() {
                    let text = text_of(line);
                    assert_eq!(
                        text.chars().count(),
                        usize::from(width),
                        "row {i} of {n}x{n} compact={compact} is {text:?}"
                    );
                }
            }
        }
    }

    // ---- the color map

    #[test]
    fn the_face_color_map_is_the_one_the_plan_fixes() {
        assert_eq!(color_of(Face::U), Color::White);
        assert_eq!(color_of(Face::D), Color::Yellow);
        assert_eq!(color_of(Face::F), Color::Green);
        assert_eq!(color_of(Face::B), Color::Blue);
        assert_eq!(color_of(Face::R), Color::Red);
        assert_eq!(color_of(Face::L), Color::Magenta);
    }

    // ---- placement

    #[test]
    fn the_u_face_sits_centered_over_the_f_column() {
        let rows = lines(&Cube::solved(3), false);
        for (i, line) in rows[..3].iter().enumerate() {
            let spans = &line.spans;
            assert_eq!(spans.len(), 3, "U row {i}: indent, the face, the pad");
            assert_eq!(spans[0].content.as_ref(), " ".repeat(indent_of(3)));
            assert_eq!(spans[0].style, Style::default(), "the indent is unstyled");
            assert_eq!(spans[1].content.as_ref(), "██████");
            assert_eq!(spans[1].style, Style::default().fg(Color::White));
            assert_eq!(spans[2].style, Style::default(), "the pad is unstyled");
        }
    }

    #[test]
    fn the_middle_band_runs_l_f_r_b_between_gutters() {
        let rows = lines(&Cube::solved(3), false);
        for (i, line) in rows[4..7].iter().enumerate() {
            let spans = &line.spans;
            assert_eq!(spans.len(), 7, "strip row {i}: four faces and three gutters");
            for (j, face) in [Face::L, Face::F, Face::R, Face::B].iter().enumerate() {
                let span = &spans[j * 2];
                assert_eq!(span.content.as_ref(), "██████", "{face:?} on strip row {i}");
                assert_eq!(
                    span.style,
                    Style::default().fg(color_of(*face)),
                    "{face:?} on strip row {i}"
                );
            }
            for j in 0..3 {
                let span = &spans[j * 2 + 1];
                assert_eq!(span.content.as_ref(), " ", "gutter {j} on strip row {i}");
                assert_eq!(span.style, Style::default(), "gutter {j} carries no style");
            }
        }
    }

    #[test]
    fn the_d_face_sits_under_the_f_column() {
        let rows = lines(&Cube::solved(3), false);
        for (i, line) in rows[8..11].iter().enumerate() {
            let spans = &line.spans;
            assert_eq!(spans.len(), 3, "D row {i}");
            assert_eq!(spans[0].content.as_ref(), " ".repeat(indent_of(3)));
            assert_eq!(spans[1].content.as_ref(), "██████");
            assert_eq!(spans[1].style, Style::default().fg(Color::Yellow));
        }
    }

    #[test]
    fn every_face_is_a_solid_block_in_its_net_position() {
        for n in 2..=7u8 {
            let nn = usize::from(n);
            let grid = cells_of(&lines(&Cube::solved(n), false));
            let indent = indent_of(nn);
            let stride = 2 * nn + 1;
            // Each face with the top row of its band and the left cell of its block.
            let places = [
                (Face::U, 0, indent),
                (Face::L, nn + 1, 0),
                (Face::F, nn + 1, indent),
                (Face::R, nn + 1, indent + stride),
                (Face::B, nn + 1, indent + 2 * stride),
                (Face::D, 2 * nn + 2, indent),
            ];
            for (face, top, left) in places {
                for (r, row) in grid[top..top + nn].iter().enumerate() {
                    for (c, (glyph, style)) in row[left..left + 2 * nn].iter().enumerate() {
                        assert_eq!(
                            *glyph, FULL_BLOCK,
                            "{face:?} cell ({r},{c}) on {n}x{n} is not a block"
                        );
                        assert_eq!(
                            *style,
                            Style::default().fg(color_of(face)),
                            "{face:?} cell ({r},{c}) on {n}x{n} is the wrong color"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_bands_are_separated_by_an_unstyled_blank_row() {
        for n in 2..=7u8 {
            for compact in [false, true] {
                let rows = lines(&Cube::solved(n), compact);
                let band_rows = if compact {
                    usize::from(n).div_ceil(2)
                } else {
                    usize::from(n)
                };
                for gutter in [band_rows, 2 * band_rows + 1] {
                    let line = &rows[gutter];
                    assert_eq!(line.spans.len(), 1, "gutter {gutter} of {n}x{n} is one span");
                    assert_eq!(line.spans[0].style, Style::default());
                    assert!(
                        text_of(line).chars().all(|c| c == ' '),
                        "gutter {gutter} of {n}x{n} is {:?}",
                        text_of(line)
                    );
                }
            }
        }
    }

    #[test]
    fn the_corners_of_the_cross_are_unstyled_spaces() {
        for n in 2..=7u8 {
            let nn = usize::from(n);
            let grid = cells_of(&lines(&Cube::solved(n), false));
            let indent = indent_of(nn);
            for top in [0, 2 * nn + 2] {
                for row in grid[top..top + nn].iter() {
                    let left = &row[..indent];
                    let right = &row[indent + 2 * nn..];
                    for (glyph, style) in left.iter().chain(right.iter()) {
                        assert_eq!(*glyph, ' ', "a corner of the {n}x{n} cross is not blank");
                        assert_eq!(*style, Style::default(), "a corner carries a style");
                    }
                }
            }
        }
    }

    // ---- compact mode

    #[test]
    fn compact_mode_pairs_two_sticker_rows_into_one_text_row() {
        for n in [2u8, 4, 6] {
            let rows = lines(&Cube::solved(n), true);
            assert_eq!(
                rows.len(),
                3 * (usize::from(n) / 2) + 2,
                "an even {n}x{n} halves every band"
            );
        }
    }

    #[test]
    fn a_compact_u_band_carries_the_upper_row_in_front_of_the_lower() {
        // 3x3: text row 0 is sticker rows 0 and 1, text row 1 is sticker row 2 with nothing under it.
        let grid = cells_of(&lines(&Cube::solved(3), true));
        assert_eq!(grid.len(), 8, "two rows a band plus two gutters");
        let indent = indent_of(3);

        let (glyph, style) = grid[0][indent];
        assert_eq!(glyph, HALF_BLOCK);
        assert_eq!(
            style,
            Style::default().fg(Color::White).bg(Color::White),
            "both halves of the first text row are U stickers"
        );

        let (glyph, style) = grid[1][indent];
        assert_eq!(glyph, HALF_BLOCK);
        assert_eq!(style, Style::default().fg(Color::White));
        assert_eq!(style.bg, None, "the third sticker row has no row under it");
    }

    #[test]
    fn an_odd_cubes_last_compact_row_leaves_its_background_unset() {
        for n in [3u8, 5, 7] {
            let nn = usize::from(n);
            let band_rows = nn.div_ceil(2);
            let grid = cells_of(&lines(&Cube::solved(n), true));
            let indent = indent_of(nn);
            // The F column top to bottom: U, then F in the strip, then D.
            for (top, face) in [
                (0, Face::U),
                (band_rows + 1, Face::F),
                (2 * band_rows + 2, Face::D),
            ] {
                let (glyph, style) = grid[top + band_rows - 1][indent];
                assert_eq!(glyph, HALF_BLOCK, "{face:?} last row of {n}x{n}");
                assert_eq!(
                    style.fg,
                    Some(color_of(face)),
                    "{face:?} last row of {n}x{n} keeps its foreground"
                );
                assert_eq!(
                    style.bg, None,
                    "{face:?} last row of {n}x{n} has no lower sticker row"
                );

                let (_, paired) = grid[top + band_rows - 2][indent];
                assert_eq!(
                    paired.bg,
                    Some(color_of(face)),
                    "{face:?} row above it does pair with one"
                );
            }
        }
    }

    // ---- the glyph set

    #[test]
    fn each_mode_draws_only_its_own_block_glyph() {
        for n in 2..=7u8 {
            let full: String = lines(&Cube::solved(n), false).iter().map(text_of).collect();
            assert!(full.contains(FULL_BLOCK), "full mode draws blocks on {n}x{n}");
            assert!(
                !full.contains(HALF_BLOCK),
                "full mode draws no half blocks on {n}x{n}"
            );

            let compact: String = lines(&Cube::solved(n), true).iter().map(text_of).collect();
            assert!(
                compact.contains(HALF_BLOCK),
                "compact mode draws half blocks on {n}x{n}"
            );
            assert!(
                !compact.contains(FULL_BLOCK),
                "compact mode draws no full blocks on {n}x{n}"
            );
        }
    }

    #[test]
    fn the_net_draws_nothing_a_cp437_console_cannot_render() {
        for n in 2..=7u8 {
            for compact in [false, true] {
                for (i, line) in lines(&Cube::solved(n), compact).iter().enumerate() {
                    let text = text_of(line);
                    for c in text.chars() {
                        assert!(
                            NET_CP437.contains(&c),
                            "row {i} of {n}x{n} compact={compact} drew {c:?}, \
                             which no CP437 font carries: {text:?}"
                        );
                    }
                }
            }
        }
    }
}
