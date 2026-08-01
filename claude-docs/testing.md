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
- **Rendering** is the part that genuinely resists automated testing, so it is deliberately
  left out. See below.

The general shape to aim for: if something is hard to test, that is usually a boundary
problem, not a testing problem. Move the clock read, the file path or the event source out
to the caller and the test writes itself.

## House layout

Unit tests are colocated in a `#[cfg(test)] mod tests` at the bottom of the module they
cover, with `use super::*;` at the top. There is no `tests/` directory and no integration
test crate. Colocation buys access to private items, which matters here: `trim_count`,
`average_window`, `is_legal`, `tmp_path` and the private fields of `App` are all tested
directly.

Current state, 114 tests, all green:

| Module | Tests | Focus |
|---|---|---|
| `app.rs` | 59 | State machine, keys, `/commands`, persistence side effects |
| `stats.rs` | 31 | Trimmed averages, penalties, session stats, personal bests |
| `storage.rs` | 16 | Round trips, atomic write, missing versus corrupt files, version migration |
| `scramble.rs` | 8 | Move pools, lengths, the legality rule, determinism |
| `types.rs` | 0 | Formatting is covered indirectly through `stats` and `app` |
| `ui.rs` | 0 | Not covered on purpose |

`types.rs` having no test module of its own is a gap of convenience, not a policy. If you
touch `format_millis` or `format_solve`, add one.

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

`scramble.rs` tests randomized output, so it drives `generate_with_rng` with a seeded
`StdRng` and asserts properties rather than exact strings: the move count is exactly right
for the puzzle, every move comes from the puzzle's pool, and the legality rule holds at every
position across the whole sequence. Four hundred seeds per puzzle is cheap and catches rule
violations that a single sample would miss. Determinism gets its own test (same seed, same
scramble), and the `thread_rng` entry point gets a smaller well-formedness check. Two tests
guard against the constraint drifting back to something stricter than TNoodle:
`legality_predicate_rejects_illegal_sequences` pins `is_legal` down case by case, and
`big_cubes_do_produce_runs_of_three_on_one_axis` asserts that same-axis runs longer than two
really do come out of the generator.

`storage.rs` tests the file layer against real files in the system temp directory, using an
RAII guard so nothing survives the run.

## Testing the state machine

`app.rs` tests drive the real `App` through synthetic input. Three techniques do all the
work.

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

## What is deliberately not covered

**`ui.rs` rendering.** No assertions are made about the drawn frame. Layout correctness is
verified by eye, and the type system plus the module boundary carry the rest: `draw` takes
`&App`, so it cannot change state, and every value it prints was already computed and tested
in `app.rs` or `stats.rs`. The genuine rendering risk is panicking on a tiny terminal, and
that is handled structurally rather than by test, with saturating arithmetic everywhere and
no direct indexing into a `Rect`. Snapshot testing a `TestBackend` buffer is possible with
ratatui, but the churn cost on a UI that is still moving is higher than the bug rate it
would catch.

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

1. **Pick the module.** It is command handling, so it belongs in `app.rs`, in the
   `// ----- commands` section of the test module, next to the other command tests.

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
   assert_eq!(app.times_scroll, 0);
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
