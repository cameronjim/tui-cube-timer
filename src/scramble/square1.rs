//! Random-move Square-1 scramble generation in WCA notation.

use rand::Rng;
use std::fmt::Write;

/// Half-hour slots in one layer.
const SLOTS: usize = 12;
/// Slashes per scramble. TNoodle's random-state output runs 9 to 13, and 12 is its median.
const SLASH_COUNT: usize = 12;
/// Smallest twist amount TNoodle emits.
const MIN_TWIST: i8 = -5;
/// Largest twist amount TNoodle emits. With `MIN_TWIST` this covers all 12 rotations once.
const MAX_TWIST: i8 = 6;

/// TNoodle's solved piece array. The top starts on a corner, the bottom on an edge.
const SOLVED: [u8; 2 * SLOTS] = [
    0, 0, 1, 2, 2, 3, 4, 4, 5, 6, 6, 7, 8, 9, 9, 10, 11, 11, 12, 13, 13, 14, 15, 15,
];

/// Both layers as 24 half-hour slots: 0..12 the top, 12..24 the bottom.
///
/// A corner fills two adjacent slots with one piece id and an edge fills one, so
/// two neighbouring slots hold the same id exactly when a corner spans them. The
/// slice plane cuts each layer between slots 11 and 0 and between slots 5 and 6,
/// which is why those four pairs are the only ones a slash cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shape {
    slots: [u8; 2 * SLOTS],
}

impl Shape {
    const fn solved() -> Shape {
        Shape { slots: SOLVED }
    }

    /// Turn the top `top` half-hours and the bottom `bottom`, clockwise when positive.
    ///
    /// Always physical: a layer of a Square-1 spins freely, and only the slash is
    /// ever blocked.
    fn twisted(self, top: i8, bottom: i8) -> Shape {
        let mut slots = self.slots;
        for (base, amount) in [(0usize, top), (SLOTS, bottom)] {
            let shift = (-i32::from(amount)).rem_euclid(SLOTS as i32) as usize;
            for i in 0..SLOTS {
                slots[base + i] = self.slots[base + (shift + i) % SLOTS];
            }
        }
        Shape { slots }
    }

    /// Whether the slice plane misses every corner, which is what a slash needs.
    fn can_slash(self) -> bool {
        self.slots[0] != self.slots[11]
            && self.slots[5] != self.slots[6]
            && self.slots[12] != self.slots[23]
            && self.slots[17] != self.slots[18]
    }

    /// Swap the two front halves. Only physical while `can_slash` holds.
    fn slashed(self) -> Shape {
        let mut slots = self.slots;
        for i in 0..SLOTS / 2 {
            slots.swap(SLOTS / 2 + i, SLOTS + i);
        }
        Shape { slots }
    }
}

/// Every twist TNoodle allows: both amounts in `MIN_TWIST..=MAX_TWIST`, minus the no-op.
fn all_twists() -> impl Iterator<Item = (i8, i8)> {
    (MIN_TWIST..=MAX_TWIST)
        .flat_map(|top| (MIN_TWIST..=MAX_TWIST).map(move |bottom| (top, bottom)))
        .filter(|&(top, bottom)| top != 0 || bottom != 0)
}

/// Append one twist group in WCA notation, no spaces inside the parentheses.
fn push_twist(out: &mut String, top: i8, bottom: i8) {
    let _ = write!(out, "({top},{bottom})");
}

/// One Square-1 scramble in WCA notation.
///
/// Twist groups and slashes alternate, `(top,bottom) / (top,bottom) / ...`, and the
/// scramble closes on a twist. Every twist before a slash is chosen from the ones
/// that leave the puzzle slashable, so the sequence is turnable end to end.
pub fn scramble<R: Rng>(rng: &mut R) -> String {
    let mut shape = Shape::solved();
    let mut out = String::with_capacity(SLASH_COUNT * 11 + 8);
    let mut candidates: Vec<(i8, i8)> = Vec::with_capacity(143);

    for _ in 0..SLASH_COUNT {
        candidates.clear();
        candidates
            .extend(all_twists().filter(|&(top, bottom)| shape.twisted(top, bottom).can_slash()));
        // Every shape can be rotated into a slashable position, so this never empties.
        debug_assert!(!candidates.is_empty());
        if candidates.is_empty() {
            break;
        }

        let (top, bottom) = candidates[rng.gen_range(0..candidates.len())];
        shape = shape.twisted(top, bottom).slashed();
        push_twist(&mut out, top, bottom);
        out.push_str(" / ");
    }

    // Nothing follows the closing twist, so it is unconstrained.
    candidates.clear();
    candidates.extend(all_twists());
    let (top, bottom) = candidates[rng.gen_range(0..candidates.len())];
    push_twist(&mut out, top, bottom);

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Token {
        Twist(i8, i8),
        Slash,
    }

    /// Parse one signed integer, rejecting anything the generator would not write.
    fn amount(text: &str, scramble: &str) -> i8 {
        assert!(!text.is_empty(), "empty amount in {scramble:?}");
        assert!(
            !text.starts_with('+'),
            "amounts are never signed with + ({scramble:?})"
        );
        assert!(text != "-0", "zero is never written as -0 ({scramble:?})");
        text.parse::<i8>()
            .unwrap_or_else(|_| panic!("bad amount {text:?} in {scramble:?}"))
    }

    /// Split a scramble into tokens, asserting the notation along the way.
    fn parse(scramble: &str) -> Vec<Token> {
        assert!(!scramble.is_empty(), "scramble must not be empty");
        assert_eq!(
            scramble.trim(),
            scramble,
            "no leading or trailing whitespace"
        );
        assert!(
            !scramble.contains("  "),
            "tokens are separated by a single space: {scramble:?}"
        );
        assert!(
            !scramble.contains('\t') && !scramble.contains('\n'),
            "spaces are the only separator: {scramble:?}"
        );

        scramble
            .split(' ')
            .map(|token| {
                if token == "/" {
                    return Token::Slash;
                }
                let body = token
                    .strip_prefix('(')
                    .and_then(|t| t.strip_suffix(')'))
                    .unwrap_or_else(|| panic!("bad token {token:?} in {scramble:?}"));
                let (top, bottom) = body
                    .split_once(',')
                    .unwrap_or_else(|| panic!("bad token {token:?} in {scramble:?}"));
                assert!(
                    !bottom.contains(','),
                    "a twist has exactly two amounts: {token:?} in {scramble:?}"
                );
                Token::Twist(amount(top, scramble), amount(bottom, scramble))
            })
            .collect()
    }

    /// Replay a scramble on the simulator, asserting every move is turnable.
    ///
    /// Returns the shape it ends on.
    fn replay(scramble: &str) -> Shape {
        let tokens = parse(scramble);
        let mut shape = Shape::solved();

        for (i, token) in tokens.iter().enumerate() {
            let want_twist = i % 2 == 0;
            match *token {
                Token::Twist(top, bottom) => {
                    assert!(
                        want_twist,
                        "two twists in a row at index {i} of {scramble:?}"
                    );
                    assert!(
                        (MIN_TWIST..=MAX_TWIST).contains(&top)
                            && (MIN_TWIST..=MAX_TWIST).contains(&bottom),
                        "twist ({top},{bottom}) at index {i} is outside -5..=6 in {scramble:?}"
                    );
                    assert!(
                        top != 0 || bottom != 0,
                        "(0,0) is a no-op and must never be emitted: {scramble:?}"
                    );
                    shape = shape.twisted(top, bottom);
                }
                Token::Slash => {
                    assert!(
                        !want_twist,
                        "two slashes in a row at index {i} of {scramble:?}"
                    );
                    assert!(
                        shape.can_slash(),
                        "slash at index {i} is blocked by a corner in {scramble:?}"
                    );
                    shape = shape.slashed();
                }
            }
            let mut sorted = shape.slots;
            sorted.sort_unstable();
            let mut want = SOLVED;
            want.sort_unstable();
            assert_eq!(
                sorted, want,
                "a move lost or duplicated a piece: {scramble:?}"
            );
        }

        assert!(
            matches!(tokens.last(), Some(Token::Twist(..))),
            "a scramble ends on a twist: {scramble:?}"
        );
        shape
    }

    fn counts(scramble: &str) -> (usize, usize) {
        let tokens = parse(scramble);
        let slashes = tokens.iter().filter(|t| **t == Token::Slash).count();
        (tokens.len() - slashes, slashes)
    }

    // ---- the simulator

    #[test]
    fn solved_shape_matches_tnoodle() {
        assert_eq!(
            Shape::solved().slots,
            [0, 0, 1, 2, 2, 3, 4, 4, 5, 6, 6, 7, 8, 9, 9, 10, 11, 11, 12, 13, 13, 14, 15, 15]
        );
        assert!(Shape::solved().can_slash(), "a solved Square-1 slashes");
    }

    #[test]
    fn twisting_is_a_rotation_and_undoes_itself() {
        let solved = Shape::solved();
        assert_eq!(solved.twisted(0, 0), solved, "the no-op changes nothing");
        for top in MIN_TWIST..=MAX_TWIST {
            for bottom in MIN_TWIST..=MAX_TWIST {
                let there = solved.twisted(top, bottom);
                assert_eq!(
                    there.twisted(-top, -bottom),
                    solved,
                    "({top},{bottom}) then its inverse must return to solved"
                );
                let mut sorted = there.slots;
                sorted.sort_unstable();
                let mut want = SOLVED;
                want.sort_unstable();
                assert_eq!(sorted, want, "({top},{bottom}) must permute the pieces");
            }
        }
    }

    #[test]
    fn twisting_by_one_moves_the_layer_one_slot_clockwise() {
        // Clockwise by one sends the piece in slot 11 to slot 0.
        let top = Shape::solved().twisted(1, 0).slots;
        assert_eq!(top[..SLOTS], [7, 0, 0, 1, 2, 2, 3, 4, 4, 5, 6, 6]);
        // The bottom is untouched by a top-only twist.
        assert_eq!(top[SLOTS..], SOLVED[SLOTS..]);
        let bottom = Shape::solved().twisted(0, -1).slots;
        assert_eq!(bottom[..SLOTS], SOLVED[..SLOTS]);
        assert_eq!(
            bottom[SLOTS..],
            [9, 9, 10, 11, 11, 12, 13, 13, 14, 15, 15, 8]
        );
    }

    #[test]
    fn slashing_swaps_the_front_halves_and_is_its_own_inverse() {
        let solved = Shape::solved();
        let once = solved.slashed();
        assert_eq!(once.slots[6..12], SOLVED[12..18]);
        assert_eq!(once.slots[12..18], SOLVED[6..12]);
        assert_eq!(once.slots[0..6], SOLVED[0..6]);
        assert_eq!(once.slots[18..24], SOLVED[18..24]);
        assert_eq!(once.slashed(), solved, "two slashes cancel");
    }

    #[test]
    fn can_slash_rejects_a_corner_on_the_slice_plane() {
        let solved = Shape::solved();
        // Two half-hours from solved puts a top corner across both cuts.
        assert!(!solved.twisted(2, 0).can_slash());
        assert!(!solved.twisted(-1, 0).can_slash());
        // A single half-hour lands the cuts back between a corner and an edge.
        assert!(solved.twisted(1, 0).can_slash());
        // A half turn maps the solved layer onto itself.
        assert!(solved.twisted(6, 6).can_slash());
        // The bottom layer is offset by one, so its blocked amounts differ.
        assert!(!solved.twisted(0, 1).can_slash());
        assert!(solved.twisted(0, 2).can_slash());
    }

    #[test]
    fn from_solved_exactly_sixty_three_twists_stay_slashable() {
        let solved = Shape::solved();
        let legal_top: Vec<i8> = (MIN_TWIST..=MAX_TWIST)
            .filter(|&t| solved.twisted(t, 0).can_slash())
            .collect();
        let legal_bottom: Vec<i8> = (MIN_TWIST..=MAX_TWIST)
            .filter(|&b| solved.twisted(0, b).can_slash())
            .collect();
        // A corner spans the cut when the layer is turned 2, 5, 8 or 11 slots from
        // square, and the bottom layer sits one slot ahead of the top.
        assert_eq!(legal_top, [-5, -3, -2, 0, 1, 3, 4, 6]);
        assert_eq!(legal_bottom, [-4, -3, -1, 0, 2, 3, 5, 6]);

        let legal = all_twists()
            .filter(|&(t, b)| solved.twisted(t, b).can_slash())
            .count();
        assert_eq!(legal, legal_top.len() * legal_bottom.len() - 1);
        assert_eq!(legal, 63);
    }

    // ---- the twist pool

    #[test]
    fn the_twist_pool_is_tnoodles_hundred_and_forty_three() {
        let twists: Vec<(i8, i8)> = all_twists().collect();
        assert_eq!(twists.len(), 143, "12 by 12 amounts minus the no-op");
        assert!(!twists.contains(&(0, 0)), "(0,0) is excluded");
        assert!(twists.contains(&(-5, 6)) && twists.contains(&(6, -5)));
        assert!(!twists.contains(&(-6, 0)) && !twists.contains(&(7, 0)));
        let unique: std::collections::HashSet<(i8, i8)> = twists.iter().copied().collect();
        assert_eq!(unique.len(), twists.len(), "no twist is listed twice");
    }

    // ---- generated scrambles

    #[test]
    fn every_twist_is_turnable_and_every_slash_possible() {
        for seed in 0..500u64 {
            let scramble = scramble(&mut StdRng::seed_from_u64(seed));
            replay(&scramble);
        }
    }

    #[test]
    fn shape_and_length_are_fixed() {
        for seed in 0..200u64 {
            let scramble = scramble(&mut StdRng::seed_from_u64(seed));
            let (twists, slashes) = counts(&scramble);
            assert_eq!(slashes, SLASH_COUNT, "wrong slash count: {scramble:?}");
            assert_eq!(twists, SLASH_COUNT + 1, "wrong twist count: {scramble:?}");
        }
    }

    #[test]
    fn notation_matches_tnoodle_spacing() {
        let scramble = scramble(&mut StdRng::seed_from_u64(7));
        assert!(scramble.starts_with('('), "starts on a twist: {scramble:?}");
        assert!(scramble.ends_with(')'), "ends on a twist: {scramble:?}");
        assert!(
            scramble.contains(") / ("),
            "slashes are spaced on both sides: {scramble:?}"
        );
        assert!(
            !scramble.contains(", "),
            "no space after the comma: {scramble:?}"
        );
        assert!(
            !scramble.contains(")/"),
            "the slash is never glued to a twist: {scramble:?}"
        );
        // The whole string is built from the alphabet TNoodle uses.
        assert!(
            scramble
                .chars()
                .all(|c| c.is_ascii_digit() || "(),-/ ".contains(c)),
            "unexpected character in {scramble:?}"
        );
    }

    #[test]
    fn amounts_stay_in_range_and_cover_it() {
        let mut seen = std::collections::HashSet::new();
        for seed in 0..300u64 {
            let scramble = scramble(&mut StdRng::seed_from_u64(seed));
            for token in parse(&scramble) {
                if let Token::Twist(top, bottom) = token {
                    for amount in [top, bottom] {
                        assert!(
                            (MIN_TWIST..=MAX_TWIST).contains(&amount),
                            "{amount} is outside -5..=6 in {scramble:?}"
                        );
                        seen.insert(amount);
                    }
                }
            }
        }
        let want: std::collections::HashSet<i8> = (MIN_TWIST..=MAX_TWIST).collect();
        assert_eq!(seen, want, "every allowed amount should turn up");
    }

    #[test]
    fn scrambles_leave_cubeshape() {
        let mut off_shape = 0;
        for seed in 0..100u64 {
            let scramble = scramble(&mut StdRng::seed_from_u64(seed));
            if replay(&scramble) != Shape::solved() {
                off_shape += 1;
            }
        }
        assert!(
            off_shape > 90,
            "a Square-1 scramble should almost always change shape, only {off_shape} of 100 did"
        );
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
    fn consecutive_draws_from_one_rng_differ() {
        let mut rng = StdRng::seed_from_u64(99);
        let a = scramble(&mut rng);
        let b = scramble(&mut rng);
        assert_ne!(a, b, "the rng must not be reset between scrambles");
        replay(&a);
        replay(&b);
    }

    #[test]
    fn thread_rng_scrambles_are_well_formed() {
        let mut rng = rand::thread_rng();
        for _ in 0..20 {
            let scramble = scramble(&mut rng);
            replay(&scramble);
            assert_eq!(counts(&scramble), (SLASH_COUNT + 1, SLASH_COUNT));
        }
    }

    #[test]
    fn the_parser_rejects_malformed_notation() {
        // Guards the replay test: a broken generator must not slip past the parser.
        assert!(std::panic::catch_unwind(|| parse("(1,0)/ (2,0)")).is_err());
        assert!(std::panic::catch_unwind(|| parse("(1,0)  / (2,0)")).is_err());
        assert!(std::panic::catch_unwind(|| parse("(1, 0) / (2,0)")).is_err());
        assert!(std::panic::catch_unwind(|| parse("(1,0) / (2,0) ")).is_err());
        assert!(std::panic::catch_unwind(|| parse("1,0 / (2,0)")).is_err());
        assert!(std::panic::catch_unwind(|| parse("(1,0,2) / (2,0)")).is_err());
        assert!(std::panic::catch_unwind(|| parse("(-0,0) / (2,0)")).is_err());
        assert!(std::panic::catch_unwind(|| replay("(2,0) / (1,0)")).is_err());
        assert!(std::panic::catch_unwind(|| replay("(1,0) (2,0)")).is_err());
        assert!(std::panic::catch_unwind(|| replay("(1,0) / (2,0) /")).is_err());
    }
}
