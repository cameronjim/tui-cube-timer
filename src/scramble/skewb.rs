//! Random-move Skewb scramble generation in WCA notation.

use rand::Rng;

/// The four corner axes a Skewb turns about, in TNoodle's `RULB` order.
///
/// Skewb scrambling holds one corner fixed, the same trick 2x2 uses, so four
/// of the eight corner axes reach every state and the other four never appear.
const AXES: [&str; 4] = ["R", "U", "L", "B"];

/// A Skewb turn is 120 degrees one way or the other, so there is no `2`.
const SUFFIXES: [&str; 2] = ["", "'"];

/// Moves per scramble. TNoodle pads every random state out to exactly this.
const MOVE_COUNT: usize = 11;

/// One Skewb scramble in WCA notation.
pub fn scramble<R: Rng>(rng: &mut R) -> String {
    let mut out = String::with_capacity(MOVE_COUNT * 3);
    let mut last: Option<usize> = None;

    for i in 0..MOVE_COUNT {
        // No Skewb axis is opposite or parallel to another, so the only
        // redundancy TNoodle's search prunes is a repeat of the previous axis.
        let axis = match last {
            Some(prev) => {
                // Draw from the three survivors, then step over the excluded
                // one, which keeps all three equally likely.
                let pick = rng.gen_range(0..AXES.len() - 1);
                if pick >= prev {
                    pick + 1
                } else {
                    pick
                }
            }
            None => rng.gen_range(0..AXES.len()),
        };
        debug_assert!(last != Some(axis));

        if i > 0 {
            out.push(' ');
        }
        out.push_str(AXES[axis]);
        out.push_str(SUFFIXES[rng.gen_range(0..SUFFIXES.len())]);
        last = Some(axis);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    /// Split a token like "R'" into (axis letter, suffix).
    fn split_token(token: &str) -> (&str, &str) {
        match token.strip_suffix('\'') {
            Some(base) => (base, "'"),
            None => (token, ""),
        }
    }

    /// Decode a scramble into axis indices, asserting notation validity.
    fn decode(scramble: &str) -> Vec<usize> {
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
                AXES.iter()
                    .position(|a| *a == name)
                    .unwrap_or_else(|| panic!("unknown move {name:?} in {scramble:?}"))
            })
            .collect()
    }

    #[test]
    fn pool_is_the_four_fixed_corner_axes() {
        assert_eq!(AXES, ["R", "U", "L", "B"]);
        assert_eq!(SUFFIXES, ["", "'"]);
        assert_eq!(MOVE_COUNT, 11);
    }

    #[test]
    fn format_pool_and_length_hold_over_many_generations() {
        for seed in 0..400u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let text = scramble(&mut rng);
            let moves = decode(&text);
            assert_eq!(
                moves.len(),
                MOVE_COUNT,
                "Skewb scrambles are exactly 11 moves: {text:?}"
            );
        }
    }

    #[test]
    fn no_axis_ever_repeats_consecutively() {
        for seed in 0..400u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let text = scramble(&mut rng);
            let moves = decode(&text);
            for i in 1..moves.len() {
                assert_ne!(
                    moves[i - 1],
                    moves[i],
                    "axis repeats at index {i} in {text:?}"
                );
            }
        }
    }

    #[test]
    fn double_turns_never_appear() {
        for seed in 0..400u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let text = scramble(&mut rng);
            assert!(
                !text.contains('2'),
                "a Skewb turn is 120 degrees, so `2` is not notation: {text:?}"
            );
        }
    }

    #[test]
    fn every_axis_and_both_directions_appear() {
        let mut axes_seen = [false; 4];
        let mut plain = false;
        let mut prime = false;
        for seed in 0..100u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            for token in scramble(&mut rng).split(' ') {
                let (name, suffix) = split_token(token);
                let axis = AXES.iter().position(|a| *a == name).expect("known axis");
                axes_seen[axis] = true;
                match suffix {
                    "" => plain = true,
                    _ => prime = true,
                }
            }
        }
        assert!(axes_seen.iter().all(|s| *s), "not every axis was generated");
        assert!(plain && prime, "not both turn directions were generated");
    }

    #[test]
    fn every_ordered_pair_of_distinct_axes_occurs() {
        // Guards against the constraint drifting stricter than TNoodle's, and
        // against the skip-the-previous-axis arithmetic biasing a successor.
        let mut pairs = [[0usize; 4]; 4];
        for seed in 0..600u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let moves = decode(&scramble(&mut rng));
            for w in moves.windows(2) {
                pairs[w[0]][w[1]] += 1;
            }
        }
        for (from, row) in pairs.iter().enumerate() {
            for (to, count) in row.iter().enumerate() {
                if from == to {
                    assert_eq!(*count, 0, "{from}->{to} must be impossible");
                } else {
                    // 6000 transitions over 12 legal pairs is 500 expected.
                    assert!(
                        *count > 300,
                        "{from}->{to} is starved at {count}, the successor draw is biased"
                    );
                }
            }
        }
    }

    #[test]
    fn seeded_generation_is_deterministic() {
        let a = scramble(&mut StdRng::seed_from_u64(1234));
        let b = scramble(&mut StdRng::seed_from_u64(1234));
        assert_eq!(a, b, "same seed must give the same scramble");
        let c = scramble(&mut StdRng::seed_from_u64(4321));
        assert_ne!(a, c, "different seeds should give different scrambles");
    }

    #[test]
    fn a_seeded_scramble_reads_as_wca_notation() {
        let text = scramble(&mut StdRng::seed_from_u64(7));
        let tokens: Vec<&str> = text.split(' ').collect();
        assert_eq!(tokens.len(), 11, "{text:?}");
        for token in tokens {
            assert!(
                matches!(token, "R" | "R'" | "U" | "U'" | "L" | "L'" | "B" | "B'"),
                "token {token:?} is not Skewb notation in {text:?}"
            );
        }
    }
}
