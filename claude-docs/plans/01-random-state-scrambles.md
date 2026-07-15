# Plan 01: random-state scrambles for 2x2, Pyraminx and Skewb

Phase two of the cube engine. Today every scramble is random moves; the WCA stopped
scrambling most events that way because random moves land on easy states more often
than fair ones. The official generator, TNoodle, instead picks a uniformly random
legal state and hands you the exact move sequence that produces it. This phase brings
that to the three events whose state spaces are small enough to solve exhaustively:
2x2 (3,674,160 states), Pyraminx (933,120) and Skewb (3,149,280).

## The method, in one paragraph

Every reachable state gets an index. A breadth-first search outward from the solved
state fills a table with the exact distance to solved for every index; the table for
the biggest of the three puzzles is under 20 MB and builds in well under a second in
release. A scramble is then: draw a uniformly random reachable state, search for a
solution of an exact target length (the distance table prunes that search to almost
nothing), and emit the inverse of the solution. The distance table doubles as proof
of correctness, because the count of states at each depth is published from prior
exhaustive searches, and a single wrong move definition changes those counts.

## TNoodle's emission rules, verified against its source

| Event | Moves | Suffixes | Length | Sampling |
|---|---|---|---|---|
| 2x2 | `U R F` | `'` and `2` | exactly 11 tokens | permutation and orientation coordinates drawn independently and uniformly; every pair is reachable |
| Pyraminx | `U L R B`, tips `u l r b` | `'` only | exactly 11 core tokens, then one token per unsolved tip, tips last | edge permutation drawn until even, edge orientation and axial orientation uniform, tips uniform and independent |
| Skewb | `R U L B` | `'` only | exactly 11 tokens | uniform over the 3,149,280 reachable states |

TNoodle finds the solution with `generateExactly(state, 11)`: a depth-first search
for a canonical solution of exactly 11 moves (no two consecutive moves on one axis),
retrying at 12 in the theoretical case none exists. The scramble is the inverse.
Solutions shorter than 11 padded out this way are why real 2x2 scrambles are always
11 moves even though the average optimal solution is 8.76.

## The correctness fixtures, from published exhaustive searches

The suite must reproduce every row. These are Jaap Scherphuis's God's-algorithm
counts; getting all twelve right per puzzle while summing to the exact total leaves
no room for a wrong permutation, orientation delta or missed parity constraint.

| Depth | 2x2 | Pyraminx | Skewb |
|---|---|---|---|
| 0 | 1 | 1 | 1 |
| 1 | 9 | 8 | 8 |
| 2 | 54 | 48 | 48 |
| 3 | 321 | 288 | 288 |
| 4 | 1,847 | 1,728 | 1,728 |
| 5 | 9,992 | 9,896 | 10,248 |
| 6 | 50,136 | 51,808 | 59,304 |
| 7 | 227,536 | 220,111 | 315,198 |
| 8 | 870,072 | 480,467 | 1,225,483 |
| 9 | 1,887,748 | 166,276 | 1,455,856 |
| 10 | 623,800 | 2,457 | 81,028 |
| 11 | 2,644 | 32 | 90 |
| Total | 3,674,160 | 933,120 | 3,149,280 |

Pyraminx counts exclude tips (independent, 81 combinations, handled separately).

## Module boundaries

| File | Responsibility | Owner |
|---|---|---|
| `src/solver/mod.rs` | The shared engine: BFS distance tables, exact-length canonical search, uniform reachable sampling, and the per-puzzle dispatch. Scaffolded complete. | Fable (scaffold), integrator |
| `src/solver/cube2.rs` | 2x2 coordinates, move tables, TNoodle-shape scrambles. | Agent A |
| `src/solver/pyraminx.rs` | Pyraminx coordinates, move tables, tips, scrambles. | Agent B |
| `src/solver/skewb.rs` | Skewb coordinates, move tables, scrambles. | Agent C |
| `src/scramble/` rewiring, docs | Dispatch the three events to `solver`, delete the replaced random-move generators, update every doc tier. | Integrator |

`solver` sits beside `scramble` and `cube`: `scramble` produces strings, `cube`
models facelets for the preview, `solver` knows distances and produces the strings
for random-state events. `scramble::generate` stays the only public path in; nothing
else changes for `app` or `ui`.

## The state encoding discipline

Each puzzle module encodes a state as one usize built from independent coordinates.
Orientation coordinates are indexed by position, which keeps every coordinate's
transition a function of that coordinate and the move alone, so each coordinate gets
its own small move table (built once by decode, apply, encode over just that
coordinate's range) and the full-state `apply` is table lookups only. Tables and the
distance table live behind a `OnceLock`, built on first use, shared by everything
including tests.

Encodings may span more than the reachable count (Pyraminx's 720 edge permutations
are half unreachable); the BFS marks unreachable indices 255 and sampling rejects
them, which is what makes uniformity a property of the table rather than of parity
arithmetic that could silently be wrong. Keep the encoded space within a small
multiple of the reachable count, and never above roughly 20 million.

Index 0 must be the solved state. Move semantics must mirror TNoodle's solver for
that puzzle (`TwoByTwoSolver.java`, `PyraminxSolver.java`, `SkewbSolver.java` in
thewca/tnoodle-lib), because the notation's physical meaning is defined by what the
official generator does; fetch the source and document the mapping in the module
header. The depth distribution cannot catch a relabeled axis, so the source mapping
plus CJ solving one scramble on real hardware is the check for that.

## What must be tested, per module

- The full depth distribution table above, exactly, all twelve rows and the total.
- Round trip on many seeds: random state, solve, applying the solution reaches
  solved; applying the emitted scramble to solved reaches the sampled state.
- Emission shape: token counts, move sets, suffix sets, tip placement and order,
  single spaces, no leading or trailing whitespace.
- Determinism under a seed, distinct results across seeds.
- Sampled-depth sanity: over a few thousand seeded samples, the mean solution depth
  sits near the published average (8.76 for 2x2, 7.80 Pyraminx, 8.36 Skewb).
- 2x2 only: a cross-model pin against `crate::cube`: map a facelet `Cube` to the
  solver's coordinates, and verify every move and many seeded scrambles agree
  between the two models. The two were written independently; agreement on 3,674,160
  states' worth of moves is the strongest check this codebase can express.
- Integrator: `scramble::generate` for the three events has the right shape end to
  end, existing generator-level properties (non-empty, deterministic, single-spaced)
  stay green, and the 2x2 path still applies cleanly to the preview's facelet cube.

## Performance constraints

Tables build lazily on the first scramble of that event, never in the draw loop; a
first-use pause well under a second in release is acceptable and documented. Debug
test runs pay the build once per binary thanks to the shared `OnceLock`. If the
suite's wall time grows past roughly twenty seconds, the integrator sets
`[profile.test] opt-level = 1` in Cargo.toml with a comment saying why, rather than
weakening any test.

## Docs shipped with this change (integrator)

- `docs/scrambles.md`: which events are random-state now and what that means.
- `claude-docs/algorithms.md`: the method: coordinates, BFS tables, exact-length
  search, the distributions as fixtures.
- `CLAUDE.md` and `claude-docs/architecture.md`: the `solver/` module rows, line
  counts, data flow.
- `claude-docs/testing.md`: counts and the new modules' testing story.

## Gates

`cargo test` fully green, `cargo clippy --all-targets` silent, docs matching the
code, one squashed lowercase commit, PR stacked on `feat/cube-preview`.

---

## Outcome

Shipped as planned, with one deliberate departure. This plan described
`generateExactly` as a plain depth-first search of fixed order, and the scaffolded
`Engine::solve_exactly` was written that way. It is not what shipped: the search
now shuffles its candidate moves at every node, taking the randomness from an
`rng` parameter threaded down from the caller.

The reason is that a scramble is the solution written backwards, so the last token
of the scramble is the first move the search tried. With a fixed order that move is
almost always the first in the list, which pinned the tail of every scramble
emitted. Measured over the first 400 seeds: `U'` ended 397 of 400 2x2 scrambles,
`U'` ended the core of 400 of 400 Pyraminx scrambles, and `R'` ended 400 of 400
Skewb scrambles. Sampling was uniform throughout and every scramble genuinely
reached the state drawn for it, so nothing here was a fairness bug; it was a fair
scramble that reads as broken, which for a scramble generator is close enough to
the same thing. With the shuffle in, all nine 2x2 tokens and all eight of the other
two appear as the final token, none of them more than about a sixth of the time.

Shuffling cannot change whether a solution of a given length exists, only which one
comes back, so correctness is untouched and determinism is preserved: the rng is the
caller's, and the same seed still gives the same scramble. Each puzzle module
documents the order it draws from the rng, because a test reproducing a scramble's
state has to draw in the same order. Three tests were added for the defect itself,
one per puzzle, plus one on the toy puzzle in `solver/mod.rs`.

Two smaller notes. The debug suite came in at roughly 7 s of test time and under 9 s
of wall time, so the pre-authorised `[profile.test] opt-level = 1` was not needed and
was not added. And the first-scramble pause is under a second for two of the three
but not all: measured in release it is 0.08 s for Pyraminx, 0.42 s for 2x2 and 1.30 s
for Skewb, whose encoded space is 9.4 million states. `docs/scrambles.md` says so
plainly rather than rounding it down.
