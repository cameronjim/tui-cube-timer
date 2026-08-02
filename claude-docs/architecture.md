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

Eight modules, each with one job. Three of them are directories, because their single
responsibility grew large enough to need internal structure. The boundaries are
deliberate: `stats`, `scramble` and `cstimer` are pure and know nothing about terminals,
`ui` is read-only with respect to state, and only `storage` touches the filesystem.

| Module | Owns | Depends on |
| --- | --- | --- |
| `src/main.rs` | Terminal setup and teardown, startup load, the event loop | `app`, `storage`, `ui` |
| `src/app/` | `App` state, the timer state machine, key handling, `/commands`, save-file repair, the derived fields the UI reads | `cstimer`, `scramble`, `stats`, `storage`, `types` |
| `src/ui/` | Every widget drawn, the block font, layout degradation | `app`, `types` |
| `src/scramble/` | Scramble generation per puzzle | `types`, `rand` |
| `src/stats.rs` | Averages, session summaries, personal bests | `types` |
| `src/cstimer.rs` | Conversion between `SaveFile` and csTimer's export format, both directions | `types`, `serde_json` |
| `src/storage.rs` | Data file location, JSON load and atomic save, format migration, wall clock | `types` |
| `src/types.rs` | `Puzzle`, `Penalty`, `Solve`, `Session`, `Settings`, `SaveFile`, the default-session ids, time and date formatting | serde only |

`cstimer.rs` is a pair of pure functions, `export(&SaveFile) -> String` and
`import(&str) -> Result<Import, String>`, and it opens nothing. `/export` and `/import` in
`app/commands.rs` own the file handling and the id bookkeeping on the way in, which keeps
the format knowledge in one file and the IO where every other write already is. The format
itself is described in [algorithms.md](algorithms.md).

`src/scramble/` is one file per puzzle family behind a dispatching `mod.rs`. The
generators share nothing but the `Rng` they are handed, because the puzzles have no
notation in common:

| File | Covers |
| --- | --- |
| `mod.rs` | `generate` and `generate_with_rng`, dispatching on `Puzzle`; one-handed maps to the 3x3 generator here |
| `cube.rs` | 2x2 through 7x7: the move-type model, the pools, the same-axis-run rule |
| `pyraminx.rs` | Eleven layer turns plus tips |
| `skewb.rs` | Eleven fixed-corner turns |
| `megaminx.rs` | Seven Pochmann lines |
| `square1.rs` | Twists and slashes over a 24-slot shape simulator |
| `clock.rs` | Fifteen dial tokens around a `y2` |

`src/ui/` splits the same way, along the line between drawing and arithmetic. `mod.rs`
holds `draw`, the header, the stats strip, the times list and the status line; `timer.rs`
holds the big countdown in the middle of the frame and the block font it is drawn in;
`overlay.rs` holds the three popups, the help reference, the session picker and one solve in
full, which are the only things drawn over the frame rather than into it; `layout.rs` holds
the pure geometry, meaning panel heights, word wrapping, popup placement, the visible slice
of a scrolling list and `inner_of`. Nothing in `layout.rs` sees a `Frame` or an `App`, which
is what makes the degradation rules testable as ordinary functions rather than by eye.

| File | Covers | Non-test lines |
| --- | --- | --- |
| `mod.rs` | `draw`, the palette, `panel`, the header, stats, the trend sparkline, times list and status line | 430 |
| `timer.rs` | `timer_view`, `draw_timer`, the personal-best banner, `GLYPH_H` and the 5-row block font | 207 |
| `overlay.rs` | `draw_help`, `draw_sessions`, `draw_detail` | 240 |
| `layout.rs` | Panel heights, word wrap, `list_window`, `fit_count`, `stats_height`, popup placement | 268 |

`src/app/` splits by question asked. `mod.rs` is the state machine: timer states, key
handling, `on_tick`, and `refresh_derived`. `commands.rs` is command mode, entered through
the single `pub(super) fn on_command_key` and never touched from outside `app`.
`selection.rs` is the state that says which solve or session you are pointing at, which the
timer never reads: the times cursor, the solve-detail overlay and the sessions picker.
`progress.rs` answers "how is this session going", which is the trend window and the
personal-best celebration, neither of which is timer state. `repair.rs` answers "is this
save file internally consistent", which nothing in the timer flow ever asks, and is a
handful of free functions over `&mut SaveFile` rather than `App` methods. The split is
invisible from outside: `crate::app::App` and its public fields and methods keep the paths
they had when `app` was one file.

| File | Covers | Non-test lines |
| --- | --- | --- |
| `mod.rs` | `App`, `TimerState`, `InputMode`, the timer key handling, tick, `refresh_derived` | 497 |
| `commands.rs` | `on_command_key`, `execute_command`, every `cmd_*` handler | 381 |
| `selection.rs` | `on_key_times`, `open_solve_detail`, `recall_scramble`, `open_sessions_overlay`, `on_key_sessions` | 116 |
| `repair.rs` | `sanitize`, `free_next_id`, `take_id`, `dedupe_ids`, `evict_misfiled_defaults` | 95 |
| `progress.rs` | `trend_of`, `pb_banner_text`, `note_pb`, `clear_pb_banner`, `expire_pb_banner` | 76 |
| `testkit.rs` | `#[cfg(test)]` scaffolding the other five share: `TempPath`, `test_app`, `press`, `run_command` | 147 |

`types.rs` is the shared vocabulary and is kept dependency-light on purpose, so a change
to persistence or rendering never ripples into it. The dependency graph is acyclic and
shallow: `main.rs` at the top, `types.rs` at the bottom, nothing in between reaching
sideways except `ui` reading `app`.

Three rules keep the seams clean:

1. **`ui` never does `Instant` math.** Anything time-derived that the renderer needs
   is precomputed into plain fields on `App` (`display_millis`, `inspection_remaining`,
   `pending_inspection_penalty`, `inspection_stage`, `pb_banner`) by `App::on_tick` or by a
   state transition. The renderer reads numbers, not clocks.
2. **`ui` never computes statistics either.** `App::stats` and `App::pbs` are cached and
   the renderer reads them. See below for why this is a rule and not a preference.
3. **`app` never draws and `ui` never mutates.** `draw(frame: &mut Frame, app: &App)`
   takes a shared reference, so the type system enforces it.

### Derived statistics are cached, not recomputed

`App` carries two fields the renderer treats as read-only inputs:

```rust
/// Statistics for the active session, cached by `App::refresh_derived`.
pub stats: SessionStats,
/// All-time bests across every session of the active puzzle, cached by `App::refresh_derived`.
pub pbs: PersonalBests,
```

`App::refresh_derived` recomputes both, and every path that changes the solve list, a
penalty, the session list or the active session calls it. `App::new` calls it once after
`sanitize`. `/rename` is the only mutation that skips it, because a name changes no
number.

This started as rule 2 of `CLAUDE.md`: `stats.rs` is pure, `app` owns state, and a
renderer that calls `stats::personal_bests` is a renderer doing work that is not
rendering. It became load bearing for a second reason. `personal_bests` walks every solve
of every session of the active puzzle and re-sorts a sliding window at each step, once for
every average it tracks and now up to a thousand solves wide, so calling it from `draw` put
an unbounded amount of work inside a loop that runs every 15 ms. A save file with enough solves in it, whether from years of practice or from a
hand-edited file, could take longer than a frame to render and leave the terminal
unresponsive with no way out. Caching moves that cost to the handful of moments when the
numbers actually change, which is at most once per solve.

The 64 MB read limit in `storage::load`, described under Persistence, is the other half
of the same fix: it bounds how large the input can be before any of this runs.

### The trend window and the personal-best banner

Two more read-only fields exist for the same reason, and both live behind
`app/progress.rs`:

```rust
/// Effective times of the last `TREND_LEN` solves of the active session, oldest first.
pub trend: Vec<u64>,
/// The personal-best celebration currently on screen, cleared after `PB_BANNER`.
pub pb_banner: Option<String>,
```

`trend` is what `ui::draw_trend` hands to ratatui's `Sparkline`. `progress::trend_of` takes
the last `TREND_LEN` of 50 solves of the active session and keeps their
`effective_millis()`, so a `+2` plots as the time it cost and a DNF, having no time, is
absent rather than zero. It is refreshed by `refresh_derived` alongside the statistics, on
exactly the same set of mutations, which is why a penalty change or a session switch moves
the bars immediately.

`pb_banner` is the celebration line, and `banner_since` beside it is the private `Instant`
that retires it. `finish_solve` reads `pbs.single` and `pbs.ao5` *before* the new solve
joins them, calls `refresh_derived`, and then calls `note_pb` with the two old values;
`pb_banner_text` compares old against new and names each one the solve strictly beat:

```rust
fn improved(prev: Option<u64>, now: Option<u64>) -> Option<u64> {
    match (prev, now) {
        (Some(p), Some(n)) if n < p => Some(n),
        _ => None,
    }
}
```

The `Some(p)` is the rule that keeps a first-ever record quiet: there was nothing to beat,
so nothing is celebrated. The strict `<` is the rule that keeps an equalled record quiet
too. Because the comparison runs against `App::pbs`, which `refresh_derived` builds from
the sessions of the active puzzle only, a 2x2 record can never raise the banner on 3x3.

The banner comes down two ways: `expire_pb_banner`, called first thing in `on_tick`, drops
it once `PB_BANNER` of five seconds has passed, and `start_timing` drops it because the
next solve has begun. Arming does not, so glancing at the banner and reaching for the space
bar does not cost you the rest of the five seconds. `ui::timer` reads the field twice, once
for the bold black-on-light-green line it inserts above the digits and once to colour the
idle digits underneath in the same light green.

### Selection state and the judge-call stage

Four more fields exist purely so the renderer has something to read:

```rust
/// Times-list cursor, counted from the newest solve: 0 is the newest.
pub times_selected: usize,
/// Open solve-detail overlay, holding the same index-from-newest as `times_selected`.
pub solve_detail: Option<usize>,
/// Open sessions overlay, holding the cursor's index into `save.sessions`.
pub sessions_overlay: Option<usize>,
/// Judge-call stage of the running inspection: 0 under 8s, 1 from 8s, 2 from 12s.
pub inspection_stage: u8,
```

Counting from the newest solve rather than by list position is what makes the first two
indices survive a new solve arriving: index 0 is whatever is newest right now, and appending
never renumbers anything the user is looking at. Deleting does, which is why
`cmd_delete_solve` resets the cursor and closes the overlay outright rather than trying to
fix them up. `ui` clamps both anyway, since the solve behind an index can vanish between the
keypress and the frame.

`sessions_overlay` is the odd one of the three, and deliberately: it indexes
`save.sessions` in the order they are drawn rather than counting from an end, because the
session list is stable while the popup is open in a way the solve list is not. `ui` clamps
it too.

`inspection_stage` exists so the two warning colours cannot drift from the thresholds that
produce them: `App::refresh_inspection` decides the stage and `ui::timer::timer_view` does
nothing but match on it. Both the countdown colour and the caption underneath come out of
that one match:

```rust
// src/ui/timer.rs
let (stage_color, call) = match app.inspection_stage {
    0 => (C_INSPECT, None),
    1 => (C_STAGE1, Some("8s")),
    _ => (C_STAGE2, Some("12s")),
};
```

The caption slot is shared: an earned penalty takes it back off the call, so `+2` or `DNF`
in red replaces `8s` or `12s` and recolours the digits with it. A call is a thing that has
not cost you anything yet, and once one has, saying so is the more useful of the two. The
judge calls are silent; there is no audible signal and nothing about them leaves the frame.

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

Three things happen per iteration, in a fixed order. `on_tick` refreshes the derived fields
from the monotonic clock. `terminal.draw` rebuilds every line of the frame and ratatui diffs
the resulting cell buffer against the previous one, so only changed cells actually hit the
wire. Then `event::poll` blocks for at most 15 ms waiting for input. Nothing else is written
to the terminal from inside the loop, which is what lets ratatui own the screen outright.

That 15 ms is the loop's only pacing mechanism. When nothing is happening the loop runs
at roughly 66 frames per second; when a key arrives it returns early and the frame after
it reflects the new state immediately. The display is truncated to centiseconds, which
change every 10 ms, so a 15 ms poll means the hundredths digit occasionally skips a value
under load. At the speed a running timer scrolls that is invisible, and the recorded time
is unaffected: solve duration comes from `Instant::elapsed()` at the moment of the
stopping keypress, never from a frame counter.

One consequence of redrawing everything is that `ui` can be stateless. The times
selection, which popup is open and the command buffer all live on `App`, so a resize, a
repaint and a state change are the same operation from the renderer's point of view.

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

**Idle to Inspecting.** Only when `save.settings.inspection` is true. The space `Release`
triggers `start_inspection()`, which stamps `inspection_start`, clears any pending penalty,
sets `inspection_remaining` to 15 and resets `inspection_stage` to 0. Inspection
defaults to off and is toggled with `/inspect`, and because the flag lives on the save file
rather than on `App`, the choice survives a restart.

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
green" affordance that stackmat timers train into cubers. `ui` reads `armed_ready()`
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
stamps `stopped_at`, generates a fresh scramble, refreshes the derived fields, raises the
personal-best banner if the solve earned one, and writes the save file. The order of the
last three matters: the banner is decided by comparing the records read before the push
against the ones `refresh_derived` has just recomputed.

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

### Selection: the times cursor and the two list overlays

Everything in this section lives in `app/selection.rs`. It is reached from keys the timer
does not want, and it changes no timer state, which is the whole reason it is not in
`mod.rs`.

The times list carries a selection rather than a scroll offset. `on_key_idle` hands the
cursor keys to `on_key_times`, which moves `times_selected` by 1 for the arrows and `j`/`k`,
by `TIMES_PAGE` of 10 for `PageUp` and `PageDown`, and sets it to 0 for `Home`; both
`select_newer` and `select_older` saturate, and `select_older` clamps to the oldest solve,
so no key can walk the cursor off either end. Where the panel scrolls to follow it is
`ui/layout.rs`'s `list_window`, which is pure arithmetic, tested as such, and shared with
the sessions picker. Anything that changes the visible history, a new scramble included,
resets the cursor to 0 through `new_scramble`.

`Enter` in `Idle` calls `open_solve_detail`, which refuses an empty session and otherwise
stores the clamped index in `solve_detail`. `/sessions` calls `open_sessions_overlay`, which
stores `active_index()` so the cursor starts on the session you are in. Either field being
`Some` makes its overlay **modal**, and the two are checked in `on_key` after the inert-key
guard and before command mode, detail first:

```rust
if self.solve_detail.is_some() {
    if key.kind == KeyEventKind::Press {
        self.on_key_solve_detail(key);
    }
    return;
}

if self.sessions_overlay.is_some() {
    if key.kind == KeyEventKind::Press {
        self.on_key_sessions(key);
    }
    return;
}
```

The precedence runs **detail > sessions > help**. Detail outranks sessions because you can
only have opened it on top of the list, so closing gives back the thing underneath rather
than the whole screen at once. Help is not modal and cannot be open beside the sessions
picker at all: `toggle_help` clears `sessions_overlay` and `open_sessions_overlay` clears
`show_help`, so the exclusion is enforced where the state changes rather than in the
renderer.

Only three keys do anything inside the detail overlay: `r` calls `recall_scramble`, `Esc`
and `Enter` close. Because both branches sit below the `Timing` check, a solve running
underneath an overlay still stops on the first key, and because `open_solve_detail` is
reachable only from `on_key_idle`, `Enter` can never open a popup mid-solve in the first
place.

`recall_scramble` copies the stored scramble onto `app.scramble` and closes the overlay,
which is the whole feature: a solve records the scramble it was done on, so re-attempting a
case needs no new storage, only a copy. It converts the index-from-newest back to a list
position with `checked_sub` and bails if the arithmetic does not hold, since the solve may
have been deleted while the overlay was open.

`on_key_sessions` is the picker. The arrows and `j`/`k` move the cursor one row,
`PageUp`/`PageDown` move it by `SESSIONS_PAGE` of 10, `Home` goes to the top, and all four
clamp to `sessions.len() - 1`. `Enter` reads the id under the cursor, closes the overlay,
and calls `switch_to_session` unless that id is already active, in which case the switch is
skipped so the scramble on screen is not thrown away for nothing. `Esc` closes without
choosing. `switch_to_session` is the same function `/session <id>` calls, so a switch made
from the picker re-scrambles, refreshes the derived statistics, announces itself on the
status line and saves, exactly as the typed command does.

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
are tried first via `Puzzle::from_name`, so `/3x3` needs no entry in the match, and
neither does an alias. `from_name` is where `pyra`, `mega`, `square1` and `square-1` are
accepted alongside the canonical names, which keeps the alias list in one place instead
of spread across the command dispatcher and the help overlay. Everything unrecognised
produces `unknown command: <verb>` in the status line.

| Command | Effect |
| --- | --- |
| `/2x2` … `/7x7`, `/pyraminx`, `/skewb`, `/megaminx`, `/sq1`, `/clock`, `/oh` | Activate that puzzle's default session (see the navigation rule below) |
| `/new [name]` | Create and activate a session for the current puzzle |
| `/sessions` | Open the modal sessions picker |
| `/session <id>` | Activate a session by id, adopting its puzzle |
| `/rename <name>` | Rename the active session, refused on a default |
| `/delsession [id]` | Delete a session and its solves, the active one by default |
| `/del [n]`, `/delete [n]` | Remove solve `n` as the times list numbers it, the most recent by default |
| `/dnf`, `/+2`, `/ok` | Set the most recent solve's penalty |
| `/inspect` | Toggle 15-second inspection, persisted |
| `/hidetime` | Toggle masking the running time, persisted |
| `/export [path]` | Write every session out as a csTimer export, `cubetimer-cstimer-export.json` by default |
| `/import <path>` | Adopt a csTimer export, every session in it as a new one |
| `/help` | Toggle the help overlay |
| `/quit`, `/q` | Quit |

`cmd_delete_solve` takes its argument in the numbering the user can see, where the oldest
solve is 1 and the newest is the solve count, and converts to a `Vec` index by subtracting
one. Anything outside `1..=count`, and anything that is not a number, reports on the status
line and mutates nothing. A successful delete shifts every index from the newest, so it
resets `times_selected` to 0 and clears `solve_detail` rather than leaving either pointing
at a solve that has moved.

`/inspect` and `/hidetime` are the only commands whose effect outlives the run. Both flip a
field of `SaveFile::settings` and save, which is all persistence takes: `Settings` is part
of the serialised file, so the next `storage::load` hands the preference straight back.

`/export` and `/import` are the two commands that touch a file other than the save file,
and they are the only callers of `cstimer`. `cmd_export` serialises `self.save` and writes
the result with `fs::write`, defaulting to `EXPORT_FILE`, `cubetimer-cstimer-export.json`,
in the working directory; `shown_path` resolves a relative path against the current
directory so the status line names somewhere the user can actually go and look. It changes
no session, no solve and no setting, and it does not call `save_now`, so an export is safe
at any moment. `cmd_import` reads
the file, hands the text to `cstimer::import`, and adopts each session it gets back through
the same `push_session` that `/new` uses, so every arrival takes the next free id from
`repair::take_id`. **An import never merges.** It writes into no session that already
exists, leaves `active_session_id` alone, and reports `imported <n> sessions` with
`(<n> skipped)` appended when the file held session kinds Cubetimer has no event for. Both
commands report an IO or parse failure on the status line and change nothing.

Every mutating command calls `save_now()`. Failures never propagate: `save_now` catches
the `io::Error` and puts `save failed: <error>` in `status_msg`, so a read-only disk
degrades the app to an in-memory timer rather than killing it. `q` still quits.

---

## Sessions and the puzzle-navigation rule

A `Session` is an id, a name, a `Puzzle`, an ordered `Vec<Solve>` and a creation
timestamp. `SaveFile` holds all of them plus `active_session_id`, a monotonic
`next_session_id` and a `Settings` block. Solves are append-ordered, and every statistic in
`stats.rs` reads that ordering as chronological.

**Ids 1 through 12 are reserved for the twelve permanent default sessions**, one per
puzzle, all named `default`, in the order given by `Puzzle::DEFAULT_ORDER`:

| id | puzzle | id | puzzle | id | puzzle |
| --- | --- | --- | --- | --- | --- |
| 1 | 3x3 | 5 | 6x6 | 9 | megaminx |
| 2 | 2x2 | 6 | 7x7 | 10 | sq1 |
| 3 | 4x4 | 7 | pyraminx | 11 | clock |
| 4 | 5x5 | 8 | skewb | 12 | oh |

3x3 comes first because it is the common case; the six cubes keep the ids they held
before the five WCA events were added, so no existing 3x3 or 7x7 history has to move, and
one-handed took the next free id after them for the same reason. `Puzzle::Oh` is a full
event rather than a mode: it has its own default session, its own solve list and therefore
its own personal bests, and it is only the scramble generator it shares with `Puzzle::Cube3`.
`Puzzle::default_session_id` is that mapping, `FIRST_USER_ID` is 13, and
`Session::is_default` is the single predicate everything else asks (`id < FIRST_USER_ID`).
A default cannot be deleted, renamed or retyped; its solves behave like any others. The
point is that every puzzle has one destination that always exists, so navigation never has
to invent a session or guess at the "right" one.

`App::new` runs `sanitize` over the loaded file before anything else touches it, so the
rest of the code can assume five invariants without re-checking them: ids are unique, each
reserved id holds a session of its own puzzle, all twelve defaults exist and sort first,
`active_session_id` points at a real session, and `next_session_id` clears both every id
in use and the whole reserved range. A missing default is recreated empty and a dangling
active id falls back to the 3x3 default. Two repairs handle a file that has been edited
by hand:

- `dedupe_ids` hands a fresh user id to every session after the first that claims an id
  already taken. Duplicate ids would make `/session <id>`, `/delsession` and the
  active-session lookup all resolve to whichever copy came first, silently orphaning the
  rest.
- `evict_misfiled_defaults` moves any session sitting on a reserved id that is not its own
  puzzle's out to a user id, keeping its solves and taking the active id with it if it had
  it. `sanitize` then recreates the default it displaced, so `/megaminx` lands on a
  Megaminx session rather than on whatever an edited file parked at id 9.

All of this is structural repair of an already-current file, which is a different question
from reading an older format (that is `storage::load`'s job, below) and from a *corrupt*
file.

The interesting rule is what `/4x4` does when you are sitting in a 3x3 session.

**If the current session is one you created and has no solves, it is retyped in place.**
The id and the name are kept, only `puzzle` changes, and the status line says
`session '<name>' is now 4x4`. An empty session is not committed to anything, and someone
who makes a session and immediately picks a different puzzle should not be left with a
stray.

**In every other case the command navigates to `puzzle.default_session_id()`.** That
covers both a session with solves, whose statistics are pinned to one event because an
ao12 mixing 3x3 and 4x4 times is meaningless, and a default session, which is never
retyped even when empty. The destination is a fixed id rather than the most recently
created session, so `/3x3` and `/4x4` are round trips: the same two sessions, every time.
Either way the scramble is regenerated for the new puzzle and the file is saved.

`/delsession` removes a session and its solves outright. It resolves its argument to the
active session when there is none, refuses defaults and unknown ids with a status message,
and when the session being deleted is the active one it activates that session's puzzle
default, which is guaranteed to exist by the invariant above.

`/new` names an unnamed session `session N`, where N is one more than the number of
existing sessions for that puzzle, so the counter is per-puzzle rather than global. Any
change of session or scramble resets `times_selected` to 0 so the cursor is never left
pointing into a list that no longer exists.

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

`load` starts by stating the file and passing its length to `check_size`. Anything over
`MAX_SAVE_BYTES`, 64 MB, is refused with an `InvalidData` error naming the path and the
ceiling. The order matters: `fs::read` allocates the whole file before anyone gets a
chance to object, so the check has to run against the metadata rather than against the
bytes. A real save file with tens of thousands of solves in it is a few megabytes, so the
limit sits far above anything legitimate and exists only to keep a pathological or hostile
file from exhausting memory, or from feeding an unbounded number of solves into the
statistics.

Past that, `load` treats the two failure modes very differently. A missing file is simply
a first run and yields `SaveFile::default()`, without creating anything on disk. A file
that exists but does not parse is an `io::Error` of kind `InvalidData` carrying the path
and the serde message. That distinction is the whole point: silently falling back to a
default on a parse error would mean the next save overwrites a file that might have held
months of solves. `main.rs` acts on it before the TUI starts, because an error printed
into the alternate screen is an error nobody reads:

```
cubetimer: could not read your save file.
  path:  C:\Users\you\AppData\Roaming\cubetimer\data\sessions.json
  error: ... is not a valid cubetimer data file: expected value at line 1 column 1

Refusing to start so your data is not overwritten. Fix, move or delete that file
and run cubetimer again.
```

### Versions and the migration chain

`SaveFile.version` is the format number and `types::SAVE_VERSION` is what this build
writes, currently 4. `load` sorts a parsed file into three cases:

- **Newer than `SAVE_VERSION`**: refused with an `InvalidData` error naming the path and
  both versions. Fields this build does not know about could mean anything, and saving over
  them would discard them, so a downgrade must not start.
- **Equal**: returned untouched.
- **Older**: migrated in memory, one step per format bump. Nothing is written back until
  the app next saves, so a failed run leaves the old file exactly as it was.

The steps chain rather than branch:

```rust
if save.version < 2 {
    save = migrate_to_v2(save);
}
if save.version < 3 {
    save = migrate_to_v3(save);
}
if save.version < 4 {
    save = migrate_to_v4(save);
}
```

A version-1 file therefore passes through all three, and each step only has to be a small
transformation of the format immediately before it. Adding version 5 means appending one
more `if` and one more function, never editing the earlier ones.

**Version 1 to 2** predates the reserved id range: version 1 had a single session named
`default` on 3x3 and handed out ids from 1, so an old id 2 collides with what became the
2x2 default. `migrate_to_v2` builds the six defaults version 2 had, then walks the old
sessions in file order. One named `default` folds into its puzzle's new default, carrying
its solves and `created_at`, which is what keeps a user's 3x3 history on the session
`/3x3` now leads to. Everything else keeps its name, puzzle, solves and relative order and
is renumbered from the first user id, including a second session called `default` for a
puzzle already claimed. `active_session_id` follows whichever session it pointed at.

**Version 2 to 3** adds Pyraminx, Skewb, Megaminx, Square-1 and Clock, whose default ids 7
through 11 belonged to user sessions in version 2. `migrate_to_v3` shifts every session at
or above the old first user id up by 5, keeping its name, puzzle, solves and file order,
appends the five new defaults on the ids `types.rs` reserves for them, and follows the
active session to its new id. `next_session_id` ends up past every renumbered id and never
below the version's first user id.

**Version 3 to 4** is the same move for one event. 3x3 One-Handed takes id 12, which was a
user id in version 3, so `migrate_to_v4` shifts every session at or above
`V3_FIRST_USER_ID` up by `V4_ID_SHIFT` of 1, appends the one-handed default behind the
eleven that already existed, and follows the active session. Settings need no work in the
migration at all: `SaveFile::settings` arrived with this version and carries
`#[serde(default)]`, so an older file has already parsed into both preferences off by the
time any step runs, and each step passes `old.settings` through unchanged.

That is the pattern for adding an event, and it is worth naming: a new default id is a
schema change even though no field changes type, because the reserved range is part of the
contract. One `V<n>_ID_SHIFT`, one appended default, one renumbering of user sessions, one
version bump.

`migrate_to_v2` is written against module-local constants (`V2_DEFAULT_ORDER`,
`V2_FIRST_USER_ID`), not against `Puzzle::DEFAULT_ORDER` and `FIRST_USER_ID`. That is
deliberate and worth preserving. A migration describes a historical format, and if it read
the current constants it would silently change meaning the next time an event is added:
`migrate_to_v2` would start emitting twelve defaults and renumbering from 13, producing a
file that is neither valid version 2 nor what version 3 expects to receive. Frozen
constants keep each step a fixed function of a fixed format. `V3_ID_SHIFT` and
`V4_ID_SHIFT` are the same idea, one slot per new default event, and neither changes again
now that the version after it exists. Version 4 is why `V3_FIRST_USER_ID` had to be written
down: `migrate_to_v4` needs to know where version 3's user range began, and by the time it
runs `FIRST_USER_ID` says 13.

Migration lives in `storage.rs` because it is a question about the shape of a file on
disk, and `storage.rs` is the only module that reads one. `App::sanitize` stays what it
was, structural repair of an already-current file, and by the time it runs the version
question has been settled.

Changing the serde shape again means bumping `SAVE_VERSION`, writing the next migration
step in the same change, and adding a fixture test with hand-written JSON of the old
format rather than a struct literal, since a struct literal would silently follow the new
shape.

`save` writes atomically. It creates parent directories, serialises to pretty JSON with a
trailing newline, writes that to a sibling `<name>.tmp`, and renames the temporary file
over the target. `fs::rename` replaces an existing destination on both Unix and Windows,
and a rename within one directory is atomic on both, so the real file is only ever
replaced whole. A crash mid-write leaves the previous save intact and at worst a stray
`.tmp` beside it; if the rename itself fails, the temporary file is cleaned up and the
error is returned.

The temporary file is created through `write_fresh`, which opens it with `create_new`
rather than calling `fs::write`:

```rust
fs::OpenOptions::new().write(true).create_new(true).open(path)
```

`fs::write` opens whatever it finds at the path, so a symlink planted at the predictable
`<name>.tmp` location would redirect the write somewhere else entirely, with the user's
own permissions. `create_new` refuses any path that already exists, which removes that.
The one thing legitimately found there is a `.tmp` left behind by a crashed run, so an
`AlreadyExists` error triggers a single remove-and-retry; a second failure is real and
propagates.

Saves happen on **every solve** and on every mutating command, not on a timer and not only
at exit. A crash or a closed terminal costs nothing. `main.rs` does one final save after
restoring the terminal, and reports a failure on stderr where it is visible.

### The csTimer file is not the save file

`/export` and `/import` read and write a second, unrelated JSON shape, and none of this
section applies to it. It is never loaded at startup, it carries no `version` field of
Cubetimer's, and `cstimer::import` returns sessions with id 0 for the caller to number
rather than anything that could be dropped into a `SaveFile` as it stands. Keeping the two
apart is deliberate: the save format is a compatibility contract with files already on
users' disks, while the csTimer format is a contract with another program and moves when
that program moves. A change to one is never a reason to touch the other, and only
`storage.rs` decides where Cubetimer's own data lives.

---

## How `ui` degrades

Every dimension in this section is decided by `ui/layout.rs`, which is arithmetic over
`Rect` and `&str` and nothing else. Keeping it out of `mod.rs` is what lets the rules below
be asserted directly: `header_height`, `footer_height`, `scramble_rows`, `wrapped_rows`,
`centered`, `inner_of` and `puzzle_help_rows` are ordinary functions with ordinary return
values, so the degradation behaviour is unit-tested rather than checked by resizing a
terminal and squinting.

The frame is three horizontal bands, with the middle one split into a timer column and a
times column:

```
┌ cubetimer ─ 3x3 ─ session: default (#1) ─ inspection: off ─────────────┐
│              R U2 F' L B2 D R' U F2 L' B D2 R F' U2 ...                │  header
├──────────────────────────────────────────┬─────────────────────────────┤
│                                          │ times 40/42 ─────────────── │
│              ████ ████    ████ █  █      │   42  12.34                 │
│                 █    █ ██    █ █  █      │   41  14.02+                │
│              ████ ████    ████ ████      │ > 40  DNF(13.11)            │
│              █       █ ██    █    █      │   39  11.87                 │
│              ████ ████    ████    █      │   38  13.02                 │
│                                          │  ...                        │
│              (dim state caption)         │                             │
├─ stats ──────────────────────────────────┤                             │
│current mo3 12.80   ao5 12.99   ao12 13.45│                             │
│        best single 9.87                  │                             │
│best    mo3 11.02   ao5 11.20   ao12 12.02│                             │
├──────────────────────────────────────────┴─────────────────────────────┤
│ space hold+release: start · /: commands · n: new scramble · h: help    │
└────────────────────────────────────────────────────────────────────────┘
```

There is no minimum size check and no "terminal too small" screen. Instead every
dimension is computed with saturating arithmetic and each panel drops out on its own when
it stops being useful, so the window can be dragged down to a couple of rows without a
panic and without a blank screen:

- **Header** is 3 rows below a terminal height of 9. Above that it is adaptive, between
  `HEADER_MIN_H` of 4 and `HEADER_MAX_H` of 10, and sized to the scramble it has to show.
  See below.
- **Footer** (status line, command buffer or hint) is 3 rows at 6 rows or more, otherwise
  0 and it is not drawn at all.
- **Times column** is 26 columns wide at width 60 or more, 20 columns at 44 or more, and
  disappears below that; the timer then takes the whole body.
- **Stats strip** is 5 rows when the left column has at least 12 rows, otherwise 0, and 7
  rows once it has at least 14, the extra 2 being the trend sparkline. `stats_height` is
  the whole of that decision and `trend_rows` is the conditional half of it: the strip
  appears at `STATS_H` (5) once `STATS_H + TIMER_MIN_H` fits, and the sparkline adds
  `TREND_H` (2) on top only once `STATS_H + TREND_H + TIMER_MIN_H` does. Both thresholds
  are written against `TIMER_MIN_H`, so the bars can never be the reason the block font is
  lost, and every terminal too short for them renders exactly what it did before the trend
  existed. An empty trend claims no rows at all.

  The three text rows are fixed and each opens with a dim prefix column of `STAT_PREFIX_W` (7,
  the width of `current`, the longest of the three prefixes) plus a space: `current` over
  `mo3 ao5 ao12 ao100 ao1000`, then an empty prefix over
  `best single, worst single, mean, solves`, then `best` over the same five windows again,
  this time the all-time personal bests from `App::pbs`. The middle row pays for the
  column it does not use, because the three only read as a block if their values start in
  the same place, and `stat_budget` is where that toll is taken out of the width before
  anything is packed. A row never wraps into the one below it, so `fit_count` decides how
  many entries survive the remaining budget and the rest are dropped from the right. Each
  averages row runs smallest window first, which is also the order in which the numbers
  start existing as a session grows, so what a narrow terminal keeps is what a short session
  actually has. Every label is unambiguous on its own: `current` and `best` are the two rows,
  the top and bottom rows hold nothing but averages so a rolling one sits directly over its
  personal best, and the session's own extremes in the middle spell out `best single` and
  `worst single` rather than borrowing a row name. The wider prefix costs each row four
  columns of budget. At 80 the strip is 52 columns wide and each row packs into 44, which
  holds four averages or two of the longer session entries; by 44 columns the strip is 22
  wide, every row is down to the one entry `fit_count` will never drop, and
  `best single 9.87` is wide enough that the paragraph clips it, which is the intended
  degradation: a clipped number still says more than a blank row.

  `draw_trend` takes the rows below and spends the same `STAT_PREFIX_W` plus a space on a
  dim `trend` label, so the bars begin in the column the three rows put their values in.
  What it plots is `App::trend`, tail-sliced to the width it was given, because a panel too
  narrow for fifty bars should drop the oldest solves rather than the ones just done. The
  bars are times, so a dip is a fast solve. A panel with no room for anything past the
  label draws neither.
- **Big digits** need 5 rows (`GLYPH_H`, which lives in `ui/timer.rs` and which
  `layout::TIMER_MIN_H` is derived from) and enough width for the rendered glyph string.
  When either is missing, `draw_timer` falls back to the same text as an ordinary bold
  coloured line, so the time is always legible even in a two-row body.
- **Personal-best banner**, the line `draw_timer` inserts above the digits while
  `App::pb_banner` is `Some`, is added only when the panel has a row left after the digits
  themselves, so it is the first thing the timer panel gives up and the time is never
  pushed off the screen by its own celebration.
- **State caption**, the dim line under the digits that names what the timer is doing
  (`inspecting`, `keep holding…`, `release to start` and so on), is only appended when at
  least two spare rows remain after the digits.
- **Help overlay** is `HELP_W` (62) columns wide and as tall as its content, currently 30
  text rows inside its border, placed by `centered()`, which clamps both to the available
  area. It is skipped entirely below 4 by 4. The height follows the content rather than
  being a constant because the content grows: `draw_help` builds the puzzle row from
  `Puzzle::ALL`, so a thirteenth event lengthens the popup automatically instead of
  silently clipping a line. Twelve puzzle names no longer fit on one row and the popup does
  not wrap, so `puzzle_help_rows` packs them into as many rows as the description column
  allows and the continuation rows are drawn under an empty key column.
- **Sessions overlay** is `SESSIONS_W` (44) columns and one row per session, plus the hint
  row and the border. `sessions_popup` returns both the rect and how many sessions to draw,
  because the renderer holds no scroll state: the bottom inner row always belongs to the
  `enter: switch   esc: close` hint, so a list too tall for the popup gets one row fewer and
  scrolls under the cursor through the same `list_window` the times panel uses. The title
  then counts the cursor's position, as in `sessions 14/20`, and says nothing when the whole
  list fits. Each row is `id name puzzle [count]` in fixed columns, the name clipped to
  `SESSIONS_NAME_W` (18) with an ellipsis and the twelve `default` names dimmed. The two
  states a row can be in are deliberately different marks: the active session keeps a `>` in
  the marker column, the cursor reverses its whole row, and the row that is both reads as a
  reversed row with a marker on it.
- **Solve-detail overlay** is `DETAIL_W` (52) columns and grows with the scramble it has to
  show: `detail_popup` runs the scramble through the same `scramble_rows` the header uses,
  adds `DETAIL_FIXED_ROWS` of 6 for the time, the date, the hint and the blanks between
  them, adds the border and caps at `DETAIL_MAX_H` of 20. Megaminx lands at 15 rows and a
  one-line scramble at the floor. `centered()` clamps it like the help popup, and
  `draw_detail` returns early below 4 by 4.

`draw` picks one popup and only one, in the same order `on_key` does: detail, then sessions,
then help. The detail popup wins because it is the one the user just asked for and the only
thing that can be opened on top of the list. Help and sessions cannot both be open in the
first place: `App::toggle_help` clears `sessions_overlay` and `open_sessions_overlay` clears
`show_help`, so the exclusion is enforced where the state changes rather than in the
renderer. Help is the only non-modal popup of the three, so the keys behind it keep working
and `Esc` closes it before it moves on to clearing the status line.

### The adaptive header

Scrambles are no longer one line. Megaminx emits seven, joined with newlines, and a 7x7
scramble of 100 moves wraps to several rows on a narrow terminal. A fixed 4-row header
would truncate both, so `header_height` grows it:

1. `scramble_rows` counts what the text actually needs: the scramble's explicit `\n` lines,
   each run through `wrapped_rows`, which reproduces greedy word wrap at the panel's inner
   width so the count matches what ratatui's `Wrap { trim: true }` will draw.
2. Add 2 for the borders and clamp to `HEADER_MIN_H ..= HEADER_MAX_H`, so 4 rows minimum
   and 10 maximum. Ten rows fits Megaminx's seven lines with the borders; a scramble
   wanting more than that truncates rather than eating the screen.
3. Clamp again against what is left after the footer and `TIMER_MIN_H`, the rows the block
   font needs. The header stops growing before it would push the timer below its glyph
   height.

That last step is the priority rule: when the two cannot both fit, the scramble truncates
and the big digits survive. A truncated scramble is recoverable by widening the window or
pressing `n`; a timer that has silently dropped to a plain text line mid-solve is the
worse loss of the two.

Supporting details: `inner_of` computes a bordered block's inner rect with
`saturating_sub`, so a 1-column rect yields a zero-size inner rect that every drawing
function checks for and returns from. The block font's `glyph` returns a blank cell for
any character it does not know, so no input string can misalign the rows or panic. The
times list and the sessions popup both slice with `skip`/`take` bounded by `list_window`,
and both clamp their cursor to `total - 1`. The same blank-glyph fallback is what lets
`hide_time` draw the running timer as `...` through the ordinary block-font path.

Colour encodes state and is the fastest thing to read mid-solve: white idle, yellow
inspecting, red while holding space below the arm threshold, green once `armed_ready()`,
cyan while running. The inspection countdown then shifts with `inspection_stage`, light
magenta from 8 seconds and light red from 12. Inspection past its limits overrides both,
switching the countdown to red and adding a `+2` or `DNF` caption underneath, so a penalty
already earned is never read as a judge call.

---

## See also

- [algorithms.md](algorithms.md) for averages, personal bests, scramble generation and
  inspection penalty thresholds.
- [../CLAUDE.md](../CLAUDE.md) for project-level guidance.
- [code-style.md](code-style.md) for code style rules.
- [testing.md](testing.md) for the testing policy.
