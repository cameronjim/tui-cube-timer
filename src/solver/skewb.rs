//! Skewb random-state scrambles: centre and corner coordinates, exact-11 solutions.
//!
//! The move semantics come from TNoodle's `SkewbPuzzle`, which is where the WCA's fixed
//! corner notation is actually defined: its `turn` is written for `axis:0-R 1-U 2-L 3-B`, and
//! the three centre facelets each axis cycles name the corner that axis turns. `R` turns DFR,
//! `U` turns URB, `L` turns DLB and `B` turns DRB, each a third of a turn clockwise seen from
//! outside that corner, with `X'` two of them. ULF is the one corner no move touches, which
//! is what fixed corner notation fixes.
//!
//! [`AXIS_CORNERS`] and the clockwise sense in [`rot`] are the only physical claims in the
//! file; every table below is derived from them. TNoodle's own centre cycles run F to R to D
//! for `R`, U to B to R for `U`, L to D to B for `L` and R to B to D for `B`, and
//! `the_turns_reproduce_tnoodles_facelet_cycles` checks all four of them, corners included,
//! against the derivation. Read the other way round every turn would be counter-clockwise,
//! giving a mirror of the puzzle with the same distance table, which is the one error the
//! fixtures cannot see.
//!
//! Three of the four turnable corners share a tetrad with the fixed one, DFR, URB and DLB
//! beside ULF, so `R`, `U` and `L` each twist their own corner where it stands and cycle three
//! of the other tetrad, URF, DLF, ULB and DRB. `B` turns DRB, which sits in that second
//! tetrad, so it is the one move that cycles the first: `B` twists DRB where it stands and
//! cycles DFR, URB and DLB. No corner ever crosses between the tetrads, which is what keeps
//! the coordinates small.
//!
//! A corner's twist is which axis, x, y or z, its U or D facelet lies on, counted from y. A
//! third of a turn about a corner rotates the three axes among themselves, so it adds the same
//! step to the twist of everything in the moving half, [`Rot::twist`]. Twists are indexed by
//! position rather than by piece, which keeps every coordinate's transition a function of that
//! coordinate and the move alone, so each gets its own small table and [`apply`] is nothing
//! but lookups.
//!
//! Four coordinates pack into one index with 0 solved: 360 even centre permutations, 12 even
//! permutations of the ring, 27 twists of DFR, URB and DLB, and 81 twists of the ring. Both
//! permutations stay even because every turn is a 3-cycle. The permutation of DFR, URB and DLB
//! is left out on purpose: `B` alone permutes them and every third of a `B` also adds two to
//! the ring's twist total, so that permutation is a function of the total and the index
//! already carries it.
//!
//! What is left is 9,447,840 encoded states, of which exactly a third, the 3,149,280 the
//! puzzle has, are reachable. The gap is the one constraint the index does not enforce: a
//! third of an `R`, `U` or `L` adds to its own corner's twist and cycles the ring in the same
//! motion, so the twist total of DFR, URB and DLB is pinned to the ring permutation, which is
//! the `isSolvable` check in TNoodle's `SkewbSolver`. The breadth-first table marks the other
//! two thirds unreachable and sampling rejects them, which makes uniformity a property of the
//! table rather than of parity arithmetic that could silently be wrong.

use super::Engine;
use rand::Rng;
use std::sync::OnceLock;

/// The four axes in notation and table order, TNoodle's `RULB`.
const AXES: [&str; 4] = ["R", "U", "L", "B"];

/// A corner turn is a third of a turn, so no move ever carries a `2`.
const SUFFIXES: [&str; 2] = ["", "'"];

/// The corner each axis turns, as signs over the x, y and z axes: DFR, URB, DLB, DRB.
const AXIS_CORNERS: [[i8; 3]; 4] = [[1, -1, 1], [1, 1, -1], [-1, -1, -1], [1, -1, -1]];

/// The tetrad `R`, `U` and `L` cycle: URF, DLF, ULB, DRB.
const RING_CORNERS: [[i8; 3]; 4] = [[1, 1, 1], [-1, -1, 1], [-1, 1, -1], [1, -1, -1]];

/// The six faces in TNoodle's order, U R F D L B, each as its (axis, sign) normal.
const FACES: [(usize, i8); 6] = [(1, 1), (0, 1), (2, 1), (1, -1), (0, -1), (2, -1)];

/// Corners the twist coordinate tracks: DFR, URB and DLB, the corners of `R`, `U` and `L`.
const TWISTED: usize = 3;

/// Even permutations of the six centres; every turn is a 3-cycle, so the odd half never comes up.
const N_CENTRE: usize = 360;

/// Even permutations of the four ring corners.
const N_RING: usize = 12;

/// Twists of DFR, URB and DLB.
const N_AXIS_TWIST: usize = 27;

/// Twists of the four ring corners.
const N_RING_TWIST: usize = 81;

/// The encoded state space, a third of it reachable.
const STATES: usize = N_CENTRE * N_RING * N_AXIS_TWIST * N_RING_TWIST;

/// Turnable axes, and the one and two turn powers of each.
const AXIS_COUNT: usize = 4;
const POWERS: usize = 2;
const N_MOVES: usize = AXIS_COUNT * POWERS;

/// Turns per scramble, the exact length TNoodle searches for.
const LEN: usize = 11;

/// A corner turn is a third of a turn, so three of them are the identity.
///
/// That one number is a corner's orientation count, and it is also the modulus that turns a
/// solution move into the move undoing it.
const TURN_ORDER: usize = 3;

/// A TNoodle-shape Skewb scramble: exactly 11 turns of R U L B with ' as the only suffix.
///
/// A uniformly random reachable state, solved in exactly 11 turns and written backwards, so
/// applying it to a solved Skewb lands on the state that was drawn.
///
/// The rng is drawn on twice, in order: the state, then the branch order the search takes
/// through the state's many 11 turn solutions.
pub(super) fn scramble<R: Rng>(rng: &mut R) -> String {
    let state = engine().random_reachable(distances(), rng);
    scramble_for(state, rng)
}

/// The scramble that reaches `state`: an exact-length solution for it, inverted.
///
/// The rng is the search's, not the sampler's: it picks which of the state's many
/// exact-length solutions comes back, and without it every scramble would end on the same
/// token.
fn scramble_for<R: Rng>(state: usize, rng: &mut R) -> String {
    let engine = engine();
    let dist = distances();
    // Padding a shorter solution out to eleven is what makes every Skewb scramble the same
    // length. A state with no canonical eleven is theoretical, but twelve always exists.
    let solution = engine
        .solve_exactly(state, LEN, dist, rng)
        .or_else(|| engine.solve_exactly(state, LEN + 1, dist, rng))
        .unwrap_or_default();
    let mut out = String::with_capacity(LEN * 3);
    for &(axis, power) in solution.iter().rev() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(AXES[axis]);
        // Undoing the solution turns one third of a turn into the other two thirds.
        out.push_str(SUFFIXES[TURN_ORDER - power - 1]);
    }
    out
}

/// The move structure the shared engine searches over.
fn engine() -> Engine {
    Engine { states: STATES, axes: AXIS_COUNT, powers: POWERS, apply }
}

/// Exact distance to solved for every encoded state, built once and shared with the tests.
///
/// Nine million bytes and a breadth-first sweep, so it is built on the first scramble of the
/// event rather than at startup, and never inside the draw loop.
fn distances() -> &'static [u8] {
    static DIST: OnceLock<Vec<u8>> = OnceLock::new();
    DIST.get_or_init(|| engine().distances())
}

/// One turn on an encoded state: four table lookups, no allocation.
fn apply(state: usize, axis: usize, power: usize) -> usize {
    let m = moves();
    let col = column(axis, power);
    let (centre, ring, axis_twist, ring_twist) = unpack(state);
    pack(
        usize::from(m.centre[centre * N_MOVES + col]),
        usize::from(m.ring[ring * N_MOVES + col]),
        usize::from(m.axis_twist[axis_twist * N_MOVES + col]),
        usize::from(m.ring_twist[ring_twist * N_MOVES + col]),
    )
}

/// One encoded state built from its four coordinates; all zero is solved.
fn pack(centre: usize, ring: usize, axis_twist: usize, ring_twist: usize) -> usize {
    ((centre * N_RING + ring) * N_AXIS_TWIST + axis_twist) * N_RING_TWIST + ring_twist
}

/// The four coordinates of an encoded state.
fn unpack(state: usize) -> (usize, usize, usize, usize) {
    let rest = state / N_RING_TWIST;
    (
        rest / (N_RING * N_AXIS_TWIST),
        rest / N_AXIS_TWIST % N_RING,
        rest % N_AXIS_TWIST,
        state % N_RING_TWIST,
    )
}

/// The column a move occupies in every table.
fn column(axis: usize, power: usize) -> usize {
    axis * POWERS + power - 1
}

/// One row per coordinate value, one column per move.
struct Moves {
    centre: Vec<u16>,
    ring: Vec<u8>,
    axis_twist: Vec<u8>,
    ring_twist: Vec<u8>,
}

/// The four coordinate tables, built once and shared with the tests.
///
/// Separate from the distance table on purpose: the breadth-first search calls [`apply`], so
/// one lock holding both would deadlock initializing itself.
fn moves() -> &'static Moves {
    static MOVES: OnceLock<Moves> = OnceLock::new();
    MOVES.get_or_init(build_moves)
}

/// Build every coordinate's table by decoding, turning and re-encoding its own range.
fn build_moves() -> Moves {
    let turns: Vec<Turn> = (0..AXIS_COUNT).map(turn).collect();
    let mut centre = vec![0u16; N_CENTRE * N_MOVES];
    let mut ring = vec![0u8; N_RING * N_MOVES];
    let mut axis_twist = vec![0u8; N_AXIS_TWIST * N_MOVES];
    let mut ring_twist = vec![0u8; N_RING_TWIST * N_MOVES];

    for (axis, t) in turns.iter().enumerate() {
        for power in 1..=POWERS {
            let col = column(axis, power);
            for code in 0..N_CENTRE {
                let mut p = [0usize; 6];
                decode_even(code, &mut p);
                for _ in 0..power {
                    permute(&mut p, &t.centres);
                }
                centre[code * N_MOVES + col] = encode_even(&p) as u16;
            }
            for code in 0..N_RING {
                let mut p = [0usize; 4];
                decode_even(code, &mut p);
                for _ in 0..power {
                    permute(&mut p, &t.ring);
                }
                ring[code * N_MOVES + col] = encode_even(&p) as u8;
            }
            for code in 0..N_AXIS_TWIST {
                let mut o = [0usize; TWISTED];
                decode_twist(code, &mut o);
                for _ in 0..power {
                    tilt(&mut o, &t.axis, &t.axis_twist);
                }
                axis_twist[code * N_MOVES + col] = encode_twist(&o) as u8;
            }
            for code in 0..N_RING_TWIST {
                let mut o = [0usize; 4];
                decode_twist(code, &mut o);
                for _ in 0..power {
                    tilt(&mut o, &t.ring, &t.ring_twist);
                }
                ring_twist[code * N_MOVES + col] = encode_twist(&o) as u8;
            }
        }
    }
    Moves { centre, ring, axis_twist, ring_twist }
}

/// A third of a turn as a signed permutation of the x, y and z axes.
struct Rot {
    /// Where each axis lands.
    to: [usize; 3],
    /// The sign each axis picks up on the way.
    sign: [i8; 3],
    /// Twist every piece in the moving half gains, which is the step the axes take.
    twist: usize,
}

/// The clockwise third of a turn about `corner`, seen from outside it.
///
/// Conjugating the turn about (1, 1, 1) by the corner's own signs gives the turn about that
/// corner, and a corner with an odd number of negative signs conjugates by a reflection, which
/// reverses the sense. So the sign product alone picks the step, and the signs follow it.
fn rot(corner: [i8; 3]) -> Rot {
    let step = if corner[0] * corner[1] * corner[2] < 0 { 1 } else { 2 };
    let mut to = [0usize; 3];
    let mut sign = [0i8; 3];
    for (a, (to, sign)) in to.iter_mut().zip(sign.iter_mut()).enumerate() {
        *to = (a + step) % 3;
        *sign = corner[a] * corner[*to];
    }
    Rot { to, sign, twist: step }
}

/// Everything one axis does, derived from its corner alone.
struct Turn {
    /// Destination face of the facelet on each face.
    centres: [usize; 6],
    /// Destination position of the corner at each ring position.
    ring: [usize; 4],
    /// Twist the corner leaving each ring position gains.
    ring_twist: [usize; 4],
    /// Destination position of the corner at each of DFR, URB and DLB.
    axis: [usize; TWISTED],
    /// Twist the corner leaving each of those gains.
    axis_twist: [usize; TWISTED],
}

/// One clockwise turn of `axis`: where every piece goes, and how far it tilts on the way.
///
/// A piece turns with the corner exactly when it leans towards it, and that is what splits the
/// puzzle in half: three centres and four corners move, the other three and four hold still.
fn turn(axis: usize) -> Turn {
    let corner = AXIS_CORNERS[axis];
    let r = rot(corner);
    let mut centres = [0usize; 6];
    for (f, slot) in centres.iter_mut().enumerate() {
        let (a, sign) = FACES[f];
        *slot = if sign * corner[a] > 0 { face_of(r.to[a], sign * r.sign[a]) } else { f };
    }
    let mut t = Turn {
        centres,
        ring: [0; 4],
        ring_twist: [0; 4],
        axis: [0; TWISTED],
        axis_twist: [0; TWISTED],
    };
    corners_turned(&r, corner, &RING_CORNERS, &mut t.ring, &mut t.ring_twist);
    corners_turned(&r, corner, &AXIS_CORNERS[..TWISTED], &mut t.axis, &mut t.axis_twist);
    t
}

/// Where a turn sends each corner of one group, and the twist each one gains.
///
/// The turning corner itself lands back where it was and gains a twist like everything else in
/// the moving half, which is why the destination and the twist are read separately.
fn corners_turned(
    r: &Rot,
    corner: [i8; 3],
    group: &[[i8; 3]],
    dest: &mut [usize],
    twist: &mut [usize],
) {
    for (p, &v) in group.iter().enumerate() {
        if leans(v, corner) {
            dest[p] = corner_of(turned(r, v), group);
            twist[p] = r.twist;
        } else {
            dest[p] = p;
        }
    }
}

/// Where `r` sends the corner with signs `v`.
fn turned(r: &Rot, v: [i8; 3]) -> [i8; 3] {
    let mut out = [0i8; 3];
    for ((&to, &sign), &s) in r.to.iter().zip(&r.sign).zip(&v) {
        out[to] = sign * s;
    }
    out
}

/// Whether a piece pointing along `v` lies on `corner`'s side of the cut.
fn leans(v: [i8; 3], corner: [i8; 3]) -> bool {
    v.iter().zip(&corner).map(|(&a, &b)| i32::from(a) * i32::from(b)).sum::<i32>() > 0
}

/// The face whose normal is `sign` along `axis`.
fn face_of(axis: usize, sign: i8) -> usize {
    FACES.iter().position(|&f| f == (axis, sign)).unwrap_or(0)
}

/// The position of the corner with signs `v` within `group`.
fn corner_of(v: [i8; 3], group: &[[i8; 3]]) -> usize {
    group.iter().position(|&c| c == v).unwrap_or(0)
}

/// Send every piece to where `dest` sends its position.
fn permute(p: &mut [usize], dest: &[usize]) {
    let mut moved = [0usize; 6];
    for (i, &to) in dest.iter().enumerate() {
        moved[to] = p[i];
    }
    p.copy_from_slice(&moved[..p.len()]);
}

/// Send every twist to where `dest` sends its position, adding what the turn tilts it by.
fn tilt(o: &mut [usize], dest: &[usize], twist: &[usize]) {
    let mut moved = [0usize; 6];
    for (i, &to) in dest.iter().enumerate() {
        moved[to] = (o[i] + twist[i]) % TURN_ORDER;
    }
    o.copy_from_slice(&moved[..o.len()]);
}

/// Index of an even permutation among the n!/2 of them; the identity is 0.
///
/// The last two positions carry no digit of their own: their order is whatever makes the
/// permutation even, and dropping that one bit is exactly what halves the range.
fn encode_even(p: &[usize]) -> usize {
    let mut code = 0;
    for i in 0..p.len() - 2 {
        let smaller = p[i + 1..].iter().filter(|&&later| later < p[i]).count();
        code = code * (p.len() - i) + smaller;
    }
    code
}

/// The inverse of [`encode_even`].
fn decode_even(code: usize, p: &mut [usize]) {
    let n = p.len();
    let mut digits = [0usize; 6];
    let mut rest = code;
    for i in (0..n - 2).rev() {
        digits[i] = rest % (n - i);
        rest /= n - i;
    }
    // Each digit is a rank among the values still unused, so pick and close the gap.
    let mut pool = [0usize, 1, 2, 3, 4, 5];
    let mut left = n;
    for i in 0..n - 2 {
        p[i] = pool[digits[i]];
        pool.copy_within(digits[i] + 1..left, digits[i]);
        left -= 1;
    }
    p[n - 2] = pool[0];
    p[n - 1] = pool[1];
    if inversions(p) % 2 == 1 {
        p.swap(n - 2, n - 1);
    }
}

/// Pairs of positions holding their values out of order; an even count is an even permutation.
fn inversions(p: &[usize]) -> usize {
    (0..p.len()).map(|i| p[i + 1..].iter().filter(|&&later| later < p[i]).count()).sum()
}

/// The twists of a twist coordinate, indexed by position.
fn decode_twist(code: usize, o: &mut [usize]) {
    let mut rest = code;
    for slot in o.iter_mut() {
        *slot = rest % TURN_ORDER;
        rest /= TURN_ORDER;
    }
}

/// The inverse of [`decode_twist`].
fn encode_twist(o: &[usize]) -> usize {
    o.iter().rev().fold(0, |code, &t| code * TURN_ORDER + t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::UNREACHABLE;
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use std::collections::HashSet;

    /// Jaap Scherphuis's God's-algorithm counts for the Skewb.
    ///
    /// Twelve rows summing to the exact total leaves no room for a wrong cycle, a wrong twist
    /// or a missed constraint, which is why this fixture is the file's spine.
    const DEPTHS: [usize; 12] = [
        1, 8, 48, 288, 1728, 10_248, 59_304, 315_198, 1_225_483, 1_455_856, 81_028, 90,
    ];

    /// The reachable third of the encoded space, and the number of states a Skewb has.
    const REACHABLE: usize = 3_149_280;

    /// The corner no move in fixed corner notation touches.
    const ULF: [i8; 3] = [-1, 1, 1];

    /// Seeds for the tests that run the full generator, solve included.
    const SEEDS: u64 = 400;

    /// Seeds for the tests that only draw a state, which costs a table lookup.
    const SAMPLE_SEEDS: u64 = 4_000;

    /// One parsed scramble as (axis, power) turns, the whole notation asserted on the way.
    ///
    /// Every test that parses inherits the frame checks for free, which is why nothing below
    /// splits a scramble by hand.
    fn parse(scramble: &str) -> Vec<(usize, usize)> {
        assert_eq!(scramble.trim(), scramble, "stray whitespace in {scramble:?}");
        assert!(!scramble.contains("  "), "double space in {scramble:?}");
        let mut turns = Vec::with_capacity(LEN);
        for token in scramble.split(' ') {
            let (name, power) = match token.strip_suffix('\'') {
                Some(name) => (name, 2),
                None => (token, 1),
            };
            let axis = AXES
                .iter()
                .position(|&a| a == name)
                .unwrap_or_else(|| panic!("unknown token {token:?} in {scramble:?}"));
            turns.push((axis, power));
        }
        assert_eq!(turns.len(), LEN, "turn count in {scramble:?}");
        for pair in turns.windows(2) {
            assert_ne!(pair[0].0, pair[1].0, "two turns of one axis in a row in {scramble:?}");
        }
        turns
    }

    /// The state the generator draws for `seed`, the first thing it takes from the rng.
    fn sample(seed: u64) -> usize {
        engine().random_reachable(distances(), &mut StdRng::seed_from_u64(seed))
    }

    /// The twist total of DFR, URB and DLB, the quantity the reachable third is pinned by.
    fn axis_twist_total(state: usize) -> usize {
        let mut o = [0usize; TWISTED];
        decode_twist(unpack(state).2, &mut o);
        o.iter().sum::<usize>() % TURN_ORDER
    }

    /// The twist total of the ring, which carries the permutation the index leaves out.
    fn ring_twist_total(state: usize) -> usize {
        let mut o = [0usize; 4];
        decode_twist(unpack(state).3, &mut o);
        o.iter().sum::<usize>() % TURN_ORDER
    }

    // ---- the fixture

    #[test]
    fn the_depth_distribution_matches_the_published_counts() {
        let dist = distances();
        assert_eq!(dist.len(), STATES, "the table covers the encoded space");
        assert_eq!(STATES, 9_447_840, "the encoded space is three times the puzzle");
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
        assert_eq!(REACHABLE, STATES / TURN_ORDER, "exactly a third of the encoding is reachable");
        assert_eq!(dist[0], 0, "index 0 is solved");
    }

    #[test]
    fn reachability_is_exactly_the_twist_total_the_ring_permutation_calls_for() {
        let dist = distances();
        // Every R, U or L adds to this total and cycles the ring in the same motion, so one
        // ring permutation admits one total. That is the whole of the unreachable two thirds.
        let mut wanted = [None; N_RING];
        for (state, &d) in dist.iter().enumerate() {
            if d != UNREACHABLE {
                let total = axis_twist_total(state);
                let seen = wanted[unpack(state).1].get_or_insert(total);
                assert_eq!(*seen, total, "state {state} needs a second total for its ring");
            }
        }
        assert!(wanted.iter().all(Option::is_some), "some ring permutation is unreachable");
        for (state, &d) in dist.iter().enumerate() {
            assert_eq!(
                d != UNREACHABLE,
                wanted[unpack(state).1] == Some(axis_twist_total(state)),
                "state {state} is on the wrong side of the constraint"
            );
        }
    }

    #[test]
    fn the_ring_twist_total_carries_the_permutation_the_index_leaves_out() {
        // B is the only move that permutes DFR, URB and DLB, and it is also the only move that
        // moves this total, by two each third of a turn. That is why the index can drop their
        // permutation and still describe the puzzle.
        let mut rng = StdRng::seed_from_u64(3);
        let states: Vec<usize> =
            std::iter::once(0).chain((0..200).map(|_| rng.gen_range(0..STATES))).collect();
        for &state in &states {
            let before = ring_twist_total(state);
            for (axis, name) in AXES.iter().enumerate() {
                for power in 1..=POWERS {
                    // B, the one axis whose own corner sits in the ring, is the only mover.
                    let moved = if axis == TWISTED { 2 * power % TURN_ORDER } else { 0 };
                    let after = ring_twist_total(apply(state, axis, power));
                    assert_eq!(
                        after,
                        (before + moved) % TURN_ORDER,
                        "{name}{} moved the ring twist total wrongly",
                        SUFFIXES[power - 1]
                    );
                }
            }
        }
    }

    // ---- the move model

    #[test]
    fn the_turns_reproduce_tnoodles_facelet_cycles() {
        // Read off SkewbPuzzle.turn, as "the facelet here lands there". Faces are U R F D L B,
        // TNoodle's own order, and the corners are ring positions but for B, which cycles the
        // three that R, U and L turn. Read any ring backwards and the whole puzzle mirrors.
        let centres: [[usize; 3]; 4] = [[2, 1, 3], [0, 5, 1], [4, 3, 5], [1, 5, 3]];
        let ring: [&[usize]; 4] = [&[1, 0, 3], &[2, 3, 0], &[2, 1, 3], &[]];
        let axis_ring: [&[usize]; 4] = [&[], &[], &[], &[0, 1, 2]];
        for (axis, t) in (0..AXIS_COUNT).map(turn).enumerate() {
            check_cycle(&centres[axis], &t.centres, AXES[axis], "centre");
            check_cycle(ring[axis], &t.ring, AXES[axis], "ring corner");
            check_cycle(axis_ring[axis], &t.axis, AXES[axis], "turning corner");
            // Three centres and four corners move, which is the puzzle split in half.
            assert_eq!(moved(&t.centres), 3, "{} moved the wrong centres", AXES[axis]);
            let corners = t.ring_twist.iter().chain(&t.axis_twist).filter(|&&d| d > 0).count();
            assert_eq!(corners, 4, "{} tilted the wrong number of corners", AXES[axis]);
        }
    }

    /// Assert that `dest` cycles exactly `ring`, the rest of it holding still.
    fn check_cycle(ring: &[usize], dest: &[usize], axis: &str, what: &str) {
        for (i, &from) in ring.iter().enumerate() {
            let to = ring[(i + 1) % ring.len()];
            assert_eq!(dest[from], to, "{axis} should send {what} {from} to {to}");
        }
        for (p, &to) in dest.iter().enumerate() {
            if !ring.contains(&p) {
                assert_eq!(to, p, "{axis} moved the resting {what} {p}");
            }
        }
    }

    /// Positions a destination table does not leave alone.
    fn moved(dest: &[usize]) -> usize {
        dest.iter().enumerate().filter(|(p, &to)| *p != to).count()
    }

    #[test]
    fn the_two_tetrads_hold_all_eight_corners_and_only_ulf_stays_home() {
        let mut corners: Vec<[i8; 3]> = AXIS_CORNERS[..TWISTED].to_vec();
        corners.push(ULF);
        corners.extend_from_slice(&RING_CORNERS);
        assert_eq!(corners.len(), 8, "eight corners between the two tetrads");
        assert_eq!(corners.iter().collect::<HashSet<_>>().len(), 8, "a corner listed twice");
        // A tetrad is four corners no two of which are adjacent, which the sign product says.
        for group in [&corners[..4], &corners[4..]] {
            for c in group {
                assert_eq!(
                    c[0] * c[1] * c[2],
                    group[0][0] * group[0][1] * group[0][2],
                    "{c:?} is in the wrong tetrad"
                );
            }
        }
        assert_eq!(AXIS_CORNERS[TWISTED], RING_CORNERS[3], "B turns a ring corner");
        for &corner in &AXIS_CORNERS {
            assert!(!leans(ULF, corner), "a turn of {corner:?} carries ULF with it");
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
        for (n, count) in [(6, N_CENTRE), (4, N_RING)] {
            let mut p = vec![0usize; n];
            for code in 0..count {
                decode_even(code, &mut p);
                let mut placed = p.clone();
                placed.sort_unstable();
                assert_eq!(placed, (0..n).collect::<Vec<_>>(), "code {code} is no permutation");
                assert_eq!(inversions(&p) % 2, 0, "code {code} decoded to an odd permutation");
                assert_eq!(encode_even(&p), code, "permutation of {n} at {code}");
            }
        }
        for (n, count) in [(TWISTED, N_AXIS_TWIST), (4, N_RING_TWIST)] {
            let mut o = vec![0usize; n];
            for code in 0..count {
                decode_twist(code, &mut o);
                assert!(o.iter().all(|&t| t < TURN_ORDER), "code {code} twisted past two");
                assert_eq!(encode_twist(&o), code, "twist of {n} at {code}");
            }
        }
        assert_eq!(pack(0, 0, 0, 0), 0, "the solved state is index 0");
        for state in [0, 1, 81, 2591, 1_000_000, STATES - 1] {
            let (c, r, a, t) = unpack(state);
            assert_eq!(pack(c, r, a, t), state, "packing {state}");
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
        let dist = distances();
        for seed in 0..SEEDS {
            let state = sample(seed);
            let text = scramble(&mut StdRng::seed_from_u64(seed));
            let turns = parse(&text);

            let mut rng = StdRng::seed_from_u64(seed);
            let solution = engine()
                .solve_exactly(state, LEN, dist, &mut rng)
                .unwrap_or_else(|| panic!("state {state} has no canonical eleven turn solution"));
            let solved = solution.iter().fold(state, |s, &(axis, p)| apply(s, axis, p));
            assert_eq!(solved, 0, "seed {seed}'s own solution did not solve it");

            let rebuilt = turns.iter().fold(0, |s, &(axis, p)| apply(s, axis, p));
            assert_eq!(rebuilt, state, "seed {seed} did not land on {state}: {text:?}");
        }
    }

    #[test]
    fn the_emitted_shape_is_eleven_turns_of_r_u_l_and_b() {
        for seed in 0..SEEDS {
            let text = scramble(&mut StdRng::seed_from_u64(seed));
            let turns = parse(&text);
            assert_eq!(text.split(' ').count(), LEN, "token count in {text:?}");
            for &(axis, power) in &turns {
                assert!(axis < AXIS_COUNT, "unknown axis in {text:?}");
                assert!((1..=POWERS).contains(&power), "unknown power in {text:?}");
            }
            for token in text.split(' ') {
                let suffix = &token[1..];
                assert!(SUFFIXES.contains(&suffix), "{token:?} carries a {suffix:?} in {text:?}");
            }
        }
    }

    #[test]
    fn the_same_seed_scrambles_the_same_and_other_seeds_do_not() {
        let mut seen = HashSet::new();
        for seed in 0..60u64 {
            let text = scramble(&mut StdRng::seed_from_u64(seed));
            assert_eq!(
                text,
                scramble(&mut StdRng::seed_from_u64(seed)),
                "seed {seed} is not deterministic"
            );
            seen.insert(text);
        }
        assert_eq!(seen.len(), 60, "two seeds produced the same scramble");
    }

    #[test]
    fn every_axis_and_both_powers_appear() {
        let mut seen = [0usize; N_MOVES];
        for seed in 0..SEEDS {
            for (axis, power) in parse(&scramble(&mut StdRng::seed_from_u64(seed))) {
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
    fn the_last_turn_of_a_scramble_ranges_over_the_move_set() {
        // The last turn of a scramble is the first turn of the solution behind it, so a search
        // that tried the branches in a fixed order ended every scramble on the same token.
        // Sampling was uniform throughout, which is why this needs a test of its own rather
        // than showing up in the depth distribution.
        let mut counts = [0usize; N_MOVES];
        for seed in 0..SEEDS {
            let text = scramble(&mut StdRng::seed_from_u64(seed));
            let (axis, power) = *parse(&text).last().expect("eleven turns");
            counts[column(axis, power)] += 1;
        }
        let seen = counts.iter().filter(|&&count| count > 0).count();
        let worst = counts.iter().copied().max().unwrap_or(0);
        assert!(
            seen >= 4,
            "only {seen} of the {N_MOVES} turns ever ended a scramble in {SEEDS} seeds: {counts:?}"
        );
        assert!(
            worst as u64 * 5 <= SEEDS * 3,
            "one turn ended {worst} of {SEEDS} scrambles, over three fifths: {counts:?}"
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
        // The table's own mean is 8.3636; the band is wide enough never to flake.
        let mean = total as f64 / SAMPLE_SEEDS as f64;
        assert!((8.15..=8.55).contains(&mean), "mean sampled depth {mean}, expected about 8.36");
    }
}
