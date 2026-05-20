//! Random-move scramble generation in WCA notation.
//!
//! A scramble is a sequence of moves separated by single spaces. Each move is a
//! *move type* (a face letter plus an optional layer-width prefix/suffix, e.g.
//! `R`, `Rw`, `3Rw`) followed by one of the three suffixes `` (none), `'`, `2`,
//! chosen uniformly.
//!
//! Legality rules enforced while generating (see `INTERFACES.md`):
//!   1. Consecutive moves must not use the same face letter with the same width.
//!   2. No three consecutive moves on the same axis (axes: U/D, L/R, F/B).
//!   3. If move[i] and move[i-1] share an axis, move[i] must not repeat
//!      move[i-2]'s face+width.

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

/// 2x2: outer faces U, R, F only.
const POOL_2: &[MoveType] = &[U, R, F];
/// 3x3: the six outer faces.
const POOL_3: &[MoveType] = &[U, D, L, R, F, B];
/// 4x4: six outer faces + Uw Rw Fw.
const POOL_4: &[MoveType] = &[U, D, L, R, F, B, UW, RW, FW];
/// 5x5: six outer faces + all six wide moves.
const POOL_5: &[MoveType] = &[U, D, L, R, F, B, UW, DW, LW, RW, FW, BW];
/// 6x6: 5x5 pool + 3Uw 3Rw 3Fw.
const POOL_6: &[MoveType] = &[U, D, L, R, F, B, UW, DW, LW, RW, FW, BW, U3W, R3W, F3W];
/// 7x7: 5x5 pool + all six triple-wide moves.
const POOL_7: &[MoveType] = &[
    U, D, L, R, F, B, UW, DW, LW, RW, FW, BW, U3W, D3W, L3W, R3W, F3W, B3W,
];

const SUFFIXES: [&str; 3] = ["", "'", "2"];

/// The move pool for a puzzle.
fn pool(puzzle: Puzzle) -> &'static [MoveType] {
    match puzzle {
        Puzzle::Cube2 => POOL_2,
        Puzzle::Cube3 => POOL_3,
        Puzzle::Cube4 => POOL_4,
        Puzzle::Cube5 => POOL_5,
        Puzzle::Cube6 => POOL_6,
        Puzzle::Cube7 => POOL_7,
    }
}

/// Number of moves to generate. 2x2 is 9-11 (random), everything else is fixed.
fn move_count<R: Rng>(puzzle: Puzzle, rng: &mut R) -> usize {
    match puzzle {
        Puzzle::Cube2 => rng.gen_range(9..=11),
        Puzzle::Cube3 => 20,
        Puzzle::Cube4 => 44,
        Puzzle::Cube5 => 60,
        Puzzle::Cube6 => 80,
        Puzzle::Cube7 => 100,
    }
}

/// Whether `candidate` may follow `prev` (the immediately preceding move type)
/// and `prev2` (the one before that).
fn is_legal(candidate: MoveType, prev: Option<MoveType>, prev2: Option<MoveType>) -> bool {
    let prev = match prev {
        Some(p) => p,
        // First move: anything goes.
        None => return true,
    };

    // Rule 1: no same face letter + same width twice in a row.
    if candidate.same_layer(prev) {
        return false;
    }

    if candidate.axis() != prev.axis() {
        return true;
    }

    if let Some(prev2) = prev2 {
        // Rule 2: no three consecutive moves on the same axis.
        if prev2.axis() == candidate.axis() {
            return false;
        }
        // Rule 3: sharing an axis with the previous move forbids repeating
        // move[i-2]'s face+width.
        if candidate.same_layer(prev2) {
            return false;
        }
    }

    true
}

/// Random-move scramble in WCA notation, moves separated by single spaces.
///
/// Uses [`rand::thread_rng`].
pub fn generate(puzzle: Puzzle) -> String {
    let mut rng = rand::thread_rng();
    generate_with_rng(puzzle, &mut rng)
}

/// Random-move scramble in WCA notation, moves separated by single spaces,
/// drawing randomness from `rng` (deterministic for a seeded generator).
pub fn generate_with_rng<R: Rng>(puzzle: Puzzle, rng: &mut R) -> String {
    let pool = pool(puzzle);
    let count = move_count(puzzle, rng);

    let mut out = String::with_capacity(count * 4);
    let mut candidates: Vec<MoveType> = Vec::with_capacity(pool.len());
    let mut prev: Option<MoveType> = None;
    let mut prev2: Option<MoveType> = None;

    for i in 0..count {
        candidates.clear();
        candidates.extend(pool.iter().copied().filter(|m| is_legal(*m, prev, prev2)));
        // Every supported pool always leaves at least one legal continuation.
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

        prev2 = prev;
        prev = Some(chosen);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    /// Parse a scramble token like "3Rw'" into (move type name, suffix).
    fn split_token(token: &str) -> (&str, &str) {
        if let Some(base) = token.strip_suffix('\'') {
            (base, "'")
        } else if let Some(base) = token.strip_suffix('2') {
            // Careful: "2" is never part of a move-type name in our pools
            // (the only digit-prefixed names are "3Xw").
            (base, "2")
        } else {
            (token, "")
        }
    }

    fn lookup(name: &str) -> Option<MoveType> {
        POOL_7.iter().copied().find(|m| m.name == name)
    }

    /// Decode a scramble into its move types, asserting notation validity along
    /// the way, and asserting every move comes from `pool`.
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
                let mv = lookup(name)
                    .unwrap_or_else(|| panic!("unknown move {name:?} in {scramble:?}"));
                assert!(
                    pool.contains(&mv),
                    "move {name:?} is outside this puzzle's pool ({scramble:?})"
                );
                mv
            })
            .collect()
    }

    /// Assert rules 1-3 hold across the whole sequence.
    fn assert_constraints(moves: &[MoveType], scramble: &str) {
        for i in 1..moves.len() {
            // Rule 1.
            assert!(
                !moves[i].same_layer(moves[i - 1]),
                "rule 1 violated at index {i} in {scramble:?}"
            );
        }
        for i in 2..moves.len() {
            // Rule 2.
            assert!(
                !(moves[i].axis() == moves[i - 1].axis() && moves[i].axis() == moves[i - 2].axis()),
                "rule 2 violated at index {i} in {scramble:?}"
            );
            // Rule 3.
            if moves[i].axis() == moves[i - 1].axis() {
                assert!(
                    !moves[i].same_layer(moves[i - 2]),
                    "rule 3 violated at index {i} in {scramble:?}"
                );
            }
        }
    }

    fn expected_len(puzzle: Puzzle) -> (usize, usize) {
        match puzzle {
            Puzzle::Cube2 => (9, 11),
            Puzzle::Cube3 => (20, 20),
            Puzzle::Cube4 => (44, 44),
            Puzzle::Cube5 => (60, 60),
            Puzzle::Cube6 => (80, 80),
            Puzzle::Cube7 => (100, 100),
        }
    }

    #[test]
    fn lengths_pool_and_constraints_hold_over_many_generations() {
        for puzzle in Puzzle::ALL {
            let (lo, hi) = expected_len(puzzle);
            for seed in 0..400u64 {
                let mut rng = StdRng::seed_from_u64(seed);
                let scramble = generate_with_rng(puzzle, &mut rng);
                let moves = decode(&scramble, pool(puzzle));
                assert!(
                    moves.len() >= lo && moves.len() <= hi,
                    "{} produced {} moves (expected {lo}..={hi}): {scramble:?}",
                    puzzle.name(),
                    moves.len()
                );
                assert_constraints(&moves, &scramble);
            }
        }
    }

    #[test]
    fn two_by_two_length_varies_within_nine_to_eleven() {
        let mut seen = [false; 3]; // 9, 10, 11
        for seed in 0..200u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let scramble = generate_with_rng(Puzzle::Cube2, &mut rng);
            let n = scramble.split(' ').count();
            assert!((9..=11).contains(&n), "unexpected 2x2 length {n}");
            seen[n - 9] = true;
        }
        assert!(seen.iter().all(|s| *s), "2x2 never produced all of 9/10/11");
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
            for token in generate_with_rng(Puzzle::Cube3, &mut rng)
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
        for puzzle in Puzzle::ALL {
            let a = generate_with_rng(puzzle, &mut StdRng::seed_from_u64(1234));
            let b = generate_with_rng(puzzle, &mut StdRng::seed_from_u64(1234));
            assert_eq!(a, b, "same seed must give the same scramble");
            let c = generate_with_rng(puzzle, &mut StdRng::seed_from_u64(4321));
            assert_ne!(a, c, "different seeds should give different scrambles");
        }
    }

    #[test]
    fn thread_rng_generate_is_well_formed() {
        for puzzle in Puzzle::ALL {
            let scramble = generate(puzzle);
            let moves = decode(&scramble, pool(puzzle));
            let (lo, hi) = expected_len(puzzle);
            assert!(moves.len() >= lo && moves.len() <= hi);
            assert_constraints(&moves, &scramble);
        }
    }

    #[test]
    fn legality_predicate_rejects_illegal_sequences() {
        // Rule 1: same face + same width back to back.
        assert!(!is_legal(R, Some(R), None));
        assert!(!is_legal(RW, Some(RW), Some(U)));
        // Different width on the same face is a distinct move type (rule 1 only).
        assert!(is_legal(RW, Some(R), None));
        // Opposite face on the same axis is fine as a pair.
        assert!(is_legal(L, Some(R), Some(U)));
        // Rule 2: three in a row on the L/R axis.
        assert!(!is_legal(RW, Some(L), Some(R)));
        assert!(!is_legal(L, Some(R), Some(LW)));
        // Different axis is always fine after rule 1 passes.
        assert!(is_legal(U, Some(R), Some(U)));
        // First and second moves are unconstrained beyond rule 1.
        assert!(is_legal(U, None, None));
    }
}
