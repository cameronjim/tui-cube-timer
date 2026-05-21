# Cubetimer architecture

Cubetimer is a speedcube timer that lives entirely in the terminal. It is a single Rust
binary crate (`cubetimer`) built on ratatui 0.29 and its bundled crossterm backend. This
document explains how the program is put together: what each module owns, how a frame
gets on screen, how key events become timer transitions, and how solves reach disk.

The companion document [algorithms.md](algorithms.md) covers the maths: WCA trimmed
averages, personal bests, scramble generation and inspection penalties. Project-level
conventions live in [../CLAUDE.md](../CLAUDE.md); code style rules are in
[code-style.md](code-style.md) and the testing policy is in [testing.md](testing.md).

---

## Crate layout

Seven files, each with one job. The boundaries are deliberate: `stats.rs` and
`scramble.rs` are pure and know nothing about terminals, `ui.rs` is read-only with
respect to state, and only `storage.rs` touches the filesystem.

| Module | Owns | Depends on |
| --- | --- | --- |
| `src/main.rs` | Terminal setup and teardown, startup load, the event loop | `app`, `storage`, `ui` |
| `src/app.rs` | `App` state, the timer state machine, key handling, `/commands` | `scramble`, `storage`, `types` |
| `src/ui.rs` | Every widget drawn, the block font, layout degradation | `app`, `stats`, `types` |
| `src/scramble.rs` | Scramble generation per puzzle | `types`, `rand` |
| `src/stats.rs` | Averages, session summaries, personal bests | `types` |
| `src/storage.rs` | Data file location, JSON load and atomic save, wall clock | `types` |
| `src/types.rs` | `Puzzle`, `Penalty`, `Solve`, `Session`, `SaveFile`, time formatting | serde only |

`types.rs` is the shared vocabulary and is kept dependency-light on purpose, so a change
to persistence or rendering never ripples into it. The dependency graph is acyclic and
shallow: `main.rs` at the top, `types.rs` at the bottom, nothing in between reaching
sideways except `ui.rs` reading `app` and `stats`.

Two rules keep the seams clean:

1. **`ui.rs` never does `Instant` math.** Anything time-derived that the renderer needs
   is precomputed into plain fields on `App` (`display_millis`, `inspection_remaining`,
   `pending_inspection_penalty`) by `App::on_tick` or by a state transition. The
   renderer reads numbers, not clocks.
2. **`app.rs` never draws and `ui.rs` never mutates.** `draw(frame: &mut Frame, app: &App)`
   takes a shared reference, so the type system enforces it.

---

## The rendering model

`ratatui::init()` puts the terminal into raw mode and switches to the alternate screen;
`ratatui::restore()` undoes both. Raw mode is what makes the timer possible at all:
without it the terminal line-buffers input, the shell handles Ctrl-C, and there is no way
to see a key going down as distinct from a line being submitted.

Rendering is immediate mode. There is no widget tree, no retained scene graph and no
dirty-region tracking. Every iteration of the loop rebuilds the entire frame from the
current contents of `App`:

```rust
// src/main.rs
const TICK: Duration = Duration::from_millis(15);

while !app.should_quit {
    app.on_tick();
    terminal.draw(|frame| ui::draw(frame, app))?;

    if event::poll(TICK)? {
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Repeat {
                app.on_key(key);
            }
        }
    }
}
```

Three things happen per iteration, in a fixed order. `on_tick` refreshes the derived
fields from the monotonic clock. `terminal.draw` rebuilds every line of the frame and
ratatui diffs the resulting cell buffer against the previous one, so only changed cells
actually hit the wire. Then `event::poll` blocks for at most 15 ms waiting for input.

That 15 ms is the loop's only pacing mechanism. When nothing is happening the loop runs
at roughly 66 frames per second; when a key arrives it returns early and the frame after
it reflects the new state immediately. The display is truncated to centiseconds, which
change every 10 ms, so a 15 ms poll means the hundredths digit occasionally skips a value
under load. At the speed a running timer scrolls that is invisible, and the recorded time
is unaffected: solve duration comes from `Instant::elapsed()` at the moment of the
stopping keypress, never from a frame counter.

One consequence of redrawing everything is that `ui.rs` can be stateless. Scroll position,
help visibility and the command buffer all live on `App`, so a resize, a repaint and a
state change are the same operation from the renderer's point of view.

---

## The input pipeline

Key events come from crossterm through ratatui's re-export (`ratatui::crossterm`); there
is no separate crossterm dependency, which keeps the two from drifting apart on version
bumps. `event::read()` yields an `Event`, and only `Event::Key` is acted on. Resize and
mouse events need no handling because the next frame is a full redraw anyway.

Each `KeyEvent` carries a `kind`:

- `KeyEventKind::Press` when the key goes down,
- `KeyEventKind::Release` when it comes back up,
- `KeyEventKind::Repeat` for terminal-generated auto-repeat.

`Repeat` is dropped twice, once in `main.rs` and once at the top of `App::on_key`, so the
state machine can be tested without replicating the loop's filter.

**Release events are the load-bearing part.** A speedcube timer needs the hold-then-release
gesture: you press space, hold it while the timer arms, and the run starts the instant
your hand leaves the key. That is impossible if the terminal only reports presses.
Windows is the primary target precisely because the Windows console API delivers real
key-up records, so crossterm surfaces `Release` natively with no negotiation.

Elsewhere, `main.rs` tries to obtain the same signal through the kitty keyboard protocol:

```rust
let enhanced = matches!(supports_keyboard_enhancement(), Ok(true))
    && io::stdout()
        .execute(PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::REPORT_EVENT_TYPES,
        ))
        .is_ok();
```

The flags are popped again before `ratatui::restore()` so the terminal is handed back in
the state it was found in. On a terminal that supports neither mechanism, presses still
arrive, so commands, scrolling and quitting work, but the hold-and-release start does not.

Windows brings one quirk that shapes the design: **keyboard auto-repeat on the Windows
console arrives as a stream of `Press` events, not `Repeat` events.** Holding space down
therefore looks identical to tapping it thirty times a second. The inert-key mechanism
described below exists to absorb exactly this.

---

## The timer state machine

`TimerState` has four variants, each carrying the instants it needs:

```rust
pub enum TimerState {
    Idle,
    Inspecting { started: Instant },
    Armed { since: Instant, from_inspection: bool },
    Timing { started: Instant },
}
```

```
                       inspection OFF (default)
                    ┌──────────────────────────────────────┐
                    │           space Press                │
                    ▼                                      │
                 ┌──────┐                             ┌────┴────┐
   ┌────────────▶│ Idle │                             │  Armed  │
   │             └──┬───┘                             │  since  │
   │                │                                 │  from_  │
   │  Esc /         │ space Release                   │inspect. │
   │  cancel        │ (inspection ON)                 └──┬───┬──┘
   │                ▼                                    │   │
   │        ┌───────────────┐    space Press             │   │
   │        │  Inspecting   │───────────────────────────▶│   │
   └────────┤    started    │◀───────────────────────────┘   │
            └───────────────┘   space Release, held < 300 ms │
                    ▲                                        │
                    │                    space Release,      │
                    │                    held >= 300 ms      │
                    │                            ┌───────────┘
                    │                            ▼
                 ┌──┴───┐                  ┌───────────┐
                 │ Idle │◀─────────────────┤  Timing   │
                 └──────┘   ANY key Press  │  started  │
                            finish_solve() └───────────┘
```

Transition by transition:

**Idle to Armed.** With inspection off, which is the default, a space `Press` calls
`arm(false)` and the timer is armed immediately. With inspection on, a space `Press` in
`Idle` is ignored outright, because that press belongs to the gesture that will *end*
inspection, not start it.

**Idle to Inspecting.** Only when `inspection_enabled` is true. The space `Release`
triggers `start_inspection()`, which stamps `inspection_start`, clears any pending
penalty and sets `inspection_remaining` to 15. Inspection is off at startup and is
toggled with `/inspect`; it is not persisted.

**Inspecting to Armed.** A space `Press` calls `arm(true)`. The countdown keeps running:
`on_tick` still calls `refresh_inspection` while `Armed { from_inspection: true }`,
reading from the retained `inspection_start`, so a late arm can still earn its penalty.

**Inspecting to Idle.** `Esc` calls `cancel_inspection()`, which drops the countdown and
clears `pending_inspection_penalty`. A cancelled inspection leaves no trace.

**Armed to Timing.** On space `Release`, `armed_ready()` decides:

```rust
pub fn armed_ready(&self) -> bool {
    match self.state {
        TimerState::Armed { since, .. } => since.elapsed() >= ARM_THRESHOLD,
        _ => false,
    }
}
```

`ARM_THRESHOLD` is 300 ms. Hold long enough and the release starts the run
(`start_timing()`); let go early and `unarm(from_inspection)` puts you back where you
came from. The threshold is what turns a stray tap into a no-op and gives the "hold until
green" affordance that stackmat timers train into cubers. `ui.rs` reads `armed_ready()`
directly to colour the display red while holding and green once ready.

**Armed back to its origin.** `unarm(true)` restores `Inspecting { started }` from the
saved `inspection_start`, so an aborted arm does not restart the countdown; you get the
same clock back, minus the second or two you spent fumbling. `unarm(false)` returns to
`Idle`. An `Esc` `Press` while armed also unarms, a safety valve for the case where a
`Release` event never arrives (a terminal without release reporting, or focus lost
mid-hold).

**Timing to Idle.** Any key `Press` at all stops the timer. That is handled before every
other branch in `on_key`, so no key can do its normal job while a solve is running:

```rust
if let TimerState::Timing { started } = self.state {
    if key.kind == KeyEventKind::Press {
        self.inert_key = Some(key.code);
        self.finish_solve(started);
    }
    return;
}
```

`finish_solve` reads `started.elapsed()` in milliseconds, builds a `Solve` with the
pending inspection penalty, the scramble that was on screen and a wall-clock timestamp
from `storage::now_millis()`, pushes it onto the active session, clears the pending
penalty, sets `display_millis` to the final time so the frozen result stays on screen,
stamps `stopped_at`, generates a fresh scramble and writes the save file.

### Two guards after a stop

Stopping the timer is the one place where the input model fights the hardware, so there
are two independent mechanisms.

**The inert key.** The key that stopped the timer is recorded in `inert_key` and then
swallowed completely until its `Release` is observed:

```rust
if self.inert_key == Some(key.code) {
    if key.kind == KeyEventKind::Release {
        self.inert_key = None;
    }
    return;
}
```

This is what lets a cuber slam space at the end of a solve and keep leaning on it. Every
auto-repeat `Press` that Windows sends while the key is down is dropped, and so is the
final `Release`, which would otherwise be read as "the user let go of space in Idle" and
start inspection. Only that one key code is blocked; other keys work normally throughout.

**The stop cooldown.** `STOP_COOLDOWN` is 300 ms measured from `stopped_at`. Inside that
window space cannot begin a new interaction even after a clean release and a fresh tap:

```rust
if key.code == KeyCode::Char(' ') && self.in_stop_cooldown() {
    return;
}
```

The inert key handles a key that is still held; the cooldown handles a key that bounced or
a hand that is faster than its owner intended. Only space is affected, so `/`, `n`, `h`,
the scroll keys and `q` all keep working immediately after a solve.

**Ctrl-C** is checked before both guards and sets `should_quit` unconditionally, since raw
mode swallows the signal the shell would normally deliver.

---

## Command mode

`InputMode` is `Normal` or `Command`. Pressing `/` in `Normal` mode switches to `Command`
and seeds `command_buf` with `"/"`; the leading slash is part of the buffer, which is why
`Backspace` popping it back to empty exits command mode. That gives the buffer a single
source of truth and makes "erase everything" and "cancel" the same gesture. `Esc` also
exits, `Enter` executes and returns to `Normal`, and printable characters append (Ctrl
and Alt chords are filtered out, as are control characters). While in command mode all
key `Release` events are ignored and normal-mode bindings are inert, so typing `/q` does
not quit.

`execute_command` normalises before dispatching: trim, strip a leading `/`, trim again,
split at the first whitespace, lowercase the verb. An empty line is a no-op. Puzzle names
are tried first via `Puzzle::from_name`, so `/3x3` needs no entry in the match. Everything
unrecognised produces `unknown command: <verb>` in the status line.

| Command | Effect |
| --- | --- |
| `/2x2` … `/7x7` | Switch puzzle (see the retype rule below) |
| `/new [name]` | Create and activate a session for the current puzzle |
| `/sessions` | List every session as `id:name(puzzle)[count]` |
| `/session <id>` | Activate a session by id, adopting its puzzle |
| `/rename <name>` | Rename the active session |
| `/del`, `/delete` | Remove the most recent solve |
| `/dnf`, `/+2`, `/ok` | Set the most recent solve's penalty |
| `/inspect` | Toggle 15-second inspection |
| `/help` | Toggle the help overlay |
| `/quit`, `/q` | Quit |

Every mutating command calls `save_now()`. Failures never propagate: `save_now` catches
the `io::Error` and puts `save failed: <error>` in `status_msg`, so a read-only disk
degrades the app to an in-memory timer rather than killing it. `q` still quits.

---

## Sessions and the puzzle-retype rule

A `Session` is an id, a name, a `Puzzle`, an ordered `Vec<Solve>` and a creation
timestamp. `SaveFile` holds all of them plus `active_session_id` and a monotonic
`next_session_id`. Solves are append-ordered, and every statistic in `stats.rs` reads that
ordering as chronological.

`App::new` runs `sanitize` over the loaded file before anything else touches it, so the
rest of the code can assume three invariants without re-checking them: at least one
session exists, `active_session_id` points at a real session, and `next_session_id` is
greater than every existing id. A hand-edited or truncated file is repaired rather than
rejected, which is a different question from a *corrupt* file (see persistence below).

The interesting rule is what `/4x4` does when you are sitting in a 3x3 session.

**If the current session has no solves, it is retyped in place.** The id and the name are
kept, only `puzzle` changes, and the status line says `session '<name>' is now 4x4`. An
empty session is not committed to anything, and a fresh install should not accumulate a
graveyard of untouched sessions just because someone tried a few puzzles before settling
on one.

**If the current session has solves, it keeps its puzzle forever.** Its statistics are
pinned to one event, and an ao12 that mixes 3x3 and 4x4 times is meaningless. Cubetimer
instead activates the most recently created session of the target puzzle, ordering by
`(created_at, id)` so ties break deterministically, and creates one named `default` if
none exists. Either way the scramble is regenerated for the new puzzle and the file is
saved.

`/new` names an unnamed session `session N`, where N is one more than the number of
existing sessions for that puzzle, so the counter is per-puzzle rather than global. Any
change of session or scramble resets `times_scroll` to 0 so the list is never left
scrolled into a region that no longer exists.

---

## Persistence

Everything lives in one pretty-printed JSON file. `storage::data_file_path()` resolves it
in three steps:

1. `CUBETIMER_DATA`, if set and non-empty, used verbatim as a full *file* path. This is
   the escape hatch for portable installs, for keeping practice data in a synced folder,
   and for tests, which must never touch the real save file.
2. `directories::ProjectDirs::from("", "", "cubetimer")`, giving the platform data
   directory plus `sessions.json` (on Windows, under `%APPDATA%`).
3. `./sessions.json` in the working directory, if the platform has no notion of a data
   directory at all.

`load` treats the two failure modes very differently. A missing file is simply a first
run and yields `SaveFile::default()`, without creating anything on disk. A file that
exists but does not parse is an `io::Error` of kind `InvalidData` carrying the path and
the serde message. That distinction is the whole point: silently falling back to a default
on a parse error would mean the next save overwrites a file that might have held months of
solves. `main.rs` acts on it before the TUI starts, because an error printed into the
alternate screen is an error nobody reads:

```
cubetimer: could not read your save file.
  path:  C:\Users\you\AppData\Roaming\cubetimer\data\sessions.json
  error: ... is not a valid cubetimer data file: expected value at line 1 column 1

Refusing to start so your data is not overwritten. Fix, move or delete that file
and run cubetimer again.
```

`save` writes atomically. It creates parent directories, serialises to pretty JSON with a
trailing newline, writes that to a sibling `<name>.tmp`, and renames the temporary file
over the target. `fs::rename` replaces an existing destination on both Unix and Windows,
and a rename within one directory is atomic on both, so the real file is only ever
replaced whole. A crash mid-write leaves the previous save intact and at worst a stray
`.tmp` beside it; if the rename itself fails, the temporary file is cleaned up and the
error is returned.

Saves happen on **every solve** and on every mutating command, not on a timer and not only
at exit. A crash or a closed terminal costs nothing. `main.rs` does one final save after
restoring the terminal, and reports a failure on stderr where it is visible.

---

## How `ui.rs` degrades

The frame is three horizontal bands, with the middle one split into a timer column and a
times column:

```
┌ cubetimer ─ 3x3 ─ session: default (#1) ─ inspection: off ─────────────┐
│              R U2 F' L B2 D R' U F2 L' B D2 R F' U2 ...                │  header
├──────────────────────────────────────────┬─────────────────────────────┤
│                                          │ times (42) ──────────────── │
│              ████ ████    ████ █  █      │  42  12.34                  │
│                 █    █ ██    █ █  █      │  41  14.02+                 │
│              ████ ████    ████ ████      │  40  DNF(13.11)             │
│              █       █ ██    █    █      │  39  11.87                  │
│              ████ ████    ████    █      │  38  13.02                  │
│                                          │  ...                        │
│              (dim state caption)         │                             │
├─ stats ──────────────────────────────────┤                             │
│ ao5 12.99   ao12 13.45   ao100 -         │                             │
│ best 9.87   worst 18.20   mean 13.20 ... │                             │
│ PB single 9.87  PB ao5 11.20  ...        │                             │
├──────────────────────────────────────────┴─────────────────────────────┤
│ space hold+release: start · /: commands · n: new scramble · h: help    │
└────────────────────────────────────────────────────────────────────────┘
```

There is no minimum size check and no "terminal too small" screen. Instead every
dimension is computed with saturating arithmetic and each panel drops out on its own when
it stops being useful, so the window can be dragged down to a couple of rows without a
panic and without a blank screen:

- **Header** is 4 rows when the terminal is at least 9 rows tall, otherwise 3.
- **Footer** (status line, command buffer or hint) is 3 rows at 6 rows or more, otherwise
  0 and it is not drawn at all.
- **Times column** is 26 columns wide at width 60 or more, 20 columns at 44 or more, and
  disappears below that; the timer then takes the whole body.
- **Stats strip** is 5 rows when the left column has at least 12 rows, otherwise 0.
- **Big digits** need 5 rows (`GLYPH_H`) and enough width for the rendered glyph string.
  When either is missing, `draw_timer` falls back to the same text as an ordinary bold
  coloured line, so the time is always legible even in a two-row body.
- **State caption**, the dim line under the digits that names what the timer is doing
  (`inspecting`, `keep holding…`, `release to start` and so on), is only appended when at
  least two spare rows remain after the digits.
- **Help overlay** is centred at up to 62 by 26 via `centered()`, which clamps to the
  available area, and is skipped entirely below 4 by 4.

Supporting details: `inner_of` computes a bordered block's inner rect with
`saturating_sub`, so a 1-column rect yields a zero-size inner rect that every drawing
function checks for and returns from. The block font's `glyph` returns a blank cell for
any character it does not know, so no input string can misalign the rows or panic. The
times list slices with `skip`/`take` bounded by `inner.height` and clamps `times_scroll`
to `total - 1`.

Colour encodes state and is the fastest thing to read mid-solve: white idle, yellow
inspecting, red while holding space below the arm threshold, green once `armed_ready()`,
cyan while running. Inspection past its limits switches the countdown to red and adds a
`+2` or `DNF` caption underneath.

---

## See also

- [algorithms.md](algorithms.md) for averages, personal bests, scramble generation and
  inspection penalty thresholds.
- [../CLAUDE.md](../CLAUDE.md) for project-level guidance.
- [code-style.md](code-style.md) for code style rules.
- [testing.md](testing.md) for the testing policy.
