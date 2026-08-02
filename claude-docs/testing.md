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

- **Pure logic** (averages, personal bests, scramble legality, time formatting) is a
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
  `ui/layout.rs`, which is pure arithmetic and tested like any other pure module, and the
  three drawing files (`ui/mod.rs`, `ui/timer.rs`, `ui/overlay.rs`) are driven through
  ratatui's `TestBackend`. What is asserted about the resulting frame is still modest. See
  below.

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
`run_command`, `perform_solve` and friends for all five files of `app/`, and a private `mod
testkit` at the bottom of `ui/mod.rs` holds `app_with`, `render` and `render_all` for all
three renderers. Never duplicate a helper across sibling files.

Current state, 370 tests, all green:

| Module | Tests | Focus |
|---|---|---|
| `stats.rs` | 46 | Trimmed averages, penalties, session stats, personal bests |
| `app/commands.rs` | 42 | Every `/command`, its arguments, its refusals, its persistence, the export and import round trip |
| `app/mod.rs` | 41 | State machine, keys, inspection and judge calls, the stop guards, the derived cache |
| `ui/layout.rs` | 39 | Panel heights, word wrap, the header cap, popup packing, list windows, stats packing, the sparkline thresholds |
| `storage.rs` | 31 | Round trips, atomic write, missing versus corrupt files, the size cap, every migration |
| `app/selection.rs` | 20 | The times cursor, the solve-detail overlay, the sessions picker and its modality |
| `types.rs` | 16 | `format_millis`, `format_solve`, penalty arithmetic at `u64::MAX` |
| `scramble/square1.rs` | 16 | The shape simulator, twist range, slash legality, replay |
| `cstimer.rs` | 16 | The export shape, the penalty encoding, a round trip, a handcrafted csTimer file, the skips, the errors |
| `ui/mod.rs` | 19 | Render smoke at four sizes, the chrome anchor, the stats prefix column, the trend sparkline |
| `scramble/pyraminx.rs` | 12 | Layer count, the repeat rule, tip order and frequency |
| `scramble/clock.rs` | 12 | The fifteen-token frame, amount range and uniformity |
| `app/progress.rs` | 12 | The trend window and its refreshes, which solves raise the banner and when it comes down |
| `ui/overlay.rs` | 9 | All three popups at four sizes, clamping, the cursor, which one wins |
| `scramble/megaminx.rs` | 9 | Line and move counts, the derived closing `U` |
| `ui/timer.rs` | 8 | The block font, `hide_time`, the stage colours and captions, penalty precedence, the personal-best banner |
| `scramble/skewb.rs` | 8 | Pool, length, the no-repeat rule, successor fairness |
| `scramble/cube.rs` | 8 | Move pools, lengths, the legality rule, determinism |
| `app/repair.rs` | 5 | A broken save file: missing defaults, duplicate ids, misfiled reserved ids |
| `scramble/mod.rs` | 5 | Every puzzle dispatches, is non-empty and is seed-stable |

The tests in `scramble/mod.rs` are worth their line count out of proportion to their size:
each loops over `Puzzle::ALL`, so adding a thirteenth event without writing a generator for
it fails immediately rather than shipping an empty scramble. `ui/mod.rs` does the same
thing, rendering every puzzle at every size, and so does
`cstimer::every_event_exports_a_scramble_type_that_imports_back_to_it`, which fails the
moment an event is added without a csTimer scramble type to carry it.

`cstimer.rs` is tested from both ends, and both are needed. A round trip, `export` then
`import`, proves nothing was lost in Cubetimer's own writing, but it would pass just as
happily if both directions agreed on a format csTimer does not use. So the import tests
also run against a fixture built to the shape csTimer actually writes, string-encoded
`sessionData` included, with a blindfolded session in it to assert the skip and a session
naming no scramble type to assert the default.

## Testing pure logic

`stats.rs` is the pattern worth copying. Tiny constructors at the top of the test module
keep the cases readable:

```rust
fn s(ms: u64) -> Solve { /* clean solve */ }
fn plus2(ms: u64) -> Solve { /* +2 penalty */ }
fn dnf(ms: u64) -> Solve { /* DNF */ }
fn solves(times: &[u64]) -> Vec<Solve>
fn session(id: u64, times: &[u64]) -> Session
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
(`// ---- ao5`, `// ---- personal bests`) so a reader can see which areas have coverage.

`storage.rs` tests the file layer against real files in the system temp directory, using an
RAII guard so nothing survives the run. Migrations are tested from hand-written JSON string
constants (`V1_FILE`, `V2_FILE`), never from struct literals, because a struct literal
silently follows whatever the current shape is and would stop testing the old format the
moment the new one lands. One test asserts that loading a version-2 file leaves the bytes on
disk untouched, which is the property that makes a failed run non-destructive.

### Seeded property tests, the house pattern for generators

Every file under `scramble/` tests randomized output the same way, and the pattern is worth
imitating for anything else that generates rather than computes.

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
  generator also has to actually reach everything. `every_axis_and_both_directions_appear`,
  `tip_counts_range_over_zero_through_four` and `every_ordered_pair_of_distinct_axes_occurs`
  all exist to catch a generator that is legal and starved. The last one is the sharpest: it
  counts every ordered axis pair over 600 seeds and fails if any legal pair is
  disproportionately rare, which is what would happen if the skip-the-previous-index
  arithmetic were subtly wrong.
- **Statistical assertions with wide bands.** `a_tip_is_solved_roughly_one_time_in_three`
  asserts a count falls in `950..1180` against an expectation of about 1067. Wide enough not
  to be flaky, tight enough to catch a wrong model.
- **Determinism gets its own test.** Same seed, same scramble; different seed, different
  scramble.
- **An official scramble as a fixture.** `pyraminx.rs` runs its `assert_well_formed` against
  `OFFICIAL`, a scramble copied from a real TNoodle competition sheet. This proves the
  assertions accept genuine WCA output and are not just a description of Cubetimer's own
  quirks. Worth adding for any event where a real scramble is easy to come by.
- **Guards against drift.** `legality_predicate_rejects_illegal_sequences` pins `is_legal`
  down case by case and `big_cubes_do_produce_runs_of_three_on_one_axis` asserts same-axis
  runs longer than two really do occur, which together stop the cube constraint from
  quietly becoming stricter than TNoodle's again.

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
`+2` takes the slot and the red back off it, and that a personal-best banner turns the
digits under it light green while a running inspection is left alone; `rows_of` lets
`ui/mod.rs` assert the three stats rows start their values in the same column and that the
sparkline's bars begin in that same column, on the two rows under them. Colour and column alignment
carry meaning here, so they are asserted directly. Beyond those, content is not asserted.

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

8. **Run the bar.** `cargo test` and `cargo clippy --all-targets`, then update `README.md`
   because a new command is user-visible.
