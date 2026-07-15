//! Facelet-level cube state for the NxN events: which color sits where after a scramble.
//!
//! A cube is 6*n*n stickers, each naming the face it belongs to when solved, and a move
//! is a fixed permutation of them. Pure state: no ratatui, no IO, no randomness. The
//! conventions (face order, orientation, index math) are contract, spelled out in
//! `claude-docs/plans/00-cube-model-and-preview.md`; later phases build on them.

use crate::types::Puzzle;

/// A face of the cube, doubling as the sticker color that lives there when solved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    U,
    L,
    F,
    R,
    B,
    D,
}

impl Face {
    /// Net reading order: U, then the L F R B strip, then D. Also the storage order.
    pub const ALL: [Face; 6] = [Face::U, Face::L, Face::F, Face::R, Face::B, Face::D];

    /// Position in [`Face::ALL`], which is also the face's block index in the sticker vector.
    fn index(self) -> usize {
        match self {
            Face::U => 0,
            Face::L => 1,
            Face::F => 2,
            Face::R => 3,
            Face::B => 4,
            Face::D => 5,
        }
    }
}

/// Why a scramble token failed to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveError {
    /// Not WCA cube notation this model knows.
    BadToken(String),
    /// Valid notation, but it turns every layer of this cube size or more.
    TooWide(String),
}

/// One NxN cube, sizes 2 through 7.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cube {
    n: u8,
    /// Face-major in [`Face::ALL`] order, row-major within a face, row 0 col 0 top left.
    stickers: Vec<Face>,
}

impl Cube {
    /// A solved cube. `n` must be 2 through 7, the only sizes any caller can reach.
    pub fn solved(n: u8) -> Cube {
        debug_assert!((2..=7).contains(&n), "no {n}x{n} event exists");
        let per_face = usize::from(n) * usize::from(n);
        let mut stickers = Vec::with_capacity(6 * per_face);
        for face in Face::ALL {
            stickers.extend(std::iter::repeat_n(face, per_face));
        }
        Cube { n, stickers }
    }

    /// The cube size a puzzle previews at, or None for the events without a model yet.
    pub fn size(puzzle: Puzzle) -> Option<u8> {
        match puzzle {
            Puzzle::Cube2 => Some(2),
            Puzzle::Cube3 | Puzzle::Oh => Some(3),
            Puzzle::Cube4 => Some(4),
            Puzzle::Cube5 => Some(5),
            Puzzle::Cube6 => Some(6),
            Puzzle::Cube7 => Some(7),
            _ => None,
        }
    }

    /// The state `scramble` leaves a solved cube in, or None when the event has no model.
    ///
    /// Scrambles only ever come from `scramble::generate`, so a token this refuses is a
    /// generator bug, not user input; it answers None rather than panicking the timer.
    pub fn for_scramble(puzzle: Puzzle, scramble: &str) -> Option<Cube> {
        let mut cube = Cube::solved(Cube::size(puzzle)?);
        cube.apply_scramble(scramble).ok()?;
        Some(cube)
    }

    /// The cube size.
    pub fn n(&self) -> u8 {
        self.n
    }

    /// The color at `row`, `col` of `face`, oriented as the face appears on the net.
    pub fn sticker(&self, face: Face, row: usize, col: usize) -> Face {
        let n = usize::from(self.n);
        debug_assert!(row < n && col < n, "sticker ({row},{col}) is off an {n}x{n} face");
        self.stickers[face.index() * n * n + row * n + col]
    }

    /// Whether every face is a solid block of its own color.
    // Only the tests read this so far; the random-state scramble phase needs it as the goal
    // test its search runs against.
    #[allow(dead_code)]
    pub fn is_solved(&self) -> bool {
        let per_face = usize::from(self.n) * usize::from(self.n);
        self.stickers
            .chunks_exact(per_face)
            .zip(Face::ALL)
            .all(|(chunk, face)| chunk.iter().all(|s| *s == face))
    }

    /// Apply a whitespace-separated WCA scramble, stopping at the first bad token.
    ///
    /// The moves before a bad token are already applied when the error returns, so a
    /// caller that cares about the state discards the cube rather than reusing it.
    pub fn apply_scramble(&mut self, scramble: &str) -> Result<(), MoveError> {
        for token in scramble.split_whitespace() {
            let turn = parse_turn(token, self.n)?;
            for _ in 0..turn.quarters {
                self.quarter_turn(turn.face, turn.width);
            }
        }
        Ok(())
    }

    /// Index of one sticker in the flat vector.
    fn at(&self, face: Face, row: usize, col: usize) -> usize {
        let n = usize::from(self.n);
        face.index() * n * n + row * n + col
    }

    /// Move the sticker at each position to the next, and the last back to the first.
    fn cycle(&mut self, legs: [(Face, usize, usize); 4]) {
        let mut idx = [0usize; 4];
        for (k, &(face, row, col)) in legs.iter().enumerate() {
            idx[k] = self.at(face, row, col);
        }
        let carried = self.stickers[idx[3]];
        self.stickers[idx[3]] = self.stickers[idx[2]];
        self.stickers[idx[2]] = self.stickers[idx[1]];
        self.stickers[idx[1]] = self.stickers[idx[0]];
        self.stickers[idx[0]] = carried;
    }

    /// One clockwise quarter turn of `face` carrying `width` layers with it.
    fn quarter_turn(&mut self, face: Face, width: u8) {
        let n = usize::from(self.n);
        self.rotate_face(face);
        for depth in 0..usize::from(width) {
            for i in 0..n {
                self.cycle(strip_cycle(face, n, depth, i));
            }
        }
    }

    /// Rotate the turned face's own stickers a quarter turn clockwise, in place.
    ///
    /// Clockwise is the same grid rotation for all six faces, because every face's rows
    /// and columns are oriented on the net as a viewer outside that face sees them.
    fn rotate_face(&mut self, face: Face) {
        let n = usize::from(self.n);
        let m = n - 1;
        for r in 0..n / 2 {
            for c in r..m - r {
                self.cycle([
                    (face, r, c),
                    (face, c, m - r),
                    (face, m - r, m - c),
                    (face, m - c, r),
                ]);
            }
        }
    }
}

/// One parsed scramble token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Turn {
    face: Face,
    /// 1 bare, 2 for `w`, 3 for `3..w`. Always below the cube size.
    width: u8,
    /// Clockwise quarter turns: 1 plain, 2 for `2`, 3 for `'`.
    quarters: u8,
}

/// Parse one WCA token against a cube of size `n`.
fn parse_turn(token: &str, n: u8) -> Result<Turn, MoveError> {
    let bad = || MoveError::BadToken(String::from(token));
    // Walking the bytes rather than the str keeps a non-ASCII token a `BadToken`
    // instead of a panic on a character boundary.
    let mut rest = token.as_bytes();

    let triple = matches!(rest.first(), Some(b'3'));
    if triple {
        rest = &rest[1..];
    }

    let face = match rest.first() {
        Some(b'U') => Face::U,
        Some(b'D') => Face::D,
        Some(b'L') => Face::L,
        Some(b'R') => Face::R,
        Some(b'F') => Face::F,
        Some(b'B') => Face::B,
        _ => return Err(bad()),
    };
    rest = &rest[1..];

    let wide = matches!(rest.first(), Some(b'w'));
    if wide {
        rest = &rest[1..];
    } else if triple {
        // The `3` prefix only ever counts layers, so it is meaningless without the `w`.
        return Err(bad());
    }

    let (quarters, suffix_len) = match rest.first() {
        None => (1u8, 0),
        Some(b'\'') => (3, 1),
        Some(b'2') => (2, 1),
        _ => return Err(bad()),
    };
    if rest.len() != suffix_len {
        return Err(bad());
    }

    let width = match (triple, wide) {
        (true, _) => 3,
        (false, true) => 2,
        (false, false) => 1,
    };
    // Turning n layers of an n cube is a whole-cube rotation, not a move.
    if width >= n {
        return Err(MoveError::TooWide(String::from(token)));
    }

    Ok(Turn {
        face,
        width,
        quarters,
    })
}

/// The four positions one clockwise quarter turn of `face` cycles at `depth`, index `i`.
///
/// Position `k` moves to position `k + 1` and the last moves to the first. Every row was
/// derived by carrying a net coordinate through the 3D rotation about that face's outward
/// normal, so the flipped legs are computed rather than guessed. The `R` row is the one
/// spelled out in the plan, and reproducing it is what pins the other five.
fn strip_cycle(face: Face, n: usize, depth: usize, i: usize) -> [(Face, usize, usize); 4] {
    let m = n - 1;
    let d = depth;
    // The slab's far edge: the same layer counted from the opposite side of the cube.
    let s = m - d;
    match face {
        Face::R => [
            (Face::F, i, s),
            (Face::U, i, s),
            (Face::B, m - i, d),
            (Face::D, i, s),
        ],
        Face::L => [
            (Face::U, i, d),
            (Face::F, i, d),
            (Face::D, i, d),
            (Face::B, m - i, s),
        ],
        Face::U => [
            (Face::F, d, i),
            (Face::L, d, i),
            (Face::B, d, i),
            (Face::R, d, i),
        ],
        Face::D => [
            (Face::F, s, i),
            (Face::R, s, i),
            (Face::B, s, i),
            (Face::L, s, i),
        ],
        Face::F => [
            (Face::U, s, i),
            (Face::R, i, d),
            (Face::D, d, m - i),
            (Face::L, m - i, s),
        ],
        Face::B => [
            (Face::U, d, i),
            (Face::L, m - i, d),
            (Face::D, s, m - i),
            (Face::R, i, s),
        ],
    }
}

/// The scramble that undoes `scramble`: tokens reversed, plain and prime swapped.
// Only the tests call this so far; the random-state scramble phase reaches a state by inverting
// the solution a solver found for it.
#[allow(dead_code)]
pub fn invert_scramble(scramble: &str) -> String {
    let mut out = String::with_capacity(scramble.len() + 8);
    for token in scramble.split_whitespace().rev() {
        if !out.is_empty() {
            out.push(' ');
        }
        match token.strip_suffix('\'') {
            Some(base) => out.push_str(base),
            // A half turn is its own inverse.
            None if token.ends_with('2') => out.push_str(token),
            None => {
                out.push_str(token);
                out.push('\'');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scramble::generate_with_rng;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    /// The events this model answers for, and the size each previews at.
    const MODELLED: [(Puzzle, u8); 7] = [
        (Puzzle::Cube2, 2),
        (Puzzle::Cube3, 3),
        (Puzzle::Cube4, 4),
        (Puzzle::Cube5, 5),
        (Puzzle::Cube6, 6),
        (Puzzle::Cube7, 7),
        (Puzzle::Oh, 3),
    ];

    /// Every move base WCA cube notation uses, with the layer width each one turns.
    const BASES: [(&str, u8); 18] = [
        ("U", 1),
        ("D", 1),
        ("L", 1),
        ("R", 1),
        ("F", 1),
        ("B", 1),
        ("Uw", 2),
        ("Dw", 2),
        ("Lw", 2),
        ("Rw", 2),
        ("Fw", 2),
        ("Bw", 2),
        ("3Uw", 3),
        ("3Dw", 3),
        ("3Lw", 3),
        ("3Rw", 3),
        ("3Fw", 3),
        ("3Bw", 3),
    ];

    /// A solved cube of size `n` with `scramble` applied, which must be legal.
    fn after(n: u8, scramble: &str) -> Cube {
        let mut cube = Cube::solved(n);
        cube.apply_scramble(scramble)
            .unwrap_or_else(|e| panic!("{scramble:?} on {n}x{n} is legal, got {e:?}"));
        cube
    }

    /// How many times `scramble` has to repeat before the cube is solved again.
    fn order(n: u8, scramble: &str) -> usize {
        let mut cube = Cube::solved(n);
        for k in 1..=2000 {
            cube.apply_scramble(scramble).expect("a legal scramble");
            if cube.is_solved() {
                return k;
            }
        }
        panic!("{scramble:?} never returned to solved on a {n}x{n}");
    }

    /// How many stickers of each colour the cube carries, in [`Face::ALL`] order.
    fn colour_counts(cube: &Cube) -> [usize; 6] {
        let n = usize::from(cube.n());
        let mut out = [0usize; 6];
        for face in Face::ALL {
            for r in 0..n {
                for c in 0..n {
                    out[cube.sticker(face, r, c).index()] += 1;
                }
            }
        }
        out
    }

    // ---- the solved state

    #[test]
    fn a_solved_cube_has_each_face_solid_in_its_own_colour() {
        for n in 2..=7u8 {
            let cube = Cube::solved(n);
            assert_eq!(cube.n(), n);
            assert!(cube.is_solved(), "a fresh {n}x{n} must be solved");
            for face in Face::ALL {
                for r in 0..usize::from(n) {
                    for c in 0..usize::from(n) {
                        assert_eq!(
                            cube.sticker(face, r, c),
                            face,
                            "{face:?} ({r},{c}) on a solved {n}x{n}"
                        );
                    }
                }
            }
        }
    }

    // ---- move orders

    #[test]
    fn every_move_applied_four_times_is_the_identity() {
        for n in 2..=7u8 {
            for (base, width) in BASES {
                if width >= n {
                    continue;
                }
                let mut cube = Cube::solved(n);
                for turn in 1..=4 {
                    cube.apply_scramble(base).expect("a legal move");
                    if turn < 4 {
                        assert!(
                            !cube.is_solved(),
                            "{base} applied {turn} times on {n}x{n} cannot already be solved"
                        );
                    }
                }
                assert!(
                    cube.is_solved(),
                    "{base} four times on {n}x{n} did not come back to solved"
                );
            }
        }
    }

    #[test]
    fn a_move_and_its_prime_cancel() {
        for n in 2..=7u8 {
            for (base, width) in BASES {
                if width >= n {
                    continue;
                }
                let cube = after(n, &format!("{base} {base}'"));
                assert!(cube.is_solved(), "{base} {base}' on {n}x{n} left the cube turned");
                let back = after(n, &format!("{base}' {base}"));
                assert!(back.is_solved(), "{base}' {base} on {n}x{n} left the cube turned");
            }
        }
    }

    #[test]
    fn a_double_move_applied_twice_is_the_identity() {
        for n in 2..=7u8 {
            for (base, width) in BASES {
                if width >= n {
                    continue;
                }
                let cube = after(n, &format!("{base}2 {base}2"));
                assert!(cube.is_solved(), "{base}2 twice on {n}x{n} did not come back");
            }
        }
    }

    #[test]
    fn a_double_move_is_the_same_as_the_move_applied_twice() {
        for n in 2..=7u8 {
            for (base, width) in BASES {
                if width >= n {
                    continue;
                }
                assert_eq!(
                    after(n, &format!("{base}2")),
                    after(n, &format!("{base} {base}")),
                    "{base}2 differs from {base} {base} on {n}x{n}"
                );
                assert_eq!(
                    after(n, &format!("{base}'")),
                    after(n, &format!("{base} {base} {base}")),
                    "{base}' differs from three {base} turns on {n}x{n}"
                );
            }
        }
    }

    // ---- colour conservation

    #[test]
    fn every_scramble_conserves_every_colour() {
        for (puzzle, n) in MODELLED {
            let want = usize::from(n) * usize::from(n);
            for seed in 0..60u64 {
                let scramble = generate_with_rng(puzzle, &mut StdRng::seed_from_u64(seed));
                let cube = after(n, &scramble);
                assert_eq!(
                    colour_counts(&cube),
                    [want; 6],
                    "{} seed {seed} lost or duplicated a sticker: {scramble:?}",
                    puzzle.name()
                );
            }
        }
    }

    // ---- direction pins on a solved 3x3

    #[test]
    fn an_r_turn_lifts_the_front_column_to_the_top() {
        let cube = after(3, "R");
        for r in 0..3 {
            assert_eq!(cube.sticker(Face::U, r, 2), Face::F, "U ({r},2) after R");
            assert_eq!(cube.sticker(Face::B, 2 - r, 0), Face::U, "B ({},0) after R", 2 - r);
            assert_eq!(cube.sticker(Face::D, r, 2), Face::B, "D ({r},2) after R");
            assert_eq!(cube.sticker(Face::F, r, 2), Face::D, "F ({r},2) after R");
        }
    }

    #[test]
    fn an_l_turn_lowers_the_top_column_to_the_front() {
        let cube = after(3, "L");
        for r in 0..3 {
            assert_eq!(cube.sticker(Face::F, r, 0), Face::U, "F ({r},0) after L");
            assert_eq!(cube.sticker(Face::D, r, 0), Face::F, "D ({r},0) after L");
            assert_eq!(cube.sticker(Face::B, 2 - r, 2), Face::D, "B ({},2) after L", 2 - r);
            assert_eq!(cube.sticker(Face::U, r, 0), Face::B, "U ({r},0) after L");
        }
    }

    #[test]
    fn a_u_turn_carries_the_right_row_onto_the_front() {
        let cube = after(3, "U");
        for c in 0..3 {
            assert_eq!(cube.sticker(Face::F, 0, c), Face::R, "F (0,{c}) after U");
            assert_eq!(cube.sticker(Face::L, 0, c), Face::F, "L (0,{c}) after U");
            assert_eq!(cube.sticker(Face::B, 0, c), Face::L, "B (0,{c}) after U");
            assert_eq!(cube.sticker(Face::R, 0, c), Face::B, "R (0,{c}) after U");
        }
    }

    #[test]
    fn a_d_turn_carries_the_front_row_onto_the_right() {
        let cube = after(3, "D");
        for c in 0..3 {
            assert_eq!(cube.sticker(Face::R, 2, c), Face::F, "R (2,{c}) after D");
            assert_eq!(cube.sticker(Face::B, 2, c), Face::R, "B (2,{c}) after D");
            assert_eq!(cube.sticker(Face::L, 2, c), Face::B, "L (2,{c}) after D");
            assert_eq!(cube.sticker(Face::F, 2, c), Face::L, "F (2,{c}) after D");
        }
    }

    #[test]
    fn an_f_turn_carries_the_bottom_row_of_the_top_onto_the_right() {
        let cube = after(3, "F");
        for i in 0..3 {
            assert_eq!(cube.sticker(Face::R, i, 0), Face::U, "R ({i},0) after F");
            assert_eq!(cube.sticker(Face::D, 0, i), Face::R, "D (0,{i}) after F");
            assert_eq!(cube.sticker(Face::L, i, 2), Face::D, "L ({i},2) after F");
            assert_eq!(cube.sticker(Face::U, 2, i), Face::L, "U (2,{i}) after F");
        }
    }

    #[test]
    fn a_b_turn_carries_the_top_row_of_the_top_onto_the_left() {
        let cube = after(3, "B");
        for i in 0..3 {
            assert_eq!(cube.sticker(Face::L, i, 0), Face::U, "L ({i},0) after B");
            assert_eq!(cube.sticker(Face::D, 2, i), Face::L, "D (2,{i}) after B");
            assert_eq!(cube.sticker(Face::R, i, 2), Face::D, "R ({i},2) after B");
            assert_eq!(cube.sticker(Face::U, 0, i), Face::R, "U (0,{i}) after B");
        }
    }

    #[test]
    fn the_turned_face_rotates_its_own_stickers_clockwise() {
        // R stains U's column 2 with the front's colour. A U turn then has to carry that
        // whole column round to U's row 2, which is what clockwise means for the face itself.
        let cube = after(3, "R U");
        for c in 0..3 {
            assert_eq!(cube.sticker(Face::U, 2, c), Face::F, "U (2,{c}) after R U");
            assert_eq!(cube.sticker(Face::U, 0, c), Face::U, "U (0,{c}) after R U");
            assert_eq!(cube.sticker(Face::U, 1, c), Face::U, "U (1,{c}) after R U");
        }
    }

    // ---- wide and depth pins

    #[test]
    fn a_wide_u_on_a_4x4_carries_two_rows_of_r_onto_f() {
        let cube = after(4, "Uw");
        for row in 0..2 {
            for c in 0..4 {
                assert_eq!(
                    cube.sticker(Face::F, row, c),
                    Face::R,
                    "F ({row},{c}) after Uw on a 4x4"
                );
            }
        }
        // The two rows below are still the front's own colour.
        for row in 2..4 {
            for c in 0..4 {
                assert_eq!(cube.sticker(Face::F, row, c), Face::F, "F ({row},{c}) must not move");
            }
        }
    }

    #[test]
    fn a_triple_wide_r_on_a_7x7_turns_three_layers_of_the_r_cycle() {
        let cube = after(7, "3Rw");
        for d in 0..3 {
            let s = 6 - d;
            for r in 0..7 {
                assert_eq!(cube.sticker(Face::U, r, s), Face::F, "U ({r},{s}) after 3Rw");
                assert_eq!(cube.sticker(Face::B, 6 - r, d), Face::U, "B ({},{d})", 6 - r);
                assert_eq!(cube.sticker(Face::D, r, s), Face::B, "D ({r},{s}) after 3Rw");
                assert_eq!(cube.sticker(Face::F, r, s), Face::D, "F ({r},{s}) after 3Rw");
            }
        }
        // Depths 3 through 6 are outside the slab and keep their own colours.
        for r in 0..7 {
            for c in 0..4 {
                assert_eq!(cube.sticker(Face::F, r, c), Face::F, "F ({r},{c}) must not move");
                assert_eq!(cube.sticker(Face::U, r, c), Face::U, "U ({r},{c}) must not move");
                assert_eq!(cube.sticker(Face::D, r, c), Face::D, "D ({r},{c}) must not move");
            }
            for c in 3..7 {
                assert_eq!(cube.sticker(Face::B, r, c), Face::B, "B ({r},{c}) must not move");
            }
        }
    }

    #[test]
    fn a_bare_turn_moves_only_the_outermost_layer() {
        let cube = after(5, "R");
        for r in 0..5 {
            assert_eq!(cube.sticker(Face::F, r, 4), Face::D, "F ({r},4) after R on a 5x5");
            for c in 0..4 {
                assert_eq!(cube.sticker(Face::F, r, c), Face::F, "F ({r},{c}) is not in the slab");
            }
        }
    }

    #[test]
    fn a_wide_turn_moves_two_layers_and_leaves_the_third() {
        let cube = after(5, "Rw");
        for r in 0..5 {
            assert_eq!(cube.sticker(Face::F, r, 4), Face::D, "F ({r},4) after Rw");
            assert_eq!(cube.sticker(Face::F, r, 3), Face::D, "F ({r},3) after Rw");
            // Depth 2 is the first layer a two-wide turn leaves alone.
            assert_eq!(cube.sticker(Face::F, r, 2), Face::F, "F ({r},2) is outside a Rw slab");
        }
    }

    // ---- sequences

    #[test]
    fn the_sexy_move_has_order_six() {
        let mut cube = Cube::solved(3);
        for round in 1..=6 {
            cube.apply_scramble("R U R' U'").expect("a legal 3x3 scramble");
            if round < 6 {
                assert!(
                    !cube.is_solved(),
                    "R U R' U' has order 6, so {round} rounds cannot solve it"
                );
            }
        }
        assert!(cube.is_solved(), "six rounds of R U R' U' must return to solved");
    }

    #[test]
    fn two_adjacent_faces_generate_the_famous_order_of_105() {
        // One R turn followed by one U turn comes back to solved after 105 repeats, and by
        // symmetry so does any pair of adjacent faces. Nothing pins the six cycles against
        // each other as hard as a number this specific: a single wrong flip misses it.
        for pair in ["R U", "U R", "F R", "R F", "L D", "D L", "B U", "F L", "D B"] {
            assert_eq!(order(3, pair), 105, "{pair:?} must have order 105");
        }
    }

    #[test]
    fn two_opposite_faces_commute_and_give_order_four() {
        for pair in ["R L", "L R", "U D", "F B"] {
            assert_eq!(order(3, pair), 4, "{pair:?} must have order 4");
        }
    }

    #[test]
    fn a_scramble_and_its_inverse_return_every_cube_to_solved() {
        for (puzzle, n) in MODELLED {
            for seed in 0..200u64 {
                let scramble = generate_with_rng(puzzle, &mut StdRng::seed_from_u64(seed));
                let mut cube = after(n, &scramble);
                assert!(
                    !cube.is_solved(),
                    "{} seed {seed} scrambled to solved: {scramble:?}",
                    puzzle.name()
                );
                cube.apply_scramble(&invert_scramble(&scramble))
                    .expect("the inverse of a generated scramble is legal");
                assert!(
                    cube.is_solved(),
                    "{} seed {seed} did not undo: {scramble:?}",
                    puzzle.name()
                );
            }
        }
    }

    // ---- inversion

    #[test]
    fn inverting_a_scramble_reverses_the_tokens_and_flips_the_suffixes() {
        assert_eq!(invert_scramble("R U R' U'"), "U R U' R'");
        assert_eq!(invert_scramble("R2 Uw' 3Fw"), "3Fw' Uw R2");
        assert_eq!(invert_scramble("F"), "F'");
        assert_eq!(invert_scramble("F'"), "F");
        assert_eq!(invert_scramble("F2"), "F2");
    }

    #[test]
    fn inverting_an_empty_scramble_gives_an_empty_string() {
        assert_eq!(invert_scramble(""), "");
        assert_eq!(invert_scramble("   "), "");
    }

    #[test]
    fn inverting_twice_gives_the_original_scramble() {
        for (puzzle, _) in MODELLED {
            for seed in 0..40u64 {
                let scramble = generate_with_rng(puzzle, &mut StdRng::seed_from_u64(seed));
                assert_eq!(
                    invert_scramble(&invert_scramble(&scramble)),
                    scramble,
                    "{} seed {seed} did not survive a double inversion",
                    puzzle.name()
                );
            }
        }
    }

    // ---- parsing and errors

    #[test]
    fn every_legal_token_parses_to_the_turn_it_names() {
        assert_eq!(
            parse_turn("R", 3),
            Ok(Turn {
                face: Face::R,
                width: 1,
                quarters: 1
            })
        );
        assert_eq!(
            parse_turn("Uw'", 4),
            Ok(Turn {
                face: Face::U,
                width: 2,
                quarters: 3
            })
        );
        assert_eq!(
            parse_turn("3Bw2", 7),
            Ok(Turn {
                face: Face::B,
                width: 3,
                quarters: 2
            })
        );
    }

    #[test]
    fn unparseable_tokens_are_rejected() {
        // No face letter, a digit where a suffix belongs, a bare `3`, a doubled `w`,
        // a lone `w`, an empty token and a token outside ASCII.
        for token in ["X", "R3", "3R", "Rww", "w", "", "RU", "R'2", "3", "3w", "r", "\u{00dc}"] {
            assert_eq!(
                parse_turn(token, 7),
                Err(MoveError::BadToken(String::from(token))),
                "{token:?} must not parse"
            );
        }
    }

    #[test]
    fn a_turn_as_wide_as_the_cube_is_too_wide() {
        assert_eq!(
            parse_turn("Rw", 2),
            Err(MoveError::TooWide(String::from("Rw")))
        );
        assert_eq!(
            parse_turn("3Rw", 3),
            Err(MoveError::TooWide(String::from("3Rw")))
        );
        // A wide turn is legal the moment the cube has a layer left over.
        assert!(parse_turn("Rw", 3).is_ok());
        assert!(parse_turn("3Rw", 4).is_ok());
        // And it travels out of `apply_scramble` unchanged.
        let mut cube = Cube::solved(2);
        assert_eq!(
            cube.apply_scramble("U Rw"),
            Err(MoveError::TooWide(String::from("Rw")))
        );
    }

    #[test]
    fn a_bad_token_stops_the_scramble_where_it_stands() {
        let mut cube = Cube::solved(3);
        assert_eq!(
            cube.apply_scramble("R X U"),
            Err(MoveError::BadToken(String::from("X")))
        );
        // The R before it landed, which is why a failed apply is not a usable cube.
        assert_eq!(cube, after(3, "R"));
    }

    #[test]
    fn an_empty_scramble_leaves_the_cube_alone() {
        let mut cube = Cube::solved(5);
        assert_eq!(cube.apply_scramble(""), Ok(()));
        assert_eq!(cube.apply_scramble("   "), Ok(()));
        assert!(cube.is_solved());
    }

    #[test]
    fn extra_whitespace_between_tokens_is_ignored() {
        assert_eq!(after(3, "R  U"), after(3, "R U"));
        assert_eq!(after(3, " R U\tU' "), after(3, "R"));
    }

    // ---- the puzzle mapping

    #[test]
    fn every_puzzle_maps_to_the_cube_size_it_previews() {
        assert_eq!(Cube::size(Puzzle::Cube2), Some(2));
        assert_eq!(Cube::size(Puzzle::Cube3), Some(3));
        assert_eq!(Cube::size(Puzzle::Cube4), Some(4));
        assert_eq!(Cube::size(Puzzle::Cube5), Some(5));
        assert_eq!(Cube::size(Puzzle::Cube6), Some(6));
        assert_eq!(Cube::size(Puzzle::Cube7), Some(7));
        assert_eq!(Cube::size(Puzzle::Oh), Some(3));
        assert_eq!(Cube::size(Puzzle::Pyraminx), None);
        assert_eq!(Cube::size(Puzzle::Skewb), None);
        assert_eq!(Cube::size(Puzzle::Megaminx), None);
        assert_eq!(Cube::size(Puzzle::Square1), None);
        assert_eq!(Cube::size(Puzzle::Clock), None);
        // Every one of the twelve events is accounted for above.
        let modelled = Puzzle::ALL.iter().filter(|p| Cube::size(**p).is_some()).count();
        assert_eq!(modelled, MODELLED.len(), "the modelled event count moved");
    }

    #[test]
    fn for_scramble_declines_the_events_without_a_model() {
        for puzzle in Puzzle::ALL {
            let scramble = generate_with_rng(puzzle, &mut StdRng::seed_from_u64(11));
            let cube = Cube::for_scramble(puzzle, &scramble);
            match Cube::size(puzzle) {
                Some(n) => {
                    let cube = cube.unwrap_or_else(|| {
                        panic!("{} did not preview {scramble:?}", puzzle.name())
                    });
                    assert_eq!(cube.n(), n, "{} previewed at the wrong size", puzzle.name());
                }
                None => assert!(
                    cube.is_none(),
                    "{} has no model and must not preview",
                    puzzle.name()
                ),
            }
        }
    }

    #[test]
    fn for_scramble_declines_a_scramble_it_cannot_parse() {
        assert!(Cube::for_scramble(Puzzle::Cube3, "R X U").is_none());
        assert!(Cube::for_scramble(Puzzle::Cube2, "Rw").is_none());
        // A Megaminx scramble is well formed for its own puzzle and meaningless here.
        assert!(Cube::for_scramble(Puzzle::Cube3, "R++ D-- U'").is_none());
    }

    #[test]
    fn every_generated_scramble_applies_cleanly() {
        for (puzzle, n) in MODELLED {
            for seed in 0..100u64 {
                let scramble = generate_with_rng(puzzle, &mut StdRng::seed_from_u64(seed));
                let mut cube = Cube::solved(n);
                assert_eq!(
                    cube.apply_scramble(&scramble),
                    Ok(()),
                    "{} seed {seed} would not apply: {scramble:?}",
                    puzzle.name()
                );
            }
        }
    }
}
