# Plan 02: Kociemba's two-phase algorithm, random-state 3x3

The prize. 3x3 and one-handed are the last events scrambling by random moves that the
WCA scrambles by random state. The 3x3 has 43,252,003,274,489,856,000 states, so the
table-every-state method of plan 01 is out; the answer, since 1992, is Herbert
Kociemba's two-phase algorithm, and every scramble program alive ships a descendant of
it. TNoodle calls min2phase with a 21-move cap and emits the inverse of the solution
(verified against `ThreeByThreeCubePuzzle.java`: max length 21, 60 s timeout, 200 ms
minimum search, `INVERSE_SOLUTION`). Cubetimer builds its own.

## The method

Phase 1 drives the cube into the subgroup G1 = <U, D, L2, R2, F2, B2>: every corner
and edge oriented, the four middle-slice edges home in their slice. Phase 2 finishes
inside G1 using only the ten moves that never leave it. Each phase is an IDA* search,
a depth-first search guided by pruning tables that give a provable lower bound on the
moves remaining, so almost every branch dies immediately. The search does not stop at
the first answer: it keeps trying longer phase-1 prefixes that buy shorter phase-2
suffixes until the total is 21 or better. Phase 1 never needs more than 12 moves,
phase 2 never more than 18, and in practice the loop lands at 21 or under in
milliseconds.

## Conventions (the contract; every wave builds against these)

Cubie numbering, Kociemba's own:

- Corners 0..8: URF, UFL, ULB, UBR, DFR, DLF, DBL, DRB.
- Edges 0..12: UR, UF, UL, UB, DR, DF, DL, DB, FR, FL, BL, BR. The slice edges are
  8..12 (FR FL BL BR), home in positions 8..12.

State is four arrays: `cp[8]`, `co[8]` (0..3, clockwise twists of the piece now in
that position), `ep[12]`, `eo[12]` (0..2). Applying move m to state s:
`s'.cp[i] = s.cp[m.cp[i]]`, `s'.co[i] = (s.co[m.cp[i]] + m.co[i]) % 3`, and the same
shape mod 2 for edges. A move's own arrays are what it does to a solved cube.

Moves 0..18: faces in Kociemba order U, R, F, D, L, B, powers 1..3 clockwise quarter
turns, index = face*3 + power - 1, token = letter plus "", "2", "'". Phase 2's ten
moves are all of U and D plus the three half turns index-listed once in `search.rs`.

Coordinates, with 0 solved in every one:

| Coordinate | Range | Definition |
|---|---|---|
| twist | 2187 | co[0..7] base 3, co[7] carried by the mod-3 sum |
| flip | 2048 | eo[0..11] base 2, eo[11] carried by the mod-2 sum |
| slice | 495 | rank of the 4-subset of positions holding edges 8..11, C(12,4), subset {8,9,10,11} ranked 0 |
| cperm | 40320 | factorial rank of cp |
| eperm | 40320 | factorial rank of ep[0..8], meaningful only inside G1 |
| sliceperm | 24 | factorial rank of ep[8..12] as values 8..11, meaningful only inside G1 |

Move tables per coordinate as in plan 01: decode, apply, encode over just that
coordinate's range, `u16` cells, one column per move (18 columns for phase 1's three,
10 for phase 2's three), all behind `OnceLock`s with the move tables and the pruning
tables in separate locks.

Pruning tables, four, each an exact-distance BFS over a product coordinate through
`Engine::distances` exactly as plan 01 used it:

| Table | Space | Moves |
|---|---|---|
| twist x slice | 1,082,565 | 18 |
| flip x slice | 1,013,760 | 18 |
| cperm x sliceperm | 967,680 | 10 |
| eperm x sliceperm | 967,680 | 10 |

A phase's lower bound is the max of its two tables. These are exact distances in a
projection, so they are admissible, and admissibility is testable: one move changes
any table's value by at most one.

The search takes the caller's rng and shuffles branch order at every node, exactly as
`Engine::solve_exactly` does and for the same measured reason (plan 01's outcome
note): a fixed order pins the scramble's tail. Determinism per seed is preserved.

Sampling a uniform random state: cp a uniform random permutation of 8, ep of 12, and
if their parities differ swap ep[0] and ep[1] (corner and edge parity must match or
the state needs a screwdriver); co is 7 uniform trits with the eighth carried; eo is
11 uniform bits with the twelfth carried. That is TNoodle's `Tools.randomCube()`.

Emission: solve the sampled state to a total of at most 21 moves, emit the INVERSE
(reversed order, power p becomes 4-p) as space-separated tokens. No fixed length:
real 3x3 scrambles run 17 to 21 moves. No two consecutive tokens share a face.

## Module boundaries and waves

Dependencies make this phase layered, so it ships in waves, each fully tested alone.

| File | Responsibility | Wave |
|---|---|---|
| `solver/cube3/cubies.rs` | The cubie model: state, the 18 moves, composition, parity and orientation invariants. | A, alone |
| `solver/cube3/coords.rs` | The six coordinates: encode, decode, move tables. | B, parallel with C |
| `solver/cube3/mod.rs` | Orchestration: uniform sampling, solve, inverse, tokens; the module's public `scramble(rng)`. | C, parallel with B |
| `solver/cube3/prune.rs` | The four pruning tables over product coordinates. | D |
| `solver/cube3/search.rs` | The two IDA* phases and the improve-until-21 loop. | D |
| dispatch + docs | `solver/mod.rs` and `scramble/mod.rs` route Cube3 and Oh here; every doc tier. | C (wiring), integrator (docs) |

One-handed is the same event on one hand: `scramble/mod.rs` routes Oh through the
identical Cube3 path, and the existing oh-equals-3x3 seed test keeps holding.

## What must be tested

`cubies.rs` (wave A):
- Every move applied 4/2/4 times per its order is the identity; X then X' is the identity across all 18.
- R U has order 105, opposite faces commute: the same external facts `cube/` pinned.
- Invariants hold under every move from every random state: corner parity equals edge parity, twist sum 0 mod 3, flip sum 0 mod 2.
- THE CROSS-MODEL PIN, the strongest available: a test-only bridge mapping `crate::cube::Cube` (size 3) to `Cubies` by reading sticker triples, asserting all 18 moves agree between the two independently written models from batches of random states, and that 100 seeded scrambles land both models on the same state.

`coords.rs` (wave B):
- Encode/decode round-trips over every value of every coordinate.
- Move tables agree with direct cubie application for every coordinate, every move, on thousands of random states.
- Solved maps to 0 in all six; the coordinate ranges are exactly the table sizes.

`mod.rs` sampling and emission (wave C):
- Sampled states always satisfy the three invariants; parities match; over seeded batches both parities occur, all four combinations of needing/not needing the swap.
- Uniformity smoke: distinct states over seeds, coordinate means where meaningful.
- Emission formatting from an injected solution: token order inverted, powers inverted, spacing exact.

`prune.rs` and `search.rs` (wave D):
- Admissibility on samples: one move never drops a pruning value by more than one; value 0 exactly at index 0.
- Round trip over many seeds: sample, solve, apply the solution via `cubies`, assert solved; apply the emitted scramble to a solved facelet `crate::cube::Cube` and assert it reaches the sampled state through the bridge.
- Every solution is 21 moves or fewer, no two consecutive tokens share a face, mean length over seeds sits in 17.5..21.0.
- Superflip: the state with every edge flipped and all else solved needs exactly 20 moves optimally, a published fact; assert the solver's answer is 20 or 21 and solves it.
- G1 states: a random phase-2-only scramble applied to solved yields a state the solver still solves within 21.
- Tail variety: over a few hundred seeds the final token of the scramble ranges over many values, none past 60 percent, the plan-01 regression repeated here.
- Determinism under a seed, distinct across seeds.

## Performance constraints

Table budget: about 4 MB of pruning tables and small move tables, built lazily on the
first 3x3 scramble, shared by tests through `OnceLock`s. The 3x3 is the default
session, so the build cost lands at startup for most runs: the integrator measures
it in release, and if it is worse than about 1.5 seconds that is a finding to report
for a decision, not something to hide. Search time per scramble must be milliseconds.
If the debug suite passes roughly 20 seconds, `[profile.test] opt-level = 1` with a
comment is the sanctioned lever, as in plan 01.

## Docs shipped with this change (integrator)

- `docs/scrambles.md`: 3x3 and one-handed join the random-state events; the table of which events are which updates.
- `claude-docs/algorithms.md`: the two-phase section: G1, the six coordinates, pruning as projection distances, the improve-until-21 loop, why the inverse is the scramble.
- `CLAUDE.md`, `claude-docs/architecture.md`, `claude-docs/testing.md`: module rows, line counts, data flow, test story.
- `README.md`: the random-state line grows to name 3x3 and OH.

## Gates

`cargo test` fully green, `cargo clippy --all-targets` silent, docs matching the
code, one squashed lowercase commit, PR stacked on `feat/random-state-scrambles`.

---

## Outcome

Shipped as planned in its method and its shape. The five files are the five the plan
named, the six coordinates carry the ranges and the "0 is solved" convention it
specified, the four pruning tables are the four products at the four sizes it listed
and are computed by `Engine::distances` unchanged, sampling is `Tools.randomCube()`
including the parity repair, emission is the inverse of a solution capped at 21, and
the searches take the caller's rng and shuffle branch order at every node for plan
01's measured reason. The two `OnceLock`s are two, with the pruning lock above the
move-table lock, and both build on the first 3x3 scramble. Six departures and
measurements are worth recording.

**Every cell of both phase-2 tables is reachable, which the plan's inheritance from
plan 01 did not expect.** Plan 01's lesson was that an encoded space is usually
larger than the reachable one, half for Pyraminx and a third for Skewb, and `distances`
carries `UNREACHABLE` precisely for that. It never appears here. A state inside G1 has
`cperm`, `eperm` and `sliceperm` parities summing to even, so the triple product would
indeed be half unreachable, but each table drops one of the three and the dropped one
is free to absorb the parity, which leaves both pairs whole. `U` is the one-move
witness: it makes `cperm` odd while `sliceperm` stays even, and the state is legal
because `eperm` went odd with it. `prune.rs` states this in its module doc and a test
asserts no cell is `UNREACHABLE`, which is worth having precisely because the argument
runs the other way from the previous phase's.

**The search climbs the ladder for two cubes, not one.** The plan described a single
improve-until-21 loop. A share of cubes have no split at all that fits inside 21 in the
orientation they arrive in, so a run alternates the cube it was handed with that cube
inverted, a solution of either being a solution of the other read backwards. It is
close to free, a phase-1 target costing about thirteen times the one below it, and it
is the cheap half of what min2phase does with six aims. Measured over 20,000 sampled
states, one aim alone fails to find a split inside 21 for 212 of them; the pair fails
for 9, a factor of 23. A cube that is its own inverse, the superflip being the one
everybody names, drops back to one aim, both searches being the same search.

**`PHASE2_MOVES` lives in `coords.rs`, not `search.rs`.** The plan put the list in the
search. The phase-2 move tables are the first thing that has to know what their own
columns are, and a coordinate cannot depend on the search that reads it, so the list
moved down and `search.rs` imports it.

**Solutions run about a move longer than min2phase's.** Over 20,000 seeded states the
lengths are 3 at 16, 12 at 17, 80 at 18, 589 at 19, 3,159 at 20, 16,148 at 21 and 9
past 21; the mean is 20.77, and 20.84 over the 300-seed sweep the suite runs. TNoodle
typically lands at 19 to 20. Four fifths of the answers sitting at exactly 21 is what a
search that usually needs its whole budget looks like, and the two reasons are two aims
against six and pruning tables without the symmetry reduction min2phase uses to tighten
its bound for the same memory. Sampling is uniform either way and every emitted scramble
genuinely reaches the state drawn for it, so this is a quality-of-search figure and not
a fairness one.

**The beyond-21 fallback is real and measurable: 9 scrambles in 20,000, at 23 to 25
moves.** When neither aim decomposes inside 21, `solve` raises the cap to
`MAX_PHASE1 + MAX_PHASE2` and runs again, which cannot fail. That is a deliberate choice
about which failure to have, TNoodle covering the same case by searching harder under a
60 second timeout, and a generator that occasionally stalls for seconds in the draw path
is worse to ship than one that occasionally emits a longer scramble. The consequence for
the tests is worth naming: `<= 21` is not an invariant, so the suite pins the ceiling the
code can promise, the share inside 21 and the mean, rather than a cap the code does not
guarantee.

**The profile lever was needed this time, and the startup pause came in well under the
plan's threshold.** Plan 01 left `[profile.test] opt-level = 1` pre-authorised and
unused; wiring the 3x3 in took the debug suite from 42.7 s to 59.4 s and the lever takes
it to 7.0 s for the same 544 tests, with no fixture shortened. The plan asked for the
startup measurement and set 1.5 s as the line worth reporting: measured in release,
`App::new` to a scramble in hand is about 0.40 s, of which about 0.397 s is the table
build and the rest one solve. A solve averages 12 ms over a 50-seed batch with the
slowest at 120 ms, so every scramble after the first costs milliseconds.

Three smaller notes. The table budget came in at 5.5 MB rather than the roughly 4 MB the
plan estimated, and the difference is entirely in the half it called small: the pruning
tables are 3.8 MB as predicted, and the six move tables are 1.7 MB, `cperm` and `eperm`
being 40,320 rows of ten `u16` each. `scramble` resamples the one state whose scramble
would be empty, which the plan did not call for and which makes "never empty" a property
of the code rather than of the odds. And `cubies::twist_sum` and `flip_sum` turned out to
have no non-test caller, the sampler carrying its last orientation rather than summing the
rest, so both are `#[cfg(test)]` rather than carrying a dead-code exemption; the
scaffolding `#[allow(dead_code)]` on `mod cube3` was what had been hiding that.
