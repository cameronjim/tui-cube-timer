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
}
