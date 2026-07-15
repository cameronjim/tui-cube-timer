//! 2x2 random-state scrambles: corner coordinates, move tables, exact-11 solutions.
//!
//! A 2x2 is eight corners and nothing else, so with no centers to hold still one corner can
//! be declared solved and the puzzle described by the other seven. This module shares
//! TNoodle's `TwoByTwoSolver` numbering exactly, so a scramble here means what a scramble on
//! a competition sheet means:
//!
//! ```text
//!            +----------+
//!            |*3*    *2*|
//!            |    U     |
//!            |*1*    *0*|
//! +----------+----------+----------+----------+
//! | 3      1 | 1      0 | 0      2 | 2      3 |
//! |     L    |    F     |    R     |    B     |
//! | 7      5 | 5      4 | 4      6 | 6      7 |
//! +----------+----------+----------+----------+
//!            |*5*    *4*|
//!            |    D     |
//!            |*7*    *6*|
//!            +----------+
//! ```
//!
//! In words: 0 UFR, 1 UFL, 2 UBR, 3 UBL, 4 DFR, 5 DFL, 6 DBR, 7 DBL, and 7 never moves.
//! The starred facelet is the primary one, on U for the top four corners and on D for the
//! bottom four, and a corner's orientation counts clockwise steps from the primary facelet
//! slot of wherever it currently sits. Fixing DBL is also what makes U, R and F a complete
//! move set: every state is reachable without ever turning D, L or B.
//!
//! Two coordinates follow from that, and [`CYCLES`] and [`TWISTS`] carry TNoodle's
//! `moveCubies` cycles verbatim. The permutation coordinate is the factorial rank of the
//! seven corners over `0..5040`; the orientation coordinate is six base-3 twists over
//! `0..729`, the seventh being whatever brings the total back to a multiple of three. Both
//! are indexed by position rather than by cubie, which is what keeps each coordinate's
//! transition a function of that coordinate and the move alone, so each gets its own move
//! table and [`apply`] is nothing but lookups. A state is `perm * 729 + orient`, index 0 is
//! solved, and all 3,674,160 indices are reachable, which the depth-distribution test pins.
//!
//! Sampling goes through [`Engine::random_reachable`] rather than drawing the two
//! coordinates by hand. Since the index is a bijection of the pair, one uniform draw over
//! the index space *is* two independent uniform draws, so the two agree; going through the
//! table means uniformity rests on what the search actually reached instead of on an
//! argument written here, and rejection never fires because nothing is unreachable.

use super::Engine;
use rand::Rng;
use std::sync::OnceLock;

/// Corners the coordinates track. DBL is fixed, so seven of the eight move.
const CORNERS: usize = 7;

/// Permutations of the seven free corners.
const N_PERM: usize = 5040;

/// Orientations of the seven free corners: six free twists, the seventh determined.
const N_ORIENT: usize = 729;

/// Turnable axes, in TNoodle's order.
const AXES: usize = 3;

/// Quarter turns one move may carry.
const POWERS: usize = 3;

/// Columns in each move table: axis * `POWERS` + power - 1.
const MOVES: usize = AXES * POWERS;

/// Face letter per axis.
const AXIS_NAMES: [char; AXES] = ['U', 'R', 'F'];

/// Tokens TNoodle emits per 2x2 scramble, whatever the optimal solution costs.
const SCRAMBLE_LEN: usize = 11;

/// The four positions each axis cycles, the first moving to the second and the last to the first.
const CYCLES: [[usize; 4]; AXES] = [[1, 3, 2, 0], [0, 2, 6, 4], [1, 0, 4, 5]];

/// Clockwise twists a quarter turn adds, entry k landing on the next position in [`CYCLES`].
///
/// A U turn keeps all four primary facelets on U and so twists nothing. R and F each tilt the
/// U-D axis into the turning plane, which alternates one and two around the cycle.
const TWISTS: [[u8; 4]; AXES] = [[0, 0, 0, 0], [1, 2, 1, 2], [1, 2, 1, 2]];

/// One move table per coordinate, row-major with one row per coordinate value.
struct Tables {
    perm: Vec<u16>,
    orient: Vec<u16>,
}

static TABLES: OnceLock<Tables> = OnceLock::new();

static DISTANCES: OnceLock<Vec<u8>> = OnceLock::new();

/// A TNoodle-shape 2x2 scramble: exactly 11 moves of U R F, suffixes plain, ' and 2.
pub(super) fn scramble<R: Rng>(rng: &mut R) -> String {
    let state = engine().random_reachable(distances(), rng);
    scramble_for(state, rng)
}

/// The corner group as the shared search sees it.
fn engine() -> Engine {
    Engine {
        states: N_PERM * N_ORIENT,
        axes: AXES,
        powers: POWERS,
        apply,
    }
}

/// Where `power` quarter turns of `axis` take `state`, by table lookup only.
fn apply(state: usize, axis: usize, power: usize) -> usize {
    let tables = tables();
    let column = axis * POWERS + power - 1;
    let perm = usize::from(tables.perm[(state / N_ORIENT) * MOVES + column]);
    let orient = usize::from(tables.orient[(state % N_ORIENT) * MOVES + column]);
    perm * N_ORIENT + orient
}

/// Exact distance to solved for every state, built once and shared.
fn distances() -> &'static [u8] {
    DISTANCES.get_or_init(|| engine().distances())
}

/// The move tables, built once and shared.
///
/// Separate from [`DISTANCES`] on purpose: the breadth-first search calls [`apply`], so a
/// single lock holding both would deadlock initializing itself.
fn tables() -> &'static Tables {
    TABLES.get_or_init(|| {
        let mut perm = vec![0u16; N_PERM * MOVES];
        for index in 0..N_PERM {
            for axis in 0..AXES {
                let mut state = perm_decode(index);
                for power in 1..=POWERS {
                    turn_perm(&mut state, axis);
                    perm[index * MOVES + axis * POWERS + power - 1] = perm_encode(&state) as u16;
                }
            }
        }
        let mut orient = vec![0u16; N_ORIENT * MOVES];
        for index in 0..N_ORIENT {
            for axis in 0..AXES {
                let mut state = orient_decode(index);
                for power in 1..=POWERS {
                    turn_orient(&mut state, axis);
                    orient[index * MOVES + axis * POWERS + power - 1] = orient_encode(&state) as u16;
                }
            }
        }
        Tables { perm, orient }
    })
}

/// The scramble that reaches `state`: an exact-length solution for it, inverted.
///
/// Padding a short solution out to eleven is what makes every 2x2 scramble eleven moves even
/// though the average optimal solution is 8.76. No state is known to need the twelve-move
/// retry, and the empty fallback below it exists only so the timer cannot panic on one.
///
/// The rng is the search's, not the sampler's: it picks which of the state's many exact-length
/// solutions comes back, and without it every scramble would end on the same token.
fn scramble_for<R: Rng>(state: usize, rng: &mut R) -> String {
    let engine = engine();
    let dist = distances();
    let solution = engine
        .solve_exactly(state, SCRAMBLE_LEN, dist, rng)
        .or_else(|| engine.solve_exactly(state, SCRAMBLE_LEN + 1, dist, rng))
        .unwrap_or_default();
    let mut out = String::with_capacity(solution.len() * 3);
    for &(axis, power) in solution.iter().rev() {
        if !out.is_empty() {
            out.push(' ');
        }
        push_token(&mut out, axis, POWERS + 1 - power);
    }
    out
}

/// Append the WCA token for `power` quarter turns of `axis`.
fn push_token(out: &mut String, axis: usize, power: usize) {
    out.push(AXIS_NAMES[axis]);
    match power {
        2 => out.push('2'),
        3 => out.push('\''),
        _ => {}
    }
}

/// The corner at each position, from the permutation coordinate.
fn perm_decode(mut index: usize) -> [u8; CORNERS] {
    let mut digits = [0usize; CORNERS];
    for (i, digit) in digits.iter_mut().enumerate().rev() {
        let radix = CORNERS - i;
        *digit = index % radix;
        index /= radix;
    }
    // Each digit picks from the corners not yet placed, held sorted so the rank is canonical.
    let mut pool = [0u8, 1, 2, 3, 4, 5, 6];
    let mut left = CORNERS;
    let mut perm = [0u8; CORNERS];
    for (i, slot) in perm.iter_mut().enumerate() {
        let digit = digits[i];
        *slot = pool[digit];
        pool.copy_within(digit + 1..left, digit);
        left -= 1;
    }
    perm
}

/// The factorial rank of `perm`, zero for the identity.
fn perm_encode(perm: &[u8; CORNERS]) -> usize {
    let mut index = 0;
    for (i, &corner) in perm.iter().enumerate() {
        let smaller = perm[i + 1..].iter().filter(|&&later| later < corner).count();
        index = index * (CORNERS - i) + smaller;
    }
    index
}

/// The twist at each position, from the orientation coordinate.
fn orient_decode(mut index: usize) -> [u8; CORNERS] {
    let mut orient = [0u8; CORNERS];
    let mut total = 0u8;
    for twist in orient[..CORNERS - 1].iter_mut().rev() {
        *twist = (index % 3) as u8;
        total += *twist;
        index /= 3;
    }
    // The seventh twist is whatever brings the total back to a multiple of three.
    orient[CORNERS - 1] = (3 - total % 3) % 3;
    orient
}

/// The base-3 rank of the first six twists, zero when nothing is twisted.
fn orient_encode(orient: &[u8; CORNERS]) -> usize {
    orient[..CORNERS - 1]
        .iter()
        .fold(0, |index, &twist| index * 3 + usize::from(twist))
}

/// One clockwise quarter turn of `axis` on a permutation indexed by position.
fn turn_perm(perm: &mut [u8; CORNERS], axis: usize) {
    let cycle = CYCLES[axis];
    let carried = perm[cycle[3]];
    perm[cycle[3]] = perm[cycle[2]];
    perm[cycle[2]] = perm[cycle[1]];
    perm[cycle[1]] = perm[cycle[0]];
    perm[cycle[0]] = carried;
}

/// One clockwise quarter turn of `axis` on an orientation indexed by position.
fn turn_orient(orient: &mut [u8; CORNERS], axis: usize) {
    let cycle = CYCLES[axis];
    let twist = TWISTS[axis];
    let carried = orient[cycle[3]];
    orient[cycle[3]] = (orient[cycle[2]] + twist[2]) % 3;
    orient[cycle[2]] = (orient[cycle[1]] + twist[1]) % 3;
    orient[cycle[1]] = (orient[cycle[0]] + twist[0]) % 3;
    orient[cycle[0]] = (carried + twist[3]) % 3;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::{Cube, Face};
    use crate::solver::UNREACHABLE;
    use crate::types::Puzzle;
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use std::collections::HashSet;

    /// The three facelets of each corner position, clockwise seen from outside that corner.
    ///
    /// Slot 0 is the primary facelet the module header describes, so a corner's orientation
    /// is simply which slot its primary sticker currently occupies. Every coordinate was read
    /// off `cube`'s own net conventions, not off this module's tables: `strip_cycle` says which
    /// row and column of each face touches which neighbour, and that fixes all 24 of them.
    const FACELETS: [[(Face, usize, usize); 3]; 8] = [
        [(Face::U, 1, 1), (Face::R, 0, 0), (Face::F, 0, 1)],
        [(Face::U, 1, 0), (Face::F, 0, 0), (Face::L, 0, 1)],
        [(Face::U, 0, 1), (Face::B, 0, 0), (Face::R, 0, 1)],
        [(Face::U, 0, 0), (Face::L, 0, 0), (Face::B, 0, 1)],
        [(Face::D, 0, 1), (Face::F, 1, 1), (Face::R, 1, 0)],
        [(Face::D, 0, 0), (Face::L, 1, 1), (Face::F, 1, 0)],
        [(Face::D, 1, 1), (Face::R, 1, 1), (Face::B, 1, 0)],
        [(Face::D, 1, 0), (Face::B, 1, 1), (Face::L, 1, 0)],
    ];

    /// The state index a facelet 2x2 sits in, DBL solved as the coordinates require.
    ///
    /// This is the cross-model pin: `cube` and this module were written independently, from
    /// facelet permutations and from corner coordinates, and everything below that compares
    /// them is checking two separate pieces of arithmetic against each other.
    fn state_of(cube: &Cube) -> usize {
        assert_eq!(cube.n(), 2, "the corner coordinates only describe a 2x2");
        let mut perm = [0u8; CORNERS];
        let mut orient = [0u8; CORNERS];
        for (position, slots) in FACELETS.into_iter().enumerate() {
            let colours = slots.map(|(face, row, col)| cube.sticker(face, row, col));
            // A corner is named by the three colours it carries, and all eight sets differ.
            let corner = FACELETS
                .iter()
                .position(|home| home.iter().all(|(face, _, _)| colours.contains(face)))
                .unwrap_or_else(|| panic!("no corner of a 2x2 carries {colours:?}"));
            let primary = if corner < 4 { Face::U } else { Face::D };
            let twist = colours
                .iter()
                .position(|colour| *colour == primary)
                .unwrap_or_else(|| panic!("{colours:?} has no {primary:?} sticker"));
            if position == 7 {
                assert_eq!((corner, twist), (7, 0), "DBL left home, so no index describes this");
                continue;
            }
            perm[position] = corner as u8;
            orient[position] = twist as u8;
        }
        // The seventh twist read off the stickers must be the one the coordinate derives.
        assert_eq!(
            orient_decode(orient_encode(&orient)),
            orient,
            "the twists on the cube do not sum to a multiple of three"
        );
        perm_encode(&perm) * N_ORIENT + orient_encode(&orient)
    }

    /// The WCA token for `power` quarter turns of `axis`.
    fn token(axis: usize, power: usize) -> String {
        let mut out = String::new();
        push_token(&mut out, axis, power);
        out
    }

    /// Split a scramble into moves, asserting the whole notation on the way through.
    fn parse(scramble: &str) -> Vec<(usize, usize)> {
        // Splitting on a single space rather than on whitespace is what catches a doubled
        // space, a leading one and a trailing one, all of which leave an empty token.
        let tokens: Vec<&str> = scramble.split(' ').collect();
        assert_eq!(tokens.len(), SCRAMBLE_LEN, "not eleven tokens: {scramble:?}");
        let mut moves = Vec::with_capacity(tokens.len());
        for text in tokens {
            let mut chars = text.chars();
            let face = chars
                .next()
                .unwrap_or_else(|| panic!("an empty token in {scramble:?}"));
            let axis = AXIS_NAMES
                .iter()
                .position(|&name| name == face)
                .unwrap_or_else(|| panic!("{text:?} does not turn U, R or F in {scramble:?}"));
            let power = match chars.next() {
                None => 1,
                Some('2') => 2,
                Some('\'') => 3,
                Some(other) => panic!("{other:?} is not a legal suffix in {scramble:?}"),
            };
            assert!(chars.next().is_none(), "{text:?} is too long in {scramble:?}");
            moves.push((axis, power));
        }
        for pair in moves.windows(2) {
            assert_ne!(pair[0].0, pair[1].0, "consecutive turns of one face in {scramble:?}");
        }
        moves
    }

    /// A uniformly sampled state, the same way [`scramble`] draws one.
    fn sample(seed: u64) -> usize {
        engine().random_reachable(distances(), &mut StdRng::seed_from_u64(seed))
    }

    // ---- the coordinates

    #[test]
    fn the_solved_state_is_index_zero_in_both_models() {
        assert_eq!(perm_encode(&[0, 1, 2, 3, 4, 5, 6]), 0);
        assert_eq!(orient_encode(&[0; CORNERS]), 0);
        assert_eq!(state_of(&Cube::solved(2)), 0);
        assert_eq!(distances()[0], 0);
    }

    #[test]
    fn both_coordinates_survive_a_decode_and_an_encode() {
        for index in 0..N_PERM {
            let perm = perm_decode(index);
            let mut placed = perm;
            placed.sort_unstable();
            assert_eq!(placed, [0, 1, 2, 3, 4, 5, 6], "index {index} is not a permutation");
            assert_eq!(perm_encode(&perm), index);
        }
        for index in 0..N_ORIENT {
            let orient = orient_decode(index);
            assert!(orient.iter().all(|&twist| twist < 3), "index {index} twisted past two");
            assert_eq!(orient.iter().sum::<u8>() % 3, 0, "index {index} is unbalanced");
            assert_eq!(orient_encode(&orient), index);
        }
    }

    #[test]
    fn the_state_space_is_the_size_the_puzzle_has() {
        assert_eq!(N_PERM * N_ORIENT, 3_674_160);
    }

    // ---- the move tables

    #[test]
    fn four_quarter_turns_of_an_axis_are_the_identity_and_the_twists_stay_balanced() {
        for axis in 0..AXES {
            for index in [0, 1, 2_519, N_PERM - 1] {
                let mut perm = perm_decode(index);
                for _ in 0..4 {
                    turn_perm(&mut perm, axis);
                }
                assert_eq!(perm, perm_decode(index), "axis {axis} on permutation {index}");
            }
            for index in [0, 1, 364, N_ORIENT - 1] {
                let mut orient = orient_decode(index);
                for turn in 1..=4 {
                    turn_orient(&mut orient, axis);
                    assert_eq!(
                        orient_decode(orient_encode(&orient)),
                        orient,
                        "axis {axis} turn {turn} left a twist the coordinate cannot derive"
                    );
                }
                assert_eq!(orient, orient_decode(index), "axis {axis} on orientation {index}");
            }
        }
    }

    #[test]
    fn a_power_is_the_quarter_turn_repeated() {
        for state in [0, 1, 12_345, 2_000_000, N_PERM * N_ORIENT - 1] {
            for axis in 0..AXES {
                let mut repeated = state;
                for power in 1..=POWERS {
                    repeated = apply(repeated, axis, 1);
                    assert_eq!(apply(state, axis, power), repeated, "axis {axis} power {power}");
                }
                assert_eq!(apply(repeated, axis, 1), state, "axis {axis} is not of order four");
            }
        }
    }

    // ---- the distance table

    #[test]
    fn the_depth_distribution_matches_the_published_counts() {
        // Jaap Scherphuis's God's-algorithm counts for the 2x2 corner group. Getting all
        // twelve right while summing to the exact total leaves no room for a wrong cycle,
        // a wrong twist or a coordinate that double-counts.
        const EXPECTED: [usize; 12] = [
            1, 9, 54, 321, 1_847, 9_992, 50_136, 227_536, 870_072, 1_887_748, 623_800, 2_644,
        ];
        let mut counts = [0usize; 256];
        for &depth in distances() {
            counts[usize::from(depth)] += 1;
        }
        assert_eq!(&counts[..EXPECTED.len()], &EXPECTED[..]);
        assert_eq!(
            counts[usize::from(UNREACHABLE)],
            0,
            "some index of the coordinate space is unreachable"
        );
        assert!(
            counts[EXPECTED.len()..].iter().all(|&count| count == 0),
            "a state sits deeper than eleven moves"
        );
        assert_eq!(EXPECTED.iter().sum::<usize>(), N_PERM * N_ORIENT);
    }

    #[test]
    fn the_mean_optimal_depth_sits_near_the_published_average() {
        const SAMPLES: usize = 4_000;
        let dist = distances();
        let total: usize = (0..SAMPLES as u64)
            .map(|seed| usize::from(dist[sample(seed)]))
            .sum();
        let mean = total as f64 / SAMPLES as f64;
        assert!((8.5..9.0).contains(&mean), "mean optimal depth {mean} is not near 8.76");
    }

    // ---- solutions and their inverses

    #[test]
    fn a_solution_solves_and_the_scramble_built_from_it_rebuilds_the_state() {
        let engine = engine();
        let dist = distances();
        for seed in 0..300u64 {
            let sampled = sample(seed);
            let mut rng = StdRng::seed_from_u64(seed);
            let solution = engine
                .solve_exactly(sampled, SCRAMBLE_LEN, dist, &mut rng)
                .unwrap_or_else(|| panic!("seed {seed} has no eleven move solution"));
            assert_eq!(solution.len(), SCRAMBLE_LEN);
            let solved = solution
                .iter()
                .fold(sampled, |state, &(axis, power)| apply(state, axis, power));
            assert_eq!(solved, 0, "seed {seed}'s own solution did not solve it");
            let scramble = scramble_for(sampled, &mut rng);
            let rebuilt = parse(&scramble)
                .iter()
                .fold(0, |state, &(axis, power)| apply(state, axis, power));
            assert_eq!(rebuilt, sampled, "seed {seed} did not rebuild from {scramble:?}");
        }
    }

    // ---- emission

    #[test]
    fn every_scramble_is_eleven_canonical_tokens_of_u_r_and_f() {
        for seed in 0..400u64 {
            let scramble = scramble(&mut StdRng::seed_from_u64(seed));
            assert_eq!(scramble.trim(), scramble, "stray whitespace in {scramble:?}");
            assert_eq!(parse(&scramble).len(), SCRAMBLE_LEN);
        }
    }

    #[test]
    fn every_face_and_every_suffix_turns_up() {
        let mut seen = [[false; POWERS]; AXES];
        for seed in 0..200u64 {
            for (axis, power) in parse(&scramble(&mut StdRng::seed_from_u64(seed))) {
                seen[axis][power - 1] = true;
            }
        }
        assert!(
            seen.iter().flatten().all(|&hit| hit),
            "some U R F turn never appears: {seen:?}"
        );
    }

    #[test]
    fn the_last_token_of_a_scramble_ranges_over_the_move_set() {
        const SEEDS: usize = 400;
        // The last token of a scramble is the first move of the solution behind it, so a
        // search that tried the branches in a fixed order ended every scramble on U'. That is
        // uniformity-neutral and still the first thing a speedcuber notices.
        let mut counts = [[0usize; POWERS]; AXES];
        for seed in 0..SEEDS as u64 {
            let scramble = scramble(&mut StdRng::seed_from_u64(seed));
            let (axis, power) = *parse(&scramble).last().expect("eleven tokens");
            counts[axis][power - 1] += 1;
        }
        let seen = counts.iter().flatten().filter(|&&count| count > 0).count();
        let worst = counts.iter().flatten().copied().max().unwrap_or(0);
        assert!(
            seen >= 4,
            "only {seen} of the nine tokens ever ended a scramble in {SEEDS} seeds: {counts:?}"
        );
        assert!(
            worst * 5 <= SEEDS * 3,
            "one token ended {worst} of {SEEDS} scrambles, over three fifths: {counts:?}"
        );
    }

    #[test]
    fn scrambles_are_stable_under_a_seed_and_differ_across_seeds() {
        let mut seen = HashSet::new();
        for seed in 0..60u64 {
            let scramble = scramble(&mut StdRng::seed_from_u64(seed));
            assert_eq!(
                scramble,
                super::scramble(&mut StdRng::seed_from_u64(seed)),
                "seed {seed} is not deterministic"
            );
            seen.insert(scramble);
        }
        assert_eq!(seen.len(), 60, "two seeds produced the same scramble");
    }

    // ---- the cross-model pin

    #[test]
    fn the_facelet_model_agrees_with_every_move_in_the_tables() {
        let mut rng = StdRng::seed_from_u64(7);
        for round in 0..200 {
            // Only U, R and F, because any other face would move DBL out from under the
            // coordinates. Walking the two models forward together is what makes this a
            // check on the tables rather than on one lucky state.
            let mut cube = Cube::solved(2);
            let mut state = 0usize;
            for _ in 0..rng.gen_range(0..14) {
                let axis = rng.gen_range(0..AXES);
                let power = rng.gen_range(1..=POWERS);
                cube.apply_scramble(&token(axis, power)).expect("a legal 2x2 turn");
                state = apply(state, axis, power);
            }
            assert_eq!(state_of(&cube), state, "round {round}: the two models drifted apart");
            for axis in 0..AXES {
                for power in 1..=POWERS {
                    let mut turned = cube.clone();
                    turned.apply_scramble(&token(axis, power)).expect("a legal 2x2 turn");
                    assert_eq!(
                        state_of(&turned),
                        apply(state, axis, power),
                        "round {round}: {} disagrees with the move tables",
                        token(axis, power)
                    );
                }
            }
        }
    }

    #[test]
    fn the_facelet_model_reaches_the_state_each_scramble_sampled() {
        for seed in 0..100u64 {
            // One rng for both draws, in the order `scramble` uses it: the state, then the
            // branch order behind the solution.
            let mut rng = StdRng::seed_from_u64(seed);
            let sampled = engine().random_reachable(distances(), &mut rng);
            let scramble = scramble_for(sampled, &mut rng);
            let mut cube = Cube::solved(2);
            cube.apply_scramble(&scramble).expect("a 2x2 scramble on a 2x2");
            assert_eq!(state_of(&cube), sampled, "seed {seed} landed elsewhere: {scramble:?}");
            // And the public entry point samples exactly this state for this seed.
            assert_eq!(
                super::scramble(&mut StdRng::seed_from_u64(seed)),
                scramble,
                "seed {seed} is wired to a different state"
            );
        }
    }

    // ---- interop

    #[test]
    fn every_scramble_applies_to_a_facelet_cube_and_leaves_it_scrambled() {
        for seed in 0..200u64 {
            let scramble = scramble(&mut StdRng::seed_from_u64(seed));
            let cube = Cube::for_scramble(Puzzle::Cube2, &scramble)
                .unwrap_or_else(|| panic!("seed {seed} would not apply: {scramble:?}"));
            assert_eq!(cube.n(), 2);
            assert!(!cube.is_solved(), "seed {seed} scrambled to solved: {scramble:?}");
        }
    }
}
