# Code style

This expands rules 2, 3 and 4 of `CLAUDE.md` with the concrete shapes already used in
`src/`. When something here disagrees with the code, the code is the bug unless the entry
says otherwise.

## Module boundaries

Each of the eight modules owns exactly one concern, and the dependency arrows only point
one way:

```
main.rs  ->  app/   ->  cstimer.rs, scramble/, stats.rs, storage.rs, types.rs
main.rs  ->  ui/    ->  app/ (read-only), types.rs
```

`app/`, `scramble/` and `ui/` are directories, not files, because one responsibility
outgrew one file. `scramble/mod.rs` dispatches on `Puzzle` to one generator per puzzle
family; `ui/mod.rs` draws the frame, `ui/timer.rs` draws the big countdown and owns the
block font, `ui/overlay.rs` draws the popups on top and `ui/layout.rs` holds the pure
geometry they all draw into; `app/mod.rs` runs the state machine, `app/commands.rs` runs
command mode, `app/selection.rs` owns the times cursor and all four overlays,
`app/progress.rs` owns the trend and the personal-best celebration, and `app/repair.rs`
repairs a save file. A directory is still one module for the purposes of this document: the
boundary rules below apply to `ui/` as a whole, not to each file inside it, and the same
goes for `app/`.

`cstimer.rs` is the newest of the eight and shows the shape a pure module should keep:
`export` takes a `&SaveFile` and returns a `String`, `import` takes a `&str` and returns
sessions, and neither opens a file or knows what an id is. The commands that call it do the
IO through the same paths every other write uses. A converter that wrote its own file would
have put a second answer to "where does data live" in the crate.

`types.rs` sits at the bottom and depends on nothing but `serde`. Its module doc says so
out loud: keep it dependency-light. Anything that grows a dependency there ripples through
every other module.

### The renderer never computes

`ui` is a pure function of `App`. Every value that needs `Instant` math is folded into a
plain field by `App::on_tick` before the frame is drawn:

```rust
// app/mod.rs, the producer
TimerState::Timing { started } => {
    self.display_millis = started.elapsed().as_millis() as u64;
}
```

```rust
// ui/mod.rs, the consumer: reads the field, never the clock
let text = format_millis(app.display_millis);
```

Wrong, even though it compiles:

```rust
// ui/mod.rs
if let TimerState::Timing { started } = app.state {
    let text = format_millis(started.elapsed().as_millis() as u64);
}
```

That version makes the frame time part of the displayed value, puts timer semantics in two
places, and makes the state machine untestable without a renderer. If the UI needs a new
derived value, add a field to `App` and refresh it where the state changes.

The same rule covers statistics, and there it also guards a performance cliff. `App::stats`
and `App::pbs` are refreshed by `App::refresh_derived` at every mutation site, and the
renderer reads them:

```rust
// ui/mod.rs
let st = &app.stats;
let pb = &app.pbs;
```

Calling `stats::personal_bests` from `draw` instead would put a walk over every solve of
every session of the puzzle inside a loop that runs every 15 ms. Anything the renderer needs
that is not `O(what is on screen)` belongs in a cached field.

### One owner per decision

`storage.rs` decides where data lives, `app` decides when to write it, and nobody else
touches either question. `App::save_now` is the single funnel:

```rust
/// Persist to disk. Never panics; failures surface in `status_msg`.
fn save_now(&mut self) {
    if let Err(e) = storage::save(&self.data_path, &self.save) {
        self.status_msg = Some(format!("save failed: {}", e));
    }
}
```

A command handler calls `self.save_now()`. It does not call `storage::save`, does not build
a path, and does not read `CUBETIMER_DATA`. Likewise, `stats.rs` never sees a `Session`
list it did not receive as an argument, which is what keeps it a pile of pure functions.

### Reaching around, and what to do instead

Reaching around looks like `ui` opening a file, `stats.rs` calling `storage::now_millis`,
or a caller poking `app.save.sessions[0]` instead of using `current_session()`. When you
need something a module does not expose, add the narrowest possible method to that module
and call it. `App::current_session`, `App::current_session_mut` and `App::armed_ready` all
exist for exactly that reason: they hide `active_index()` and `ARM_THRESHOLD` from
everybody else.

### Splitting a file

The split line is roughly 500 lines of non-test code, and it is a responsibility split, not
a line-count split. Never split by "first half, second half".

The three directories show what a good split looks like. `scramble/` divides by puzzle
family, because the generators share nothing but the `Rng` they are handed. `ui/` divides
by kind of work: `mod.rs` draws the frame, `timer.rs` draws the one panel with a rendering
model of its own, `overlay.rs` draws the popups over both, and `layout.rs` computes geometry
and touches neither `Frame` nor `App`, which turned the degradation rules from something
checked by eye into ordinary unit tests. `app/` divides by question asked: `repair.rs`
answers "is this save file internally consistent" as free functions over `&mut SaveFile`,
`commands.rs` owns command mode behind the single `on_command_key` entry point,
`selection.rs` owns which solve or session you are pointing at and which overlay is up,
neither of which the timer asks,
`progress.rs` owns how the session is going, which is the trend and the personal-best
celebration, and `mod.rs` keeps the state machine. Every one of those splits made the code
more testable, which is the sign you cut in the right place.

Two habits make a split of this kind cheap. First, the public surface does not move: `App`,
`TimerState` and `InputMode` are still `crate::app::*`, so `main.rs` and `ui` never learned
that `app` became a directory. Second, tests move with their subject, and the scaffolding
they share moves to a `#[cfg(test)] mod testkit` beside them rather than being duplicated.

A third habit is worth naming from the splits that produced `app/selection.rs`,
`ui/timer.rs` and `app/progress.rs`: the parent keeps one entry point per cluster and the
child keeps everything behind it. `on_key_idle` hands its cursor keys to
`selection::on_key_times` rather than importing `TIMES_PAGE` back out, `draw_body` calls
`timer::draw_timer` rather than knowing what `GLYPH_H` is, and `on_tick` calls
`expire_pb_banner` rather than knowing that the banner lasts five seconds. A constant that
has to travel back up to the parent is a sign the cut was made one function too deep.

No file in `src/` is over the line now, but one is close enough to name. `app/mod.rs` is at
493 non-test lines after the trend, the personal-best banner and the overlay toggles moved
out to `app/progress.rs` and `app/selection.rs`, which is under 500 with nothing to spare;
the next cut there is the inspection cluster, meaning the five `INSPECTION_*` constants with
`start_inspection`, `cancel_inspection`, `refresh_inspection` and `on_key_inspecting` behind
them. Below it are `app/commands.rs` at 432, `ui/overlay.rs` at 391, `ui/mod.rs` at 380,
`cstimer.rs` at 372 and `storage.rs` at 345, none of which has a seam worth cutting yet.
Treat growth past roughly 500 in any of them as the prompt to look again.

## Comments

### Doc comments

`///` on items, `//!` at the top of a module, first letter capitalized, and enough
specificity that the reader stops asking questions. This one is the model:

```rust
/// Read the save file.
///
/// A missing file is not an error, it just means "first run", and yields
/// `SaveFile::default()`. Anything that exists but cannot be parsed *is* an
/// error, so the caller can bail out instead of overwriting real user data
/// with a fresh default on the next save.
pub fn load(path: &Path) -> io::Result<SaveFile> {
```

It states the behavior, then the one thing a caller could not guess from the signature: the
asymmetry between missing and corrupt, and the data-loss reason behind it. Short items get
one line, and that is fine:

```rust
/// 0 = U/D, 1 = L/R, 2 = F/B.
const fn axis(self) -> u8 {
```

### Never multi-line blocks

Block comments are out, including the doc form. Rewrite them as consecutive `//` or `///`
lines. Bad:

```rust
/*
 * Finishes the running solve. It records the solve, then it saves the
 * file, and then it goes back to Idle with a new scramble.
 */
fn finish_solve(&mut self, started: Instant) {
```

Good, and shorter, because the narration of the body is dropped:

```rust
/// Finish the running solve: record it, persist, and go back to Idle with a
/// fresh scramble.
fn finish_solve(&mut self, started: Instant) {
```

### Inline comments

Use `//` only where a doc comment cannot go: inside a function body, on a match arm, above
a `const` in a group. Same brevity, same capitalization. The bar is that the comment
carries information the code does not:

```rust
// The key that stopped the timer stays dead until the user lets go of
// it: Windows console auto-repeat keeps delivering *Press* events while
// it is held, and those must not re-arm or restart inspection. Only its
// Release clears the block; other keys are unaffected.
if self.inert_key == Some(key.code) {
```

Nothing in that block is visible from reading the `if`. Compare with a comment that just
reads the line back:

```rust
// Set should_quit to true.
self.should_quit = true;
```

Delete that one. The same applies to section banners: `// ----- command mode` is a
navigation aid in a long file, not a comment, and it stays useful only while there are few
of them.

## Naming

- Modules, functions and locals are `snake_case`; types, enums and variants are
  `CamelCase`; consts are `SCREAMING_SNAKE_CASE`.
- Constants carry their unit or meaning in the name: `ARM_THRESHOLD`, `STOP_COOLDOWN`,
  `INSPECTION_PLUS2_MS`, `INSPECTION_DNF_MS`, `TICK`. Millisecond integers end in `_MS` or
  `millis`; a `Duration` does not need the suffix.
- Command handlers are `cmd_<verb>`: `cmd_new_session`, `cmd_switch_puzzle`,
  `cmd_delete_last`, `cmd_set_penalty`. Key handlers are `on_key_<state>`: `on_key_idle`,
  `on_key_inspecting`, `on_key_armed`.
- Predicates read as questions with no prefix ceremony: `armed_ready`, `in_stop_cooldown`,
  `is_legal`, `same_layer`.
- Getter and mutable-getter pairs are `x()` and `x_mut()`, as in `current_session` and
  `current_session_mut`.
- Numeric literals over four digits get separators: `1_700_000_000_000`, `15_000`.
- Test helpers are short on purpose, because they appear dozens of times per file: `s`,
  `dnf`, `plus2`, `ms`, `ago`, `press`, `release`.

Prefer the domain word over the generic one. It is `scramble`, `solve`, `penalty`,
`inspection` and `session`, never `data`, `item`, `value` or `info`.

## Error handling

### `io::Result` in, status message out

I/O returns `io::Result` and travels by `?` until it reaches a place that can tell the user
something. There are exactly three such places:

1. **Startup, in `main.rs`.** A corrupt save file prints the path and the error and exits
   non-zero, before the terminal is put in raw mode, so the message is actually readable
   and the file is never overwritten.
2. **The running app, in `App::save_now`.** The error becomes `status_msg` and the app
   keeps going. The user sees `save failed: ...` in the status line and can still quit
   cleanly.
3. **Shutdown, in `main.rs`.** The final save happens after `ratatui::restore()`, so its
   failure goes to stderr rather than into a terminal that is no longer in raw mode.

Errors created here carry context rather than a bare kind:

```rust
serde_json::from_slice::<SaveFile>(&bytes).map_err(|err| {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{} is not a valid cubetimer data file: {}", path.display(), err),
    )
})
```

### Never panic in the event loop

Anything reachable from `App::on_key` or `App::on_tick` must be panic-free, because a panic
there leaves the terminal in raw mode with no cursor. In practice:

- No `unwrap` or `expect` on anything that depends on user data, the filesystem or the
  clock. `active_index()` falls back to `0`, `data_file_path()` falls back to
  `./sessions.json`, `now_millis()` falls back to `0`.
- No indexing or slicing that a weird save file could push out of range. `App::sanitize`
  runs once in `App::new` and establishes the invariants the rest of the code leans on: at
  least one session exists, the active id points at a real session, and `next_session_id`
  cannot collide.
- Saturating and checked arithmetic at the edges: `saturating_sub` for scroll positions,
  `saturating_add` for ids, saturating `Rect` math throughout `ui/layout.rs` so a two-column
  terminal degrades instead of panicking.
- `debug_assert!` is fine for a condition that is genuinely impossible, but pair it with a
  real fallback for release builds, the way `generate_with_rng` does with its empty
  candidate list.
- `unwrap` and `expect` are allowed in tests, where a panic is the reporting mechanism.
  Give `expect` a message that names what failed: `.expect("save")`, not `.unwrap()`.

## Prose in the repo

Rule 4 applies to everything a human reads: `README.md`, `claude-docs/*`, doc comments,
status messages, commit subjects.

- Cubetimer when writing about the product, cubetimer for the binary, the crate, the data
  directory and anything else that is literally typed.
- No em dashes, in any file. A comma, a colon, a semicolon or two sentences will do the
  job.
- Status messages are lowercase, terse, and shaped like the thing they report:
  `inspection: on`, `no solves yet`, `usage: /session <id>`, `unknown command: foo`. They
  are read at a glance mid-solve, so they never grow into sentences.
- Skip the filler openings ("Basically", "It's worth noting that", "In order to"), the
  hedging, and the summary paragraph that repeats what was just said. Say the thing once.
