//! The cubie-level 3x3: corners and edges as permutations plus orientations.
//!
//! Kociemba's numbering, the contract in `claude-docs/plans/02-kociemba-two-phase.md`:
//! corners 0..8 are URF UFL ULB UBR DFR DLF DBL DRB, edges 0..12 are UR UF UL UB DR DF
//! DL DB FR FL BL BR with the four slice edges last so the slice coordinate is a subset
//! rank. Everything above this file works in coordinates; this is the only place the
//! puzzle itself is described.
//!
//! Orientation needs a reference on every position and on every piece. Each corner
//! position has three facelets, taken with the U or D one first and the other two
//! clockwise as seen from outside that corner, so URF reads U R F and DFR reads D F R.
//! Each edge position has two, the U or D one first for the eight U and D edges and the
//! F or B one first for the four slice edges. A piece's own reference facelet is the one
//! that sits in slot 0 when the piece is home, and its orientation is the slot that
//! facelet occupies now: clockwise thirds for a corner, flipped or not for an edge. The
//! test bridge reads those same slots off a facelet `crate::cube::Cube`.
//!
//! [`QUARTER`] holds one clockwise quarter turn per face, which is what a move's own
//! arrays mean, and each of the six follows from one physical fact: which face direction
//! the turn carries where. A turn sending position p to position q gives `cp[q] = p`, and
//! the orientation delta at q is the slot of q that p's slot 0 lands on. Tracking the
//! turned face's own facelet instead makes that arithmetic, since the turn holds its own
//! face still: the delta is q's slot for that facelet minus p's, mod 3 for a corner and
//! mod 2 for an edge. Hence U and D twist and flip nothing, their facelet being slot 0 on
//! every piece of the layer; R and L alternate one and two twists and flip nothing, their
//! facelet alternating slot 1 and slot 2 on the corners and staying slot 1 on all four
//! edges; and F and B do both, their facelet alternating on the corners and alternating
//! between slot 0 and slot 1 on the edges.

/// The facelet model, read only by the test-only bridge at the bottom of the file.
#[cfg(test)]
use crate::cube::{Cube, Face};

/// One 3x3 state at cubie level, Kociemba's numbering.
///
/// Corners 0..8 are URF UFL ULB UBR DFR DLF DBL DRB; edges 0..12 are UR UF UL UB DR DF
/// DL DB FR FL BL BR, the slice edges being 8..12. `cp[i]` is which corner sits in
/// position i, `co[i]` its clockwise twist there, and `ep`/`eo` the same for edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Cubies {
    pub cp: [u8; 8],
    pub co: [u8; 8],
    pub ep: [u8; 12],
    pub eo: [u8; 12],
}

/// The solved state: every piece home, every orientation zero.
pub(super) const SOLVED: Cubies = Cubies {
    cp: [0, 1, 2, 3, 4, 5, 6, 7],
    co: [0; 8],
    ep: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
    eo: [0; 12],
};

/// Total moves: faces U R F D L B in Kociemba order, powers 1..=3, index face*3+power-1.
pub(super) const N_MOVES: usize = 18;

/// Turnable faces, in Kociemba's order U R F D L B.
const FACES: usize = 6;

/// Quarter turns a move may carry.
const POWERS: usize = 3;

/// The WCA token per move index, face major, then plain, `2` and `'`.
const TOKENS: [&str; N_MOVES] = [
    "U", "U2", "U'", "R", "R2", "R'", "F", "F2", "F'", "D", "D2", "D'", "L", "L2", "L'", "B",
    "B2", "B'",
];

/// One clockwise quarter turn per face, in Kociemba's face order: its effect on a solved cube.
///
/// Each entry reads position by position: `cp[q]` names the position whose corner lands in
/// q and `co[q]` the twist that lands with it, `ep` and `eo` likewise. The comment on each
/// gives the layer's corner and edge cycles, read clockwise from outside the face, and the
/// deltas are the slot arithmetic from the module header.
const QUARTER: [Cubies; FACES] = [
    // U carries F to L, L to B, B to R, R to F. Corners ULB to UBR to URF to UFL to ULB,
    // edges UB to UR to UF to UL to UB. U is slot 0 on all eight pieces of the layer, so
    // nothing twists and nothing flips.
    Cubies {
        cp: [3, 0, 1, 2, 4, 5, 6, 7],
        co: [0; 8],
        ep: [3, 0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11],
        eo: [0; 12],
    },
    // R carries F to U, U to B, B to D, D to F. Corners DFR to URF to UBR to DRB to DFR,
    // edges FR to UR to BR to DR to FR. The R facelet is slot 1 on URF and DRB and slot 2
    // on UBR and DFR, so the twists alternate two and one around the cycle; it is slot 1 on
    // all four edges of the layer, so none of them flips.
    Cubies {
        cp: [4, 1, 2, 0, 7, 5, 6, 3],
        co: [2, 0, 0, 1, 1, 0, 0, 2],
        ep: [8, 1, 2, 3, 11, 5, 6, 7, 4, 9, 10, 0],
        eo: [0; 12],
    },
    // F carries U to R, R to D, D to L, L to U. Corners UFL to URF to DFR to DLF to UFL,
    // edges FL to UF to FR to DF to FL. The F facelet is slot 2 on URF and DLF and slot 1
    // on UFL and DFR, so the twists alternate one and two; on the edges it is slot 0 on FR
    // and FL and slot 1 on UF and DF, and every step of the cycle crosses between the two,
    // so all four flip.
    Cubies {
        cp: [1, 5, 2, 3, 0, 4, 6, 7],
        co: [1, 2, 0, 0, 2, 1, 0, 0],
        ep: [0, 9, 2, 3, 4, 8, 6, 7, 1, 5, 10, 11],
        eo: [0, 1, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0],
    },
    // D carries F to R, R to B, B to L, L to F. Corners DLF to DFR to DRB to DBL to DLF,
    // edges DF to DR to DB to DL to DF. D is slot 0 throughout the layer, so nothing
    // twists and nothing flips.
    Cubies {
        cp: [0, 1, 2, 3, 5, 6, 7, 4],
        co: [0; 8],
        ep: [0, 1, 2, 3, 5, 6, 7, 4, 8, 9, 10, 11],
        eo: [0; 12],
    },
    // L carries U to F, F to D, D to B, B to U. Corners ULB to UFL to DLF to DBL to ULB,
    // edges UL to FL to DL to BL to UL. The L facelet is slot 2 on UFL and DBL and slot 1
    // on ULB and DLF, so the twists alternate one and two; it is slot 1 on all four edges,
    // so none of them flips.
    Cubies {
        cp: [0, 2, 6, 3, 4, 1, 5, 7],
        co: [0, 1, 2, 0, 0, 2, 1, 0],
        ep: [0, 1, 10, 3, 4, 5, 9, 7, 8, 2, 6, 11],
        eo: [0; 12],
    },
    // B carries U to L, L to D, D to R, R to U. Corners UBR to ULB to DBL to DRB to UBR,
    // edges UB to BL to DB to BR to UB. The B facelet is slot 2 on ULB and DRB and slot 1
    // on UBR and DBL, so the twists alternate one and two; on the edges it is slot 0 on BL
    // and BR and slot 1 on UB and DB, so all four flip.
    Cubies {
        cp: [0, 1, 3, 7, 4, 5, 2, 6],
        co: [0, 0, 1, 2, 0, 0, 2, 1],
        ep: [0, 1, 2, 11, 4, 5, 6, 10, 8, 9, 3, 7],
        eo: [0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 1],
    },
];

/// The WCA token for a move index, "U" through "B'".
pub(super) fn token(mv: usize) -> &'static str {
    debug_assert!(mv < N_MOVES, "move {mv} is not one of the eighteen");
    // An index past the eighteen is a caller bug, and an empty token beats a panic in the
    // draw path the scramble ends up in.
    TOKENS.get(mv).copied().unwrap_or("")
}

/// The state after one move on `s`.
pub(super) fn apply_move(s: &Cubies, mv: usize) -> Cubies {
    debug_assert!(mv < N_MOVES, "move {mv} is not one of the eighteen");
    // The wrap keeps a bad index inside the table rather than panicking in release.
    let quarter = &QUARTER[(mv / POWERS) % FACES];
    let power = mv % POWERS + 1;
    let mut out = compose(s, quarter);
    for _ in 1..power {
        out = compose(&out, quarter);
    }
    out
}

/// Whether the corners sit in an odd permutation.
pub(super) fn corner_parity(s: &Cubies) -> bool {
    parity(&s.cp)
}

/// Whether the edges sit in an odd permutation.
///
/// Equal to [`corner_parity`] on every state a cube can reach: a quarter turn is a 4-cycle
/// on both orbits, so it flips both parities and they can never come apart.
pub(super) fn edge_parity(s: &Cubies) -> bool {
    parity(&s.ep)
}

/// The corner twists totalled mod 3, zero on every state a cube can reach.
///
/// Test scaffolding, like the bridge below: the sampler carries the eighth twist rather than
/// summing the eight, so nothing outside a test asks the question. Gated rather than allowed,
/// so a real caller appearing is a compile error and not a silent dead-code exemption.
#[cfg(test)]
pub(super) fn twist_sum(s: &Cubies) -> u8 {
    s.co.iter().sum::<u8>() % 3
}

/// The edge flips totalled mod 2, zero on every state a cube can reach. Test scaffolding,
/// for the same reason as [`twist_sum`].
#[cfg(test)]
pub(super) fn flip_sum(s: &Cubies) -> u8 {
    s.eo.iter().sum::<u8>() % 2
}

/// Move `m` applied to state `s`, the plan's composition, position by position.
fn compose(s: &Cubies, m: &Cubies) -> Cubies {
    let mut out = *s;
    for (i, (&from, &twist)) in m.cp.iter().zip(m.co.iter()).enumerate() {
        let from = usize::from(from);
        out.cp[i] = s.cp[from];
        out.co[i] = (s.co[from] + twist) % 3;
    }
    for (i, (&from, &flip)) in m.ep.iter().zip(m.eo.iter()).enumerate() {
        let from = usize::from(from);
        out.ep[i] = s.ep[from];
        out.eo[i] = (s.eo[from] + flip) % 2;
    }
    out
}

/// Whether `perm` is odd, by counting inversions.
fn parity(perm: &[u8]) -> bool {
    let mut odd = false;
    for (i, &piece) in perm.iter().enumerate() {
        for &later in &perm[i + 1..] {
            odd ^= later < piece;
        }
    }
    odd
}

// ---- the facelet bridge, test scaffolding for this module and its siblings

/// The three facelets of each corner position, the U or D one first, then clockwise.
///
/// Derived from `cube`'s net conventions rather than from anything in this module: `U`
/// has F at row 2 and R at column 2, `D` has F at row 0, each side face has U at row 0,
/// `L` and `B` have their column 2 towards F and L respectively, and `R` and `F` have
/// their column 0 towards F and L. Those are the same 24 slots `solver/cube2.rs` reads
/// off a 2x2, scaled to the middle and outer indices of a 3x3.
#[cfg(test)]
const CORNER_FACELETS: [[(Face, usize, usize); 3]; 8] = [
    [(Face::U, 2, 2), (Face::R, 0, 0), (Face::F, 0, 2)],
    [(Face::U, 2, 0), (Face::F, 0, 0), (Face::L, 0, 2)],
    [(Face::U, 0, 0), (Face::L, 0, 0), (Face::B, 0, 2)],
    [(Face::U, 0, 2), (Face::B, 0, 0), (Face::R, 0, 2)],
    [(Face::D, 0, 2), (Face::F, 2, 2), (Face::R, 2, 0)],
    [(Face::D, 0, 0), (Face::L, 2, 2), (Face::F, 2, 0)],
    [(Face::D, 2, 0), (Face::B, 2, 2), (Face::L, 2, 0)],
    [(Face::D, 2, 2), (Face::R, 2, 2), (Face::B, 2, 0)],
];

/// The two facelets of each edge position, the U or D one first for the eight U and D
/// edges and the F or B one first for the four slice edges.
#[cfg(test)]
const EDGE_FACELETS: [[(Face, usize, usize); 2]; 12] = [
    [(Face::U, 1, 2), (Face::R, 0, 1)],
    [(Face::U, 2, 1), (Face::F, 0, 1)],
    [(Face::U, 1, 0), (Face::L, 0, 1)],
    [(Face::U, 0, 1), (Face::B, 0, 1)],
    [(Face::D, 1, 2), (Face::R, 2, 1)],
    [(Face::D, 0, 1), (Face::F, 2, 1)],
    [(Face::D, 1, 0), (Face::L, 2, 1)],
    [(Face::D, 2, 1), (Face::B, 2, 1)],
    [(Face::F, 1, 2), (Face::R, 1, 0)],
    [(Face::F, 1, 0), (Face::L, 1, 2)],
    [(Face::B, 1, 2), (Face::L, 1, 0)],
    [(Face::B, 1, 0), (Face::R, 1, 2)],
];

/// The cubie state a facelet 3x3 sits in: the cross-model pin.
///
/// A corner shows three stickers and an edge two, and the colour set names the piece
/// because all eight corner sets and all twelve edge sets differ. Orientation is then
/// the slot holding the piece's own reference sticker, and the colour of that sticker is
/// whatever sits in slot 0 of the piece's home position, so the tables above carry it.
///
/// It sits outside the tests module because `search` reads it too: a solution is checked
/// against the cubie model and the scramble it emits against the facelet one, and both
/// halves of that need this one mapping.
#[cfg(test)]
pub(super) fn cubies_of(cube: &Cube) -> Cubies {
    assert_eq!(cube.n(), 3, "the cubie model only describes a 3x3");
    let mut out = SOLVED;
    for (position, slots) in CORNER_FACELETS.into_iter().enumerate() {
        let colours = slots.map(|(face, row, col)| cube.sticker(face, row, col));
        let corner = CORNER_FACELETS
            .iter()
            .position(|home| home.iter().all(|(face, _, _)| colours.contains(face)))
            .unwrap_or_else(|| panic!("no corner of a 3x3 carries {colours:?}"));
        let (reference, _, _) = CORNER_FACELETS[corner][0];
        out.cp[position] = corner as u8;
        out.co[position] = colours
            .iter()
            .position(|colour| *colour == reference)
            .unwrap_or_else(|| panic!("{colours:?} has no {reference:?} sticker")) as u8;
    }
    for (position, slots) in EDGE_FACELETS.into_iter().enumerate() {
        let colours = slots.map(|(face, row, col)| cube.sticker(face, row, col));
        let edge = EDGE_FACELETS
            .iter()
            .position(|home| home.iter().all(|(face, _, _)| colours.contains(face)))
            .unwrap_or_else(|| panic!("no edge of a 3x3 carries {colours:?}"));
        let (reference, _, _) = EDGE_FACELETS[edge][0];
        out.ep[position] = edge as u8;
        out.eo[position] = colours
            .iter()
            .position(|colour| *colour == reference)
            .unwrap_or_else(|| panic!("{colours:?} has no {reference:?} sticker")) as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::invert_scramble;
    use crate::scramble::generate_with_rng;
    use crate::types::Puzzle;
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    /// The move index a WCA token names, of the eighteen this model turns.
    fn move_of(text: &str) -> usize {
        (0..N_MOVES)
            .find(|&mv| token(mv) == text)
            .unwrap_or_else(|| panic!("{text:?} is not one of the eighteen 3x3 moves"))
    }

    /// The state `moves` leaves a solved cube in.
    fn after(moves: &[usize]) -> Cubies {
        moves.iter().fold(SOLVED, |s, &mv| apply_move(&s, mv))
    }

    /// How many times `moves` repeats before the cube comes back to solved.
    fn order(moves: &[usize]) -> usize {
        let mut state = SOLVED;
        for k in 1..=2_000 {
            state = moves.iter().fold(state, |s, &mv| apply_move(&s, mv));
            if state == SOLVED {
                return k;
            }
        }
        panic!("{moves:?} never returned to solved");
    }

    /// A random walk of `len` moves from solved, carried in both models at once.
    fn walk<R: Rng>(rng: &mut R, len: usize) -> (Cube, Cubies) {
        let mut cube = Cube::solved(3);
        let mut state = SOLVED;
        for _ in 0..len {
            let mv = rng.gen_range(0..N_MOVES);
            cube.apply_scramble(token(mv)).expect("a legal 3x3 turn");
            state = apply_move(&state, mv);
        }
        (cube, state)
    }

    /// The move undoing `mv`: the same face, the complementary power.
    fn inverse(mv: usize) -> usize {
        let face = mv / POWERS;
        let power = mv % POWERS + 1;
        face * POWERS + (POWERS + 1 - power) - 1
    }

    /// How many positions of `perm` hold a piece that is not theirs.
    fn displaced(perm: &[u8]) -> usize {
        perm.iter()
            .enumerate()
            .filter(|(i, &piece)| usize::from(piece) != *i)
            .count()
    }

    // ---- tokens

    #[test]
    fn the_eighteen_tokens_index_by_face_and_power() {
        assert_eq!(FACES * POWERS, N_MOVES);
        for (face, letter) in ["U", "R", "F", "D", "L", "B"].into_iter().enumerate() {
            for (power, suffix) in ["", "2", "'"].into_iter().enumerate() {
                let mv = face * POWERS + power;
                assert_eq!(
                    token(mv),
                    format!("{letter}{suffix}"),
                    "move {mv} is face {face} power {}",
                    power + 1
                );
            }
        }
        assert_eq!(token(0), "U");
        assert_eq!(token(4), "R2");
        assert_eq!(token(17), "B'");
    }

    #[test]
    fn every_token_turns_the_facelet_cube_and_its_inverse_puts_it_back() {
        for mv in 0..N_MOVES {
            let mut cube = Cube::solved(3);
            cube.apply_scramble(token(mv)).expect("a legal 3x3 turn");
            assert!(!cube.is_solved(), "{} left the cube solved", token(mv));
            cube.apply_scramble(&invert_scramble(token(mv)))
                .expect("the inverse of a 3x3 turn is legal");
            assert!(cube.is_solved(), "{} did not undo", token(mv));
            // The inverse move index has to name that same undoing token.
            assert_eq!(
                token(inverse(mv)),
                invert_scramble(token(mv)),
                "move {mv} and its inverse index disagree"
            );
        }
    }

    // ---- move orders and identities

    #[test]
    fn the_solved_state_sits_at_zero_in_both_models() {
        assert_eq!(cubies_of(&Cube::solved(3)), SOLVED);
        assert!(!corner_parity(&SOLVED));
        assert!(!edge_parity(&SOLVED));
        assert_eq!(twist_sum(&SOLVED), 0);
        assert_eq!(flip_sum(&SOLVED), 0);
    }

    #[test]
    fn every_quarter_turn_has_order_four() {
        for face in 0..FACES {
            for power in [1, 3] {
                let mv = face * POWERS + power - 1;
                let mut state = SOLVED;
                for turn in 1..=4 {
                    state = apply_move(&state, mv);
                    if turn < 4 {
                        assert_ne!(
                            state,
                            SOLVED,
                            "{} applied {turn} times cannot already be solved",
                            token(mv)
                        );
                    }
                }
                assert_eq!(state, SOLVED, "{} four times is not the identity", token(mv));
            }
        }
    }

    #[test]
    fn every_half_turn_is_its_own_inverse_and_is_the_quarter_turn_twice() {
        for face in 0..FACES {
            let quarter = face * POWERS;
            let half = quarter + 1;
            assert_eq!(after(&[half, half]), SOLVED, "{} twice is not the identity", token(half));
            assert_eq!(
                after(&[half]),
                after(&[quarter, quarter]),
                "{} differs from {} twice",
                token(half),
                token(quarter)
            );
            assert_eq!(
                after(&[quarter + 2]),
                after(&[quarter, quarter, quarter]),
                "{} differs from three {} turns",
                token(quarter + 2),
                token(quarter)
            );
        }
    }

    #[test]
    fn every_move_cancels_its_inverse_from_any_state() {
        let mut rng = StdRng::seed_from_u64(2);
        for _ in 0..60 {
            let len = rng.gen_range(0..14);
            let (_, state) = walk(&mut rng, len);
            for mv in 0..N_MOVES {
                let there = apply_move(&state, mv);
                assert_eq!(
                    apply_move(&there, inverse(mv)),
                    state,
                    "{} then {} moved the cube",
                    token(mv),
                    token(inverse(mv))
                );
                assert_eq!(
                    apply_move(&apply_move(&state, inverse(mv)), mv),
                    state,
                    "{} then {} moved the cube",
                    token(inverse(mv)),
                    token(mv)
                );
            }
        }
    }

    #[test]
    fn each_quarter_turn_moves_four_corners_and_four_edges_and_nothing_else() {
        for face in 0..FACES {
            let mv = face * POWERS;
            let state = after(&[mv]);
            assert_eq!(displaced(&state.cp), 4, "{} does not cycle four corners", token(mv));
            assert_eq!(displaced(&state.ep), 4, "{} does not cycle four edges", token(mv));
            // U and D hold the whole middle slice still; the other four faces each contain
            // exactly two of the four slice edges, which is what makes 8..12 a subset the
            // slice coordinate can rank.
            let slice = (8..12).filter(|&i| usize::from(state.ep[i]) != i).count();
            let want = if face % (FACES / 2) == 0 { 0 } else { 2 };
            assert_eq!(slice, want, "{} moves the wrong slice edges", token(mv));
        }
    }

    #[test]
    fn two_adjacent_faces_generate_the_famous_order_of_105() {
        // The same external fact `cube/` pins on its facelet model, and for the same
        // reason: one wrong cycle or one wrong flip in any of the six misses 105 exactly.
        for pair in [["R", "U"], ["U", "R"], ["F", "R"], ["L", "D"], ["B", "U"], ["F", "L"]] {
            let moves = pair.map(move_of);
            assert_eq!(order(&moves), 105, "{pair:?} must have order 105");
        }
    }

    #[test]
    fn two_opposite_faces_commute_and_give_order_four() {
        let mut rng = StdRng::seed_from_u64(4);
        for face in 0..FACES / 2 {
            let opposite = face + FACES / 2;
            assert_eq!(order(&[face * POWERS, opposite * POWERS]), 4);
            for _ in 0..20 {
                let len = rng.gen_range(0..14);
                let (_, state) = walk(&mut rng, len);
                for a in 0..POWERS {
                    for b in 0..POWERS {
                        let (a, b) = (face * POWERS + a, opposite * POWERS + b);
                        assert_eq!(
                            after_state(&state, &[a, b]),
                            after_state(&state, &[b, a]),
                            "{} and {} do not commute",
                            token(a),
                            token(b)
                        );
                    }
                }
            }
        }
    }

    /// The state `moves` leaves `state` in.
    fn after_state(state: &Cubies, moves: &[usize]) -> Cubies {
        moves.iter().fold(*state, |s, &mv| apply_move(&s, mv))
    }

    // ---- the invariants

    #[test]
    fn every_move_keeps_the_parities_and_the_orientation_sums() {
        let mut rng = StdRng::seed_from_u64(9);
        for round in 0..200 {
            let len = rng.gen_range(0..25);
            let (_, state) = walk(&mut rng, len);
            for mv in 0..=N_MOVES {
                // Move N_MOVES is the walk's own state, so the batch is checked as well as
                // every single move out of it.
                let state = if mv == N_MOVES { state } else { apply_move(&state, mv) };
                assert_eq!(
                    corner_parity(&state),
                    edge_parity(&state),
                    "round {round}: the parities came apart"
                );
                assert_eq!(twist_sum(&state), 0, "round {round}: the twists do not sum to 0 mod 3");
                assert_eq!(flip_sum(&state), 0, "round {round}: the flips do not sum to 0 mod 2");
            }
        }
    }

    #[test]
    fn a_quarter_turn_is_odd_on_both_orbits_and_a_half_turn_is_even() {
        // This is why the two parities can never come apart, and it is what a random state
        // has to respect when it is sampled rather than turned into place.
        for face in 0..FACES {
            for power in 1..=POWERS {
                let mv = face * POWERS + power - 1;
                let state = after(&[mv]);
                let odd = power != 2;
                assert_eq!(corner_parity(&state), odd, "{} on the corners", token(mv));
                assert_eq!(edge_parity(&state), odd, "{} on the edges", token(mv));
            }
        }
    }

    // ---- the cross-model pin

    #[test]
    fn the_facelet_model_agrees_with_every_move_from_hundreds_of_states() {
        let mut rng = StdRng::seed_from_u64(11);
        for round in 0..300 {
            // Walking the two models forward together is what makes this a check on the
            // move definitions rather than on one lucky state.
            let len = rng.gen_range(0..21);
            let (cube, state) = walk(&mut rng, len);
            assert_eq!(cubies_of(&cube), state, "round {round}: the two models drifted apart");
            for mv in 0..N_MOVES {
                let mut turned = cube.clone();
                turned.apply_scramble(token(mv)).expect("a legal 3x3 turn");
                assert_eq!(
                    cubies_of(&turned),
                    apply_move(&state, mv),
                    "round {round}: {} disagrees with the facelet cube",
                    token(mv)
                );
            }
        }
    }

    #[test]
    fn a_seeded_scramble_lands_both_models_on_the_same_state() {
        for seed in 0..100u64 {
            let scramble = generate_with_rng(Puzzle::Cube3, &mut StdRng::seed_from_u64(seed));
            let mut cube = Cube::solved(3);
            cube.apply_scramble(&scramble).expect("a 3x3 scramble on a 3x3");
            let state = scramble
                .split_whitespace()
                .fold(SOLVED, |s, text| apply_move(&s, move_of(text)));
            assert_eq!(
                cubies_of(&cube),
                state,
                "seed {seed} landed elsewhere: {scramble:?}"
            );
            assert_ne!(state, SOLVED, "seed {seed} scrambled to solved: {scramble:?}");
        }
    }
}
