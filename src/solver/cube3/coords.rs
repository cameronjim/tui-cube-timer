//! The six two-phase coordinates: encode, decode, and one move table per coordinate.
//!
//! A coordinate is a projection of the cubie state, and the whole point of the six the plan
//! names is that a move acts on each one by itself. `twist` reads only `co`, and a move
//! rewrites `co` from `co` and its own deltas; `slice` reads only which positions hold slice
//! edges, and a move permutes positions. So the effect of a move on a coordinate is a
//! function of that coordinate and the move alone, which is what makes [`MoveTables`] a
//! table rather than a cache: decode a value, turn the cubies, encode again, once per value
//! per move, and the search afterwards never touches a cubie.
//!
//! The three phase-2 coordinates hold that property only inside G1, where every piece is
//! oriented and the four slice edges are home in their slice. `eperm` ranks `ep[0..8]`, and
//! phase 2's ten moves keep those eight positions among themselves precisely because the
//! four faces they only half turn swap two slice positions with each other rather than with
//! a U or D position. A row of a phase-2 table is therefore decoded into a G1-consistent
//! state, orientations zero and the slice edges in the slice, and any other filling would
//! make the table a function of more than its own coordinate.
//!
//! `slice` ranks a set of four positions out of twelve, and the ranking convention is a
//! contract the pruning tables and the tests both lean on: it is the combinatorial number
//! system read on mirrored positions `11 - p`, so the four mirrored positions taken in
//! ascending order contribute C(m, 1), C(m, 2), C(m, 3) and C(m, 4). Mirroring is what puts
//! the solved set {8, 9, 10, 11} at 0 instead of at 494, which is what the plan requires of
//! every coordinate here. Reading the set and not the order is what makes it 495 values.
//!
//! The permutation coordinates share one factorial rank, and it depends on nothing but the
//! relative order of the values it is handed, so `sliceperm` ranks `ep[8..12]` holding 8..11
//! exactly as `cperm` ranks `cp` holding 0..7, with no offset to subtract first.

use super::cubies::{apply_move, Cubies, N_MOVES, SOLVED};
use std::sync::OnceLock;

/// Coordinate ranges, 0 solved in every one. Definitions in the plan's table.
pub(super) const N_TWIST: usize = 2187;
pub(super) const N_FLIP: usize = 2048;
pub(super) const N_SLICE: usize = 495;
pub(super) const N_CPERM: usize = 40320;
pub(super) const N_EPERM: usize = 40320;
pub(super) const N_SLICEPERM: usize = 24;

/// The ten moves that never leave G1: U U2 U' R2 F2 D D2 D' L2 B2, in cubies' indexing.
///
/// The plan puts this list in `search.rs`, and it lives here instead because the phase-2 move
/// tables are the first thing that has to know what their columns are, and a coordinate cannot
/// depend on the search that reads it. `search.rs` imports it from here.
pub(super) const PHASE2_MOVES: [usize; 10] = [0, 1, 2, 4, 7, 9, 10, 11, 13, 16];

/// Columns in a phase-1 move table: every move of the cube, in cubies' order.
pub(super) const PHASE1_COLUMNS: usize = N_MOVES;

/// Columns in a phase-2 move table: [`PHASE2_MOVES`], in that order.
pub(super) const PHASE2_COLUMNS: usize = PHASE2_MOVES.len();

/// Corners, which `twist` and `cperm` both range over.
const CORNERS: usize = 8;

/// Edges, which `flip` ranges over.
const EDGES: usize = 12;

/// The first edge of the middle slice: edges 8..12 are FR FL BL BR.
const FIRST_SLICE_EDGE: u8 = 8;

/// Edges outside the middle slice, home in positions 0..8, which is `eperm`'s range.
const UD_EDGES: usize = 8;

/// Edges in the middle slice, home in positions 8..12, which is `sliceperm`'s range.
const SLICE_EDGES: usize = 4;

/// The longest permutation ranked here, `cp` and `ep[0..8]` both being eight long.
const MAX_PERM: usize = 8;

/// The three phase-1 coordinates of a state: twist, flip, slice.
pub(super) fn phase1(s: &Cubies) -> (usize, usize, usize) {
    (twist_encode(&s.co), flip_encode(&s.eo), slice_encode(&s.ep))
}

/// The three phase-2 coordinates of a state already inside G1: cperm, eperm, sliceperm.
///
/// `cperm` is total, but the two edge ranks only mean anything in G1: outside it a slice
/// edge can sit in `ep[0..8]`, and then neither half of the split is a permutation of its
/// own positions and neither rank tracks the cube any more.
pub(super) fn phase2(s: &Cubies) -> (usize, usize, usize) {
    (
        perm_encode(&s.cp),
        perm_encode(&s.ep[..UD_EDGES]),
        perm_encode(&s.ep[UD_EDGES..]),
    )
}

/// The column [`PHASE2_MOVES`] gives `mv`, or None for one of the eight that leave G1.
pub(super) fn phase2_column(mv: usize) -> Option<usize> {
    PHASE2_MOVES.iter().position(|&listed| listed == mv)
}

/// One move table per coordinate, row-major: `table[value * columns + column]`.
///
/// Phase-1 coordinates carry [`PHASE1_COLUMNS`] columns in cubies' move order, phase-2
/// coordinates [`PHASE2_COLUMNS`] in [`PHASE2_MOVES`] order. Every cell holds the value the
/// move leaves, so a search moves by lookup and never rebuilds a cubie state. `u16` is
/// enough because the largest range here is 40320.
pub(super) struct MoveTables {
    pub twist: Vec<u16>,
    pub flip: Vec<u16>,
    pub slice: Vec<u16>,
    pub cperm: Vec<u16>,
    pub eperm: Vec<u16>,
    pub sliceperm: Vec<u16>,
}

static TABLES: OnceLock<MoveTables> = OnceLock::new();

impl MoveTables {
    /// The shared instance, built on first use.
    ///
    /// One lock covers all six, unlike `cube2`'s two, because nothing under this
    /// initializer reaches back for a table: a row is built by decoding a value, turning the
    /// cubies directly, and encoding again. The pruning tables do call this one, so they
    /// keep a lock of their own, but that lock sits above this one and never below it.
    pub fn get() -> &'static MoveTables {
        TABLES.get_or_init(|| MoveTables {
            twist: phase1_table(N_TWIST, twist_state, |s| twist_encode(&s.co)),
            flip: phase1_table(N_FLIP, flip_state, |s| flip_encode(&s.eo)),
            slice: phase1_table(N_SLICE, slice_state, |s| slice_encode(&s.ep)),
            cperm: phase2_table(N_CPERM, cperm_state, |s| perm_encode(&s.cp)),
            eperm: phase2_table(N_EPERM, eperm_state, |s| perm_encode(&s.ep[..UD_EDGES])),
            sliceperm: phase2_table(N_SLICEPERM, sliceperm_state, |s| {
                perm_encode(&s.ep[UD_EDGES..])
            }),
        })
    }
}

/// One phase-1 table: all eighteen moves over every value of the coordinate.
fn phase1_table(
    states: usize,
    decode: fn(usize) -> Cubies,
    encode: fn(&Cubies) -> usize,
) -> Vec<u16> {
    let mut table = vec![0u16; states * PHASE1_COLUMNS];
    for value in 0..states {
        let state = decode(value);
        for mv in 0..PHASE1_COLUMNS {
            table[value * PHASE1_COLUMNS + mv] = encode(&apply_move(&state, mv)) as u16;
        }
    }
    table
}

/// One phase-2 table: [`PHASE2_MOVES`] over every value of the coordinate.
///
/// `decode` has to return a G1-consistent state, which is what keeps the ten columns a
/// function of this coordinate alone.
fn phase2_table(
    states: usize,
    decode: fn(usize) -> Cubies,
    encode: fn(&Cubies) -> usize,
) -> Vec<u16> {
    let mut table = vec![0u16; states * PHASE2_COLUMNS];
    for value in 0..states {
        let state = decode(value);
        for (column, &mv) in PHASE2_MOVES.iter().enumerate() {
            table[value * PHASE2_COLUMNS + column] = encode(&apply_move(&state, mv)) as u16;
        }
    }
    table
}

/// A state carrying nothing but `twist`.
fn twist_state(value: usize) -> Cubies {
    Cubies { co: twist_decode(value), ..SOLVED }
}

/// A state carrying nothing but `flip`.
fn flip_state(value: usize) -> Cubies {
    Cubies { eo: flip_decode(value), ..SOLVED }
}

/// A state carrying nothing but `slice`.
fn slice_state(value: usize) -> Cubies {
    Cubies { ep: slice_decode(value), ..SOLVED }
}

/// A G1 state carrying nothing but `cperm`.
fn cperm_state(value: usize) -> Cubies {
    let mut out = SOLVED;
    perm_decode(value, &mut out.cp);
    out
}

/// A G1 state carrying nothing but `eperm`: the four slice edges stay home in their slice.
fn eperm_state(value: usize) -> Cubies {
    let mut out = SOLVED;
    perm_decode(value, &mut out.ep[..UD_EDGES]);
    out
}

/// A G1 state carrying nothing but `sliceperm`: the four slice edges permuted among themselves.
fn sliceperm_state(value: usize) -> Cubies {
    let mut out = SOLVED;
    perm_decode(value, &mut out.ep[UD_EDGES..]);
    for edge in &mut out.ep[UD_EDGES..] {
        *edge += FIRST_SLICE_EDGE;
    }
    out
}

/// The base-3 rank of the first seven twists, zero when nothing is twisted.
fn twist_encode(co: &[u8; CORNERS]) -> usize {
    co[..CORNERS - 1].iter().fold(0, |rank, &twist| rank * 3 + usize::from(twist))
}

/// The twist at each corner position, the eighth carried by the mod-3 sum.
fn twist_decode(mut rank: usize) -> [u8; CORNERS] {
    let mut co = [0u8; CORNERS];
    let mut total = 0u8;
    for twist in co[..CORNERS - 1].iter_mut().rev() {
        *twist = (rank % 3) as u8;
        total += *twist;
        rank /= 3;
    }
    // The eighth twist is whatever brings the total back to a multiple of three.
    co[CORNERS - 1] = (3 - total % 3) % 3;
    co
}

/// The base-2 rank of the first eleven flips, zero when nothing is flipped.
fn flip_encode(eo: &[u8; EDGES]) -> usize {
    eo[..EDGES - 1].iter().fold(0, |rank, &flip| rank * 2 + usize::from(flip))
}

/// The flip at each edge position, the twelfth carried by the mod-2 sum.
fn flip_decode(mut rank: usize) -> [u8; EDGES] {
    let mut eo = [0u8; EDGES];
    let mut total = 0u8;
    for flip in eo[..EDGES - 1].iter_mut().rev() {
        *flip = (rank % 2) as u8;
        total += *flip;
        rank /= 2;
    }
    // The twelfth flip is whatever makes the count even.
    eo[EDGES - 1] = total % 2;
    eo
}

/// The C(12,4) rank of the positions holding slice edges, `{8, 9, 10, 11}` ranked 0.
///
/// The mirrored-position convention is in the module header; the loop reads positions from
/// the back so the mirrored value climbs from 0, and `k` is which of the four terms this
/// position contributes.
fn slice_encode(ep: &[u8; EDGES]) -> usize {
    let mut rank = 0;
    let mut k = 1;
    for (position, &edge) in ep.iter().enumerate().rev() {
        if edge >= FIRST_SLICE_EDGE {
            rank += choose(EDGES - 1 - position, k);
            k += 1;
        }
    }
    rank
}

/// An edge permutation whose slice edges sit where `slice` says they do.
///
/// Which slice edge goes where is not the coordinate's business, so they go in ascending
/// order and the other eight fill the gaps the same way. Unranking is the greedy walk the
/// combinatorial number system allows: the largest mirrored position still affordable is in
/// the set, take it and move on to the next term.
fn slice_decode(mut rank: usize) -> [u8; EDGES] {
    let mut inside = [false; EDGES];
    let mut k = SLICE_EDGES;
    for (position, slot) in inside.iter_mut().enumerate() {
        if k == 0 {
            break;
        }
        let term = choose(EDGES - 1 - position, k);
        if rank >= term {
            rank -= term;
            *slot = true;
            k -= 1;
        }
    }
    let mut ep = [0u8; EDGES];
    let mut slice = FIRST_SLICE_EDGE;
    let mut other = 0u8;
    for (edge, &in_slice) in ep.iter_mut().zip(inside.iter()) {
        if in_slice {
            *edge = slice;
            slice += 1;
        } else {
            *edge = other;
            other += 1;
        }
    }
    ep
}

/// The factorial rank of `perm`, zero when it is in ascending order.
///
/// Only relative order matters, so `ep[8..12]` holding 8..11 ranks like any other four.
fn perm_encode(perm: &[u8]) -> usize {
    let n = perm.len();
    let mut rank = 0;
    for (i, &piece) in perm.iter().enumerate() {
        let smaller = perm[i + 1..].iter().filter(|&&later| later < piece).count();
        rank = rank * (n - i) + smaller;
    }
    rank
}

/// The permutation of `0..perm.len()` at factorial rank `rank`, written into `perm`.
fn perm_decode(mut rank: usize, perm: &mut [u8]) {
    debug_assert!(perm.len() <= MAX_PERM, "no coordinate here ranks more than eight pieces");
    // A longer slice would run off the digit buffer, so it gets its first eight filled in
    // rather than a panic on the way to a scramble.
    let n = perm.len().min(MAX_PERM);
    let mut digits = [0usize; MAX_PERM];
    for (i, digit) in digits[..n].iter_mut().enumerate().rev() {
        let radix = n - i;
        *digit = rank % radix;
        rank /= radix;
    }
    // Each digit picks from the values not yet placed, held sorted so the rank is canonical.
    let mut pool = [0u8; MAX_PERM];
    for (value, slot) in pool.iter_mut().enumerate() {
        *slot = value as u8;
    }
    let mut left = n;
    for (i, slot) in perm.iter_mut().enumerate().take(n) {
        let digit = digits[i];
        *slot = pool[digit];
        pool.copy_within(digit + 1..left, digit);
        left -= 1;
    }
}

/// C(n, k), zero when k is the larger. Exact at every step, so no rounding creeps in.
fn choose(n: usize, k: usize) -> usize {
    if k > n {
        return 0;
    }
    let mut out = 1;
    for i in 0..k {
        out = out * (n - i) / (i + 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::cube3::cubies::token;
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    /// The move index of R, the quarter turn every slice assertion below leans on.
    const R_TURN: usize = 3;

    /// Every edge in order, what a sorted `ep` has to come back as.
    const ALL_EDGES: [u8; EDGES] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

    /// The ten tokens [`PHASE2_MOVES`] has to name, in that order.
    const PHASE2_TOKENS: [&str; PHASE2_COLUMNS] =
        ["U", "U2", "U'", "R2", "F2", "D", "D2", "D'", "L2", "B2"];

    /// A random walk of up to `cap` moves from solved, drawn from `moves`.
    fn walk<R: Rng>(rng: &mut R, cap: usize, moves: &[usize]) -> Cubies {
        let len = rng.gen_range(0..cap);
        (0..len).fold(SOLVED, |state, _| {
            let mv = moves[rng.gen_range(0..moves.len())];
            apply_move(&state, mv)
        })
    }

    /// Every move of the cube, the columns a phase-1 table carries.
    fn all_moves() -> Vec<usize> {
        (0..N_MOVES).collect()
    }

    /// The positions of `ep` holding a slice edge, which is all `slice` reads.
    fn slice_set(ep: &[u8; EDGES]) -> Vec<usize> {
        ep.iter()
            .enumerate()
            .filter(|(_, &edge)| edge >= FIRST_SLICE_EDGE)
            .map(|(position, _)| position)
            .collect()
    }

    /// An edge permutation with slice edges at `positions` and the rest filled in order.
    fn ep_with_slice_at(positions: &[usize]) -> [u8; EDGES] {
        let mut ep = [0u8; EDGES];
        let mut slice = FIRST_SLICE_EDGE;
        let mut other = 0u8;
        for (position, edge) in ep.iter_mut().enumerate() {
            if positions.contains(&position) {
                *edge = slice;
                slice += 1;
            } else {
                *edge = other;
                other += 1;
            }
        }
        ep
    }

    // ---- the ranges

    #[test]
    fn the_six_ranges_are_the_sizes_the_plan_names() {
        assert_eq!(N_TWIST, 3usize.pow(7));
        assert_eq!(N_FLIP, 2usize.pow(11));
        assert_eq!(N_SLICE, choose(12, 4));
        assert_eq!(N_CPERM, (1..=8).product::<usize>());
        assert_eq!(N_EPERM, (1..=8).product::<usize>());
        assert_eq!(N_SLICEPERM, (1..=4).product::<usize>());
    }

    #[test]
    fn the_solved_state_sits_at_zero_in_both_triples() {
        assert_eq!(phase1(&SOLVED), (0, 0, 0));
        assert_eq!(phase2(&SOLVED), (0, 0, 0));
    }

    // ---- encode and decode, exhaustively

    #[test]
    fn every_twist_and_every_flip_survives_a_decode_and_an_encode() {
        for rank in 0..N_TWIST {
            let co = twist_decode(rank);
            assert!(co.iter().all(|&twist| twist < 3), "twist {rank} twisted past two");
            assert_eq!(co.iter().sum::<u8>() % 3, 0, "twist {rank} is unbalanced");
            assert_eq!(twist_encode(&co), rank);
        }
        for rank in 0..N_FLIP {
            let eo = flip_decode(rank);
            assert!(eo.iter().all(|&flip| flip < 2), "flip {rank} flipped past one");
            assert_eq!(eo.iter().sum::<u8>() % 2, 0, "flip {rank} is unbalanced");
            assert_eq!(flip_encode(&eo), rank);
        }
    }

    #[test]
    fn every_slice_value_survives_a_decode_and_an_encode() {
        let mut seen = Vec::with_capacity(N_SLICE);
        for rank in 0..N_SLICE {
            let ep = slice_decode(rank);
            let mut placed = ep;
            placed.sort_unstable();
            assert_eq!(placed, ALL_EDGES, "slice {rank} lost an edge");
            let set = slice_set(&ep);
            assert_eq!(set.len(), SLICE_EDGES, "slice {rank} holds the wrong count");
            assert_eq!(slice_encode(&ep), rank);
            seen.push(set);
        }
        // Distinct sets for distinct ranks is the other half of the bijection.
        let mut sorted = seen;
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), N_SLICE, "two ranks name the same set of positions");
    }

    #[test]
    fn every_permutation_value_survives_a_decode_and_an_encode() {
        for rank in 0..N_CPERM {
            let mut cp = [0u8; CORNERS];
            perm_decode(rank, &mut cp);
            let mut placed = cp;
            placed.sort_unstable();
            assert_eq!(placed, [0, 1, 2, 3, 4, 5, 6, 7], "cperm {rank} is not a permutation");
            assert_eq!(perm_encode(&cp), rank);
        }
        // eperm ranks eight of twelve edges and shares the arithmetic, so the same sweep
        // covers it through the state the table decodes rather than through the bare rank.
        for rank in 0..N_EPERM {
            let state = eperm_state(rank);
            assert_eq!(state.ep[UD_EDGES..], [8, 9, 10, 11], "eperm {rank} moved the slice");
            assert_eq!(perm_encode(&state.ep[..UD_EDGES]), rank);
        }
        for rank in 0..N_SLICEPERM {
            let state = sliceperm_state(rank);
            let mut placed = state.ep;
            placed.sort_unstable();
            assert_eq!(placed, ALL_EDGES, "sliceperm {rank} lost an edge");
            assert_eq!(perm_encode(&state.ep[UD_EDGES..]), rank);
        }
    }

    // ---- the slice ranking convention

    #[test]
    fn the_slice_rank_counts_up_from_the_solved_slice() {
        // The convention the pruning tables inherit, pinned by hand: mirrored positions in
        // the combinatorial number system, so the solved set is 0, the set that pulls one
        // slice edge to the nearest U or D position is 1, and the four U-layer positions are
        // last. Read unmirrored, the solved set would rank 494 and the plan's "0 solved"
        // would be broken for this coordinate alone.
        assert_eq!(slice_encode(&ep_with_slice_at(&[8, 9, 10, 11])), 0);
        assert_eq!(slice_encode(&ep_with_slice_at(&[7, 9, 10, 11])), 1);
        assert_eq!(slice_encode(&ep_with_slice_at(&[7, 8, 10, 11])), 2);
        assert_eq!(slice_encode(&ep_with_slice_at(&[0, 1, 2, 3])), N_SLICE - 1);
        assert_eq!(slice_set(&slice_decode(0)), vec![8, 9, 10, 11]);
        assert_eq!(slice_set(&slice_decode(1)), vec![7, 9, 10, 11]);
        assert_eq!(slice_set(&slice_decode(N_SLICE - 1)), vec![0, 1, 2, 3]);
    }

    #[test]
    fn the_slice_coordinate_reads_the_set_and_not_the_order() {
        // Two states differing only in which slice edge sits where must share a rank, which
        // is the difference between 495 values and the 11880 an ordered rank would need.
        let moves = all_moves();
        let mut rng = StdRng::seed_from_u64(21);
        for _ in 0..200 {
            let state = walk(&mut rng, 21, &moves);
            let set = slice_set(&state.ep);
            let mut shuffled = state;
            shuffled.ep.swap(set[0], set[SLICE_EDGES - 1]);
            assert_eq!(slice_encode(&shuffled.ep), slice_encode(&state.ep));
        }
    }

    // ---- the move tables against the cubies

    #[test]
    fn the_phase_one_tables_agree_with_the_cubies_on_every_move() {
        let tables = MoveTables::get();
        let moves = all_moves();
        let mut rng = StdRng::seed_from_u64(31);
        for round in 0..2_000 {
            let state = walk(&mut rng, 25, &moves);
            let (twist, flip, slice) = phase1(&state);
            for mv in 0..PHASE1_COLUMNS {
                let want = phase1(&apply_move(&state, mv));
                let got = (
                    usize::from(tables.twist[twist * PHASE1_COLUMNS + mv]),
                    usize::from(tables.flip[flip * PHASE1_COLUMNS + mv]),
                    usize::from(tables.slice[slice * PHASE1_COLUMNS + mv]),
                );
                assert_eq!(got, want, "round {round}: {} disagrees with the cubies", token(mv));
            }
        }
    }

    #[test]
    fn the_phase_two_tables_agree_with_the_cubies_on_every_move_of_g1() {
        let tables = MoveTables::get();
        let mut rng = StdRng::seed_from_u64(37);
        for round in 0..2_000 {
            // Only phase-2 moves, because outside G1 the two edge ranks describe nothing.
            let state = walk(&mut rng, 25, &PHASE2_MOVES);
            let (cperm, eperm, sliceperm) = phase2(&state);
            for (column, &mv) in PHASE2_MOVES.iter().enumerate() {
                let want = phase2(&apply_move(&state, mv));
                let got = (
                    usize::from(tables.cperm[cperm * PHASE2_COLUMNS + column]),
                    usize::from(tables.eperm[eperm * PHASE2_COLUMNS + column]),
                    usize::from(tables.sliceperm[sliceperm * PHASE2_COLUMNS + column]),
                );
                assert_eq!(got, want, "round {round}: {} disagrees with the cubies", token(mv));
            }
        }
    }

    #[test]
    fn every_table_is_the_shape_its_coordinate_asks_for_and_stays_in_range() {
        let tables = MoveTables::get();
        for (name, table, states, columns) in [
            ("twist", &tables.twist, N_TWIST, PHASE1_COLUMNS),
            ("flip", &tables.flip, N_FLIP, PHASE1_COLUMNS),
            ("slice", &tables.slice, N_SLICE, PHASE1_COLUMNS),
            ("cperm", &tables.cperm, N_CPERM, PHASE2_COLUMNS),
            ("eperm", &tables.eperm, N_EPERM, PHASE2_COLUMNS),
            ("sliceperm", &tables.sliceperm, N_SLICEPERM, PHASE2_COLUMNS),
        ] {
            assert_eq!(table.len(), states * columns, "the {name} table is the wrong size");
            let worst = table.iter().copied().max().unwrap_or(0);
            assert!(
                usize::from(worst) < states,
                "the {name} table holds {worst}, outside its {states} values"
            );
            // Every column is a permutation of the coordinate's values, because a move is
            // invertible and the coordinate transition inherits that. A degenerate column,
            // one that collapsed two values together, would still pass a round trip.
            for column in 0..columns {
                let mut reached: Vec<u16> =
                    (0..states).map(|value| table[value * columns + column]).collect();
                reached.sort_unstable();
                reached.dedup();
                assert_eq!(
                    reached.len(),
                    states,
                    "column {column} of the {name} table is not a permutation of its values"
                );
            }
        }
    }

    // ---- what G1 means

    #[test]
    fn a_walk_of_phase_two_moves_never_leaves_g1() {
        assert_eq!(PHASE2_MOVES.map(token), PHASE2_TOKENS);
        let tables = MoveTables::get();
        let mut rng = StdRng::seed_from_u64(43);
        for round in 0..500 {
            let mut state = SOLVED;
            for step in 0..30 {
                let mv = PHASE2_MOVES[rng.gen_range(0..PHASE2_COLUMNS)];
                state = apply_move(&state, mv);
                assert_eq!(
                    phase1(&state),
                    (0, 0, 0),
                    "round {round} step {step}: {} left G1",
                    token(mv)
                );
            }
        }
        // The tables say the same thing, which is what the phase-2 search relies on: from
        // the solved value of each phase-1 coordinate no phase-2 column moves.
        for &mv in &PHASE2_MOVES {
            assert_eq!(tables.twist[mv], 0, "{} twists a solved cube", token(mv));
            assert_eq!(tables.flip[mv], 0, "{} flips a solved cube", token(mv));
            assert_eq!(tables.slice[mv], 0, "{} moves the slice", token(mv));
        }
    }

    #[test]
    fn a_quarter_turn_of_r_moves_the_slice_and_the_table_is_not_degenerate() {
        let tables = MoveTables::get();
        let (twist, flip, slice) = phase1(&apply_move(&SOLVED, R_TURN));
        assert_eq!(token(R_TURN), "R");
        assert_ne!(slice, 0, "R has to carry two slice edges out of the slice");
        assert_ne!(twist, 0, "R has to twist the corners it turns");
        assert_eq!(flip, 0, "R flips no edge");
        // Row 0 of each table starts at index 0, so the column is the whole offset.
        assert_eq!(usize::from(tables.slice[R_TURN]), slice);
        assert_eq!(usize::from(tables.twist[R_TURN]), twist);
        let moved = (0..N_SLICE)
            .filter(|&value| usize::from(tables.slice[value * PHASE1_COLUMNS + R_TURN]) != value)
            .count();
        // R permutes positions 0, 4, 8 and 11 in one cycle, so it fixes exactly the 70 sets
        // avoiding all four and the one set that is all four.
        assert_eq!(moved, N_SLICE - 71, "R fixes the wrong number of slice sets");
    }

    #[test]
    fn the_phase_two_move_list_maps_back_to_its_columns() {
        for (column, &mv) in PHASE2_MOVES.iter().enumerate() {
            assert_eq!(phase2_column(mv), Some(column));
        }
        for mv in (0..N_MOVES).filter(|mv| !PHASE2_MOVES.contains(mv)) {
            assert_eq!(phase2_column(mv), None, "{} is not a phase-2 move", token(mv));
        }
    }
}
