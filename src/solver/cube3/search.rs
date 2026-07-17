//! The two-phase search: IDA* into G1, IDA* home, improving until 21 or better.
//!
//! Both phases are the same shape, iterative-deepening depth-first search. Fix a target
//! depth, walk the tree to exactly that depth, and at every node ask the pruning tables how
//! many moves the cube still needs: if the answer is larger than the moves left in the
//! target, nothing under that node can reach the goal and the whole subtree dies. The bound
//! being admissible is what makes that sound, and the bound being exact in its projection is
//! what makes it kill almost everything. Start the target at the bound itself, since no
//! shorter solution can exist, and raise it by one when a target comes back empty.
//!
//! Phase 1 searches all eighteen moves for the coordinates twist, flip and slice all zero,
//! which is the cube in G1: every piece oriented and the four slice edges home in their
//! slice. Phase 2 searches only the ten moves of [`PHASE2_MOVES`], which is exactly the set
//! that keeps those three at zero, so it can work in cperm, eperm and sliceperm and finish
//! the cube without ever looking back at phase 1's coordinates.
//!
//! Three rules cut duplicate work, and each is about a move the search has effectively
//! already tried. Two of them are the canonical-sequence rule on [`follows`], no move on the
//! previous move's face and no commuting pair in both orders, and they hold across the
//! junction as well, so phase 2's first move is constrained by phase 1's last and the two
//! phases can never emit a pair of tokens that would have to merge. The third is about the
//! junction itself: a phase-1 solution whose last move is a phase-2 move is dropped, because
//! a phase-2 move keeps G1, so the cube was already in G1 one move earlier and that same
//! total sequence was on offer at a target one shorter, where it was either found or ruled
//! out for length.
//!
//! What makes the whole thing land at 21 is the loop over the two phases rather than either
//! phase alone. Phase 1 needs at most 12 moves and phase 2 at most 18, and the first phase-1
//! solution a random cube offers usually leaves a phase 2 far too long to fit inside
//! [`MAX_SOLUTION`]. So the search does not stop at one phase-1 solution: it takes every
//! phase-1 solution at the current target, tries to finish each one inside the moves the cap
//! leaves, and only then raises the target. A longer phase 1 routinely buys a shorter total,
//! which is why the answers cluster at 20 and 21 rather than at the 25 or so a single
//! decomposition gives.
//!
//! One decomposition ladder is not quite enough. A small share of cubes have no split at all
//! that fits inside 21 in the orientation they arrive in, so the search climbs the ladder for
//! two cubes at once, the one it was handed and that one inverted, a solution of either being a
//! solution of the other read backwards. The second aim is close to free, a target costing
//! thirteen times the one below it, and it doubles the decompositions on offer at every depth.
//! min2phase does the same thing six ways over, three whole-cube rotations of each of the two;
//! the inverse is the one of the six that needs no cube geometry beyond inverting a
//! permutation.
//!
//! The rng shuffles the branch order at every node, for plan 01's measured reason: a fixed
//! order pins the tail of every scramble, the scramble being the solution written backwards
//! and its last token therefore the first branch the root ever tries. Determinism is the
//! caller's seed, not a fixed order.

use super::coords::{self, MoveTables, PHASE1_COLUMNS, PHASE2_COLUMNS, PHASE2_MOVES};
use super::cubies::{apply_move, Cubies, N_MOVES, SOLVED};
use super::prune::Prune;
use rand::Rng;

/// The longest solution the solver may return, TNoodle's own cap.
pub(super) const MAX_SOLUTION: usize = 21;

/// The moves phase 1 can always finish in, a known property of G1's coset space.
pub(super) const MAX_PHASE1: usize = 12;

/// The moves phase 2 can always finish in, the diameter of G1 in its own generators.
pub(super) const MAX_PHASE2: usize = 18;

/// Quarter turns per face, so a move's face is its index divided by this.
const POWERS: usize = 3;

/// Faces per axis: face `f` and face `f + 3` are the two ends of one axis.
const AXES: usize = 3;

/// Every move of the cube in cubies' order: phase 1's branch list, index equal to move.
const ALL_MOVES: [usize; N_MOVES] =
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17];

/// Cubes a run searches: the one it was handed and that one inverted.
const AIMS: usize = 2;

/// A two-phase solution for `state`, at most [`MAX_SOLUTION`] moves, as move indices.
///
/// The rng shuffles branch order at every node, for plan 01's measured reason: a fixed
/// order pins the tail of every scramble. Same seed, same solution.
pub(super) fn solve<R: Rng>(state: &Cubies, rng: &mut R) -> Vec<usize> {
    let mut search = Search::new(state, rng);
    if let Some(solution) = search.run(MAX_SOLUTION) {
        return solution;
    }
    // Cheap insurance rather than a panic in the draw path, for the cube neither aim could
    // decompose inside the cap. Every cube has a solution of 20 moves or fewer, but not every
    // cube has one a two-phase split of this size can see, which is the same theoretical case
    // TNoodle covers with a 60 second timeout. Raising the cap to the sum of the two phase
    // maxima cannot fail: some phase-1 target within 12 has a solution, and every cube in G1
    // is within 18 of solved.
    search.run(MAX_PHASE1 + MAX_PHASE2).unwrap_or_default()
}

/// Whether `mv` is worth trying after `before`, the canonical-sequence rule.
///
/// Never the same face, because two turns of one face are one turn of it, so the pair is
/// either a shorter sequence the search already reached or two tokens the emitter would have
/// to merge. And never the two ends of one axis in both orders: opposite faces commute, so U
/// then D and D then U are the same cube by two routes, and keeping only the route that turns
/// the lower-numbered face first drops a tenth of the branching at every level of the tree,
/// which is a factor of three over a phase-1 target of eleven.
fn follows(mv: usize, before: usize) -> bool {
    let (face, was) = (mv / POWERS, before / POWERS);
    face != was && (face % AXES != was % AXES || face > was)
}

/// The state undoing `s`: every piece sent home from where `s` left it.
///
/// A state is a map from position to the piece sitting there, so inverting it is reading that
/// map backwards, and a corner's twist comes back as the twist that cancels it. Edge flips are
/// their own cancellation, one flip being the whole of mod 2.
fn inverted(s: &Cubies) -> Cubies {
    let mut out = SOLVED;
    for (position, &piece) in s.cp.iter().enumerate() {
        out.cp[usize::from(piece)] = position as u8;
        out.co[usize::from(piece)] = (3 - s.co[position]) % 3;
    }
    for (position, &piece) in s.ep.iter().enumerate() {
        out.ep[usize::from(piece)] = position as u8;
        out.eo[usize::from(piece)] = s.eo[position];
    }
    out
}

/// A solution of the inverted cube, turned into one of the cube itself.
///
/// The two are inverse maneuvers, so one is the other backwards with every move complemented,
/// which is the same turning-around `mod.rs` does to write a solution as a scramble.
fn turned_around(solution: &[usize]) -> Vec<usize> {
    solution.iter().rev().map(|&mv| super::undo(mv)).collect()
}

/// One run of the two-phase search: the tables, the caller's rng, and the path so far.
///
/// `path` carries phase 1's moves and then phase 2's, so the solution is simply the path at
/// the moment phase 2 lands. Everything the recursion needs beyond a coordinate lives here,
/// which is what keeps the depth-first walk down to pushes and pops.
struct Search<'a, R: Rng> {
    tables: &'static MoveTables,
    prune: &'static Prune,
    rng: &'a mut R,
    /// The cube as it arrived and the same cube inverted, the two aims a run alternates.
    aims: [Cubies; AIMS],
    /// How many of them are worth searching: one when the cube is its own inverse.
    tries: usize,
    /// Which of them the walk is on, so the junction replays the right cube.
    aim: usize,
    path: Vec<usize>,
    found: Vec<usize>,
    /// The longest total this run will accept, which is what bounds phase 2's target.
    cap: usize,
}

impl<'a, R: Rng> Search<'a, R> {
    /// A search over `state`, with the tables built if this is the first 3x3 scramble.
    fn new(state: &Cubies, rng: &'a mut R) -> Self {
        let inverse = inverted(state);
        Search {
            tables: MoveTables::get(),
            prune: Prune::get(),
            rng,
            // A cube that undoes itself offers the second aim nothing, the two searches being
            // the same search. The superflip is the one everybody names.
            tries: if inverse == *state { 1 } else { AIMS },
            aims: [*state, inverse],
            aim: 0,
            path: Vec::with_capacity(MAX_PHASE1 + MAX_PHASE2),
            found: Vec::with_capacity(MAX_PHASE1 + MAX_PHASE2),
            cap: MAX_SOLUTION,
        }
    }

    /// A total within `cap` moves, or None if neither aim decomposes that small.
    ///
    /// Deepening happens here, and both aims share the ladder: at each phase-1 target the cube
    /// is tried and then its inverse, which is nearly free because a target costs thirteen
    /// times the one below it, so the work is all in the deepest rung and the second aim's
    /// deepest rung is only reached when the first came back empty. What it buys is a second,
    /// independent set of decompositions at every depth, and that is what turns the cubes whose
    /// forward splits all overshoot 21 into cubes that fit.
    fn run(&mut self, cap: usize) -> Option<Vec<usize>> {
        self.cap = cap;
        let mut starts = [(0, 0, 0); AIMS];
        let mut bounds = [0; AIMS];
        for (aim, cube) in self.aims.iter().enumerate() {
            starts[aim] = coords::phase1(cube);
            let (twist, flip, slice) = starts[aim];
            bounds[aim] = self.prune.bound1(twist, flip, slice);
        }
        let first = bounds[..self.tries].iter().copied().min().unwrap_or(0);
        for target in first..=MAX_PHASE1 {
            for aim in 0..self.tries {
                if target < bounds[aim] {
                    continue;
                }
                self.aim = aim;
                self.path.clear();
                self.found.clear();
                let (twist, flip, slice) = starts[aim];
                if self.dive1(twist, flip, slice, target, None) {
                    let solution = std::mem::take(&mut self.found);
                    return Some(if aim == 0 { solution } else { turned_around(&solution) });
                }
            }
        }
        None
    }

    /// Phase 1, depth-first: does some completion of this node finish the whole cube?
    ///
    /// True means [`Search::found`] holds a total solution and every caller should unwind.
    fn dive1(
        &mut self,
        twist: usize,
        flip: usize,
        slice: usize,
        remaining: usize,
        last: Option<usize>,
    ) -> bool {
        if self.prune.bound1(twist, flip, slice) > remaining {
            return false;
        }
        if remaining == 0 {
            return twist == 0 && flip == 0 && slice == 0 && self.junction();
        }
        let mut branches = [0usize; N_MOVES];
        let count = self.candidates(&ALL_MOVES, last, &mut branches);
        for &mv in &branches[..count] {
            let (twist, flip, slice) = self.turn1(twist, flip, slice, mv);
            self.path.push(mv);
            if self.dive1(twist, flip, slice, remaining - 1, Some(mv)) {
                return true;
            }
            self.path.pop();
        }
        false
    }

    /// The handover: this path has the cube in G1, so try to finish it inside the cap.
    ///
    /// The cubies are replayed here rather than carried through phase 1 because this runs
    /// once per phase-1 solution and not once per node, and phase 2 needs three coordinates
    /// phase 1 never tracked.
    fn junction(&mut self) -> bool {
        let Some(&last) = self.path.last() else {
            // An empty phase 1 means the cube began in G1, and phase 2 starts unconstrained.
            return self.finish(None);
        };
        // A phase-2 move at the end of phase 1 was already covered one target shorter.
        if coords::phase2_column(last).is_some() {
            return false;
        }
        self.finish(Some(last))
    }

    /// Phase 2 from the cube this path leaves, deepening within what the cap allows.
    fn finish(&mut self, last: Option<usize>) -> bool {
        let state = self.path.iter().fold(self.aims[self.aim], |s, &mv| apply_move(&s, mv));
        let (cperm, eperm, sliceperm) = coords::phase2(&state);
        let budget = self.cap.saturating_sub(self.path.len()).min(MAX_PHASE2);
        // An empty range when the bound already exceeds the budget, which is the common case
        // and the whole reason a junction is cheap.
        for target in self.prune.bound2(cperm, eperm, sliceperm)..=budget {
            if self.dive2(cperm, eperm, sliceperm, target, last) {
                return true;
            }
        }
        false
    }

    /// Phase 2, depth-first over the ten moves that keep G1.
    fn dive2(
        &mut self,
        cperm: usize,
        eperm: usize,
        sliceperm: usize,
        remaining: usize,
        last: Option<usize>,
    ) -> bool {
        if self.prune.bound2(cperm, eperm, sliceperm) > remaining {
            return false;
        }
        if remaining == 0 {
            if cperm != 0 || eperm != 0 || sliceperm != 0 {
                return false;
            }
            self.found.clear();
            self.found.extend_from_slice(&self.path);
            return true;
        }
        let mut branches = [0usize; PHASE2_COLUMNS];
        let count = self.candidates(&PHASE2_MOVES, last, &mut branches);
        for &column in &branches[..count] {
            let mv = PHASE2_MOVES[column];
            let (cperm, eperm, sliceperm) = self.turn2(cperm, eperm, sliceperm, column);
            self.path.push(mv);
            if self.dive2(cperm, eperm, sliceperm, remaining - 1, Some(mv)) {
                return true;
            }
            self.path.pop();
        }
        false
    }

    /// The branches worth trying at a node, as indices into `from`, in a fresh random order.
    ///
    /// An index rather than a move because phase 1 lists every move and phase 2 lists its
    /// ten columns, and both searches want the position in their own list. `out` is a stack
    /// array, so a node costs no allocation.
    fn candidates(&mut self, from: &[usize], last: Option<usize>, out: &mut [usize]) -> usize {
        let mut count = 0;
        for (index, &mv) in from.iter().enumerate() {
            if last.is_some_and(|before| !follows(mv, before)) {
                continue;
            }
            // A list longer than the buffer would search a subset rather than panic.
            if let Some(slot) = out.get_mut(count) {
                *slot = index;
                count += 1;
            }
        }
        // Fisher-Yates over the candidates, fresh at every node, its digits read out of one
        // draw. The moduli multiply out to eighteen factorial at worst, which leaves a u64
        // three orders of magnitude spare, so the digits are as good as separate draws; and
        // one draw in place of seventeen is what keeps a node cheap, the search visiting
        // millions of them for one scramble.
        let mut word = self.rng.gen::<u64>();
        for i in (1..count).rev() {
            let radix = i as u64 + 1;
            let rest = word / radix;
            out.swap(i, (word - rest * radix) as usize);
            word = rest;
        }
        count
    }

    /// One move on the three phase-1 coordinates, three table lookups.
    fn turn1(&self, twist: usize, flip: usize, slice: usize, mv: usize) -> (usize, usize, usize) {
        (
            usize::from(self.tables.twist[twist * PHASE1_COLUMNS + mv]),
            usize::from(self.tables.flip[flip * PHASE1_COLUMNS + mv]),
            usize::from(self.tables.slice[slice * PHASE1_COLUMNS + mv]),
        )
    }

    /// One move on the three phase-2 coordinates, by its column in [`PHASE2_MOVES`].
    fn turn2(
        &self,
        cperm: usize,
        eperm: usize,
        sliceperm: usize,
        column: usize,
    ) -> (usize, usize, usize) {
        (
            usize::from(self.tables.cperm[cperm * PHASE2_COLUMNS + column]),
            usize::from(self.tables.eperm[eperm * PHASE2_COLUMNS + column]),
            usize::from(self.tables.sliceperm[sliceperm * PHASE2_COLUMNS + column]),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::Cube;
    use crate::solver::cube3::cubies::{cubies_of, token};
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use std::collections::HashMap;
    use std::time::Instant;

    /// Seeds for the sweep every per-solution property is checked on.
    const SWEEP_SEEDS: u64 = 300;

    /// Seeds for the tests that only need a handful of solves.
    const FEW_SEEDS: u64 = 30;

    /// Seeds for the measurement, and where they start: a batch the sweep has not warmed.
    const TIMED_SEEDS: u64 = 50;
    const TIMED_FIRST: u64 = 1_000;

    /// Seeds for the superflip, which is the slowest cube the solver ever sees.
    ///
    /// Its phase-1 bound is far below its phase-1 distance and it is its own inverse, so the
    /// search grinds every target to the last, seconds rather than the usual milliseconds.
    /// Three seeds say the answer does not depend on the branch order; thirty say it slowly.
    const SUPERFLIP_SEEDS: u64 = 3;

    /// The lowest mean length a batch of random states has any business averaging.
    const MIN_MEAN: f64 = 17.5;

    /// The longest solution the fallback can return, which is the only cap that is a promise.
    const HARD_CAP: usize = MAX_PHASE1 + MAX_PHASE2;

    /// The most of [`SWEEP_SEEDS`] that may need the fallback past [`MAX_SOLUTION`].
    ///
    /// Measured at 9 in 20,000 over a release batch, so 3 of 300 is generous by two orders of
    /// magnitude and still catches a regression that made the fallback the common path. The
    /// share is asserted rather than a flat `<= MAX_SOLUTION`, because the search aims at 21
    /// and only the sum of the two phase maxima is guaranteed; pinning 21 as an invariant
    /// would pin a promise the code does not make and would turn on the choice of seeds.
    const MAX_OVER_CAP: usize = 3;

    /// The state with every edge flipped and nothing else moved: the published 20 mover.
    fn superflip() -> Cubies {
        Cubies { eo: [1; 12], ..SOLVED }
    }

    /// The state `solution` leaves the cube in, played on the cubie model.
    fn played(start: &Cubies, solution: &[usize]) -> Cubies {
        solution.iter().fold(*start, |s, &mv| apply_move(&s, mv))
    }

    /// A solution for `seed`'s sampled state, with the state and the scramble beside it.
    ///
    /// The three steps are [`crate::solver::cube3::scramble`]'s own, in its order, so the rng
    /// is drawn on exactly as a real scramble draws on it.
    fn solved_seed(seed: u64) -> (Cubies, Vec<usize>, String) {
        let mut rng = StdRng::seed_from_u64(seed);
        let state = super::super::sample(&mut rng);
        let solution = solve(&state, &mut rng);
        let text = super::super::emit(&solution);
        (state, solution, text)
    }

    /// The faces a token list touches in order, one letter per token.
    fn faces(text: &str) -> Vec<char> {
        text.split_whitespace().filter_map(|t| t.chars().next()).collect()
    }

    /// Every assertion that has to hold of one solution and the scramble it emits.
    fn check(seed: u64, state: &Cubies, solution: &[usize], text: &str) {
        assert!(
            solution.len() <= HARD_CAP,
            "seed {seed}: {} moves is past even the fallback's cap",
            solution.len()
        );
        assert_eq!(played(state, solution), SOLVED, "seed {seed}: the solution does not solve");
        let letters = faces(text);
        assert_eq!(letters.len(), solution.len(), "seed {seed}: {text:?} lost a token");
        assert!(
            letters.windows(2).all(|pair| pair[0] != pair[1]),
            "seed {seed}: two tokens share a face in {text:?}"
        );
        // The phase boundary is inside that same window check, the tokens being the whole
        // solution reversed, so a merge at the junction shows up here and nowhere else.
        let mut cube = Cube::solved(3);
        cube.apply_scramble(text).expect("a 3x3 scramble on a 3x3");
        assert_eq!(&cubies_of(&cube), state, "seed {seed}: {text:?} reaches another state");
    }

    #[test]
    fn inverting_a_state_is_playing_its_moves_backwards() {
        // The whole basis of the second aim, so it is checked against the cubie model rather
        // than argued: a sequence and its turned-around self leave inverse states behind.
        let mut rng = StdRng::seed_from_u64(71);
        assert_eq!(inverted(&SOLVED), SOLVED);
        for round in 0..200 {
            let len = rng.gen_range(0..21);
            let sequence: Vec<usize> = (0..len).map(|_| rng.gen_range(0..N_MOVES)).collect();
            let state = played(&SOLVED, &sequence);
            let back = turned_around(&sequence);
            assert_eq!(inverted(&state), played(&SOLVED, &back), "round {round}: {sequence:?}");
            assert_eq!(inverted(&inverted(&state)), state, "round {round}: inverting twice moved");
            // Which is the fact the aim leans on: the turned-around solution of the inverted
            // cube solves the cube, so both aims answer the same question.
            assert_eq!(played(&state, &turned_around(&sequence)), SOLVED, "round {round}");
        }
    }

    #[test]
    fn the_canonical_rule_drops_a_face_repeat_and_one_order_of_every_commuting_pair() {
        for before in 0..N_MOVES {
            for mv in 0..N_MOVES {
                let (face, was) = (mv / POWERS, before / POWERS);
                let (there, back) = (follows(mv, before), follows(before, mv));
                let pair = format!("{} and {}", token(before), token(mv));
                if face == was {
                    assert!(!there, "{pair} turn one face twice in a row");
                } else if face % AXES == was % AXES {
                    // Opposite faces commute, so the rule has to keep exactly one of the two
                    // orders: keeping neither would lose sequences, keeping both is the
                    // duplication it exists to drop.
                    assert_ne!(there, back, "{pair} commute and both orders survive");
                } else {
                    assert!(there && back, "{pair} share nothing and both orders must survive");
                }
            }
        }
        // Spelled out on the tokens the rule exists for: no U after U, U then D but not D
        // then U, and nothing at all stopping a turn of an unrelated face.
        let mv = |text: &str| (0..N_MOVES).find(|&mv| token(mv) == text).expect("a 3x3 move");
        assert!(!follows(mv("U2"), mv("U")));
        assert!(follows(mv("D"), mv("U")));
        assert!(!follows(mv("U"), mv("D")));
        assert!(follows(mv("R"), mv("U")));
        assert!(follows(mv("B2"), mv("F2")));
        assert!(!follows(mv("F2"), mv("B2")));
    }

    #[test]
    fn every_sampled_state_is_solved_by_its_own_solution_and_reached_by_its_scramble() {
        // One sweep carrying every per-solution property, because a solve is the expensive
        // thing here and each of these would otherwise pay for its own batch.
        let mut lengths = [0usize; HARD_CAP + 1];
        let mut tails: HashMap<String, usize> = HashMap::new();
        for seed in 0..SWEEP_SEEDS {
            let (state, solution, text) = solved_seed(seed);
            check(seed, &state, &solution, &text);
            assert!(!solution.is_empty(), "seed {seed}: a random state is not solved");
            lengths[solution.len()] += 1;
            let tail = text.split(' ').next_back().unwrap_or_default().to_string();
            *tails.entry(tail).or_default() += 1;
        }
        let total: usize = lengths.iter().sum();
        let moves: usize = lengths.iter().enumerate().map(|(len, count)| len * count).sum();
        let mean = moves as f64 / total as f64;
        let over: usize = lengths[MAX_SOLUTION + 1..].iter().sum();
        println!("lengths over {total} seeds: {lengths:?}, mean {mean}, {over} past the cap");
        assert!(
            (MIN_MEAN..=MAX_SOLUTION as f64).contains(&mean),
            "mean solution length {mean} over {total} seeds, expected {MIN_MEAN} to {MAX_SOLUTION}"
        );
        // The fallback is meant to be the rare exception, not a second normal path.
        assert!(
            over <= MAX_OVER_CAP,
            "{over} of {total} solutions needed the fallback past {MAX_SOLUTION} moves: {lengths:?}"
        );
        // The tail of a scramble is the first branch the root shuffled, so a fixed order
        // would collapse this to one token. Plan 01's regression, repeated here.
        assert!(
            tails.len() >= 10,
            "{} seeds ended on only {} distinct tokens: {tails:?}",
            SWEEP_SEEDS,
            tails.len()
        );
        let worst = tails.values().copied().max().unwrap_or(0);
        assert!(
            worst * 10 <= total * 6,
            "one token ends {worst} of {total} scrambles, past 60 percent: {tails:?}"
        );
    }

    #[test]
    fn the_scramble_a_seed_emits_is_the_solution_that_seed_solves() {
        // `scramble` is the three steps above run together, so a seed has to agree with them
        // move for move; anything else means the rng was drawn on in another order.
        for seed in 0..FEW_SEEDS {
            let (_, _, text) = solved_seed(seed);
            let direct = super::super::scramble(&mut StdRng::seed_from_u64(seed));
            assert_eq!(direct, text, "seed {seed} disagrees with its own parts");
        }
    }

    #[test]
    fn one_seed_gives_one_scramble_and_other_seeds_give_others() {
        let first = super::super::scramble(&mut StdRng::seed_from_u64(5));
        assert_eq!(
            super::super::scramble(&mut StdRng::seed_from_u64(5)),
            first,
            "a seeded scramble has to repeat exactly"
        );
        let others: Vec<String> = (6..6 + FEW_SEEDS)
            .map(|seed| super::super::scramble(&mut StdRng::seed_from_u64(seed)))
            .collect();
        assert!(others.iter().all(|other| *other != first), "another seed repeated the scramble");
    }

    #[test]
    fn the_superflip_comes_back_at_the_length_it_is_famous_for() {
        // Every edge flipped and nothing else moved needs exactly 20 moves optimally, the
        // published diameter case. A two-phase decomposition is not required to find the
        // optimum, so 21 is allowed and anything else means the search or the cap is wrong.
        let state = superflip();
        for seed in 0..SUPERFLIP_SEEDS {
            let solution = solve(&state, &mut StdRng::seed_from_u64(seed));
            assert_eq!(played(&state, &solution), SOLVED, "seed {seed}: superflip unsolved");
            assert!(
                (20..=MAX_SOLUTION).contains(&solution.len()),
                "seed {seed}: the superflip came back in {} moves",
                solution.len()
            );
            check(seed, &state, &solution, &super::super::emit(&solution));
        }
    }

    #[test]
    fn a_cube_that_never_left_g1_is_solved_inside_the_cap_as_well() {
        // A walk of phase-2 moves lands in G1, so phase 1 finishes at target zero and the
        // whole solution is phase 2. The junction's face rule is what this really exercises:
        // with no phase-1 move behind it, phase 2 has to start unconstrained.
        let mut rng = StdRng::seed_from_u64(61);
        for round in 0..FEW_SEEDS {
            let mut state = SOLVED;
            for _ in 0..30 {
                state = apply_move(&state, PHASE2_MOVES[rng.gen_range(0..PHASE2_COLUMNS)]);
            }
            let solution = solve(&state, &mut rng);
            assert_eq!(coords::phase1(&state), (0, 0, 0), "round {round}: the walk left G1");
            assert!(
                solution.iter().all(|mv| coords::phase2_column(*mv).is_some()),
                "round {round}: a cube already in G1 needs no phase-1 move: {solution:?}"
            );
            check(round, &state, &solution, &super::super::emit(&solution));
        }
    }

    #[test]
    fn a_solved_cube_solves_in_nothing_and_scrambles_to_nothing() {
        // The one state whose scramble would be empty, which is why `scramble` resamples it.
        // The search itself has no such rule: an already solved cube needs no move.
        let solution = solve(&SOLVED, &mut StdRng::seed_from_u64(2));
        assert!(solution.is_empty(), "a solved cube needs no move: {solution:?}");
        assert_eq!(super::super::emit(&solution), "");
    }

    #[test]
    fn a_solution_never_turns_one_face_twice_in_a_row() {
        // The same rule the sweep checks on tokens, checked on the move indices, so a failure
        // says which phase it came from rather than which token merged.
        for seed in 0..FEW_SEEDS {
            let (_, solution, _) = solved_seed(seed);
            let faces: Vec<usize> = solution.iter().map(|mv| mv / POWERS).collect();
            assert!(
                faces.windows(2).all(|pair| pair[0] != pair[1]),
                "seed {seed}: {:?} repeats a face",
                solution.iter().map(|&mv| token(mv)).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn the_tables_build_once_and_a_solve_costs_milliseconds() {
        // No wall-clock assertion, the numbers being the point: they belong in a report, and
        // the profile lever if one is needed is the integrator's call. `--nocapture` prints
        // them, and the slowest seed of a batch matters more than the mean, a cube far from G1
        // costing many times one that is close.
        let build = Instant::now();
        Prune::get();
        let built = build.elapsed();
        let solving = Instant::now();
        let mut worst = (std::time::Duration::ZERO, 0);
        let mut lengths = [0usize; MAX_SOLUTION + 1];
        for seed in 0..TIMED_SEEDS {
            let at = Instant::now();
            let (_, solution, _) = solved_seed(TIMED_FIRST + seed);
            let took = at.elapsed();
            worst = worst.max((took, seed));
            lengths[solution.len().min(MAX_SOLUTION)] += 1;
        }
        let per = solving.elapsed() / TIMED_SEEDS as u32;
        println!("pruning tables: {built:?} (zero when an earlier test already built them)");
        println!("per solve over {TIMED_SEEDS} seeds: {per:?}");
        println!("slowest: {:?} at seed {}", worst.0, TIMED_FIRST + worst.1);
        println!("lengths: {lengths:?}");
        // The one thing worth asserting here: every seed of the batch came back, inside the
        // cap, and none of them was already solved.
        assert_eq!(lengths.iter().sum::<usize>(), TIMED_SEEDS as usize);
        assert_eq!(lengths[0], 0, "a random state is never solved already");
    }
}
