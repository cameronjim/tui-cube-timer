//! Random-state scrambles for the events small enough to solve exhaustively.
//!
//! Each puzzle module encodes states as indices with 0 solved, and the shared [`Engine`]
//! does the rest: a breadth-first distance table over every state, uniform sampling of
//! reachable states, and the exact-length canonical search TNoodle uses, whose inverse
//! is the scramble. Method and fixtures in `claude-docs/plans/01-random-state-scrambles.md`.

mod cube2;
mod pyraminx;
mod skewb;

use crate::types::Puzzle;
use rand::Rng;

/// A random-state scramble, or None for the events that still scramble by random moves.
pub fn scramble<R: Rng>(puzzle: Puzzle, rng: &mut R) -> Option<String> {
    match puzzle {
        Puzzle::Cube2 => Some(cube2::scramble(rng)),
        Puzzle::Pyraminx => Some(pyraminx::scramble(rng)),
        Puzzle::Skewb => Some(skewb::scramble(rng)),
        _ => None,
    }
}

/// One puzzle's move structure, everything the shared machinery needs to know.
///
/// Moves are (axis, power) pairs: `axes` turnable axes, powers running 1..=`powers`
/// clockwise quarter turns. `apply` must be pure and total over `0..states`.
pub(super) struct Engine {
    pub states: usize,
    pub axes: usize,
    pub powers: usize,
    pub apply: fn(usize, usize, usize) -> usize,
}

/// Distance value marking a state the search never reached.
pub(super) const UNREACHABLE: u8 = u8::MAX;

/// Upper bound on the moves the exact-length search shuffles at one node.
///
/// The largest puzzle here offers nine, three axes times three powers, so a stack array of
/// this size keeps the shuffle out of the allocator on a function that recurses eleven deep.
const MAX_MOVES: usize = 16;

impl Engine {
    /// Exact distance to solved for every state index, [`UNREACHABLE`] where no path exists.
    pub fn distances(&self) -> Vec<u8> {
        let mut dist = vec![UNREACHABLE; self.states];
        dist[0] = 0;
        let mut frontier = vec![0usize];
        let mut depth: u8 = 0;
        while !frontier.is_empty() {
            let mut next = Vec::new();
            for &s in &frontier {
                for axis in 0..self.axes {
                    for power in 1..=self.powers {
                        let t = (self.apply)(s, axis, power);
                        if dist[t] == UNREACHABLE {
                            dist[t] = depth + 1;
                            next.push(t);
                        }
                    }
                }
            }
            depth += 1;
            frontier = next;
        }
        dist
    }

    /// A uniformly random reachable state, by rejection against the distance table.
    ///
    /// Uniformity is a property of the table, not of any parity arithmetic here: every
    /// index the breadth-first search reached is equally likely, and nothing else can win.
    pub fn random_reachable<R: Rng>(&self, dist: &[u8], rng: &mut R) -> usize {
        loop {
            let s = rng.gen_range(0..self.states);
            if dist[s] != UNREACHABLE {
                return s;
            }
        }
    }

    /// A canonical solution of exactly `len` moves from `start`, if one exists.
    ///
    /// Canonical means no two consecutive moves on one axis, which is also why a padded
    /// solution cannot silently cancel with itself. The distance table prunes every branch
    /// that could not reach solved in the moves remaining.
    ///
    /// One state does not yield one solution: a state of this length usually has many, and
    /// `rng` decides which comes back, because the branches at every node are tried in a
    /// freshly shuffled order. A fixed order would instead pin the whole tail of every
    /// emitted scramble, since the scramble is the solution written backwards and the first
    /// move of the solution would always be the first move the search tried. Determinism
    /// therefore comes from the caller's rng: the same seed still gives the same scramble.
    pub fn solve_exactly<R: Rng>(
        &self,
        start: usize,
        len: usize,
        dist: &[u8],
        rng: &mut R,
    ) -> Option<Vec<(usize, usize)>> {
        debug_assert!(
            self.axes * self.powers <= MAX_MOVES,
            "a puzzle with more moves per node than the shuffle buffer holds"
        );
        let mut path = Vec::with_capacity(len);
        if self.dfs(start, len, None, dist, rng, &mut path) {
            Some(path)
        } else {
            None
        }
    }

    /// The search behind [`Engine::solve_exactly`], depth-first with table pruning.
    fn dfs<R: Rng>(
        &self,
        s: usize,
        remaining: usize,
        last_axis: Option<usize>,
        dist: &[u8],
        rng: &mut R,
        path: &mut Vec<(usize, usize)>,
    ) -> bool {
        if remaining == 0 {
            return s == 0;
        }
        if usize::from(dist[s]) > remaining {
            return false;
        }
        let mut moves = [(0usize, 0usize); MAX_MOVES];
        let mut count = 0;
        for axis in 0..self.axes {
            if last_axis == Some(axis) {
                continue;
            }
            for power in 1..=self.powers {
                // A puzzle past MAX_MOVES searches a subset rather than indexing off the end.
                if let Some(slot) = moves.get_mut(count) {
                    *slot = (axis, power);
                    count += 1;
                }
            }
        }
        // Fisher-Yates over the candidates, fresh at every node.
        for i in (1..count).rev() {
            moves.swap(i, rng.gen_range(0..=i));
        }
        for &(axis, power) in &moves[..count] {
            let t = (self.apply)(s, axis, power);
            path.push((axis, power));
            if self.dfs(t, remaining - 1, Some(axis), dist, rng, path) {
                return true;
            }
            path.pop();
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use std::collections::HashSet;

    /// A toy two-axis puzzle over Z/8: axis 0 adds its power, axis 1 subtracts it.
    ///
    /// Small enough to check the engine's three operations by hand before the real
    /// puzzles lean on them.
    fn toy() -> Engine {
        fn apply(s: usize, axis: usize, power: usize) -> usize {
            match axis {
                0 => (s + power) % 8,
                _ => (s + 8 - power) % 8,
            }
        }
        Engine { states: 8, axes: 2, powers: 2, apply }
    }

    #[test]
    fn the_toy_distance_table_is_exact_and_total() {
        let dist = toy().distances();
        // From 0, one move reaches 1, 2, 6, 7; two moves cover the rest.
        assert_eq!(dist, vec![0, 1, 1, 2, 2, 2, 1, 1]);
    }

    /// Whether any canonical sequence of exactly `len` moves takes `s` to solved.
    fn brute_force(eng: &Engine, s: usize, len: usize, last_axis: Option<usize>) -> bool {
        if len == 0 {
            return s == 0;
        }
        (0..eng.axes).filter(|&axis| last_axis != Some(axis)).any(|axis| {
            (1..=eng.powers)
                .any(|power| brute_force(eng, (eng.apply)(s, axis, power), len - 1, Some(axis)))
        })
    }

    #[test]
    fn the_exact_length_search_agrees_with_brute_force_and_its_answers_solve() {
        let eng = toy();
        let dist = eng.distances();
        let mut rng = StdRng::seed_from_u64(3);
        for start in 0..8 {
            for len in 0..7 {
                // Shuffling the branch order cannot change whether a solution exists, so the
                // agreement asserted here is the same one it was before the order was random.
                let sol = eng.solve_exactly(start, len, &dist, &mut rng);
                let expect = brute_force(&eng, start, len, None);
                assert_eq!(sol.is_some(), expect, "start {start}, exact length {len}");
                let Some(sol) = sol else { continue };
                assert_eq!(sol.len(), len);
                let mut s = start;
                let mut last = None;
                for &(axis, power) in &sol {
                    assert_ne!(last, Some(axis), "consecutive moves share an axis");
                    last = Some(axis);
                    s = (eng.apply)(s, axis, power);
                }
                assert_eq!(s, 0, "the solution must end solved");
            }
        }
    }

    #[test]
    fn one_state_yields_many_different_solutions_and_the_same_one_per_seed() {
        let eng = toy();
        let dist = eng.distances();
        // Canonical on two axes means the signs alternate, so a length has to be generous
        // before the toy has room for many solutions: state 1 in seven moves has 42 of them.
        const START: usize = 1;
        const LEN: usize = 7;
        let mut seen = HashSet::new();
        for seed in 0..200u64 {
            let sol = eng
                .solve_exactly(START, LEN, &dist, &mut StdRng::seed_from_u64(seed))
                .unwrap_or_else(|| panic!("seed {seed} found no {LEN} move solution"));
            // The rng is the whole source of the difference, so a seed has to repeat exactly.
            assert_eq!(
                Some(sol.clone()),
                eng.solve_exactly(START, LEN, &dist, &mut StdRng::seed_from_u64(seed)),
                "seed {seed} is not deterministic"
            );
            seen.insert(sol);
        }
        assert!(
            seen.len() >= 20,
            "200 seeds returned only {} distinct solutions, so the order is barely shuffled",
            seen.len()
        );
        // The first move is the one a fixed order pinned, and it is the token a scramble ends on.
        let firsts: HashSet<(usize, usize)> =
            seen.iter().filter_map(|sol| sol.first().copied()).collect();
        assert_eq!(
            firsts.len(),
            eng.axes * eng.powers,
            "some first move never came back: {firsts:?}"
        );
    }

    #[test]
    fn sampling_only_ever_returns_reachable_states() {
        let eng = toy();
        // Poison half the table to prove rejection respects it.
        let mut dist = eng.distances();
        for s in [1, 3, 5, 7] {
            dist[s] = UNREACHABLE;
        }
        let mut rng = StdRng::seed_from_u64(5);
        for _ in 0..200 {
            let s = eng.random_reachable(&dist, &mut rng);
            assert!(dist[s] != UNREACHABLE);
        }
    }
}
