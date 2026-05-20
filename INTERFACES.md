# cubetimer — module interface contract

A speedcube timer TUI in Rust (ratatui 0.29). Binary crate `cubetimer`. Windows is the
primary target (crossterm delivers key **Release** events there, which the hold-space
timer flow relies on).

**Module layout** (each file owned by exactly one author; do not edit files you don't own):

```
src/main.rs      — terminal setup + event loop        (owner: app agent)
src/app.rs       — state machine, key handling, commands (owner: app agent)
src/ui.rs        — all rendering                      (owner: ui agent)
src/scramble.rs  — scramble generation                (owner: scramble agent)
src/stats.rs     — averages, PBs (pure functions)     (owner: stats agent)
src/storage.rs   — load/save JSON persistence         (owner: storage agent)
src/types.rs     — shared types (ALREADY WRITTEN — read it, never modify it)
```

`main.rs` declares: `mod app; mod scramble; mod stats; mod storage; mod types; mod ui;`
Modules reference each other as `crate::types::...` etc.

Dependencies available (Cargo.toml is already written, do not modify):
ratatui = "0.29" (use `ratatui::crossterm` re-export — do NOT add a separate crossterm dep),
serde + derive, serde_json, rand = "0.8", directories = "5".

Times: raw times stored in **milliseconds**; displayed truncated to centiseconds via
`types::format_millis` / `types::format_solve` (already implemented).

---

## src/scramble.rs

```rust
use crate::types::Puzzle;
use rand::Rng;

/// Random-move scramble in WCA notation, moves separated by single spaces.
pub fn generate(puzzle: Puzzle) -> String;                       // uses thread_rng
pub fn generate_with_rng<R: Rng>(puzzle: Puzzle, rng: &mut R) -> String;
```

Rules:
- Suffixes: ``(none)``, `'`, `2`, chosen uniformly.
- Constraint: never the same face twice in a row (regardless of layer width — `R` then
  `Rw` is NOT allowed since both turn the R face... treat face+width as distinct move
  *types* but the **axis rule** below governs). Precise rule set:
  1. Consecutive moves must not use the same face letter with the same width.
  2. No three consecutive moves on the same axis (axes: U/D, L/R, F/B).
  3. Additionally if move[i] and move[i-1] share an axis, move[i] must not repeat
     move[i-2]'s face+width.
- Move pools & lengths:
  - 2x2: faces U, R, F only; 9–11 moves.
  - 3x3: U D L R F B; exactly 20 moves.
  - 4x4: U D L R F B + Uw Rw Fw; 44 moves.
  - 5x5: U D L R F B + Uw Dw Lw Rw Fw Bw; 60 moves.
  - 6x6: 5x5 pool + 3Uw 3Rw 3Fw; 80 moves.
  - 7x7: 5x5 pool + 3Uw 3Dw 3Lw 3Rw 3Fw 3Bw; 100 moves.
- Include `#[cfg(test)]` unit tests (seeded rng, e.g. `rand::rngs::StdRng::seed_from_u64`)
  verifying: lengths, legal move pool, constraint rules hold over many generations.

## src/stats.rs

```rust
use crate::types::{Session, Solve};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvgResult { Time(u64), Dnf, NotEnough }

impl AvgResult {
    /// "12.34" / "DNF" / "-"
    pub fn display(&self) -> String;
}

/// WCA trimmed average of the LAST n solves (most recent n in the slice's tail).
/// Trim ceil(n/20) best and worst (1 for ao5/ao12, 5 for ao100).
/// DNF sorts as worst; if more DNFs than the trim count -> AvgResult::Dnf.
/// Fewer than n solves -> NotEnough. Result truncated to ms mean of remaining.
pub fn average_of(n: usize, solves: &[Solve]) -> AvgResult;

/// Best rolling aoN anywhere in the session (None if never enough / all DNF windows).
pub fn best_average_of(n: usize, solves: &[Solve]) -> Option<u64>;

#[derive(Debug, Clone)]
pub struct SessionStats {
    pub count: usize,           // total solves
    pub valid_count: usize,     // non-DNF
    pub best: Option<u64>,      // best single (effective ms)
    pub worst: Option<u64>,
    pub mean: Option<u64>,      // plain mean of non-DNF effective times
    pub ao5: AvgResult,
    pub ao12: AvgResult,
    pub ao100: AvgResult,
}

pub fn session_stats(solves: &[Solve]) -> SessionStats;

#[derive(Debug, Clone, Default)]
pub struct PersonalBests {
    pub single: Option<u64>,
    pub ao5: Option<u64>,
    pub ao12: Option<u64>,
    pub ao100: Option<u64>,
}

/// All-time PBs across the given sessions (caller filters to one puzzle).
pub fn personal_bests(sessions: &[&Session]) -> PersonalBests;
```

Effective time = `Solve::effective_millis()` (None = DNF). Include thorough
`#[cfg(test)]` tests: ao5 with/without DNFs, 2 DNFs -> Dnf, +2 handling, trim counts,
rolling best, empty input.

## src/storage.rs

```rust
use crate::types::SaveFile;
use std::io;
use std::path::{Path, PathBuf};

/// Env var CUBETIMER_DATA (full file path) overrides; else
/// directories::ProjectDirs::from("", "", "cubetimer") data_dir + "sessions.json";
/// fallback "./sessions.json".
pub fn data_file_path() -> PathBuf;

/// Missing file -> Ok(SaveFile::default()). Corrupt/unparseable file -> Err
/// (NEVER silently overwrite user data). Wrap serde errors into io::Error.
pub fn load(path: &Path) -> io::Result<SaveFile>;

/// Pretty-printed JSON. Creates parent dirs. Atomic: write to sibling
/// "<name>.tmp" then fs::rename over the target.
pub fn save(path: &Path, data: &SaveFile) -> io::Result<()>;

/// Unix epoch milliseconds now.
pub fn now_millis() -> u64;
```

Tests: round-trip via a temp file under `std::env::temp_dir()` (unique name, clean up),
missing-file default, corrupt-file error.

## src/app.rs  (+ owns src/main.rs)

```rust
use ratatui::crossterm::event::KeyEvent;
use std::path::PathBuf;
use std::time::Instant;
use crate::types::{Penalty, SaveFile, Session};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerState {
    Idle,
    Inspecting { started: Instant },
    Armed { since: Instant, from_inspection: bool },
    Timing { started: Instant },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode { Normal, Command }

pub struct App {
    pub save: SaveFile,
    pub state: TimerState,
    pub input_mode: InputMode,
    pub command_buf: String,        // includes leading '/'
    pub scramble: String,
    pub inspection_enabled: bool,   // default false (opt in with /inspect)
    pub status_msg: Option<String>,
    pub show_help: bool,
    pub times_scroll: usize,        // 0 = newest visible at top
    pub should_quit: bool,
    // Derived-for-rendering fields, refreshed by on_tick()/state changes so ui.rs
    // never does Instant math:
    pub display_millis: u64,               // big timer value to render
    pub inspection_remaining: Option<i64>, // whole seconds, may go negative
    pub pending_inspection_penalty: Penalty,
    pub data_path: PathBuf,
}

impl App {
    pub fn new(save: SaveFile, data_path: PathBuf) -> App; // fresh scramble for active session
    pub fn current_session(&self) -> &Session;
    pub fn current_session_mut(&mut self) -> &mut Session;
    /// True when Armed long enough (>= 300ms) that releasing space starts the timer.
    pub fn armed_ready(&self) -> bool;
    pub fn on_key(&mut self, key: KeyEvent);  // sees both Press and Release kinds
    pub fn on_tick(&mut self);
}
```

Behavior spec:
- **Normal mode, Idle**: `/` -> Command mode (buf = "/"). `q` -> quit. `n` -> new
  scramble. `h` or `?` -> toggle help overlay. Up/Down -> scroll times list.
  Space **Press** with inspection off (**the default**) -> Armed{from_inspection:false}.
  Space **Release** with inspection on -> start Inspecting.
- **Inspecting**: only reachable once `/inspect` has turned inspection on (it starts
  off). Countdown from 15. Space Press -> Armed{from_inspection:true}.
  Esc -> cancel to Idle (clears pending penalty). on_tick sets
  pending_inspection_penalty: >15s elapsed -> Plus2, >17s -> Dnf.
- **Armed**: on Space **Release**: if `armed_ready()` -> Timing{started:now};
  else -> back to where it came from (Inspecting keeps its original `started`; store
  what's needed to restore it — adjust the struct if you need an extra field, but keep
  the fields above intact for ui.rs).
- **Timing**: on_tick updates display_millis. ANY key Press stops: finalize solve
  with raw elapsed ms, penalty = pending_inspection_penalty, scramble = current,
  timestamp = storage::now_millis(); push to current session; storage::save
  (on error: status_msg, don't crash); reset pending penalty; new scramble; -> Idle.
  The stopping keypress is consumed (must not also trigger its normal action), and
  that key stays **completely inert until its Release is observed** — every further
  Press of it is dropped (Windows auto-repeat resends Press, not Repeat), so the
  user may keep holding space indefinitely after stopping. Additionally, for
  `STOP_COOLDOWN` (300ms) after a solve is finalized, space Press/Release cannot
  begin a new interaction (no arming, no inspection); other keys are unaffected.
- **Command mode**: printable chars append to command_buf; Backspace pops (popping the
  leading '/' exits command mode); Esc exits; Enter executes then exits. Unknown
  command -> status_msg "unknown command: ...".
- **Commands**: `/2x2 /3x3 /4x4 /5x5 /6x6 /7x7` switch puzzle. If the current session
  has **no solves**, retype it in place (keeps id and name), status
  `session '<name>' is now <puzzle>`. Otherwise (session has solves, so its stats are
  pinned to one puzzle) activate the most recently *created* session of that puzzle,
  else create one named "default". Either way: new scramble, save. `/new [name]` new session for current puzzle (default name: "session N"),
  make active. `/sessions` -> status_msg listing "id:name(puzzle)[count]" of all
  sessions. `/session <id>` switch to that session (and its puzzle). `/rename <name>`
  rename current. `/del` delete last solve (confirm not required). `/dnf` `/+2` `/ok`
  set penalty of last solve. `/inspect` toggle inspection (off at startup). `/help` toggle help overlay.
  `/quit` or `/q` quit. All mutating commands save to disk.
- Session/scramble state changes reset times_scroll to 0. status_msg is replaced by
  each new command result; cleared on next command.
- App must be quittable even if save fails (`q` still works; show error in status_msg).

`main.rs`: `ratatui::init()` / `ratatui::restore()`; loop: `event::poll(15ms)`, forward
key events to `app.on_key`, call `app.on_tick()` each iteration, `terminal.draw(|f| ui::draw(f, &app))`.
On startup: `storage::load(storage::data_file_path())` — on corrupt-file error, print a
plain message telling the user the path and exit(1) WITHOUT starting the TUI.
On quit: final save, restore terminal.
Filter: ignore key events with `kind == KeyEventKind::Repeat`.

## src/ui.rs

```rust
use ratatui::Frame;
use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App);
```

Layout (full screen, ratatui blocks with rounded borders):

```
┌ cubetimer ─ 3x3 ─ session: default (#1) ──────────────────────────────┐
│  scramble text, wrapped, centered, bold                                │  ~4 rows
├───────────────────────────────────────────────┬────────────────────────┤
│                                               │ Times ──────────────── │
│        BIG TIMER (block-glyph digits)         │ 42  12.34              │
│        e.g.  12.34  or  0:07 countdown        │ 41  14.02+             │
│                                               │ 40  DNF(13.11)         │
│                                               │ …scrollable            │
├───────────────────────────────────────────────┤ (right col ~24 wide)   │
│ ao5 12.99   ao12 13.45   ao100 —              │                        │
│ best 9.87   mean 13.20   solves 42            ├────────────────────────┤
│ PB single 9.87  PB ao5 11.20  PB ao12 12.01   │ (PB panel or continue) │
├───────────────────────────────────────────────┴────────────────────────┤
│ status/help line: keybinds, or command buffer when typing, or status   │
└─────────────────────────────────────────────────────────────────────────┘
```

- Big digits: hand-rolled 5-row block font (chars `0-9 : .`) using `█` (and spaces);
  render centered. Timer text: `types::format_millis(app.display_millis)`.
- Colors by state: Idle=white; Inspecting=yellow showing the countdown number
  (`inspection_remaining`, show "+2" in red once negative past the penalty thresholds);
  Armed & !armed_ready=red; Armed & armed_ready=green; Timing=cyan.
- Stats panel: `stats::session_stats(&session.solves)` + `stats::personal_bests(...)`
  filtered to sessions of the current puzzle. Use `AvgResult::display()`,
  `types::format_millis`.
- Times list: newest first, numbered descending (count..1), `types::format_solve`,
  respect `app.times_scroll`. Highlight best single (green) and worst (red).
- Bottom line: Command mode -> show `command_buf` + block cursor; else status_msg if
  set; else hint line: `space hold+release: start · /: commands · n: new scramble · h: help · q: quit`.
- Help overlay (`app.show_help`): centered popup (Clear widget) listing all keys and
  commands from the app.rs spec.
- Handle small terminals gracefully (never panic on tiny Rects; saturating math).
- If state is Inspecting, big area shows the countdown seconds as big digits instead
  of the timer.

Keep everything `pub(crate)`-private except `draw`. No unit tests required for ui.rs.
