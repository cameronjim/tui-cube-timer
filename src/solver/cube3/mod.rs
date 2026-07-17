//! Random-state 3x3 scrambles by Kociemba's two-phase algorithm.
//!
//! The 3x3 has 43 quintillion states, so plan 01's table-every-state method is out.
//! Phase 1 drives the cube into the subgroup where every piece is oriented and the four
//! middle-slice edges are home in their slice; phase 2 finishes inside it with the ten
//! moves that never leave it. Each phase is a depth-first search cut down by pruning
//! tables holding exact distances in a projection of the cube, and the loop keeps
//! trading a longer phase 1 for a shorter phase 2 until the total is 21 moves or
//! better, TNoodle's own cap, with a documented fallback for the rare state no split
//! fits inside it. The scramble is the solution played backwards. Method, conventions
//! and fixtures in `claude-docs/plans/02-kociemba-two-phase.md`.

mod cubies;
mod coords;
mod prune;
mod search;

use self::cubies::{Cubies, SOLVED};
use rand::Rng;

/// Quarter turns per face in `cubies`' move numbering, which numbers moves face major.
///
/// It is the stride from one face to the next, so it is also what makes the move undoing
/// index `mv` the one at power 4 minus `mv`'s own on that same face.
const POWERS: usize = 3;

/// A TNoodle-shape 3x3 scramble: the inverse of a two-phase solution for a uniformly
/// random state, 16 to 21 tokens and nearly always 20 or 21.
///
/// Three steps, each testable on its own: draw the state, solve it, write the solution
/// backwards. [`search::solve`] aims at [`search::MAX_SOLUTION`] moves and a random state
/// almost never falls in fewer than 17, so that is the length a scramble comes out at;
/// the rare state no split fits inside the cap comes back longer, measured at 9 in 20,000.
/// The rng is drawn on twice over, in order: the state, then the branch order the search
/// takes, so one seed gives one scramble.
pub(super) fn scramble<R: Rng>(rng: &mut R) -> String {
    let mut state = sample(rng);
    // TNoodle's minimum-distance rule, which here has one state to exclude out of 4.3 times
    // 10 to the 19th: solved, whose scramble is the empty string.
    while state == SOLVED {
        state = sample(rng);
    }
    let solution = search::solve(&state, rng);
    emit(&solution)
}

/// A uniformly random state of the ones a 3x3 can actually reach, TNoodle's
/// `Tools.randomCube()`.
///
/// Four draws, each constrained by exactly one of the invariants `cubies` pins. The two
/// permutations are drawn free and then reconciled: every quarter turn is a 4-cycle on
/// both orbits, so corner and edge parity can never come apart, and one swap of two edges
/// is the whole repair when they differ. Orientation carries its last piece, seven free
/// corner twists leaving the eighth determined by the mod-3 sum and eleven free edge flips
/// leaving the twelfth determined by the mod-2 sum.
///
/// Uniformity survives the repair because swapping a fixed pair of edges is a bijection
/// between the odd and the even permutations, so ep stays uniform over the parity class cp
/// picked out, and the carried orientations are uniform for the same reason.
///
/// Private rather than `pub(super)`: [`Cubies`] is only usable inside this module, so a
/// wider visibility here would leak a more private type. `search` reaches it, as its own
/// tests need to, by being a child of this module.
fn sample<R: Rng>(rng: &mut R) -> Cubies {
    let mut state = SOLVED;
    shuffle(&mut state.cp, rng);
    shuffle(&mut state.ep, rng);
    if cubies::corner_parity(&state) != cubies::edge_parity(&state) {
        state.ep.swap(0, 1);
    }
    draw_carried(&mut state.co, 3, rng);
    draw_carried(&mut state.eo, 2, rng);
    state
}

/// Fisher-Yates over `perm`: every ordering equally likely, using the caller's rng.
fn shuffle<R: Rng>(perm: &mut [u8], rng: &mut R) {
    for i in (1..perm.len()).rev() {
        perm.swap(i, rng.gen_range(0..=i));
    }
}

/// Uniform orientations into `slots`, the last carrying the total to 0 mod `modulus`.
///
/// The carried slot is uniform too, the sum of the free ones being uniform mod `modulus`,
/// which is what keeps the whole draw uniform over the orientations a cube can reach.
fn draw_carried<R: Rng>(slots: &mut [u8], modulus: u8, rng: &mut R) {
    // An empty array carries nothing, which cannot happen here and needs no fallback.
    if let Some((carried, free)) = slots.split_last_mut() {
        let mut sum = 0;
        for slot in free.iter_mut() {
            *slot = rng.gen_range(0..modulus);
            sum = (sum + *slot) % modulus;
        }
        *carried = (modulus - sum) % modulus;
    }
}

/// A solution written backwards as WCA tokens, which is the scramble reaching the state
/// it solved.
///
/// Reversed order with every move replaced by the one undoing it, so power p becomes
/// 4 - p on the same face. Single spaces, no trailing space, and empty for an empty
/// solution.
fn emit(solution: &[usize]) -> String {
    let mut out = String::with_capacity(solution.len() * 3);
    for &mv in solution.iter().rev() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(cubies::token(undo(mv)));
    }
    out
}

/// The move undoing `mv`: the same face, the complementary power.
fn undo(mv: usize) -> usize {
    let face = mv / POWERS;
    let power = mv % POWERS;
    face * POWERS + (POWERS - 1 - power)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use std::collections::HashSet;

    /// Seeds for the sampling tests, which cost a shuffle and twenty draws each.
    const SAMPLE_SEEDS: u64 = 4_000;

    /// Rounds for the emission property tests.
    const EMIT_ROUNDS: usize = 200;

    /// The eight corners and the twelve edges, spelled out where a tally needs the width.
    const CORNERS: usize = 8;
    const EDGES: usize = 12;

    /// A hashable stand-in for a state, `Cubies` being `Eq` but not `Hash`.
    type Key = ([u8; CORNERS], [u8; CORNERS], [u8; EDGES], [u8; EDGES]);

    /// The four arrays of a state, in a shape a `HashSet` will take.
    fn key(s: &Cubies) -> Key {
        (s.cp, s.co, s.ep, s.eo)
    }

    /// The move index a WCA token names, of the eighteen the cubie model turns.
    fn move_of(text: &str) -> usize {
        (0..cubies::N_MOVES)
            .find(|&mv| cubies::token(mv) == text)
            .unwrap_or_else(|| panic!("{text:?} is not one of the eighteen 3x3 moves"))
    }

    /// The two permutations `sample` draws for `seed`, before it reconciles their parity.
    ///
    /// The draw order is part of the contract: cp, then ep, then the swap, so replaying
    /// only those two rounds says both what was drawn and whether the swap had to fire.
    fn drawn(seed: u64) -> Cubies {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut probe = SOLVED;
        shuffle(&mut probe.cp, &mut rng);
        shuffle(&mut probe.ep, &mut rng);
        probe
    }

    /// Whether `perm` holds each of its own indices exactly once.
    fn is_permutation(perm: &[u8]) -> bool {
        let mut seen = vec![false; perm.len()];
        for &piece in perm {
            match seen.get_mut(usize::from(piece)) {
                Some(slot) if !*slot => *slot = true,
                _ => return false,
            }
        }
        true
    }

    /// A random solution of up to the solver's cap, for the emission tests.
    fn random_solution<R: Rng>(rng: &mut R) -> Vec<usize> {
        let len = rng.gen_range(0..=search::MAX_SOLUTION);
        (0..len).map(|_| rng.gen_range(0..cubies::N_MOVES)).collect()
    }

    /// The lowest and the highest count in a tally.
    fn spread(counts: &[usize]) -> (usize, usize) {
        counts.iter().fold((usize::MAX, 0), |(lo, hi), &c| (lo.min(c), hi.max(c)))
    }

    // ---- sampling

    #[test]
    fn every_sampled_state_satisfies_the_invariants_a_cube_can_reach() {
        for seed in 0..SAMPLE_SEEDS {
            let s = sample(&mut StdRng::seed_from_u64(seed));
            assert_eq!(
                cubies::corner_parity(&s),
                cubies::edge_parity(&s),
                "seed {seed}: the parities came apart, {:?} against {:?}",
                s.cp,
                s.ep
            );
            assert_eq!(cubies::twist_sum(&s), 0, "seed {seed}: twists {:?} not 0 mod 3", s.co);
            assert_eq!(cubies::flip_sum(&s), 0, "seed {seed}: flips {:?} not 0 mod 2", s.eo);
            assert!(is_permutation(&s.cp), "seed {seed}: cp {:?} is no permutation", s.cp);
            assert!(is_permutation(&s.ep), "seed {seed}: ep {:?} is no permutation", s.ep);
            assert!(s.co.iter().all(|&t| t < 3), "seed {seed}: twist out of range in {:?}", s.co);
            assert!(s.eo.iter().all(|&f| f < 2), "seed {seed}: flip out of range in {:?}", s.eo);
        }
    }

    #[test]
    fn both_parities_are_drawn_and_the_swap_fires_on_exactly_the_mismatches() {
        // The four combinations of the two drawn parities, corner major: two of them need
        // the swap and two do not, so a batch this size exercises both paths hundreds of
        // times over. A repair that fired on the wrong side would break the invariant test
        // above; what this one adds is that it fires on the right states and only there.
        let mut seen = [0usize; 4];
        for seed in 0..SAMPLE_SEEDS {
            let probe = drawn(seed);
            let corners = cubies::corner_parity(&probe);
            let edges = cubies::edge_parity(&probe);
            seen[usize::from(corners) * 2 + usize::from(edges)] += 1;

            let s = sample(&mut StdRng::seed_from_u64(seed));
            assert_eq!(s.cp, probe.cp, "seed {seed}: cp is not the permutation drawn");
            let mut want = probe.ep;
            if corners != edges {
                want.swap(0, 1);
            }
            assert_eq!(s.ep, want, "seed {seed}: ep is not the drawn permutation reconciled");
        }
        // Each of the four is a quarter of the draws, about a thousand of four thousand.
        let (low, high) = spread(&seen);
        assert!(low > 0, "one of the four parity combinations never came up: {seen:?}");
        assert!(
            (800..=1200).contains(&low) && (800..=1200).contains(&high),
            "the four parity combinations came out {seen:?}, expected about a thousand each"
        );
        let swapped = seen[1] + seen[2];
        assert!(
            (1_700..=2_300).contains(&swapped),
            "the swap fired {swapped} times in {SAMPLE_SEEDS}, expected about half"
        );
    }

    #[test]
    fn seeded_draws_are_distinct_and_land_every_piece_in_every_position() {
        let mut states: HashSet<Key> = HashSet::new();
        let mut corners = vec![0usize; CORNERS * CORNERS];
        let mut edges = vec![0usize; EDGES * EDGES];
        let mut twists = 0u64;
        let mut flips = 0u64;
        for seed in 0..SAMPLE_SEEDS {
            let s = sample(&mut StdRng::seed_from_u64(seed));
            // Two seeds landing on one state out of 43 quintillion is not chance.
            assert!(states.insert(key(&s)), "seed {seed} repeated an earlier state");
            for (position, &piece) in s.cp.iter().enumerate() {
                corners[position * CORNERS + usize::from(piece)] += 1;
            }
            for (position, &piece) in s.ep.iter().enumerate() {
                edges[position * EDGES + usize::from(piece)] += 1;
            }
            twists += s.co.iter().copied().map(u64::from).sum::<u64>();
            flips += s.eo.iter().copied().map(u64::from).sum::<u64>();
        }
        // An eighth and a twelfth of four thousand: 500 and 333. A shuffle that is legal
        // but biased, the classic `0..len` in place of `0..=i`, skews these rather than the
        // means below, which is the whole reason the tally is here.
        let (low, high) = spread(&corners);
        assert!(
            (400..=600).contains(&low) && (400..=600).contains(&high),
            "corner placements ran {low} to {high} in {SAMPLE_SEEDS}, expected about 500"
        );
        let (low, high) = spread(&edges);
        assert!(
            (255..=415).contains(&low) && (255..=415).contains(&high),
            "edge placements ran {low} to {high} in {SAMPLE_SEEDS}, expected about 333"
        );
        // A uniform trit averages 1.0 and a uniform bit 0.5, carried slots included.
        let twist_mean = twists as f64 / (SAMPLE_SEEDS * CORNERS as u64) as f64;
        let flip_mean = flips as f64 / (SAMPLE_SEEDS * EDGES as u64) as f64;
        assert!(
            (0.93..=1.07).contains(&twist_mean),
            "mean corner twist {twist_mean}, expected about 1.0"
        );
        assert!(
            (0.46..=0.54).contains(&flip_mean),
            "mean edge flip {flip_mean}, expected about 0.5"
        );
    }

    #[test]
    fn the_same_seed_draws_the_same_state_and_other_seeds_do_not() {
        let first = sample(&mut StdRng::seed_from_u64(7));
        assert_eq!(sample(&mut StdRng::seed_from_u64(7)), first, "seeded draws must agree");
        let others: Vec<Cubies> =
            (8..20).map(|seed| sample(&mut StdRng::seed_from_u64(seed))).collect();
        assert!(others.iter().all(|other| *other != first), "another seed repeated the state");
        assert!(others.windows(2).any(|w| w[0] != w[1]), "every seed drew the same state");
    }

    // ---- emission

    #[test]
    fn the_power_stride_agrees_with_the_cubie_models_numbering() {
        // `POWERS` is the one fact this file assumes about how `cubies` indexes a move, so
        // it is pinned against the tokens rather than trusted.
        assert_eq!(cubies::N_MOVES % POWERS, 0, "the eighteen moves must divide into faces");
        for face in 0..cubies::N_MOVES / POWERS {
            let letter = cubies::token(face * POWERS);
            assert_eq!(letter.len(), 1, "the first power of a face is a bare letter: {letter:?}");
            assert_eq!(cubies::token(face * POWERS + 1), format!("{letter}2"));
            assert_eq!(cubies::token(face * POWERS + 2), format!("{letter}'"));
        }
    }

    #[test]
    fn the_undoing_move_shares_its_face_and_puts_the_cube_back() {
        for mv in 0..cubies::N_MOVES {
            let back = undo(mv);
            assert_eq!(undo(back), mv, "{} does not undo its own undoing", cubies::token(mv));
            assert_eq!(back / POWERS, mv / POWERS, "{} changed face", cubies::token(mv));
            assert_eq!(mv % POWERS + back % POWERS, POWERS - 1, "the powers must sum to four");
            let turned = cubies::apply_move(&SOLVED, mv);
            assert_eq!(
                cubies::apply_move(&turned, back),
                SOLVED,
                "{} then {} moved the cube",
                cubies::token(mv),
                cubies::token(back)
            );
        }
    }

    #[test]
    fn emission_reverses_the_order_and_inverts_every_power() {
        assert_eq!(emit(&[]), "", "an empty solution emits nothing");
        assert_eq!(emit(&[move_of("U")]), "U'");
        assert_eq!(emit(&[move_of("U'")]), "U");
        assert_eq!(emit(&[move_of("U2")]), "U2");
        // A solution of U R2 F' is undone by F R2 U', which is the scramble behind it.
        assert_eq!(emit(&[move_of("U"), move_of("R2"), move_of("F'")]), "F R2 U'");
        let solution = [move_of("B"), move_of("L2"), move_of("D'"), move_of("R")];
        assert_eq!(emit(&solution), "R' D L2 B'");
    }

    #[test]
    fn the_emitted_frame_is_single_spaced_with_one_token_per_move() {
        let mut rng = StdRng::seed_from_u64(19);
        for round in 0..EMIT_ROUNDS {
            let solution = random_solution(&mut rng);
            let text = emit(&solution);
            assert_eq!(text.trim(), text, "round {round}: stray whitespace in {text:?}");
            assert!(!text.contains("  "), "round {round}: double space in {text:?}");
            let tokens: Vec<&str> =
                if text.is_empty() { Vec::new() } else { text.split(' ').collect() };
            assert_eq!(tokens.len(), solution.len(), "round {round}: token count in {text:?}");
            for (i, token) in tokens.iter().enumerate() {
                let mv = solution[solution.len() - 1 - i];
                assert_eq!(
                    move_of(token),
                    undo(mv),
                    "round {round}: token {i} of {text:?} does not undo {}",
                    cubies::token(mv)
                );
            }
        }
    }

    #[test]
    fn a_sequence_and_the_emitted_inverse_leave_any_state_where_it_was() {
        // The inversion is what makes a scramble out of a solution, so it is checked on the
        // cube rather than on the tokens: an off-by-one in the power arithmetic solves fine
        // and scrambles wrongly.
        let mut rng = StdRng::seed_from_u64(23);
        for round in 0..EMIT_ROUNDS {
            let start = sample(&mut rng);
            let solution = random_solution(&mut rng);
            let mut state = solution.iter().fold(start, |s, &mv| cubies::apply_move(&s, mv));
            for token in emit(&solution).split_whitespace() {
                state = cubies::apply_move(&state, move_of(token));
            }
            assert_eq!(state, start, "round {round}: {solution:?} was not undone");
        }
    }
}
