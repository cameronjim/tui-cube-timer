//! The four pruning tables: exact distances in product-coordinate projections.
//!
//! An IDA* search needs a lower bound on the moves remaining, and the bound has to be
//! admissible, never larger than the truth, or the search prunes the answer away. The bound
//! here is a distance in a projection of the cube: forget everything except two coordinates,
//! and the distance to solved in that smaller world cannot exceed the distance in the real
//! one, because a real solution projects to a walk of its own length. Exact distances are
//! the strongest bound a projection can give, and a breadth-first sweep over the whole
//! product is how they are computed, [`Engine::distances`] doing the sweep exactly as it does
//! for the puzzles small enough to table whole.
//!
//! Which pairs, and why pairs at all: phase 1 has to zero twist, flip and slice together, and
//! pairing each orientation coordinate with slice is what lets the bound say something the
//! coordinates cannot say apart, that the orientations and the slice edges have to come home
//! in the same moves. Phase 2 pairs each permutation coordinate with sliceperm for the same
//! reason. Pairing all three would be the ideal bound and 2.2 billion cells, so the max of
//! two overlapping pairs is the compromise every descendant of Kociemba's solver makes: the
//! max of two admissible bounds is admissible, being a bound both of them respect.
//!
//! Layout is row major on the first coordinate, `twist * N_SLICE + slice` and its three
//! siblings, so index 0 is solved in both halves and the tables inherit "0 solved" from the
//! coordinates rather than restating it.
//!
//! The engine's move grid is axes times powers, which is what phase 1 already is: six faces
//! of three powers, column `face * 3 + power - 1`, cubies' own numbering. Phase 2's ten moves
//! do not factor that way, U carrying three powers and R2 one, so they go in as ten axes of a
//! single power each and the axis is the column of `coords::PHASE2_MOVES`.
//!
//! Every cell of all four tables is reachable, which is worth stating because the phase-2
//! pair looks like it should not be. A state inside G1 must have `cperm`, `eperm` and
//! `sliceperm` parities summing to even, since a quarter turn of U or D is odd on the corners
//! and odd on the eight U and D edges while a half turn of R, F, L or B is odd on those eight
//! and odd on the slice, so no phase-2 move can break the sum. Each of the two tables drops
//! one of the three, and the dropped one is free to absorb the parity, which leaves both
//! products whole. The one-move witness is U: it makes `cperm` odd while `sliceperm` stays
//! even, and the state is legal because `eperm` went odd with it.

use super::super::Engine;
use super::coords::{
    MoveTables, N_CPERM, N_EPERM, N_FLIP, N_SLICE, N_SLICEPERM, N_TWIST, PHASE1_COLUMNS,
    PHASE2_COLUMNS,
};
use std::sync::OnceLock;

/// Turnable faces, the axes of a phase-1 table.
const FACES: usize = 6;

/// Quarter turns per face, the powers of a phase-1 table.
const POWERS: usize = 3;

/// Powers per axis in a phase-2 table: the ten moves are listed flat, one per axis.
const PHASE2_POWERS: usize = 1;

/// The tables, each a `Vec<u8>` of exact distances over a product coordinate.
///
/// Phase 1 pairs twist and flip each with slice under all eighteen moves, phase 2 pairs
/// cperm and eperm each with sliceperm under the ten of G1. Four megabytes together, so
/// they are built on the first 3x3 scramble and never in a draw loop.
pub(super) struct Prune {
    twist_slice: Vec<u8>,
    flip_slice: Vec<u8>,
    cperm_sliceperm: Vec<u8>,
    eperm_sliceperm: Vec<u8>,
}

static PRUNE: OnceLock<Prune> = OnceLock::new();

impl Prune {
    /// The shared instance, built on first use, separate lock from the move tables.
    ///
    /// The sweep below reads [`MoveTables::get`], so this lock sits above that one and never
    /// below it: one lock holding both would deadlock initializing itself.
    pub fn get() -> &'static Prune {
        PRUNE.get_or_init(|| Prune {
            twist_slice: phase1_engine(N_TWIST * N_SLICE, twist_slice_move).distances(),
            flip_slice: phase1_engine(N_FLIP * N_SLICE, flip_slice_move).distances(),
            cperm_sliceperm: phase2_engine(N_CPERM * N_SLICEPERM, cperm_sliceperm_move)
                .distances(),
            eperm_sliceperm: phase2_engine(N_EPERM * N_SLICEPERM, eperm_sliceperm_move)
                .distances(),
        })
    }

    /// A lower bound on the phase-1 moves left, the max of the two phase-1 tables.
    ///
    /// Zero only on a cube already in G1, distance zero sitting at index 0 alone.
    pub fn bound1(&self, twist: usize, flip: usize, slice: usize) -> usize {
        let by_twist = self.twist_slice[twist * N_SLICE + slice];
        let by_flip = self.flip_slice[flip * N_SLICE + slice];
        usize::from(by_twist.max(by_flip))
    }

    /// A lower bound on the phase-2 moves left, the max of the two phase-2 tables.
    ///
    /// Meaningful only on a cube inside G1, which is the only place its coordinates are.
    pub fn bound2(&self, cperm: usize, eperm: usize, sliceperm: usize) -> usize {
        let by_corners = self.cperm_sliceperm[cperm * N_SLICEPERM + sliceperm];
        let by_edges = self.eperm_sliceperm[eperm * N_SLICEPERM + sliceperm];
        usize::from(by_corners.max(by_edges))
    }
}

/// The engine over a phase-1 product: six faces, three powers, eighteen columns.
fn phase1_engine(states: usize, apply: fn(usize, usize, usize) -> usize) -> Engine {
    Engine { states, axes: FACES, powers: POWERS, apply }
}

/// The engine over a phase-2 product: ten axes of one power, one per phase-2 column.
fn phase2_engine(states: usize, apply: fn(usize, usize, usize) -> usize) -> Engine {
    Engine { states, axes: PHASE2_COLUMNS, powers: PHASE2_POWERS, apply }
}

/// The phase-1 column an (axis, power) pair names, which is cubies' own move index.
fn phase1_column(axis: usize, power: usize) -> usize {
    axis * POWERS + power - 1
}

/// The phase-2 column an (axis, power) pair names, the axis itself.
fn phase2_column(axis: usize, power: usize) -> usize {
    debug_assert_eq!(power, PHASE2_POWERS, "a phase-2 axis carries one power");
    axis
}

/// One move on a product index, `high` and `low` being the two halves' move tables.
///
/// `stride` is the low half's range, which is both the divisor that splits the index and the
/// multiplier that puts it back together, and `columns` is the width of the two tables.
fn product_move(
    state: usize,
    high: &[u16],
    low: &[u16],
    stride: usize,
    columns: usize,
    column: usize,
) -> usize {
    let turned_high = usize::from(high[state / stride * columns + column]);
    let turned_low = usize::from(low[state % stride * columns + column]);
    turned_high * stride + turned_low
}

/// One move on a twist-slice index.
fn twist_slice_move(state: usize, axis: usize, power: usize) -> usize {
    let t = MoveTables::get();
    let column = phase1_column(axis, power);
    product_move(state, &t.twist, &t.slice, N_SLICE, PHASE1_COLUMNS, column)
}

/// One move on a flip-slice index.
fn flip_slice_move(state: usize, axis: usize, power: usize) -> usize {
    let t = MoveTables::get();
    let column = phase1_column(axis, power);
    product_move(state, &t.flip, &t.slice, N_SLICE, PHASE1_COLUMNS, column)
}

/// One phase-2 move on a cperm-sliceperm index.
fn cperm_sliceperm_move(state: usize, axis: usize, power: usize) -> usize {
    let t = MoveTables::get();
    let column = phase2_column(axis, power);
    product_move(state, &t.cperm, &t.sliceperm, N_SLICEPERM, PHASE2_COLUMNS, column)
}

/// One phase-2 move on an eperm-sliceperm index.
fn eperm_sliceperm_move(state: usize, axis: usize, power: usize) -> usize {
    let t = MoveTables::get();
    let column = phase2_column(axis, power);
    product_move(state, &t.eperm, &t.sliceperm, N_SLICEPERM, PHASE2_COLUMNS, column)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::cube3::coords::PHASE2_MOVES;
    use crate::solver::cube3::search::{MAX_PHASE1, MAX_PHASE2};
    use crate::solver::UNREACHABLE;
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    /// Product states drawn per table for the admissibility sweep.
    const SAMPLES: usize = 4_000;

    /// One table with everything a test needs to say about it: name, cells, the moves it was
    /// swept under, the function that swept it and the depth its phase can never exceed.
    struct Table {
        name: &'static str,
        dist: &'static [u8],
        moves: Vec<(usize, usize)>,
        apply: fn(usize, usize, usize) -> usize,
        cap: usize,
    }

    /// The four tables, each paired with the (axis, power) grid its sweep used.
    fn tables() -> Vec<Table> {
        let prune = Prune::get();
        let phase1: Vec<(usize, usize)> =
            (0..FACES).flat_map(|axis| (1..=POWERS).map(move |power| (axis, power))).collect();
        let phase2: Vec<(usize, usize)> =
            (0..PHASE2_COLUMNS).map(|axis| (axis, PHASE2_POWERS)).collect();
        vec![
            Table {
                name: "twist x slice",
                dist: &prune.twist_slice,
                moves: phase1.clone(),
                apply: twist_slice_move,
                cap: MAX_PHASE1,
            },
            Table {
                name: "flip x slice",
                dist: &prune.flip_slice,
                moves: phase1,
                apply: flip_slice_move,
                cap: MAX_PHASE1,
            },
            Table {
                name: "cperm x sliceperm",
                dist: &prune.cperm_sliceperm,
                moves: phase2.clone(),
                apply: cperm_sliceperm_move,
                cap: MAX_PHASE2,
            },
            Table {
                name: "eperm x sliceperm",
                dist: &prune.eperm_sliceperm,
                moves: phase2,
                apply: eperm_sliceperm_move,
                cap: MAX_PHASE2,
            },
        ]
    }

    #[test]
    fn every_table_is_the_size_its_product_asks_for() {
        let prune = Prune::get();
        assert_eq!(prune.twist_slice.len(), 1_082_565);
        assert_eq!(prune.flip_slice.len(), 1_013_760);
        assert_eq!(prune.cperm_sliceperm.len(), 967_680);
        assert_eq!(prune.eperm_sliceperm.len(), 967_680);
        // The plan's four sizes, spelled again as the products they are.
        assert_eq!(prune.twist_slice.len(), N_TWIST * N_SLICE);
        assert_eq!(prune.flip_slice.len(), N_FLIP * N_SLICE);
        assert_eq!(prune.cperm_sliceperm.len(), N_CPERM * N_SLICEPERM);
        assert_eq!(prune.eperm_sliceperm.len(), N_EPERM * N_SLICEPERM);
    }

    #[test]
    fn distance_zero_sits_at_index_zero_and_nowhere_else() {
        // The searches lean on this: a bound of zero is what tells phase 1 the cube reached
        // G1 and phase 2 that it is solved, without decoding anything.
        for table in tables() {
            assert_eq!(table.dist[0], 0, "the {} table does not start solved", table.name);
            let strays = table.dist.iter().skip(1).filter(|&&d| d == 0).count();
            assert_eq!(strays, 0, "the {} table holds {strays} more zeroes", table.name);
        }
    }

    #[test]
    fn every_cell_of_every_table_was_reached_within_its_phase() {
        // Full reachability, including both phase-2 products: the module header derives why
        // the parity constraint that couples the three phase-2 coordinates cannot make a hole
        // in a product of two of them.
        for table in tables() {
            let missing = table.dist.iter().filter(|&&d| d == UNREACHABLE).count();
            assert_eq!(missing, 0, "the {} table left {missing} cells unswept", table.name);
            let worst = table.dist.iter().copied().max().unwrap_or(0);
            assert!(
                usize::from(worst) <= table.cap,
                "the {} table reaches {worst}, past its phase's {} moves",
                table.name,
                table.cap
            );
        }
    }

    #[test]
    fn the_deepest_cell_of_each_table_is_the_depth_it_has_always_been() {
        // Facts about the cube, so they are pinned rather than bounded: a wrong stride or a
        // wrong column in the sweep moves these before it breaks anything else. They also say
        // how much each projection gives up. Phase 1 needs 12 moves in the worst case and
        // neither of its projections sees past 9, which is why the phase-1 search still has
        // real work to do; phase 2's projections reach 14 of its 18, which is why a junction
        // that cannot fit is usually rejected on its first lookup.
        let prune = Prune::get();
        let deepest = |dist: &[u8]| usize::from(dist.iter().copied().max().unwrap_or(0));
        assert_eq!(deepest(&prune.twist_slice), 9);
        assert_eq!(deepest(&prune.flip_slice), 9);
        assert_eq!(deepest(&prune.cperm_sliceperm), 14);
        assert_eq!(deepest(&prune.eperm_sliceperm), 12);
    }

    #[test]
    fn one_move_never_changes_a_distance_by_more_than_one() {
        // Admissibility in the form that is testable: the tables hold exact distances in a
        // graph whose moves are invertible, so a move is a step along the way home or a step
        // away from it and never a leap. A wrong product stride passes the size tests and
        // fails this one immediately.
        let mut rng = StdRng::seed_from_u64(101);
        for table in tables() {
            let apply = table.apply;
            for round in 0..SAMPLES {
                let state = rng.gen_range(0..table.dist.len());
                let here = i32::from(table.dist[state]);
                for &(axis, power) in &table.moves {
                    let there = i32::from(table.dist[apply(state, axis, power)]);
                    assert!(
                        (here - there).abs() <= 1,
                        "{}: round {round} at {state} jumps {here} to {there} on {axis}.{power}",
                        table.name
                    );
                }
            }
        }
    }

    #[test]
    fn a_move_towards_solved_exists_from_every_sampled_cell() {
        // The other half of exactness: a distance of d is not merely consistent, it is
        // attained, so some move drops it to d - 1 and the search can always descend.
        let mut rng = StdRng::seed_from_u64(103);
        for table in tables() {
            let apply = table.apply;
            for _ in 0..SAMPLES {
                let state = rng.gen_range(0..table.dist.len());
                let here = table.dist[state];
                if here == 0 {
                    continue;
                }
                let descends = table
                    .moves
                    .iter()
                    .any(|&(axis, power)| table.dist[apply(state, axis, power)] == here - 1);
                assert!(descends, "{}: no move leaves {state} closer than {here}", table.name);
            }
        }
    }

    #[test]
    fn the_bounds_read_the_tables_the_way_the_search_indexes_them() {
        let prune = Prune::get();
        assert_eq!(prune.bound1(0, 0, 0), 0, "a solved cube is already in G1");
        assert_eq!(prune.bound2(0, 0, 0), 0, "a solved cube is solved");
        let mut rng = StdRng::seed_from_u64(107);
        for _ in 0..SAMPLES {
            let (twist, flip, slice) = (
                rng.gen_range(0..N_TWIST),
                rng.gen_range(0..N_FLIP),
                rng.gen_range(0..N_SLICE),
            );
            assert_eq!(
                prune.bound1(twist, flip, slice),
                usize::from(
                    prune.twist_slice[twist * N_SLICE + slice]
                        .max(prune.flip_slice[flip * N_SLICE + slice])
                )
            );
            let (cperm, eperm, sliceperm) = (
                rng.gen_range(0..N_CPERM),
                rng.gen_range(0..N_EPERM),
                rng.gen_range(0..N_SLICEPERM),
            );
            assert_eq!(
                prune.bound2(cperm, eperm, sliceperm),
                usize::from(
                    prune.cperm_sliceperm[cperm * N_SLICEPERM + sliceperm]
                        .max(prune.eperm_sliceperm[eperm * N_SLICEPERM + sliceperm])
                )
            );
        }
    }

    #[test]
    fn no_phase_two_move_leaves_the_phase_one_bound_where_it_was_by_accident() {
        // The witness the module header names, kept as a test: U makes cperm odd with
        // sliceperm still even, the pair the parity argument says has to be reachable.
        let prune = Prune::get();
        let after_u = cperm_sliceperm_move(0, 0, 1);
        assert_eq!(after_u % N_SLICEPERM, 0, "U cannot move the slice edges");
        assert_ne!(after_u / N_SLICEPERM, 0, "U has to cycle four corners");
        assert_eq!(prune.cperm_sliceperm[after_u], 1, "U is one move from solved");
        assert!(PHASE2_MOVES.contains(&0), "U has to be a phase-2 move");
    }
}
