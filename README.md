# Cubetimer

Cubetimer is a speedcube timer that lives in your terminal. It hands you a
scramble, times the solve, keeps your averages, and remembers everything the
next time you open it. If you have used csTimer, the rhythm will feel familiar:
hold the space bar, wait for green, let go, solve, hit any key to stop.

Everything else (sessions, penalties, switching puzzles) happens from a small
command line at the bottom of the screen, so your hands never have to leave the
keyboard or reach for a mouse.

## Built with

Cubetimer is one Rust binary running one loop: wait up to 15 milliseconds for a
keypress, update the state, redraw the whole screen from that state.
[ratatui](https://ratatui.rs) draws the interface, and the compiled `.exe`
carries every library it uses inside it, so there is no runtime and no installer
on any machine you copy it to. Eight modules divide the work, each with a single
job: `types` holds the shared vocabulary, `app` owns the state machine and every
`/command`, `ui` turns state into a frame, `scramble` generates WCA notation one
puzzle family per file, `stats` does the WCA math, `storage` reads and writes the
save file atomically, and `cstimer` translates to and from csTimer's format.
Cubetimer is built for Windows first, because the hold-and-release space bar flow
needs a terminal that reports key releases and the Windows console does that
natively.

The full tour lives in [claude-docs/architecture.md](claude-docs/architecture.md),
and setup in [docs/getting-started.md](docs/getting-started.md).

## Quick start

```
git clone https://github.com/cameronjim/tui-cube-timer.git
cd tui-cube-timer
cargo build --release
```

The finished program lands at `target\release\cubetimer.exe`, and you can run it
from there using:
```
target\release\cubetimer.exe
```
or drop that one file anywhere on your PATH and run using:
from there using:
```
cubetimer
```
[docs/getting-started.md](docs/getting-started.md) walks the same route from a
bare machine: installing Rust, choosing a toolchain flavour, and what cargo
fetches on that first build.

## Keys

| Key | What it does |
|---|---|
| `space` | hold until green, release to start (a tap starts inspection when inspection is on) |
| any key | stops a running timer |
| `n` | new scramble |
| `/` | opens the command line |
| `↑` `↓` or `k` `j` | move the times-list selection one solve at a time |
| `PgUp` `PgDn` | move the times-list selection ten at a time |
| `Home` | jump the times-list selection back to your newest solve |
| `Enter` | open the selected solve in full; `r` inside loads its scramble |
| `h` or `?` | open and close the help overlay |
| arrows, `PgUp` `PgDn`, `Home` | inside `/sessions`, move the cursor; `Enter` switches |
| `Esc` | cancel inspection, close a popup, leave the command line, clear the status line |
| `q` | quit |

`Ctrl+C` also quits, from anywhere, if you ever need it.

## Commands

Press `/` to open the command line, type, and press `Enter`. Backspacing past
the leading slash gets you out again, as does `Esc`. Capitalisation and extra
spaces do not matter.

| Command | What it does |
|---|---|
| `/2x2` `/3x3` `/4x4` `/5x5` `/6x6` `/7x7` | switch to that puzzle's default session |
| `/pyraminx` `/skewb` `/megaminx` `/sq1` `/clock` | the same, for the five non-cube events |
| `/oh` | the same, for 3x3 one-handed |
| `/new [name]` | start a new session for the current puzzle |
| `/sessions` | open a picker listing every session with its id, puzzle and solve count |
| `/session <id>` | switch to a session by id |
| `/rename <name>` | rename the session you are in |
| `/delsession [id]` | delete a session and its solves, current one by default |
| `/delsession <from>-<to>` | delete every session in that id range, defaults refused |
| `/dnf` `/+2` `/ok` | set or clear the penalty on your last solve |
| `/del [n]` | delete solve number `n`, or your newest solve if you leave `n` off |
| `/inspect` | turn 15 second inspection on or off |
| `/hidetime` | hide or show the running time while you solve |
| `/export [path]` | write every session out as a csTimer file |
| `/import <path>` | read a csTimer file in as new sessions |
| `/trend` | open and close the graph of your last 50 solves |
| `/help` | open and close the help overlay |
| `/quit` or `/q` | quit |

The longer puzzle names have short forms: `/pyra` for `/pyraminx`, `/mega` for
`/megaminx`, `/square1` or `/square-1` for `/sq1`, and `/3x3oh` for `/oh`.

## Learn more

The user guide is in `docs/`, one topic per file:

- [Getting started](docs/getting-started.md): what to install, how to build, and which terminals work.
- [Using the timer](docs/using-the-timer.md): the screen, the colours, your first solve, and hiding the running time.
- [Inspection](docs/inspection.md): the 15 second countdown, the 8s and 12s judge calls, and the automatic penalties.
- [Reviewing solves](docs/reviewing-solves.md): moving through the times list, the solve popup, recalling a scramble, and `/del`.
- [Sessions and puzzles](docs/sessions.md): the twelve defaults, making your own, the picker, and switching puzzles.
- [Stats and the trend graph](docs/stats-and-trend.md): WCA averages, the stats strip, `/trend`, and the personal-best banner.
- [Taking your times with you](docs/import-export.md): `/export` and `/import`, and how they line up with csTimer.
- [Where your times live](docs/your-data.md): the save file, autosave, and `CUBETIMER_DATA`.
- [Scrambles](docs/scrambles.md): the notation, and how official each event's scrambles really are.

Everything about how Cubetimer is put together, the event loop, the module
boundaries, the save-file versioning and the scramble research, lives in
`claude-docs/`. If you are planning to send a change, the ground rules are in
[CLAUDE.md](CLAUDE.md): every change ships with its tests, and `cargo test` plus
`cargo clippy --all-targets` must both come back clean.
