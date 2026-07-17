//! Scramble generation in WCA notation, one generator per puzzle family.
//!
//! Seven events scramble by random moves, from the modules below. The 2x2, the 3x3,
//! one-handed, Pyraminx and Skewb are random-state and come from `crate::solver`, which draws
//! a uniformly random legal state and emits the moves that reach it. Either way [`generate`]
//! and [`generate_with_rng`] are the only way in and always hand back a scramble.

mod clock;
mod cube;
mod megaminx;
mod square1;

use crate::types::Puzzle;
use rand::Rng;

/// Scramble for a puzzle, drawing randomness from the thread-local generator.
pub fn generate(puzzle: Puzzle) -> String {
    let mut rng = rand::thread_rng();
    generate_with_rng(puzzle, &mut rng)
}

/// The same scramble, drawing randomness from `rng` so a seed always repeats.
pub fn generate_with_rng<R: Rng>(puzzle: Puzzle, rng: &mut R) -> String {
    match puzzle {
        // The events `solver` answers for, whose scrambles are random-state. One-handed is a
        // 3x3 with a hand behind your back and takes the same 3x3 path, so it comes through
        // here rather than being mapped onto `Puzzle::Cube3` first.
        Puzzle::Cube2 | Puzzle::Cube3 | Puzzle::Oh | Puzzle::Pyraminx | Puzzle::Skewb => {
            random_state(puzzle, rng)
        }
        Puzzle::Cube4 | Puzzle::Cube5 | Puzzle::Cube6 | Puzzle::Cube7 => {
            cube::scramble(puzzle, rng)
        }
        Puzzle::Megaminx => megaminx::scramble(rng),
        Puzzle::Square1 => square1::scramble(rng),
        Puzzle::Clock => clock::scramble(rng),
    }
}

/// The random-state scramble for one of the events `solver` knows how to solve.
///
/// `solver::scramble` returns None for every other event, and the arm above is what makes
/// that case unreachable, so nothing outside this module ever sees an `Option`.
fn random_state<R: Rng>(puzzle: Puzzle, rng: &mut R) -> String {
    crate::solver::scramble(puzzle, rng)
        .unwrap_or_else(|| unreachable!("{} is not a random-state event", puzzle.name()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Puzzle;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    /// What a 3x3 scramble aims at, and what the solver can promise if it misses.
    ///
    /// The two-phase search targets 21 moves and falls back to the sum of its two phase maxima
    /// for the rare state it cannot split that small, measured at 9 in 20,000. So 21 is the
    /// number to assert a share against and 30 is the one to assert outright.
    const CUBE3_TARGET: usize = 21;
    const CUBE3_CEILING: usize = 30;

    #[test]
    fn every_puzzle_produces_a_non_empty_scramble() {
        for puzzle in Puzzle::ALL {
            let s = generate_with_rng(puzzle, &mut StdRng::seed_from_u64(7));
            assert!(!s.is_empty(), "{} produced no scramble", puzzle.name());
            assert_eq!(s.trim(), s, "{} left stray whitespace", puzzle.name());
        }
    }

    #[test]
    fn every_puzzle_is_deterministic_under_a_seed() {
        for puzzle in Puzzle::ALL {
            let a = generate_with_rng(puzzle, &mut StdRng::seed_from_u64(99));
            let b = generate_with_rng(puzzle, &mut StdRng::seed_from_u64(99));
            assert_eq!(a, b, "{} is not seed-stable", puzzle.name());
        }
    }

    #[test]
    fn thread_rng_generate_works_for_every_puzzle() {
        for puzzle in Puzzle::ALL {
            assert!(!generate(puzzle).is_empty(), "{}", puzzle.name());
        }
    }

    #[test]
    fn one_handed_scrambles_exactly_like_a_3x3() {
        for seed in 0..50u64 {
            let oh = generate_with_rng(Puzzle::Oh, &mut StdRng::seed_from_u64(seed));
            let cube3 = generate_with_rng(Puzzle::Cube3, &mut StdRng::seed_from_u64(seed));
            assert_eq!(oh, cube3, "seed {seed} produced a different one-handed scramble");
        }
    }

    /// The tokens of a scramble, asserting the frame every event's notation shares.
    fn tokens(puzzle: Puzzle, seed: u64) -> Vec<String> {
        let s = generate_with_rng(puzzle, &mut StdRng::seed_from_u64(seed));
        assert_eq!(s.trim(), s, "{} left stray whitespace: {s:?}", puzzle.name());
        assert!(!s.contains("  "), "{} doubled a space: {s:?}", puzzle.name());
        s.split(' ').map(str::to_owned).collect()
    }

    // The four pins below are on the dispatch rather than on `solver`: they are what says the
    // random-state events are wired to the right generator and not merely that it works.

    #[test]
    fn a_2x2_scramble_arrives_as_eleven_moves_of_u_r_and_f() {
        for seed in 0..40u64 {
            let tokens = tokens(Puzzle::Cube2, seed);
            assert_eq!(tokens.len(), 11, "seed {seed} gave {tokens:?}");
            for token in &tokens {
                let (face, suffix) = token.split_at(1);
                assert!(matches!(face, "U" | "R" | "F"), "{token} is not a 2x2 face");
                assert!(matches!(suffix, "" | "'" | "2"), "{token} is not 2x2 notation");
            }
        }
    }

    #[test]
    fn a_skewb_scramble_arrives_as_eleven_turns_of_r_u_l_and_b() {
        for seed in 0..40u64 {
            let tokens = tokens(Puzzle::Skewb, seed);
            assert_eq!(tokens.len(), 11, "seed {seed} gave {tokens:?}");
            for token in &tokens {
                let (corner, suffix) = token.split_at(1);
                assert!(matches!(corner, "R" | "U" | "L" | "B"), "{token} is not a Skewb corner");
                // A corner turn is a third of a turn, so `2` is not Skewb notation.
                assert!(matches!(suffix, "" | "'"), "{token} carries a suffix Skewb never uses");
            }
        }
    }

    #[test]
    fn a_pyraminx_scramble_arrives_as_eleven_core_turns_and_up_to_four_tips() {
        for seed in 0..40u64 {
            let tokens = tokens(Puzzle::Pyraminx, seed);
            let core = tokens.iter().take_while(|t| t.starts_with(['U', 'L', 'R', 'B'])).count();
            assert_eq!(core, 11, "seed {seed} gave {tokens:?}");
            let tips = &tokens[core..];
            assert!(tips.len() <= 4, "seed {seed} turned a tip twice: {tokens:?}");
            for token in tokens.iter() {
                let (vertex, suffix) = token.split_at(1);
                assert!(
                    matches!(vertex, "U" | "L" | "R" | "B" | "u" | "l" | "r" | "b"),
                    "{token} is not a Pyraminx vertex"
                );
                assert!(matches!(suffix, "" | "'"), "{token} carries a suffix Pyraminx never uses");
            }
            // Tips come last, in u l r b order, one token each; strictly increasing says both.
            let order: Vec<usize> =
                tips.iter().map(|t| "ulrb".find(|c: char| t.starts_with(c)).unwrap_or(4)).collect();
            for pair in order.windows(2) {
                assert!(pair[0] < pair[1], "tips out of order in {tokens:?}");
            }
        }
    }

    #[test]
    fn a_3x3_scramble_arrives_as_about_twenty_moves_of_the_six_faces() {
        // Both events that take the two-phase solver, because the dispatch sends them through
        // it separately. The length is not fixed the way the tabled puzzles' is: a random state
        // needs whatever it needs, so the pins are the ceiling, the share inside the target and
        // the mean below.
        const SEEDS: u64 = 100;
        for puzzle in [Puzzle::Cube3, Puzzle::Oh] {
            let mut total = 0usize;
            let mut inside = 0usize;
            for seed in 0..SEEDS {
                let tokens = tokens(puzzle, seed);
                assert!(
                    (1..=CUBE3_CEILING).contains(&tokens.len()),
                    "{} seed {seed} gave {} moves: {tokens:?}",
                    puzzle.name(),
                    tokens.len()
                );
                inside += usize::from(tokens.len() <= CUBE3_TARGET);
                total += tokens.len();
                for token in &tokens {
                    let (face, suffix) = token.split_at(1);
                    assert!(
                        matches!(face, "U" | "D" | "L" | "R" | "F" | "B"),
                        "{token} is not a 3x3 outer face"
                    );
                    assert!(
                        matches!(suffix, "" | "'" | "2"),
                        "{token} carries a suffix 3x3 notation does not use"
                    );
                }
                // Two turns of one face are one turn of it, so a scramble that names a face
                // twice running has lost a move somewhere in the solver or the emitter.
                let faces: Vec<&str> = tokens.iter().map(|t| t.split_at(1).0).collect();
                assert!(
                    faces.windows(2).all(|pair| pair[0] != pair[1]),
                    "{} seed {seed} turns one face twice running: {tokens:?}",
                    puzzle.name()
                );
            }
            // A uniformly random state almost never falls within 17 moves of solved, so a mean
            // this side of 17.5 means the sampler is drawing something other than a real state.
            let mean = total as f64 / SEEDS as f64;
            assert!(
                mean > 17.5,
                "{} averaged {mean} moves over {SEEDS} seeds, too short for a random state",
                puzzle.name()
            );
            // The fallback past the target is the rare exception, not a second normal path.
            assert!(
                inside * 20 >= SEEDS as usize * 19,
                "{} kept only {inside} of {SEEDS} scrambles inside {CUBE3_TARGET} moves",
                puzzle.name()
            );
        }
    }
}
