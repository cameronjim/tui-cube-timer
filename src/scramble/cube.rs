//! Random-move scrambles for the NxN cubes, 2x2 through 7x7.

use crate::types::Puzzle;
use rand::Rng;

/// A turnable layer group: a face letter plus a layer width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MoveType {
    /// Notation without the suffix, e.g. "R", "Rw", "3Rw".
    name: &'static str,
    /// Face letter: one of `U D L R F B`.
    face: u8,
    /// 1 = outer layer, 2 = two layers (`w`), 3 = three layers (`3..w`).
    width: u8,
}

impl MoveType {
    const fn new(name: &'static str, face: u8, width: u8) -> MoveType {
        MoveType { name, face, width }
    }

    /// 0 = U/D, 1 = L/R, 2 = F/B.
    const fn axis(self) -> u8 {
        match self.face {
            b'U' | b'D' => 0,
            b'L' | b'R' => 1,
            _ => 2,
        }
    }

    /// Same face letter *and* same layer width.
    const fn same_layer(self, other: MoveType) -> bool {
        self.face == other.face && self.width == other.width
    }
}

const U: MoveType = MoveType::new("U", b'U', 1);
const D: MoveType = MoveType::new("D", b'D', 1);
const L: MoveType = MoveType::new("L", b'L', 1);
const R: MoveType = MoveType::new("R", b'R', 1);
const F: MoveType = MoveType::new("F", b'F', 1);
const B: MoveType = MoveType::new("B", b'B', 1);

const UW: MoveType = MoveType::new("Uw", b'U', 2);
const DW: MoveType = MoveType::new("Dw", b'D', 2);
const LW: MoveType = MoveType::new("Lw", b'L', 2);
const RW: MoveType = MoveType::new("Rw", b'R', 2);
const FW: MoveType = MoveType::new("Fw", b'F', 2);
const BW: MoveType = MoveType::new("Bw", b'B', 2);

const U3W: MoveType = MoveType::new("3Uw", b'U', 3);
const D3W: MoveType = MoveType::new("3Dw", b'D', 3);
const L3W: MoveType = MoveType::new("3Lw", b'L', 3);
const R3W: MoveType = MoveType::new("3Rw", b'R', 3);
const F3W: MoveType = MoveType::new("3Fw", b'F', 3);
const B3W: MoveType = MoveType::new("3Bw", b'B', 3);

/// 2x2: outer faces U, R, F only, because a 1-of-2 turn is half the cube.
const POOL_2: &[MoveType] = &[U, R, F];
/// 3x3: the six outer faces.
const POOL_3: &[MoveType] = &[U, D, L, R, F, B];
/// 4x4: six outer faces + Uw Rw Fw, because a 2-of-4 turn is half the cube.
const POOL_4: &[MoveType] = &[U, D, L, R, F, B, UW, RW, FW];
/// 5x5: six outer faces + all six wide moves, since 2 of 5 is never half.
const POOL_5: &[MoveType] = &[U, D, L, R, F, B, UW, DW, LW, RW, FW, BW];
/// 6x6: 5x5 pool + 3Uw 3Rw 3Fw, because a 3-of-6 turn is half the cube.
const POOL_6: &[MoveType] = &[U, D, L, R, F, B, UW, DW, LW, RW, FW, BW, U3W, R3W, F3W];
/// 7x7: 5x5 pool + all six triple-wides, since neither 2 nor 3 of 7 is half.
const POOL_7: &[MoveType] = &[
    U, D, L, R, F, B, UW, DW, LW, RW, FW, BW, U3W, D3W, L3W, R3W, F3W, B3W,
];

const SUFFIXES: [&str; 3] = ["", "'", "2"];

/// The move pool for a cube. Only the six cube variants ever reach this module.
fn pool(puzzle: Puzzle) -> &'static [MoveType] {
    match puzzle {
        Puzzle::Cube2 => POOL_2,
        Puzzle::Cube3 => POOL_3,
        Puzzle::Cube4 => POOL_4,
        Puzzle::Cube5 => POOL_5,
        Puzzle::Cube6 => POOL_6,
        Puzzle::Cube7 => POOL_7,
        other => unreachable!("{} is not an NxN cube", other.name()),
    }
}

/// Number of moves to generate, matching what TNoodle emits for each event.
fn move_count(puzzle: Puzzle) -> usize {
    match puzzle {
        Puzzle::Cube2 => 11,
        Puzzle::Cube3 => 20,
        Puzzle::Cube4 => 44,
        Puzzle::Cube5 => 60,
        Puzzle::Cube6 => 80,
        Puzzle::Cube7 => 100,
        other => unreachable!("{} is not an NxN cube", other.name()),
    }
}

/// Whether `candidate` may follow `run`, the trailing block of same-axis moves.
fn is_legal(candidate: MoveType, run: &[MoveType]) -> bool {
    match run.first() {
        Some(first) if first.axis() == candidate.axis() => {
            !run.iter().any(|m| m.same_layer(candidate))
        }
        // A move on a fresh axis starts a new block and is always legal.
        _ => true,
    }
}

/// Random-move cube scramble in WCA notation, moves separated by single spaces.
pub fn scramble<R: Rng>(puzzle: Puzzle, rng: &mut R) -> String {
    let pool = pool(puzzle);
    let count = move_count(puzzle);

    let mut out = String::with_capacity(count * 4);
    let mut candidates: Vec<MoveType> = Vec::with_capacity(pool.len());
    let mut run: Vec<MoveType> = Vec::with_capacity(pool.len());

    for i in 0..count {
        candidates.clear();
        candidates.extend(pool.iter().copied().filter(|m| is_legal(*m, &run)));
        // Every pool spans at least two axes, so a legal move always exists.
        debug_assert!(!candidates.is_empty());
        if candidates.is_empty() {
            break;
        }

        let chosen = candidates[rng.gen_range(0..candidates.len())];
        let suffix = SUFFIXES[rng.gen_range(0..SUFFIXES.len())];

        if i > 0 {
            out.push(' ');
        }
        out.push_str(chosen.name);
        out.push_str(suffix);

        if run.first().is_some_and(|m| m.axis() != chosen.axis()) {
            run.clear();
        }
        run.push(chosen);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scramble::generate;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    /// The puzzles this module answers for.
    const CUBES: [Puzzle; 6] = [
        Puzzle::Cube2,
        Puzzle::Cube3,
        Puzzle::Cube4,
        Puzzle::Cube5,
        Puzzle::Cube6,
        Puzzle::Cube7,
    ];

    /// Parse a scramble token like "3Rw'" into (move type name, suffix).
    fn split_token(token: &str) -> (&str, &str) {
        if let Some(base) = token.strip_suffix('\'') {
            (base, "'")
        } else if let Some(base) = token.strip_suffix('2') {
            // Safe because no move-type name ends in a digit.
            (base, "2")
        } else {
            (token, "")
        }
    }

    fn lookup(name: &str) -> Option<MoveType> {
        POOL_7.iter().copied().find(|m| m.name == name)
    }

    /// Decode a scramble into move types, asserting notation and pool validity.
    fn decode(scramble: &str, pool: &[MoveType]) -> Vec<MoveType> {
        assert!(!scramble.is_empty(), "scramble must not be empty");
        assert!(
            !scramble.contains("  "),
            "moves must be separated by a single space: {scramble:?}"
        );
        assert_eq!(scramble.trim(), scramble, "no leading/trailing whitespace");

        scramble
            .split(' ')
            .map(|token| {
                let (name, suffix) = split_token(token);
                assert!(
                    SUFFIXES.contains(&suffix),
                    "bad suffix in token {token:?} of {scramble:?}"
                );
                let mv =
                    lookup(name).unwrap_or_else(|| panic!("unknown move {name:?} in {scramble:?}"));
                assert!(
                    pool.contains(&mv),
                    "move {name:?} is outside this puzzle's pool ({scramble:?})"
                );
                mv
            })
            .collect()
    }

    /// The trailing run of same-axis moves ending at `end` (exclusive).
    fn trailing_run(moves: &[MoveType], end: usize) -> &[MoveType] {
        let axis = match moves.get(end.wrapping_sub(1)) {
            Some(m) => m.axis(),
            None => return &[],
        };
        let mut start = end;
        while start > 0 && moves[start - 1].axis() == axis {
            start -= 1;
        }
        &moves[start..end]
    }

    /// Assert no face+width repeats inside any block of same-axis moves.
    fn assert_constraints(moves: &[MoveType], scramble: &str) {
        for i in 1..moves.len() {
            let run = trailing_run(moves, i);
            assert!(
                is_legal(moves[i], run),
                "illegal move at index {i} in {scramble:?}"
            );
        }
    }

    fn expected_len(puzzle: Puzzle) -> usize {
        match puzzle {
            Puzzle::Cube2 => 11,
            Puzzle::Cube3 => 20,
            Puzzle::Cube4 => 44,
            Puzzle::Cube5 => 60,
            Puzzle::Cube6 => 80,
            Puzzle::Cube7 => 100,
            other => unreachable!("{} is not an NxN cube", other.name()),
        }
    }

    #[test]
    fn lengths_pool_and_constraints_hold_over_many_generations() {
        for puzzle in CUBES {
            let want = expected_len(puzzle);
            for seed in 0..400u64 {
                let mut rng = StdRng::seed_from_u64(seed);
                let s = scramble(puzzle, &mut rng);
                let moves = decode(&s, pool(puzzle));
                assert_eq!(
                    moves.len(),
                    want,
                    "{} produced the wrong move count: {s:?}",
                    puzzle.name()
                );
                assert_constraints(&moves, &s);
            }
        }
    }

    #[test]
    fn two_by_two_is_always_eleven_moves_of_u_r_f() {
        for seed in 0..200u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let s = scramble(Puzzle::Cube2, &mut rng);
            let moves = decode(&s, POOL_2);
            assert_eq!(moves.len(), 11, "2x2 must be exactly 11 moves: {s:?}");
        }
    }

    #[test]
    fn pools_match_the_spec() {
        let names = |p: Puzzle| -> Vec<&'static str> { pool(p).iter().map(|m| m.name).collect() };
        assert_eq!(names(Puzzle::Cube2), ["U", "R", "F"]);
        assert_eq!(names(Puzzle::Cube3), ["U", "D", "L", "R", "F", "B"]);
        assert_eq!(
            names(Puzzle::Cube4),
            ["U", "D", "L", "R", "F", "B", "Uw", "Rw", "Fw"]
        );
        assert_eq!(
            names(Puzzle::Cube5),
            ["U", "D", "L", "R", "F", "B", "Uw", "Dw", "Lw", "Rw", "Fw", "Bw"]
        );
        assert_eq!(
            names(Puzzle::Cube6),
            [
                "U", "D", "L", "R", "F", "B", "Uw", "Dw", "Lw", "Rw", "Fw", "Bw", "3Uw", "3Rw",
                "3Fw"
            ]
        );
        assert_eq!(
            names(Puzzle::Cube7),
            [
                "U", "D", "L", "R", "F", "B", "Uw", "Dw", "Lw", "Rw", "Fw", "Bw", "3Uw", "3Dw",
                "3Lw", "3Rw", "3Fw", "3Bw"
            ]
        );
    }

    #[test]
    fn all_three_suffixes_are_used() {
        let mut plain = false;
        let mut prime = false;
        let mut double = false;
        for seed in 0..50u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            for token in scramble(Puzzle::Cube3, &mut rng)
                .split(' ')
                .map(|t| split_token(t).1)
            {
                match token {
                    "" => plain = true,
                    "'" => prime = true,
                    "2" => double = true,
                    other => panic!("unexpected suffix {other:?}"),
                }
            }
        }
        assert!(plain && prime && double, "not all suffixes appeared");
    }

    #[test]
    fn seeded_generation_is_deterministic() {
        for puzzle in CUBES {
            let a = scramble(puzzle, &mut StdRng::seed_from_u64(1234));
            let b = scramble(puzzle, &mut StdRng::seed_from_u64(1234));
            assert_eq!(a, b, "same seed must give the same scramble");
            let c = scramble(puzzle, &mut StdRng::seed_from_u64(4321));
            assert_ne!(a, c, "different seeds should give different scrambles");
        }
    }

    #[test]
    fn thread_rng_generate_is_well_formed() {
        for puzzle in CUBES {
            let s = generate(puzzle);
            let moves = decode(&s, pool(puzzle));
            assert_eq!(moves.len(), expected_len(puzzle));
            assert_constraints(&moves, &s);
        }
    }

    #[test]
    fn legality_predicate_rejects_illegal_sequences() {
        // No repeated face+width inside a block of same-axis moves.
        assert!(!is_legal(R, &[R]));
        assert!(!is_legal(RW, &[RW]));
        assert!(!is_legal(R, &[R, L]));
        // Different widths on one face are distinct move types.
        assert!(is_legal(RW, &[R]));
        // Opposite faces on one axis are fine.
        assert!(is_legal(L, &[R]));
        // A fresh axis clears the block, so a repeat is legal again.
        assert!(is_legal(R, &[U]));
        // Three same-axis moves are legal when all three layers differ.
        assert!(is_legal(U3W, &[UW, U]));
        // The first move of a scramble is unconstrained.
        assert!(is_legal(U, &[]));
    }

    #[test]
    fn big_cubes_do_produce_runs_of_three_on_one_axis() {
        let mut seen = false;
        for seed in 0..200u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let s = scramble(Puzzle::Cube7, &mut rng);
            let moves = decode(&s, POOL_7);
            seen |= moves
                .windows(3)
                .any(|w| w[0].axis() == w[1].axis() && w[1].axis() == w[2].axis());
            if seen {
                break;
            }
        }
        assert!(seen, "TNoodle allows same-axis runs longer than two");
    }
}
