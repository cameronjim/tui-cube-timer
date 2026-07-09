//! Clock scramble generation in WCA notation.

use rand::Rng;

/// The nine front dials, in the order TNoodle emits them.
const FRONT_DIALS: [&str; 9] = ["UR", "DR", "DL", "UL", "U", "R", "D", "L", "ALL"];

/// The five back dials, reached after the `y2` flip.
const BACK_DIALS: [&str; 5] = ["U", "R", "D", "L", "ALL"];

/// TNoodle draws every dial from `nextInt(12) - 5`, so amounts span `-5..=6`.
const MIN_AMOUNT: i32 = -5;
const MAX_AMOUNT: i32 = 6;

/// Append one dial token, e.g. `UR4+` or `L3-`.
fn push_dial<R: Rng>(out: &mut String, dial: &str, rng: &mut R) {
    let amount = rng.gen_range(MIN_AMOUNT..=MAX_AMOUNT);
    out.push_str(dial);
    // No amount exceeds 6, so one digit is always enough.
    out.push((b'0' + amount.unsigned_abs() as u8) as char);
    // Zero renders as `0+`: TNoodle calls every non-negative amount clockwise,
    // which is why `0-` never appears in an official scramble.
    out.push(if amount >= 0 { '+' } else { '-' });
}

/// One Clock scramble in WCA notation.
///
/// Fifteen space-separated tokens: the nine front dials, `y2`, then the five
/// back dials. Amounts run `0+` through `6+` clockwise and `1-` through `5-`
/// anticlockwise, twelve equally likely values per dial. Trailing pin states
/// are absent on purpose; the WCA dropped them from official scrambles on
/// 1 January 2024.
///
/// Clock's dial turns commute, so drawing each amount independently and
/// uniformly already lands on a uniformly random state. This is a genuine
/// random-state scramble rather than an approximation of one.
pub fn scramble<R: Rng>(rng: &mut R) -> String {
    let mut out = String::with_capacity(80);

    for dial in FRONT_DIALS {
        push_dial(&mut out, dial, rng);
        out.push(' ');
    }
    out.push_str("y2");
    for dial in BACK_DIALS {
        out.push(' ');
        push_dial(&mut out, dial, rng);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use std::collections::HashMap;

    /// A scramble generated from `seed`.
    fn gen(seed: u64) -> String {
        scramble(&mut StdRng::seed_from_u64(seed))
    }

    /// Every dial token in order, as (dial, amount, sign).
    ///
    /// Asserts the frame of the scramble on the way through: fifteen
    /// single-spaced tokens, `y2` in slot nine, and nothing else that is not a
    /// dial token. Panics with the offending scramble, so a failure names it.
    fn parse(scramble: &str) -> Vec<(&str, i32, char)> {
        assert_eq!(
            scramble.trim(),
            scramble,
            "no leading or trailing whitespace: {scramble:?}"
        );
        assert!(
            !scramble.contains("  "),
            "tokens must be single-spaced: {scramble:?}"
        );

        let tokens: Vec<&str> = scramble.split(' ').collect();
        assert_eq!(tokens.len(), 15, "expected 15 tokens in {scramble:?}");
        assert_eq!(tokens[9], "y2", "slot 9 must be the flip: {scramble:?}");
        assert_eq!(
            tokens.iter().filter(|t| **t == "y2").count(),
            1,
            "exactly one flip: {scramble:?}"
        );

        tokens
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != 9)
            .map(|(i, token)| {
                let sign = token.chars().last().unwrap_or_else(|| {
                    panic!("empty token at index {i} in {scramble:?}");
                });
                assert!(
                    sign == '+' || sign == '-',
                    "token {token:?} must end in + or -: {scramble:?}"
                );
                let body = &token[..token.len() - 1];
                let split = body.len() - 1;
                let (dial, digits) = body.split_at(split);
                let amount: i32 = digits.parse().unwrap_or_else(|_| {
                    panic!("token {token:?} has a non-numeric amount in {scramble:?}");
                });
                assert!(
                    !dial.is_empty(),
                    "token {token:?} has no dial name: {scramble:?}"
                );
                (dial, amount, sign)
            })
            .collect()
    }

    /// The dial names a well-formed scramble must carry, in order.
    fn expected_dials() -> Vec<&'static str> {
        FRONT_DIALS
            .iter()
            .chain(BACK_DIALS.iter())
            .copied()
            .collect()
    }

    // ---- format

    #[test]
    fn dial_order_is_the_fixed_wca_sequence() {
        let want = expected_dials();
        for seed in 0..400u64 {
            let scramble = gen(seed);
            let got: Vec<&str> = parse(&scramble).iter().map(|(d, _, _)| *d).collect();
            assert_eq!(got, want, "wrong dial order in {scramble:?}");
        }
    }

    #[test]
    fn the_sequence_is_exactly_the_documented_fifteen_tokens() {
        assert_eq!(
            expected_dials(),
            ["UR", "DR", "DL", "UL", "U", "R", "D", "L", "ALL", "U", "R", "D", "L", "ALL"]
        );
        // The flip splits nine front dials from five back dials.
        assert_eq!(FRONT_DIALS.len() + 1 + BACK_DIALS.len(), 15);
    }

    #[test]
    fn a_real_wca_scramble_satisfies_the_same_parser() {
        // From a 2025 competition, so the shape is checked against reality and
        // not only against this generator.
        let official = "UR5+ DR1- DL5- UL4+ U3- R2- D3- L5- ALL6+ y2 U4- R3+ D3+ L6+ ALL5-";
        let parsed = parse(official);
        let dials: Vec<&str> = parsed.iter().map(|(d, _, _)| *d).collect();
        assert_eq!(dials, expected_dials());
        assert_eq!(parsed[0], ("UR", 5, '+'));
        assert_eq!(parsed[8], ("ALL", 6, '+'));
        assert_eq!(parsed[13], ("ALL", 5, '-'));
    }

    #[test]
    fn no_trailing_pin_state_tokens() {
        for seed in 0..200u64 {
            let scramble = gen(seed);
            let last = scramble.split(' ').next_back().unwrap_or_default();
            assert!(
                last.starts_with("ALL"),
                "the scramble must end on the back ALL dial, got {last:?} in {scramble:?}"
            );
            // A pin-state suffix would be a bare dial name with no amount.
            for token in scramble.split(' ') {
                assert!(
                    token == "y2" || token.ends_with('+') || token.ends_with('-'),
                    "stray token {token:?} in {scramble:?}"
                );
            }
        }
    }

    // ---- amounts

    #[test]
    fn amounts_stay_inside_the_wca_range() {
        for seed in 0..1_000u64 {
            let scramble = gen(seed);
            for (dial, amount, sign) in parse(&scramble) {
                match sign {
                    '+' => assert!(
                        (0..=6).contains(&amount),
                        "{dial}{amount}+ is outside 0+..=6+ in {scramble:?}"
                    ),
                    _ => assert!(
                        (1..=5).contains(&amount),
                        "{dial}{amount}- is outside 1-..=5- in {scramble:?}"
                    ),
                }
            }
        }
    }

    #[test]
    fn zero_is_always_clockwise() {
        for seed in 0..1_000u64 {
            let scramble = gen(seed);
            assert!(
                !scramble.contains("0-"),
                "0- is not a legal amount: {scramble:?}"
            );
        }
    }

    #[test]
    fn all_twelve_amounts_occur() {
        let mut seen: HashMap<String, u32> = HashMap::new();
        for seed in 0..500u64 {
            for (_, amount, sign) in parse(&gen(seed)) {
                *seen.entry(format!("{amount}{sign}")).or_default() += 1;
            }
        }
        let mut want: Vec<String> = (0..=6).map(|n| format!("{n}+")).collect();
        want.extend((1..=5).map(|n| format!("{n}-")));
        assert_eq!(want.len(), 12);
        for amount in &want {
            assert!(seen.contains_key(amount), "amount {amount} never appeared");
        }
        let mut got: Vec<String> = seen.keys().cloned().collect();
        got.sort();
        want.sort();
        assert_eq!(got, want, "an amount outside the legal twelve appeared");
    }

    #[test]
    fn amounts_are_roughly_uniform_over_the_twelve_values() {
        let mut counts: HashMap<i32, u32> = HashMap::new();
        let samples = 2_000u64;
        for seed in 0..samples {
            for (_, amount, sign) in parse(&gen(seed)) {
                let signed = if sign == '+' { amount } else { -amount };
                *counts.entry(signed).or_default() += 1;
            }
        }
        let total: u32 = counts.values().sum();
        assert_eq!(total as u64, samples * 14, "14 dials per scramble");
        let expected = f64::from(total) / 12.0;
        for value in MIN_AMOUNT..=MAX_AMOUNT {
            let seen = f64::from(*counts.get(&value).unwrap_or(&0));
            assert!(
                (seen - expected).abs() < expected * 0.2,
                "amount {value} appeared {seen} times, expected about {expected}"
            );
        }
    }

    #[test]
    fn each_dial_is_drawn_independently() {
        // A shared draw across dials would make every amount in a scramble equal.
        let mut varied = false;
        for seed in 0..50u64 {
            let scramble = gen(seed);
            let parsed = parse(&scramble);
            let first = (parsed[0].1, parsed[0].2);
            varied |= parsed.iter().any(|(_, a, s)| (*a, *s) != first);
            if varied {
                break;
            }
        }
        assert!(varied, "all dials in a scramble drew the same amount");
    }

    // ---- determinism

    #[test]
    fn seeded_generation_is_deterministic() {
        for seed in [0u64, 1, 42, 1234, u64::MAX] {
            assert_eq!(
                gen(seed),
                gen(seed),
                "same seed must give the same scramble"
            );
        }
    }

    #[test]
    fn different_seeds_give_different_scrambles() {
        let mut distinct = std::collections::HashSet::new();
        for seed in 0..200u64 {
            distinct.insert(gen(seed));
        }
        // 12^14 states, so 200 draws colliding would mean the seed is ignored.
        assert_eq!(distinct.len(), 200, "seeds are not reaching the output");
    }

    #[test]
    fn generation_consumes_randomness_in_a_stable_order() {
        // Two scrambles from one rng must differ from two fresh seeded runs
        // only in that the second continues the stream.
        let mut rng = StdRng::seed_from_u64(7);
        let first = scramble(&mut rng);
        let second = scramble(&mut rng);
        assert_eq!(first, gen(7), "the first draw must match a fresh seed");
        assert_ne!(first, second, "the stream must advance between calls");

        let mut again = StdRng::seed_from_u64(7);
        assert_eq!(scramble(&mut again), first);
        assert_eq!(scramble(&mut again), second);
    }
}
