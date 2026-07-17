# Testing

Rule 1 of `CLAUDE.md` is that every code change ships with its tests. This is how that gets
done in Cubetimer.

## Testing an app like this

A terminal timer looks hostile to testing at first glance. It reads keyboard events from a
raw-mode terminal, it measures wall-clock time, it draws to a screen, and it writes to a
file in the user's profile. All four of those are the classic reasons a project ends up
with no tests at all.

They come apart cleanly once the code is split by responsibility, which is why rule 2 and
rule 1 support each other:

- **Pure logic** (averages, session bests, scramble legality, time formatting) is a
  function of its arguments. It needs no scaffolding whatsoever, so it gets tested
  exhaustively, including the ugly boundaries.
- **The state machine** is the interesting part, and the trick is that it does not read the
  keyboard or the clock itself. It is handed a `KeyEvent` and it reads `Instant::elapsed`
  on instants that live in its own fields. A test can hand it any event and any instant it
  likes, which turns "hold space for 300ms, release, wait 3 seconds, press a key" into four
  ordinary function calls that run in microseconds.
- **I/O** goes through one module with a path parameter. Point that parameter somewhere
  disposable and it is just a function again.
- **Rendering** used to be the exception. It is now split: the geometry moved into
  `ui/layout.rs` and the unfolded cube into `ui/net.rs`, both pure arithmetic and tested like
  any other pure module, and the three drawing files (`ui/mod.rs`, `ui/timer.rs`,
  `ui/overlay.rs`) are driven through ratatui's `TestBackend`. What is asserted about the
  resulting frame is still modest. See below.

The general shape to aim for: if something is hard to test, that is usually a boundary
problem, not a testing problem. Move the clock read, the file path or the event source out
to the caller and the test writes itself.

## House layout

Unit tests are colocated in a `#[cfg(test)] mod tests` at the bottom of the module they
cover, with `use super::*;` at the top. There is no `tests/` directory and no integration
test crate. Colocation buys access to private items, which matters here: `trim_count`,
`average_window`, `is_legal`, `tmp_path` and the private fields of `App` are all tested
directly.

A test lives with its subject, which means a split moves tests as well as code. Where two
files inside one directory need the same scaffolding, it goes in a `#[cfg(test)] mod
testkit` beside them: `app/testkit.rs` holds `TempPath`, `test_app`, `press`, `release`,
`run_command`, `perform_solve`, `assert_preview_is_fresh` and friends for all six files of
`app/`, and a private `mod testkit` at the bottom of `ui/mod.rs` holds `app_with`, `render`
and `render_all` for all three renderers. Never duplicate a helper across sibling files.

Current state, 544 tests, all green:

| Module | Tests | Focus |
|---|---|---|
| `app/commands.rs` | 52 | Every `/command`, its arguments, its refusals, its persistence, the export and import round trip |
| `stats.rs` | 45 | Trimmed averages, penalties, session stats, session bests |
| `ui/layout.rs` | 42 | Panel heights, word wrap, the header cap, popup packing, list windows, stats packing, the trend popup's bounds |
| `app/mod.rs` | 38 | State machine, keys, the stop guards, the derived caches, the preview cube behind the scramble |
| `cube/mod.rs` | 34 | Move orders and identities at every size and width, colour conservation, direction pins, inversion, parse errors |
| `storage.rs` | 31 | Round trips, atomic write, missing versus corrupt files, the size cap, every migration |
| `ui/overlay.rs` | 27 | All five popups at four sizes, clamping, the cursor, which one wins, the trend graph's y domain, the net drawn from a known scramble, both glyph sweeps |
| `app/selection.rs` | 23 | The times cursor, the solve-detail overlay, the sessions picker and its modality, the overlay exclusions |
| `cstimer.rs` | 21 | The export shape, the penalty encoding, a round trip, a handcrafted csTimer file, the skips, the errors |
| `types.rs` | 16 | `format_millis`, `format_solve`, penalty arithmetic at `u64::MAX` |
| `scramble/square1.rs` | 16 | The shape simulator, twist range, slash legality, replay |
| `ui/net.rs` | 16 | The footprint formulas, the colour map, face placement, compact pairing, the glyph set |
| `solver/cube2.rs` | 15 | The depth distribution, both coordinates, the move tables, emission shape, the cross-model pin against `cube` |
| `solver/skewb.rs` | 13 | The depth distribution, the reachable third and what pins it, TNoodle's facelet cycles, emission shape |
| `solver/pyraminx.rs` | 13 | The depth distribution, the unreachable odd half, TNoodle's edge cycles, tips, emission shape |
| `solver/cube3/cubies.rs` | 13 | Move orders and inverses, the three invariants under every move, the cross-model bridge against `cube` |
| `solver/cube3/coords.rs` | 13 | Encode and decode round trips over every value, every move table against direct cubie application, solved at 0 |
| `ui/mod.rs` | 12 | Render smoke at four sizes, the chrome anchor, the stats prefix column and its packing, the best row's scope |
| `scramble/clock.rs` | 12 | The fifteen-token frame, amount range and uniformity |
| `app/progress.rs` | 12 | The trend window and its refreshes, which solves raise the banner and when it comes down |
| `app/inspection.rs` | 12 | The countdown, the `+2` and DNF thresholds, the judge-call stages, an aborted arm |
| `solver/cube3/search.rs` | 10 | The canonical rule, inversion, the round trip through both models, the superflip, the 21 cap, tail variety, timings |
| `solver/cube3/mod.rs` | 9 | Uniform sampling and the parity repair, the invariants, emission order and power inversion |
| `scramble/megaminx.rs` | 9 | Line and move counts, the derived closing `U` |
| `ui/timer.rs` | 8 | The block font, `hide_time`, the stage colours and captions, penalty precedence, the session-best banner |
| `solver/cube3/prune.rs` | 8 | Admissibility on samples, zero at index 0 alone, the phase caps, every cell reachable |
| `scramble/mod.rs` | 8 | Every puzzle dispatches, is non-empty and is seed-stable; the four random-state shapes at the dispatch |
| `scramble/cube.rs` | 7 | Move pools, lengths, the legality rule, determinism |
| `app/repair.rs` | 5 | A broken save file: missing defaults, duplicate ids, misfiled reserved ids |
| `solver/mod.rs` | 4 | The `Engine` against a toy puzzle: the distance table by hand, the search against brute force, its randomization, rejection sampling |

The tests in `scramble/mod.rs` are worth their line count out of proportion to their size:
each loops over `Puzzle::ALL`, so adding a thirteenth event without writing a generator for
it fails immediately rather than shipping an empty scramble. Four of those loops now cross
into `solver` for five of the twelve, which makes them the end-to-end coverage of the whole
scramble path and is exactly what they are for. `ui/mod.rs` does the same
thing, rendering every puzzle at every size, and so does
`cstimer::every_event_exports_a_scramble_type_that_imports_back_to_it`, which fails the
moment an event is added without a csTimer scramble type to carry it. `cube/mod.rs` sweeps
`Puzzle::ALL` twice for the same reason, once to pin which events have a model and once to
apply a generated scramble for each that does.

`cstimer.rs` is tested from both ends, and both are needed. A round trip, `export` then
`import`, proves nothing was lost in Cubetimer's own writing, but it would pass just as
happily if both directions agreed on a format csTimer does not use. So the import tests
also run against a fixture built to the shape csTimer actually writes, string-encoded
`sessionData` included, with a blindfolded session in it to assert the skip and a session
naming no scramble type to assert the default.

### The cube model and the preview

`cube/mod.rs` is pure state, so it is tested as algebra rather than against fixtures. Every
move applied four times, a move against its prime and a double move applied twice are all the
identity, at every size and every legal width. Colour counts are conserved across generated
scrambles. Direction is pinned sticker by sticker from a solved 3x3 for all six faces, and the
sharpest test in the file needs no stickers at all:
`two_adjacent_faces_generate_the_famous_order_of_105` asserts that `R U` returns a 3x3 to
solved after exactly 105 repeats, for nine different adjacent pairs, which a single wrong row
flip in any of the six cycles misses. `invert_scramble` gives the round trip its property
test: scramble, invert, solved, for two hundred seeds on each of the seven modelled events.

`ui/net.rs` is pure geometry and tested the same way `ui/layout.rs` is: the footprint formulas
at every size in both modes, every face solid and in its net position, the gutters and corners
unstyled, and the compact mode's row pairing including the odd cube's unpaired last row.

The two of them meet the wiring in three places, and those tests are integration tests wearing
unit-test clothes. `app/mod.rs` asserts that a generated scramble leaves a cube of the right
size that is genuinely not solved, and that an event with no model leaves `None`.
`ui/overlay.rs` recalls a known scramble through the real key path and then asserts named
cells of the rendered buffer by colour: after `R` on a solved 3x3, the U face's right column
is the front's green and F's right column is the bottom's yellow. That one assertion fails if
`cube`'s cycle, `net`'s placement, the colour map or the popup's geometry is wrong, which is
exactly why it is worth its length. It also sweeps the popup for glyphs and checks the compact
fallback and the message below it. A test that spans two of these modules belongs in the file
that owns the seam, never duplicated into both.

## Testing pure logic

`stats.rs` is the pattern worth copying. Tiny constructors at the top of the test module
keep the cases readable:

```rust
fn s(ms: u64) -> Solve { /* clean solve */ }
fn plus2(ms: u64) -> Solve { /* +2 penalty */ }
fn dnf(ms: u64) -> Solve { /* DNF */ }
fn solves(times: &[u64]) -> Vec<Solve>
```

With those in place a WCA rule becomes one legible line, and the expected value is written
as the arithmetic that produces it rather than a magic number:

```rust
let expected: u64 = (6..=95).map(|i| i * 1000).sum::<u64>() / 90;
assert_eq!(average_of(100, &v), AvgResult::Time(expected));
```

Cover the boundaries, not just the happy path: exactly `n` solves, one fewer than `n`,
`n == 0`, windows too small to survive trimming, exactly the DNF allowance, one over it,
truncation that is not rounding. The `stats.rs` module is grouped by banner comments
(`// ---- ao5`, `// ---- session bests`) so a reader can see which areas have coverage.

`storage.rs` tests the file layer against real files in the system temp directory, using an
RAII guard so nothing survives the run. Migrations are tested from hand-written JSON string
constants (`V1_FILE`, `V2_FILE`), never from struct literals, because a struct literal
silently follows whatever the current shape is and would stop testing the old format the
moment the new one lands. One test asserts that loading a version-2 file leaves the bytes on
disk untouched, which is the property that makes a failed run non-destructive.

### Seeded property tests, the house pattern for generators

Every file under `scramble/` and `solver/` tests randomized output the same way, and the pattern
is worth imitating for anything else that generates rather than computes.

Drive the generator with a seeded `StdRng` and assert **properties, not exact strings**. A
golden string breaks on any harmless change and tells you nothing about which rule was
violated. A property test says which rule broke:

```rust
for seed in 0..400u64 {
    let text = scramble(&mut StdRng::seed_from_u64(seed));
    // ... assert every rule of the notation against `text`
}
```

Four hundred seeds per puzzle runs in milliseconds and catches rule violations a single
sample would miss. The supporting shape around it:

- **A parser as the shared assertion.** Each file has a `parse` or `decode` helper that
  splits a scramble into tokens and asserts the frame on the way through: single spaces, no
  stray whitespace, the right token count, the right token in the right slot. Every test
  then runs on structured output rather than re-parsing by hand, and every test inherits the
  notation checks for free.
- **Panic messages that name the offender.** Every assertion interpolates the scramble
  (`"repeated layer in {scramble:?}"`). With 400 seeds a bare `assert!` tells you nothing.
- **Coverage assertions, not just legality.** It is not enough that no rule is broken; the
  generator also has to actually reach everything. `all_three_suffixes_are_used`,
  `every_axis_and_both_powers_appear`, `tip_counts_range_over_zero_through_four` and
  `the_last_token_of_a_scramble_ranges_over_the_move_set` all exist to catch a generator that
  is legal and starved. The last one is the sharpest, and the story behind it is under the
  solvers below.
- **Statistical assertions with wide bands.** `tip_counts_range_over_zero_through_four`
  asserts each Pyraminx tip is unsolved between 340 and 460 times in 600 seeds against an
  expectation of about 400, and `the_sampled_depth_averages_the_published_optimal` asserts a
  mean solution depth inside a quarter of a move of the published figure. Wide enough not
  to be flaky, tight enough to catch a wrong model.
- **Determinism gets its own test.** Same seed, same scramble; different seed, different
  scramble. This is why every source of randomness is threaded from one caller-supplied `Rng`
  rather than reached for internally, the solvers' search order included.
- **A published figure as a fixture.** Where the event has one, assert against it rather than
  against the generator's own behaviour. The solvers reproduce twelve rows of God's-algorithm
  counts each; `square1.rs` checks its solved slot array against TNoodle's.
- **Guards against drift.** `legality_predicate_rejects_illegal_sequences` pins `is_legal`
  down case by case and `big_cubes_do_produce_runs_of_three_on_one_axis` asserts same-axis
  runs longer than two really do occur, which together stop the cube constraint from
  quietly becoming stricter than TNoodle's again.

### The solvers

`solver/` is the one part of the tree where correctness can be *proved* rather than argued, and
the tests are built around that. Six techniques, in descending order of how much they buy:

**The depth distribution is the spine, for the three tabled puzzles.** Each of those modules
asserts the full count of states at every depth from 0 to 11, that nothing sits deeper, that the
unreachable count is exactly what the encoding predicts, and that the whole thing sums to the
puzzle's published state count. These are Jaap Scherphuis's God's-algorithm counts, twelve
numbers per puzzle, and they are not a smoke test: a single wrong cycle, a wrong orientation
delta or a missed parity constraint changes which states are reachable in how many moves, so the
histogram moves and the test names the depth it moved at. Everything else in those files is a
supporting check on something the distribution cannot see. `cube3` has no such histogram
available, its state space being 43 quintillion, which is why its anchor is the cross-model
bridge below rather than a count.

**Round trips, in both directions.** For a few hundred seeds each: sample a state, solve it,
fold the solution back over the state and assert it reaches 0; then take the emitted scramble,
parse it, fold it over 0 and assert it reaches the state that was sampled. The second half is
what actually pins the inversion and the token emission, because an off-by-one in the
power-to-suffix arithmetic solves fine and scrambles wrongly.

**The cross-model pin, on the two cubes that have a second model.** `solver/cube2.rs` maps a
facelet `crate::cube::Cube` into its own corner coordinates and asserts the two models agree, on
every one of the nine moves from two hundred random positions and on the state behind a hundred
seeded scrambles. `solver/cube3/cubies.rs` does the same over all twenty pieces and all eighteen
moves, reading each corner's three stickers and each edge's two to recover which piece is where
and how it is turned. `cube` was written from facelet permutations and the solvers from
coordinates, independently, so agreement on that many moves is the strongest check this codebase
can express. It is also the reason those events' scramble previews can be trusted.

The bridge is `pub(super)` and `#[cfg(test)]` rather than private to the tests module, because
`search.rs` uses it too: every solve in the 3x3 sweep is checked twice over, the solution folded
back on the cubie model and asserted to reach solved, then the emitted scramble folded onto a
solved facelet `Cube` and asserted to reach the state that was sampled. That second half is what
catches a merge at the phase junction, which is invisible in the move indices and visible in the
tokens.

**Where a published fact exists, it is the fixture.** The 3x3's are two. The superflip, every
edge flipped and nothing else moved, needs exactly 20 moves optimally, so the solver is asserted
to return 20 or 21 for it and to solve it; a two-phase decomposition is not obliged to find the
optimum, which is why the band and not the number. And the pruning tables are asserted
**admissible** rather than merely populated: one move never drops a table's value by more than
one, and zero sits at index 0 alone. Admissibility is the property the whole search rests on, an
over-estimating bound prunes away the answer, and it is testable in exactly that one line.

The cap is pinned as a band too, and deliberately not as `<= 21`. The search aims at 21 and
raises its cap to the sum of the two phase maxima when neither aim can fit, which measurably
happens about nine times in twenty thousand, so a test asserting 21 as an invariant would be
asserting something the code does not promise and would be one seed choice away from failing.
What the tests assert instead is the ceiling the code can promise, the share inside 21, and the
mean over a sweep. See [algorithms.md](algorithms.md#what-comes-out-and-the-two-honest-caveats).

**Mutation testing, by hand, on the search randomization.** `Engine::solve_exactly` shuffles its
candidate moves at every node, and the reason is a defect a passing suite did not catch: with a
fixed branch order, `U'` ended 397 of 400 2x2 scrambles and 400 of 400 Pyraminx cores. That was
verified by disabling the shuffle and reading the counts back out, then re-enabling it and
measuring the spread, and both numbers are recorded in
[algorithms.md](algorithms.md#the-exact-length-search-and-why-its-branch-order-is-random). Each
of the three modules now carries a `the_last_..._of_a_scramble_ranges_over_the_move_set` test
asserting the whole move set appears and that no token takes more than three fifths of the
scrambles, and `solver/mod.rs` asserts on a toy puzzle that one state yields at least twenty
distinct solutions across 200 seeds while each individual seed still repeats exactly. The
general lesson: when a defect is invisible to every existing assertion, break the fix
deliberately, measure, and write the assertion that fails.

**A toy puzzle for the engine itself.** `solver/mod.rs` tests `Engine` against a two-axis
puzzle over Z/8, small enough that the distance table is written out by hand as
`[0, 1, 1, 2, 2, 2, 1, 1]` and the exact-length search is checked against an exhaustive brute
force for every start and every length up to seven. The real puzzles then only have to be right
about their own move models.

**What the tables cost the suite, and the one profile override in the tree.** The `OnceLock`s
make each puzzle's tables a once-per-binary cost however many tests touch them, but the cost is
real, and `cube3` raised it twice over. Its own tests came first: four million pruning cells
swept breadth-first, six move tables built over every value of every coordinate, and a few
hundred two-phase solves, which took the debug suite from the roughly 7 s plan 01 left it at to
**42.7 s** while nothing was yet dispatched to it. Then wiring it in added the second half,
because the 3x3 is the default session and so every `test_app` and every `Puzzle::ALL` sweep in
the tree now pays a two-phase solve of its own: **59.4 s**.

The sanctioned lever, held in reserve since plan 01 for exactly this, is now pulled:

```toml
# The solvers build million-cell tables and search millions of nodes per scramble, which
# unoptimized costs the suite minutes rather than seconds; opt-level 1 keeps debug info.
[profile.test]
opt-level = 1
```

That takes the same 544 tests from 59.4 s to **7.0 s**, a factor of eight and a half, which puts
the suite back where it sat before any of this and is the whole of the fix: no fixture was
shortened, no seed count reduced and no assertion weakened.
It is the right lever because the cost is arithmetic rather than logic, so the optimizer removes
it without changing what is being tested, and `opt-level = 1` keeps debug info and overflow
checks, which is what distinguishes it from testing in release. A one-off release run is still
how the solvers' timings are measured, since a number quoted at `opt-level = 1` would describe
neither the suite nor the shipped binary.

### Replay verification

`square1.rs` adds one more technique, and it is the strongest of the lot. The generator
carries a shape simulator to decide which twists leave the puzzle slashable, so the tests
**replay each generated scramble through that simulator from solved**, asserting at every
step that the move is physically possible:

```rust
Token::Slash => {
    assert!(shape.can_slash(), "slash at index {i} is blocked by a corner in {scramble:?}");
    shape = shape.slashed();
}
```

It also checks after every single move that the multiset of piece ids is unchanged, so a
simulator bug that loses or duplicates a piece is caught the moment it happens rather than
showing up as a mysterious legality failure later. The simulator itself is tested separately
against known values: the solved array matches TNoodle's, a twist and its inverse return to
solved, a twist of one moves the layer exactly one slot, and two slashes cancel.

The general principle: **when the generator carries a model, make the tests replay against
that model.** It turns "the output looks plausible" into "the output is provably turnable",
and it is available to any generator that simulates rather than just samples.

## Testing the state machine

The tests across `app/` drive the real `App` through synthetic input. Three techniques do
all the work, and all three live in `app/testkit.rs`.

**Synthetic key events.** `press`, `release` and `repeat` build a `KeyEvent` with the right
`KeyEventKind`, so a test spells out the exact physical sequence the app will see, including
the Windows quirks: auto-repeat arriving as `Press`, the release that matches the key which
stopped the timer, a key held down after a stop.

**Backdated instants, never sleeps.** `ago(d)` returns `Instant::now() - d`. Writing that
instant into `App`'s state is how a test says "this has been held for 350ms" or "this solve
started 3 seconds ago":

```rust
app.state = TimerState::Armed { since: ago(ms(350)), from_inspection };
app.on_key(release(SPACE));
assert!(matches!(app.state, TimerState::Timing { .. }));
```

No test sleeps. The whole suite runs in well under a second, and none of it is flaky on a
loaded machine. If a new feature needs a timeout, store the instant in a field so a test can
backdate it, exactly as `stopped_at` is backdated to expire the stop cooldown.

**Disposable save paths.** `TempPath` builds a unique path under the system temp directory
and deletes it, plus its `.tmp` sibling, on drop. `test_app(tag)` returns
`(App, TempPath)` and the guard must stay alive for the whole test, which is why every test
binds it as `_g`:

```rust
let (mut app, _g) = test_app("stop-saves");
```

Uniqueness comes from the tag, the process id, an atomic counter and the current time, so
parallel tests never collide. Three rules follow from `cargo test` running tests in parallel
threads inside one process:

- **Never touch the user's real save file.** Every `App` in a test points at a `TempPath`.
- **Never set or read environment variables** to steer behavior. `CUBETIMER_DATA` is process
  global, so a test that sets it would corrupt whatever else is running. This is why
  `data_file_path_is_usable` asserts only that the path is non-empty instead of asserting a
  location.
- **Never assume ordering** between tests, and never share mutable state across them.

Higher-level helpers sit on top: `run_command(app, "dnf")` types `/`, then the body, then
Enter, and asserts the mode transitions on the way in and out, so commands are exercised
through the same path a user takes rather than by calling `execute_command` directly.
`start_timing_now(app)` drives Idle to Timing. `add_solve(app, millis)` seeds history
without going through the timer.

Assert on outcomes the user can observe: the resulting `TimerState`, `status_msg`, the
solve list, and for anything persistent, the file on disk read back through
`storage::load`. Failure messages carry the actual value, because a bare `assert!` in a
state machine tells you nothing:

```rust
assert!(matches!(app.state, TimerState::Armed { .. }),
    "space Press must arm straight away by default, got {:?}", app.state);
```

## Testing the renderer

Rendering used to be uncovered on purpose. It is not any more, and the split that made it
testable is `ui/layout.rs`.

**Geometry is unit-tested directly.** Every degradation rule is a pure function over `Rect`
and `&str`, so it is asserted like any other pure code, with no terminal involved:
`header_height` grows for a seven-line Megaminx scramble, stays at the minimum for a
one-line one, never exceeds `HEADER_MAX_H`, degrades on a tiny terminal, and yields to the
timer before it finishes growing. `wrapped_rows` is tested on word boundaries, on a word
wider than the line, and at zero width. `inner_of` is tested to bottom out at zero rather
than underflow. These are the assertions that used to be made by resizing a terminal and
squinting.

**Drawing is smoke-tested through `TestBackend`.** `render(app, w, h)` draws one frame into
a ratatui `TestBackend` and returns every cell's symbol as a string. The tests sweep four
sizes, `(80,30)`, `(44,12)`, `(30,8)` and `(10,4)`, over every puzzle, empty and populated,
plus the help and sessions popups, command mode, a status message, a 60-character session
name, a solve over an hour, and a times scroll past the end of the list. The helpers live in
`ui/mod.rs`'s `testkit` and are shared by all three drawing files.

Be clear about what that buys. For most of these, **not panicking is the assertion**. That
is the genuine rendering risk and the sweep is a real guard against it, but it is not a
claim that the frame looks right. One test,
`a_normal_frame_actually_draws_its_chrome`, anchors the rest by asserting the buffer
actually contains `cubetimer`, `3x3`, `stats` and `times`, so a `draw` that silently wrote
nothing cannot pass the whole file.

A handful of tests go past the anchor, and they are the ones where a wrong cell means a
wrong reading rather than an ugly one: `cells_colored` and `row_cells` let `ui/timer.rs`
assert that stage 1 recolours the countdown *and* draws `8s` in that same colour, and that a
`+2` takes the slot and the red back off it, and that a session-best banner turns the
digits under it light green while a running inspection is left alone; `rows_of` lets
`ui/mod.rs` assert the three stats rows start their values in the same column, and that a
second session of the same puzzle changes none of them. `ui/overlay.rs`
goes furthest, because the trend graph and the scramble net are the two widgets whose glyphs
are a compatibility contract: it sweeps every cell inside each popup's border and fails on
anything that is neither ASCII nor one of the CP437 characters that widget is allowed to draw,
six for the chart and two for the net. It also reads named cells of the preview by colour,
because a sticker in the wrong place is a wrong answer rather than an ugly one. Colour, column
alignment and those glyph sets carry meaning here, so they are asserted directly. Beyond
those, content is not asserted.

Full snapshot testing of the buffer is still declined. The churn cost on a UI that is still
moving is higher than the bug rate it would catch, and the pieces where a wrong value would
actually matter are computed and tested in `app/`, `stats.rs` and `ui/layout.rs` before
the renderer ever sees them.

## What is deliberately not covered

**True end-to-end TUI automation.** Nothing spawns the real binary, drives a real terminal
and reads back the rendered screen. Doing that on Windows means a ConPTY harness, and the
key release events the whole timer flow depends on make it harder still. It is out of scope
for now. If it is ever added, the shape would be an expect-style driver (`expectrl`, or
`pexpect` from a Python script) attached to a ConPTY, running a handful of smoke paths:
launch, scramble, solve, quit, and confirm the save file. That would be an addition to the
state-machine tests, never a replacement for them.

## The bar

Before any change is done, both of these hold:

```
cargo test
cargo clippy --all-targets
```

All tests green, zero clippy warnings, including warnings in test code, which is what
`--all-targets` is there for. A test that is temporarily inconvenient is not deleted or
marked `#[ignore]`; either the behavior it asserts is still correct, in which case fix the
code, or it changed on purpose, in which case update the test in the same change set and
say why in the commit.

## Adding a test

Say you add a `/scramble <text>` command that overrides the current scramble.

1. **Pick the module.** It is command handling, so the handler goes in `app/commands.rs`
   and the test goes in that file's test module, next to the other command tests. The
   helpers it needs are already in `app/testkit.rs`.

2. **Build an app.** `let (mut app, _g) = test_app("scramble-cmd");` Give the tag a name
   that reads clearly in a temp directory listing if a run ever dies mid-test. Keep `_g`
   bound; dropping it early deletes the save file underneath the app.

3. **Drive it the way a user does.** `run_command(&mut app, "scramble R U R' U'");` rather
   than calling the handler directly, so the parser, the trimming and the mode transitions
   are all under test too.

4. **Assert the observable outcome.** The scramble field, the status message, and the state
   that should not have moved:

   ```rust
   assert_eq!(app.scramble, "R U R' U'");
   assert_eq!(app.times_selected, 0);
   assert_eq!(app.state, TimerState::Idle);
   ```

5. **Cover the failure path.** Empty argument, garbage argument, and the case where a solve
   is already in progress. Commands that mutate nothing should say so through `status_msg`,
   the way `cmd_switch_session` reports `usage: /session <id>`.

6. **If it persists, read it back.** Use `storage::load(&app.data_path)` and assert on the
   loaded `SaveFile`, as `finishing_a_solve_writes_it_to_disk` does. Asserting the in-memory
   struct alone would miss a serde mistake.

7. **If it involves time, backdate.** Reach for `ago(ms(n))` and write the instant into the
   relevant field. If there is no field to write into, that is the design telling you to add
   one.

8. **Run the bar.** `cargo test` and `cargo clippy --all-targets`, then document it because
   a new command is user-visible: the commands table in `README.md`, and the `docs/` page
   for whatever the command does.
