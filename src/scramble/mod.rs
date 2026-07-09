//! Random-move scramble generation in WCA notation, one generator per puzzle family.

mod clock;
mod cube;
mod megaminx;
mod pyraminx;
mod skewb;
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
        Puzzle::Cube2
        | Puzzle::Cube3
        | Puzzle::Cube4
        | Puzzle::Cube5
        | Puzzle::Cube6
        | Puzzle::Cube7 => cube::scramble(puzzle, rng),
        // One-handed is a 3x3 with a hand behind your back, so it takes the 3x3 generator
        // verbatim. Mapping it here is also what keeps `cube` a function of cube variants only.
        Puzzle::Oh => cube::scramble(Puzzle::Cube3, rng),
        Puzzle::Pyraminx => pyraminx::scramble(rng),
        Puzzle::Skewb => skewb::scramble(rng),
        Puzzle::Megaminx => megaminx::scramble(rng),
        Puzzle::Square1 => square1::scramble(rng),
        Puzzle::Clock => clock::scramble(rng),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Puzzle;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

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

    #[test]
    fn a_one_handed_scramble_is_twenty_moves_of_3x3_notation() {
        for seed in 0..50u64 {
            let s = generate_with_rng(Puzzle::Oh, &mut StdRng::seed_from_u64(seed));
            let moves: Vec<&str> = s.split(' ').collect();
            assert_eq!(moves.len(), 20, "seed {seed} gave {s}");
            for m in moves {
                let (face, suffix) = m.split_at(1);
                assert!(
                    matches!(face, "U" | "D" | "L" | "R" | "F" | "B"),
                    "{m} is not a 3x3 outer face"
                );
                assert!(
                    matches!(suffix, "" | "'" | "2"),
                    "{m} carries a suffix 3x3 notation does not use"
                );
            }
        }
    }
}
