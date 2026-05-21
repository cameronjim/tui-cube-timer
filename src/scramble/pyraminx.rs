//! Random-move Pyraminx scrambles in WCA notation.

use rand::Rng;

/// The four large layers, in TNoodle's move-table order.
const FACES: [&str; 4] = ["U", "L", "R", "B"];

/// The four tips, emitted in this order after the layer turns.
const TIPS: [&str; 4] = ["u", "l", "r", "b"];

/// A Pyraminx turn is 120 degrees, so no move ever carries a `2`.
const SUFFIXES: [&str; 2] = ["", "'"];

/// Layer turns per scramble, matching the exact length TNoodle searches for.
const LAYER_MOVES: usize = 11;

/// Orientations a tip can hold: solved, clockwise, counter-clockwise.
const TIP_STATES: usize = 3;

/// One Pyraminx scramble in WCA notation.
///
/// Eleven layer turns drawn from `U L R B`, then the tips `u l r b` in that
/// order, each appearing at most once and only when the random state leaves it
/// unsolved. That is the shape TNoodle emits: it solves a uniformly random
/// state in exactly eleven layer turns and appends one move per unsolved tip.
pub fn scramble<R: Rng>(rng: &mut R) -> String {
    let mut out = String::with_capacity(LAYER_MOVES * 3 + TIPS.len() * 3);
    let mut last: Option<usize> = None;

    for i in 0..LAYER_MOVES {
        // Turning one layer twice in a row collapses into a single turn, so
        // TNoodle's search skips it. That is the whole constraint here: all four
        // Pyraminx axes intersect, so there are no parallel layers to also
        // exclude the way a cube excludes R after L.
        let face = match last {
            Some(prev) => (prev + rng.gen_range(1..FACES.len())) % FACES.len(),
            None => rng.gen_range(0..FACES.len()),
        };

        if i > 0 {
            out.push(' ');
        }
        out.push_str(FACES[face]);
        out.push_str(SUFFIXES[rng.gen_range(0..SUFFIXES.len())]);
        last = Some(face);
    }

    for tip in TIPS {
        // A uniformly random state leaves a tip solved one time in three, and a
        // solved tip contributes no move.
        let turn = rng.gen_range(0..TIP_STATES);
        if turn == 0 {
            continue;
        }
        out.push(' ');
        out.push_str(tip);
        out.push_str(SUFFIXES[turn - 1]);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use std::collections::BTreeSet;

    /// Seeds per property test. Cheap, and one sample would miss rule breaks.
    const SEEDS: u64 = 400;

    /// A scramble from a TNoodle-generated competition sheet, used to prove the
    /// assertions below accept genuine WCA output rather than only our own.
    const OFFICIAL: &str = "U' L' B R U' B' R U L' B L' u b'";

    fn generate(seed: u64) -> String {
        scramble(&mut StdRng::seed_from_u64(seed))
    }

    fn is_tip(token: &str) -> bool {
        token.starts_with(char::is_lowercase)
    }

    /// Split a token into its letter and its suffix.
    fn split_token(token: &str) -> (&str, &str) {
        match token.strip_suffix('\'') {
            Some(base) => (base, "'"),
            None => (token, ""),
        }
    }

    /// Split a scramble into layer tokens and tip tokens, checking formatting.
    fn parse(scramble: &str) -> (Vec<&str>, Vec<&str>) {
        assert!(!scramble.is_empty(), "scramble must not be empty");
        assert_eq!(
            scramble.trim(),
            scramble,
            "no leading or trailing whitespace: {scramble:?}"
        );
        assert!(
            !scramble.contains("  "),
            "moves must be separated by a single space: {scramble:?}"
        );

        let tokens: Vec<&str> = scramble.split(' ').collect();
        let first_tip = tokens.iter().position(|t| is_tip(t)).unwrap_or(tokens.len());
        let (layers, tips) = tokens.split_at(first_tip);
        assert!(
            tips.iter().all(|t| is_tip(t)),
            "every tip must follow every layer turn: {scramble:?}"
        );
        (layers.to_vec(), tips.to_vec())
    }

    /// Every rule a Pyraminx scramble must satisfy, in one place.
    fn assert_well_formed(scramble: &str) {
        let (layers, tips) = parse(scramble);

        assert_eq!(
            layers.len(),
            LAYER_MOVES,
            "wrong layer-turn count in {scramble:?}"
        );

        let mut previous = "";
        for token in &layers {
            let (face, suffix) = split_token(token);
            assert!(
                FACES.contains(&face),
                "layer turn {token:?} is outside U L R B in {scramble:?}"
            );
            assert!(
                SUFFIXES.contains(&suffix),
                "bad suffix in {token:?} of {scramble:?}"
            );
            assert_ne!(
                face, previous,
                "one layer turned twice in a row in {scramble:?}"
            );
            previous = face;
        }

        let mut expected = TIPS.iter();
        for token in &tips {
            let (tip, suffix) = split_token(token);
            assert!(
                SUFFIXES.contains(&suffix),
                "bad suffix in tip {token:?} of {scramble:?}"
            );
            let found = expected.any(|t| *t == tip);
            assert!(
                found,
                "tip {tip:?} is unknown, out of u l r b order, or repeated in {scramble:?}"
            );
        }
    }

    #[test]
    fn generated_scrambles_are_well_formed() {
        for seed in 0..SEEDS {
            assert_well_formed(&generate(seed));
        }
    }

    #[test]
    fn an_official_scramble_is_well_formed() {
        assert_well_formed(OFFICIAL);
    }

    #[test]
    fn layer_turns_are_always_exactly_eleven() {
        for seed in 0..SEEDS {
            let scramble = generate(seed);
            let (layers, _) = parse(&scramble);
            assert_eq!(layers.len(), 11, "expected 11 layer turns in {scramble:?}");
        }
    }

    #[test]
    fn no_move_ever_carries_a_double_turn_suffix() {
        for seed in 0..SEEDS {
            let scramble = generate(seed);
            assert!(
                !scramble.contains('2'),
                "a 120 degree turn has no `2` form: {scramble:?}"
            );
        }
    }

    #[test]
    fn no_layer_turns_twice_in_a_row() {
        for seed in 0..SEEDS {
            let scramble = generate(seed);
            let (layers, _) = parse(&scramble);
            for pair in layers.windows(2) {
                assert_ne!(
                    split_token(pair[0]).0,
                    split_token(pair[1]).0,
                    "repeated layer in {scramble:?}"
                );
            }
        }
    }

    #[test]
    fn each_face_is_followed_by_all_three_others_and_never_itself() {
        for face in FACES {
            let mut followers = BTreeSet::new();
            for seed in 0..SEEDS {
                let scramble = generate(seed);
                let (layers, _) = parse(&scramble);
                for pair in layers.windows(2) {
                    if split_token(pair[0]).0 == face {
                        followers.insert(split_token(pair[1]).0.to_string());
                    }
                }
            }
            assert!(
                !followers.contains(face),
                "{face} followed itself, which TNoodle never emits"
            );
            assert_eq!(
                followers.len(),
                3,
                "{face} should be followed by each of the other three layers, saw {followers:?}"
            );
        }
    }

    #[test]
    fn tips_are_lowercase_u_l_r_b_each_at_most_once_in_order() {
        for seed in 0..SEEDS {
            let scramble = generate(seed);
            let (_, tips) = parse(&scramble);
            let positions: Vec<usize> = tips
                .iter()
                .map(|token| {
                    let (tip, _) = split_token(token);
                    TIPS.iter()
                        .position(|known| *known == tip)
                        .unwrap_or_else(|| panic!("unknown tip {tip:?} in {scramble:?}"))
                })
                .collect();
            assert!(
                positions.windows(2).all(|pair| pair[0] < pair[1]),
                "tips must be in u l r b order with no repeats: {scramble:?}"
            );
            assert!(
                positions.len() <= TIPS.len(),
                "at most four tips: {scramble:?}"
            );
        }
    }

    #[test]
    fn tip_counts_range_over_zero_through_four() {
        let mut seen = [false; 5];
        for seed in 0..SEEDS {
            let scramble = generate(seed);
            let (_, tips) = parse(&scramble);
            seen[tips.len()] = true;
        }
        assert_eq!(seen, [true; 5], "every tip count from 0 to 4 must occur");
    }

    #[test]
    fn a_tip_is_solved_roughly_one_time_in_three() {
        let mut turned = 0usize;
        let mut total = 0usize;
        for seed in 0..SEEDS {
            let scramble = generate(seed);
            let (_, tips) = parse(&scramble);
            turned += tips.len();
            total += TIPS.len();
        }
        // Expected 2/3 of 1600, so about 1067. A wide band keeps this from being
        // a flaky assertion about the RNG while still pinning the model.
        assert!(
            (950..1180).contains(&turned),
            "{turned} of {total} tips turned, expected about two thirds"
        );
    }

    #[test]
    fn every_layer_and_every_direction_appears() {
        let mut faces = BTreeSet::new();
        let mut suffixes = BTreeSet::new();
        for seed in 0..SEEDS {
            let scramble = generate(seed);
            let (layers, _) = parse(&scramble);
            for token in layers {
                let (face, suffix) = split_token(token);
                faces.insert(face.to_string());
                suffixes.insert(suffix.to_string());
            }
        }
        assert_eq!(faces.len(), FACES.len(), "not every layer appeared");
        assert_eq!(suffixes.len(), SUFFIXES.len(), "not both directions appeared");
    }

    #[test]
    fn every_tip_and_every_tip_direction_appears() {
        let mut tips_seen = BTreeSet::new();
        let mut suffixes = BTreeSet::new();
        for seed in 0..SEEDS {
            let scramble = generate(seed);
            let (_, tips) = parse(&scramble);
            for token in tips {
                let (tip, suffix) = split_token(token);
                tips_seen.insert(tip.to_string());
                suffixes.insert(suffix.to_string());
            }
        }
        assert_eq!(tips_seen.len(), TIPS.len(), "not every tip appeared");
        assert_eq!(
            suffixes.len(),
            SUFFIXES.len(),
            "not both tip directions appeared"
        );
    }

    #[test]
    fn seeded_generation_is_deterministic() {
        let a = generate(1234);
        let b = generate(1234);
        assert_eq!(a, b, "same seed must give the same scramble");
        let c = generate(4321);
        assert_ne!(a, c, "different seeds should give different scrambles");
    }
}
