//! Pyraminx random-state scrambles: edge and axial coordinates, tips, exact-11 solutions.
//!
//! The move semantics mirror TNoodle's `PyraminxSolver`, because what the notation means
//! physically is whatever the official generator does. Vertices are numbered 0 = U,
//! 1 = L, 2 = R, 3 = B, held the WCA way: U on top, L at the front left, R at the front
//! right, B at the back. An uppercase move turns the two-layer block at its vertex
//! clockwise as seen from outside that vertex, so `U` is one third of a turn of the top
//! block and `U'` is two of them. The tips turn on their own and are the lowercase
//! `u l r b`.
//!
//! [`CLOCKWISE`] writes that clockwise order out and is the one physical claim in the
//! file; every table below is derived from it. TNoodle builds its edge tables from
//! `cycleAndOrient(edges, 5, 3, 1)` for U, `(2, 1, 0)` for L, `(0, 3, 4)` for R and
//! `(2, 4, 5)` for B, read as "the piece at the first position moves to the second".
//! Those four triples are exactly the four clockwise turns, which is checked directly by
//! `the_edge_cycles_reproduce_tnoodles_move_tables`: read the other way round every turn
//! would be counter-clockwise, giving a mirror of TNoodle with the same distance table,
//! which is the one error the fixtures cannot see.
//!
//! The six edges sit on the six vertex pairs, [`EDGE_VERTICES`], so edge 0 is the L R
//! edge, 1 is U L, 2 is L B, 3 is U R, 4 is R B and 5 is U B. Each edge piece shows a
//! facelet on each of the two faces its pair borders, and each of those faces holds one
//! further vertex, so that third vertex names the facelet. Orientation 0 means the
//! piece's marked facelet sits on the side of the lower third vertex, which is what
//! [`primary_third`] returns. Every turn cycles three edges and flips exactly two of
//! them, so the flip total stays even and the sixth flip is carried by the other five.
//!
//! The four axial pieces never move, they only twist, one of them per turn and by the
//! number of turns. Their orientations are indexed by vertex, as edge flips are indexed
//! by position, which is what keeps each coordinate's transition a function of that
//! coordinate and the move alone and so lets each have its own small table.
//!
//! A state packs the three coordinates into one index with 0 solved: 720 edge
//! permutations, of which only the 360 even ones are reachable because every turn is a
//! 3-cycle, times 32 flips times 81 twists. Half of the 1,866,240 encoded states are
//! therefore unreachable, and the breadth-first table marks them so sampling can reject
//! them, which makes uniformity a property of the table rather than of parity arithmetic
//! that could silently be wrong. Tips are not in the index at all: they are independent,
//! three states each, and drawn uniformly.

use super::Engine;
use rand::Rng;
use std::sync::OnceLock;

/// The four axes in notation and table order, one per vertex of the tetrahedron.
const AXES: [&str; 4] = ["U", "L", "R", "B"];

/// The four tips, emitted after the core turns in this order.
const TIPS: [&str; 4] = ["u", "l", "r", "b"];

/// A Pyraminx turn is 120 degrees, so no move ever carries a `2`.
const SUFFIXES: [&str; 2] = ["", "'"];

/// The other three vertices in clockwise order seen from outside each vertex.
const CLOCKWISE: [[usize; 3]; 4] = [[3, 2, 1], [3, 0, 2], [1, 0, 3], [1, 2, 0]];

/// The two vertices each edge position lies between.
const EDGE_VERTICES: [[usize; 2]; 6] = [[1, 2], [0, 1], [1, 3], [0, 2], [2, 3], [0, 3]];

/// Edge permutations, all 720 encoded; the 360 odd ones are unreachable.
const N_PERM: usize = 720;

/// Edge flip patterns: five free flips, the sixth carried by their parity.
const N_FLIP: usize = 32;

/// Axial twists: four pieces that never move, three orientations each.
const N_TWIST: usize = 81;

/// The encoded state space, half of it reachable.
const STATES: usize = N_PERM * N_FLIP * N_TWIST;

/// Turnable axes, and the one and two turn powers of each.
const AXIS_COUNT: usize = 4;
const POWERS: usize = 2;
const N_MOVES: usize = AXIS_COUNT * POWERS;

/// Core turns per scramble, the exact length TNoodle searches for.
const CORE_LEN: usize = 11;

/// A vertex turn is a third of a turn, so three of them are the identity.
///
/// That one number is the orientation count of a tip and of an axial piece, and it is
/// also the modulus that turns a solution move into the move undoing it.
const TURN_ORDER: usize = 3;

/// A TNoodle-shape Pyraminx scramble: exactly 11 core moves of U L R B with ' as the
/// only suffix, then one lowercase tip token per unsolved tip, in u l r b order.
///
/// The core is a uniformly random reachable state solved in exactly 11 turns and then
/// written backwards, so applying it to a solved puzzle lands on the state that was
/// drawn. Tips are drawn separately because they are independent of everything else.
///
/// The rng is drawn on three times over, in order: the state, the branch order the search
/// takes through the state's many 11 turn solutions, and the four tips.
pub(super) fn scramble<R: Rng>(rng: &mut R) -> String {
    let engine = engine();
    let dist = distances();
    let state = engine.random_reachable(dist, rng);
    // Padding a shorter solution out to 11 is what makes every Pyraminx scramble the same
    // length. A state with no canonical 11 is theoretical, but 12 always exists.
    let solution = engine
        .solve_exactly(state, CORE_LEN, dist, rng)
        .or_else(|| engine.solve_exactly(state, CORE_LEN + 1, dist, rng))
        .unwrap_or_default();

    let mut out = String::with_capacity(CORE_LEN * 3 + TIPS.len() * 3);
    for &(axis, power) in solution.iter().rev() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(AXES[axis]);
        // Undoing the solution turns one third of a turn into the other two thirds.
        out.push_str(SUFFIXES[TURN_ORDER - power - 1]);
    }

    for name in TIPS {
        // A uniform tip is already solved one time in three, and contributes no move.
        let turns = rng.gen_range(0..TURN_ORDER);
        if turns == 0 {
            continue;
        }
        out.push(' ');
        out.push_str(name);
        out.push_str(SUFFIXES[turns - 1]);
    }
    out
}

/// The move structure the shared engine searches over.
fn engine() -> Engine {
    Engine { states: STATES, axes: AXIS_COUNT, powers: POWERS, apply }
}

/// Exact distance to solved for every encoded state, built once and shared with the tests.
///
/// Two million bytes and a breadth-first sweep, so it is built on the first scramble of
/// the event rather than at startup, and never inside the draw loop.
fn distances() -> &'static [u8] {
    static DIST: OnceLock<Vec<u8>> = OnceLock::new();
    DIST.get_or_init(|| engine().distances())
}

/// One move on an encoded state: three table lookups, no allocation.
fn apply(state: usize, axis: usize, power: usize) -> usize {
    let m = moves();
    let col = column(axis, power);
    let (perm, flip, twist) = unpack(state);
    pack(
        usize::from(m.perm[perm * N_MOVES + col]),
        usize::from(m.flip[flip * N_MOVES + col]),
        usize::from(m.twist[twist * N_MOVES + col]),
    )
}

/// One encoded state built from its three coordinates; all zero is solved.
fn pack(perm: usize, flip: usize, twist: usize) -> usize {
    (perm * N_FLIP + flip) * N_TWIST + twist
}

/// The three coordinates of an encoded state.
fn unpack(state: usize) -> (usize, usize, usize) {
    (state / (N_FLIP * N_TWIST), state / N_TWIST % N_FLIP, state % N_TWIST)
}

/// The column a move occupies in every table.
fn column(axis: usize, power: usize) -> usize {
    axis * POWERS + power - 1
}

/// One row per coordinate value, one column per move.
struct Moves {
    perm: Vec<u16>,
    flip: Vec<u8>,
    twist: Vec<u8>,
}

/// The three coordinate tables, built once and shared with the tests.
fn moves() -> &'static Moves {
    static MOVES: OnceLock<Moves> = OnceLock::new();
    MOVES.get_or_init(build_moves)
}

/// Build every coordinate's table by decoding, turning and re-encoding its own range.
fn build_moves() -> Moves {
    let turns: Vec<([usize; 6], [bool; 6])> = (0..AXIS_COUNT).map(edge_turn).collect();
    let mut perm = vec![0u16; N_PERM * N_MOVES];
    let mut flip = vec![0u8; N_FLIP * N_MOVES];
    let mut twist = vec![0u8; N_TWIST * N_MOVES];

    for (axis, &(dest, mask)) in turns.iter().enumerate() {
        for power in 1..=POWERS {
            let col = column(axis, power);
            for code in 0..N_PERM {
                let mut p = decode_perm(code);
                for _ in 0..power {
                    let mut moved = [0usize; 6];
                    for (e, &to) in dest.iter().enumerate() {
                        moved[to] = p[e];
                    }
                    p = moved;
                }
                perm[code * N_MOVES + col] = encode_perm(&p) as u16;
            }
            for code in 0..N_FLIP {
                let mut f = decode_flip(code);
                for _ in 0..power {
                    let mut moved = [false; 6];
                    for (e, &to) in dest.iter().enumerate() {
                        moved[to] = f[e] ^ mask[e];
                    }
                    f = moved;
                }
                flip[code * N_MOVES + col] = encode_flip(&f) as u8;
            }
            for code in 0..N_TWIST {
                let mut t = decode_twist(code);
                t[axis] = (t[axis] + power) % TURN_ORDER;
                twist[code * N_MOVES + col] = encode_twist(&t) as u8;
            }
        }
    }
    Moves { perm, flip, twist }
}

/// One clockwise turn of `vertex`: where each edge position's piece goes, and whether it
/// flips on the way. Positions away from the vertex hold still.
fn edge_turn(vertex: usize) -> ([usize; 6], [bool; 6]) {
    let mut dest = [0, 1, 2, 3, 4, 5];
    let mut flips = [false; 6];
    for (e, pair) in EDGE_VERTICES.iter().enumerate() {
        if !pair.contains(&vertex) {
            continue;
        }
        let other = if pair[0] == vertex { pair[1] } else { pair[0] };
        let to = edge_of(vertex, next_clockwise(vertex, other));
        dest[e] = to;
        // The turn carries the marked facelet onto the face named by the rotated third
        // vertex, and the piece reads as flipped when that is not the new position's own.
        flips[e] = next_clockwise(vertex, primary_third(e)) != primary_third(to);
    }
    (dest, flips)
}

/// The edge position on the vertex pair `a`, `b`.
fn edge_of(a: usize, b: usize) -> usize {
    EDGE_VERTICES.iter().position(|e| e.contains(&a) && e.contains(&b)).unwrap_or(0)
}

/// The vertex following `v` in the clockwise order seen from `about`.
fn next_clockwise(about: usize, v: usize) -> usize {
    let ring = CLOCKWISE[about];
    match ring.iter().position(|&x| x == v) {
        Some(i) => ring[(i + 1) % ring.len()],
        None => v,
    }
}

/// The third vertex naming edge `e`'s orientation-zero facelet, the lower of the two.
fn primary_third(e: usize) -> usize {
    (0..AXIS_COUNT).find(|v| !EDGE_VERTICES[e].contains(v)).unwrap_or(0)
}

/// Lehmer code of a six-element permutation; the identity is 0.
fn encode_perm(p: &[usize; 6]) -> usize {
    let mut code = 0;
    for i in 0..p.len() {
        let smaller = ((i + 1)..p.len()).filter(|&j| p[j] < p[i]).count();
        code = code * (p.len() - i) + smaller;
    }
    code
}

/// The inverse of [`encode_perm`].
fn decode_perm(code: usize) -> [usize; 6] {
    let mut digits = [0usize; 6];
    let mut rest = code;
    for i in (0..digits.len()).rev() {
        let radix = digits.len() - i;
        digits[i] = rest % radix;
        rest /= radix;
    }
    // Each digit is a rank among the values still unused, so pick and close the gap.
    let mut pool = [0usize, 1, 2, 3, 4, 5];
    let mut left = pool.len();
    let mut p = [0usize; 6];
    for (i, slot) in p.iter_mut().enumerate() {
        let pick = digits[i];
        *slot = pool[pick];
        for j in pick..left - 1 {
            pool[j] = pool[j + 1];
        }
        left -= 1;
    }
    p
}

/// The six flips of a flip coordinate; the sixth is the parity of the other five.
fn decode_flip(code: usize) -> [bool; 6] {
    let mut f = [false; 6];
    let mut parity = false;
    for (i, slot) in f.iter_mut().take(5).enumerate() {
        *slot = code >> i & 1 == 1;
        parity ^= *slot;
    }
    f[5] = parity;
    f
}

/// The inverse of [`decode_flip`]; the implied sixth flip is dropped.
fn encode_flip(f: &[bool; 6]) -> usize {
    (0..5).filter(|&i| f[i]).map(|i| 1 << i).sum()
}

/// The four axial twists of a twist coordinate, indexed by vertex.
fn decode_twist(code: usize) -> [usize; 4] {
    let mut t = [0usize; 4];
    let mut rest = code;
    for slot in t.iter_mut() {
        *slot = rest % TURN_ORDER;
        rest /= TURN_ORDER;
    }
    t
}

/// The inverse of [`decode_twist`].
fn encode_twist(t: &[usize; 4]) -> usize {
    t.iter().rev().fold(0, |acc, &x| acc * TURN_ORDER + x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::UNREACHABLE;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    /// Jaap Scherphuis's God's-algorithm counts for the Pyraminx core, tips excluded.
    ///
    /// Twelve rows summing to the exact total leaves no room for a wrong cycle, a wrong
    /// flip or a missed parity constraint, which is why this fixture is the file's spine.
    const DEPTHS: [usize; 12] =
        [1, 8, 48, 288, 1728, 9896, 51808, 220111, 480467, 166276, 2457, 32];

    /// The reachable half of the encoded space: 360 even permutations times 32 times 81.
    const REACHABLE: usize = 933_120;

    /// Seeds for the tests that run the full generator, solve included.
    const SEEDS: u64 = 600;

    /// Seeds for the tests that only draw a state, which costs a table lookup.
    const SAMPLE_SEEDS: u64 = 4_000;

    /// The core turns and then the tip turns, each as (index, power).
    type Parsed = (Vec<(usize, usize)>, Vec<(usize, usize)>);

    /// One parsed scramble: core turns as (axis, power), tip turns as (tip, power).
    ///
    /// The whole frame is asserted on the way through, so every test that parses inherits
    /// the notation checks for free.
    fn parse(scramble: &str) -> Parsed {
        assert_eq!(scramble.trim(), scramble, "stray whitespace in {scramble:?}");
        assert!(!scramble.contains("  "), "double space in {scramble:?}");
        let mut core = Vec::new();
        let mut tips: Vec<(usize, usize)> = Vec::new();
        for token in scramble.split(' ') {
            let (name, power) = match token.strip_suffix('\'') {
                Some(name) => (name, 2),
                None => (token, 1),
            };
            if let Some(axis) = AXES.iter().position(|&a| a == name) {
                assert!(tips.is_empty(), "a core turn after a tip in {scramble:?}");
                core.push((axis, power));
            } else if let Some(tip) = TIPS.iter().position(|&t| t == name) {
                tips.push((tip, power));
            } else {
                panic!("unknown token {token:?} in {scramble:?}");
            }
        }
        assert_eq!(core.len(), CORE_LEN, "core turn count in {scramble:?}");
        assert!(tips.len() <= TIPS.len(), "too many tips in {scramble:?}");
        for pair in core.windows(2) {
            assert_ne!(pair[0].0, pair[1].0, "two turns of one axis in a row in {scramble:?}");
        }
        // Strictly increasing covers both the u l r b order and one token per tip.
        for pair in tips.windows(2) {
            assert!(pair[0].0 < pair[1].0, "tips out of order in {scramble:?}");
        }
        (core, tips)
    }

    /// The state and tips the generator draws for `seed`, in the order it draws them.
    ///
    /// The search sits between the two draws and takes its branch order from the same rng,
    /// so this helper has to run the solve too or the tips would not line up.
    fn sample(seed: u64) -> (usize, [usize; 4]) {
        let mut rng = StdRng::seed_from_u64(seed);
        let engine = engine();
        let dist = distances();
        let state = engine.random_reachable(dist, &mut rng);
        assert!(
            engine.solve_exactly(state, CORE_LEN, dist, &mut rng).is_some(),
            "state {state} has no canonical eleven turn solution"
        );
        let mut tips = [0usize; 4];
        for tip in tips.iter_mut() {
            *tip = rng.gen_range(0..TURN_ORDER);
        }
        (state, tips)
    }

    /// Whether an edge permutation code is an even permutation.
    fn perm_is_even(code: usize) -> bool {
        let p = decode_perm(code);
        let mut swaps = 0;
        for i in 0..p.len() {
            for j in (i + 1)..p.len() {
                if p[i] > p[j] {
                    swaps += 1;
                }
            }
        }
        swaps % 2 == 0
    }

    // ---- the fixture

    #[test]
    fn the_depth_distribution_matches_the_published_counts() {
        let dist = distances();
        assert_eq!(dist.len(), STATES, "the table covers the encoded space");
        let mut counts = [0usize; 32];
        let mut unreachable = 0;
        for &d in dist {
            if d == UNREACHABLE {
                unreachable += 1;
            } else {
                counts[usize::from(d)] += 1;
            }
        }
        for (depth, &want) in DEPTHS.iter().enumerate() {
            assert_eq!(counts[depth], want, "states at depth {depth}");
        }
        for (depth, &got) in counts.iter().enumerate().skip(DEPTHS.len()) {
            assert_eq!(got, 0, "nothing sits at depth {depth}");
        }
        assert_eq!(counts.iter().sum::<usize>(), REACHABLE, "reachable total");
        assert_eq!(unreachable, STATES - REACHABLE, "unreachable total");
        assert_eq!(REACHABLE, STATES / 2, "exactly half the encoding is reachable");
        assert_eq!(dist[0], 0, "index 0 is solved");
    }

    #[test]
    fn exactly_the_odd_edge_permutations_are_unreachable() {
        let dist = distances();
        // Every turn is a 3-cycle, so the odd half of the encoding can never be reached.
        let mut even = [false; N_PERM];
        for (code, slot) in even.iter_mut().enumerate() {
            *slot = perm_is_even(code);
        }
        for (state, &d) in dist.iter().enumerate() {
            let (perm, _, _) = unpack(state);
            assert_eq!(
                d != UNREACHABLE,
                even[perm],
                "state {state} with edge permutation {perm} is on the wrong side"
            );
        }
    }

    // ---- the move model

    #[test]
    fn the_edge_cycles_reproduce_tnoodles_move_tables() {
        // The triples TNoodle hands cycleAndOrient, as "the piece here moves there".
        let want: [[usize; 3]; 4] = [[5, 3, 1], [2, 1, 0], [0, 3, 4], [2, 4, 5]];
        for (axis, ring) in want.iter().enumerate() {
            let (dest, flips) = edge_turn(axis);
            for (i, &from) in ring.iter().enumerate() {
                let to = ring[(i + 1) % ring.len()];
                assert_eq!(dest[from], to, "{} should send edge {from} to {to}", AXES[axis]);
            }
            for e in 0..EDGE_VERTICES.len() {
                if !ring.contains(&e) {
                    assert_eq!(dest[e], e, "{} moved edge {e}", AXES[axis]);
                    assert!(!flips[e], "{} flipped the resting edge {e}", AXES[axis]);
                }
            }
            // Two flips out of three is what keeps the flip total even, and so keeps the
            // sixth flip recoverable from the other five.
            assert_eq!(
                flips.iter().filter(|&&f| f).count(),
                2,
                "{} must flip exactly two edges",
                AXES[axis]
            );
        }
    }

    #[test]
    fn each_edge_meets_exactly_two_vertices_and_each_vertex_three_edges() {
        for (vertex, ring) in CLOCKWISE.iter().enumerate() {
            let touching = EDGE_VERTICES.iter().filter(|pair| pair.contains(&vertex)).count();
            assert_eq!(touching, 3, "vertex {vertex} should carry three edges");
            assert!(!ring.contains(&vertex), "vertex {vertex} turns itself");
            // The ring is the other three vertices, so the turn is a genuine 3-cycle.
            let mut sorted = *ring;
            sorted.sort_unstable();
            let others: Vec<usize> = (0..AXIS_COUNT).filter(|&v| v != vertex).collect();
            assert_eq!(sorted.to_vec(), others, "vertex {vertex} turns the wrong three");
        }
        // The six positions are the six vertex pairs, each exactly once.
        let mut seen = Vec::new();
        for pair in EDGE_VERTICES {
            let mut sorted = pair;
            sorted.sort_unstable();
            assert!(!seen.contains(&sorted), "vertex pair {sorted:?} appears twice");
            seen.push(sorted);
        }
        assert_eq!(seen.len(), 6);
        for (e, pair) in EDGE_VERTICES.iter().enumerate() {
            assert_eq!(edge_of(pair[0], pair[1]), e);
            assert_eq!(edge_of(pair[1], pair[0]), e);
            assert!(!pair.contains(&primary_third(e)), "edge {e}'s facelet names its own vertex");
        }
    }

    #[test]
    fn every_turn_has_order_three_and_undoes_its_opposite() {
        let mut rng = StdRng::seed_from_u64(11);
        let states: Vec<usize> =
            std::iter::once(0).chain((0..200).map(|_| rng.gen_range(0..STATES))).collect();
        for axis in 0..AXIS_COUNT {
            for &state in &states {
                let once = apply(state, axis, 1);
                assert_eq!(apply(apply(once, axis, 1), axis, 1), state, "order three");
                assert_eq!(apply(once, axis, 2), state, "a turn and its opposite");
                assert_eq!(apply(once, axis, 1), apply(state, axis, 2), "two turns");
                assert_ne!(once, state, "a turn must move something");
            }
        }
    }

    #[test]
    fn the_coordinates_round_trip() {
        for code in 0..N_PERM {
            assert_eq!(encode_perm(&decode_perm(code)), code, "edge permutation {code}");
        }
        for code in 0..N_FLIP {
            let f = decode_flip(code);
            assert_eq!(encode_flip(&f), code, "edge flip {code}");
            assert_eq!(f[5], f[..5].iter().fold(false, |a, &b| a ^ b), "implied flip {code}");
        }
        for code in 0..N_TWIST {
            assert_eq!(encode_twist(&decode_twist(code)), code, "axial twist {code}");
        }
        assert_eq!(pack(0, 0, 0), 0, "the solved state is index 0");
        assert_eq!(decode_perm(0), [0, 1, 2, 3, 4, 5], "code 0 is the identity");
        for state in [0, 1, 81, 2591, 1_000_000, STATES - 1] {
            let (p, f, t) = unpack(state);
            assert_eq!(pack(p, f, t), state, "packing {state}");
        }
        for axis in 0..AXIS_COUNT {
            for power in 1..=POWERS {
                assert!(column(axis, power) < N_MOVES);
            }
        }
    }

    // ---- emission

    #[test]
    fn a_scramble_reaches_the_state_it_was_drawn_from() {
        for seed in 0..SEEDS {
            // `sample` asserts the eleven turn solution exists on its way to the tips.
            let (state, tips) = sample(seed);
            let text = scramble(&mut StdRng::seed_from_u64(seed));
            let (core, tokens) = parse(&text);
            let mut core_state = 0;
            for &(axis, power) in &core {
                core_state = apply(core_state, axis, power);
            }
            assert_eq!(core_state, state, "seed {seed} core did not land on {state}: {text:?}");

            let mut turned = [0usize; 4];
            for &(tip, power) in &tokens {
                turned[tip] = (turned[tip] + power) % TURN_ORDER;
            }
            assert_eq!(turned, tips, "seed {seed} tips did not land: {text:?}");
        }
    }

    #[test]
    fn the_emitted_shape_is_eleven_core_turns_and_at_most_four_tips() {
        for seed in 0..SEEDS {
            let text = scramble(&mut StdRng::seed_from_u64(seed));
            let (core, tips) = parse(&text);
            let tokens = text.split(' ').count();
            assert_eq!(tokens, core.len() + tips.len(), "token count in {text:?}");
            assert!((CORE_LEN..=CORE_LEN + TIPS.len()).contains(&tokens), "length of {text:?}");
            for &(axis, power) in &core {
                assert!(axis < AXIS_COUNT && (1..=POWERS).contains(&power));
            }
            for &(tip, power) in &tips {
                assert!(tip < TIPS.len() && (1..=POWERS).contains(&power));
            }
        }
    }

    #[test]
    fn the_same_seed_scrambles_the_same_and_other_seeds_do_not() {
        let first = scramble(&mut StdRng::seed_from_u64(7));
        assert_eq!(first, scramble(&mut StdRng::seed_from_u64(7)), "seeded runs must agree");
        let others: Vec<String> =
            (8..20).map(|s| scramble(&mut StdRng::seed_from_u64(s))).collect();
        assert!(others.iter().all(|o| *o != first), "another seed repeated {first:?}");
        assert!(others.windows(2).any(|w| w[0] != w[1]), "every seed scrambled alike");
    }

    #[test]
    fn tip_counts_range_over_zero_through_four() {
        let mut seen = [false; 5];
        let mut per_tip = [0usize; 4];
        for seed in 0..SEEDS {
            let text = scramble(&mut StdRng::seed_from_u64(seed));
            let (_, tips) = parse(&text);
            seen[tips.len()] = true;
            for &(tip, _) in &tips {
                per_tip[tip] += 1;
            }
        }
        for (count, &hit) in seen.iter().enumerate() {
            assert!(hit, "no scramble in {SEEDS} seeds needed {count} tips");
        }
        // A uniform tip is unsolved two times in three, so about 400 of 600 seeds each.
        for (tip, &count) in per_tip.iter().enumerate() {
            assert!(
                (340..=460).contains(&count),
                "tip {} was unsolved {count} times in {SEEDS}, expected about 400",
                TIPS[tip]
            );
        }
    }

    #[test]
    fn every_axis_and_both_powers_appear_in_the_core() {
        let mut seen = [0usize; N_MOVES];
        for seed in 0..SEEDS {
            let text = scramble(&mut StdRng::seed_from_u64(seed));
            let (core, _) = parse(&text);
            for &(axis, power) in &core {
                seen[column(axis, power)] += 1;
            }
        }
        for axis in 0..AXIS_COUNT {
            for power in 1..=POWERS {
                assert!(
                    seen[column(axis, power)] > 0,
                    "{}{} never appeared in {SEEDS} seeds",
                    AXES[axis],
                    SUFFIXES[power - 1]
                );
            }
        }
    }

    #[test]
    fn the_last_core_turn_of_a_scramble_ranges_over_the_move_set() {
        const LAST_SEEDS: usize = 400;
        // The last core turn is the first turn of the solution behind it, so a search that
        // tried the branches in a fixed order ended the core of every scramble on U', with L'
        // tenth almost as often. Sampling was uniform throughout, which is why this needed a
        // test of its own rather than showing up in the depth distribution.
        let mut counts = [0usize; N_MOVES];
        for seed in 0..LAST_SEEDS as u64 {
            let text = scramble(&mut StdRng::seed_from_u64(seed));
            let (core, _) = parse(&text);
            let (axis, power) = *core.last().expect("eleven core turns");
            counts[column(axis, power)] += 1;
        }
        let seen = counts.iter().filter(|&&count| count > 0).count();
        let worst = counts.iter().copied().max().unwrap_or(0);
        assert!(
            seen >= 3,
            "only {seen} of the {N_MOVES} turns ever ended the core in {LAST_SEEDS} seeds: {counts:?}"
        );
        assert!(
            worst * 5 <= LAST_SEEDS * 3,
            "one turn ended the core of {worst} of {LAST_SEEDS} scrambles, over three fifths: {counts:?}"
        );
    }

    // ---- sampling

    #[test]
    fn the_sampled_depth_averages_the_published_optimal() {
        let dist = distances();
        let engine = engine();
        let mut rng = StdRng::seed_from_u64(2024);
        let mut total = 0u64;
        for _ in 0..SAMPLE_SEEDS {
            let state = engine.random_reachable(dist, &mut rng);
            let d = dist[state];
            assert_ne!(d, UNREACHABLE, "sampling returned an unreachable state");
            total += u64::from(d);
        }
        // The table's own mean is 7.7955; the band is wide enough never to flake.
        let mean = total as f64 / SAMPLE_SEEDS as f64;
        assert!((7.55..=8.05).contains(&mean), "mean sampled depth {mean}, expected about 7.80");
    }
}
