# Cubetimer

Cubetimer is a speedcube timer that runs in the terminal: scrambles in WCA notation for
twelve events (2x2 through 7x7, Pyraminx, Skewb, Megaminx, Square-1, Clock, 3x3 one-handed),
random-state for the three of those small enough to solve exhaustively,
optional 15 second inspection with the 8 and 12 second judge calls, mo3 through ao1000,
session bests and sessions persisted as JSON, and csTimer import and export. It is
written in Rust on top of ratatui and crossterm, and it is Windows-first, because the
hold-and-release timer flow depends on key release events that the Windows console
delivers natively.

Read this file first, then the guide you need:

| Document | Covers |
|---|---|
| `claude-docs/architecture.md` | Module map, data flow, the event loop |
| `claude-docs/algorithms.md` | Scramble generation, WCA averages, penalty rules |
| `claude-docs/code-style.md` | Comments, module boundaries, naming, error handling |
| `claude-docs/testing.md` | What to test, how to test it, the bar for "done" |
| `README.md` | The front door: what Cubetimer is, quick start, the keys and commands tables |
| `docs/` | The user guide, one topic per file: setup, the timer, inspection, sessions, stats, import and export, data, scrambles |

## Non-negotiable rules

**1. Every code change ships with its tests.** A new feature arrives with new tests.
Changed behavior arrives with updated tests. Same change set, no follow-up promises, no
exceptions. The only change that may leave the suite untouched is one where nothing
observable changed. Details in `claude-docs/testing.md`.

**2. Separation of concerns, one responsibility per module.** The ten modules and their
single jobs:

| Module | Responsibility |
|---|---|
| `main.rs` | Process lifecycle and the event loop: load, init terminal, poll, draw, restore, final save |
| `app/` | State machine: timer states, key handling, `/commands`, and the derived fields the UI reads |
| `ui/` | Rendering only: turns `App` fields into ratatui widgets, mutates nothing |
| `scramble/` | Scramble generation in WCA notation: the dispatch, plus the nine random-move generators |
| `solver/` | Random-state scrambles for the three events small enough to solve exhaustively |
| `cube/` | Facelet cube state for the NxN events: what a scramble leaves on each sticker |
| `stats.rs` | Pure statistics: trimmed averages, session summaries, session bests |
| `cstimer.rs` | Pure conversion to and from csTimer's export format; no file IO, the commands do that |
| `storage.rs` | Where the save file lives, and reading and writing it atomically |
| `types.rs` | Shared vocabulary and the serde shape of the persisted file |

Four of those are directories split along an internal seam, and a fifth, `cube/`, is one
file:

| File | Responsibility |
|---|---|
| `app/mod.rs` | Timer state machine, key handling, tick, the derived fields the UI reads |
| `app/commands.rs` | Command mode: the `/command` parser and every `cmd_*` handler |
| `app/inspection.rs` | The 15 second countdown: the judge calls, the penalty it earns, the keys it answers |
| `app/selection.rs` | Overlay state: the times cursor, the solve detail, the sessions picker, the help, trend and preview toggles |
| `app/progress.rs` | How the session is going: the trend window and the session-best celebration |
| `app/repair.rs` | Save-file structural repair: `sanitize` and the id bookkeeping under it |
| `app/testkit.rs` | Test scaffolding shared by the six, `#[cfg(test)]` only |
| `ui/mod.rs` | `draw`, the header, the stats strip, the times list and the status line |
| `ui/timer.rs` | The big countdown: `timer_view`, `draw_timer` and the block font |
| `ui/overlay.rs` | The five popups: help, the session picker, the trend graph, the scramble preview, and one solve in full |
| `ui/net.rs` | Pure net geometry: a `Cube` as colored block-glyph lines. No `Frame`, no `App` |
| `ui/layout.rs` | Pure geometry: panel heights, word wrap, popup placement. No `Frame`, no `App` |
| `scramble/mod.rs` | Dispatch on `Puzzle`, nothing else |
| `scramble/{cube,megaminx,square1,clock}.rs` | One random-move puzzle family each |
| `solver/mod.rs` | The shared `Engine`: distance table, uniform sampling, the exact-length search, and the dispatch |
| `solver/cube2.rs` | 2x2 corner coordinates, move tables, exact-11 scrambles |
| `solver/pyraminx.rs` | Pyraminx edge and axial coordinates, tips, exact-11 scrambles |
| `solver/skewb.rs` | Skewb centre and corner coordinates, move tables, exact-11 scrambles |

No module reaches around another's API. `ui` reads `App` fields and never touches
`Instant`, the filesystem, or `stats` (statistics are cached on `App` by `refresh_derived`
because recomputing them in the 15 ms draw loop can hang on a large save file). The preview
cube is cached the same way and for the same reason, by `refresh_preview` at every site that
assigns the scramble: `cube/` is pure state that a scramble string feeds and `ui/net.rs`
draws, and it never appears in `draw`. `solver/` sits behind `scramble/` the same way:
`scramble::generate` and `generate_with_rng` are still the only public path to a scramble and
still infallible, and nothing in `app` or `ui` knows a solver exists. `app` is
the only caller of `storage::save` and of `cstimer`, which opens no file of its own.
Nothing outside `storage.rs` decides where data lives. `App`'s public surface is the whole
of `app`: `crate::app::App` keeps every path it had before the directory split, and
`main.rs` and `ui` are unaware there is more than one file behind it.

Files stay small: past roughly 500 lines of non-test code, split along responsibility lines
rather than appending. Nothing in `src/` is over the line, and counting stops at the file's
`#[cfg(test)]`. `ui/overlay.rs` is still the file closest to it at 456, then `app/mod.rs` at
454 and `app/commands.rs` at 446, with the largest of the new solvers, `solver/skewb.rs`, at
431 next, then `ui/mod.rs` at 384, `cstimer.rs` at 372, `storage.rs` at 345, `solver/pyraminx.rs`
at 329 and `cube/mod.rs` at 319 behind them. The rest of `solver/` has plenty of room,
`solver/cube2.rs` at 259 and `solver/mod.rs` at 160, and `scramble/cube.rs` dropped to 147 when
the 2x2 left it. **The seam waiting now is in `ui/overlay.rs`**: the trend cluster,
meaning `TREND_PERCENTILE`, `TREND_TRIM_MIN`, `TREND_FLAT_PAD` and the `TrendPlot` arithmetic
of `trend_top`, `trend_plot` and `trend_ticks` behind `draw_trend`, which is the one popup
that computes a picture instead of laying text out. `solver/skewb.rs` has a second one behind
that, and it is worth naming now that the file is the fourth largest: the geometry derivation,
meaning `Rot`, `rot`, `turned`, `leans`, `face_of`, `corner_of` and `corners_turned` behind the
single `turn`, which is the part that turns two arrays of corner signs into every table and is
the only part of the file that reasons about the solid rather than about indices. Nothing else
has an obvious seam left, so treat growth past roughly 500 in any of them as the prompt to look
for one.
`app/inspection.rs` is the most recent cut and it shows the shape to aim for, as
`app/progress.rs`, `app/selection.rs` and `ui/timer.rs` did before it: the parent keeps one
entry point per cluster (`start_inspection`, `refresh_inspection`, `on_key_inspecting`,
`note_best`, `on_key_times`, `draw_timer`) and the child keeps every constant and helper
behind it.

**3. Comments are single-line, always.** Never `/* */` blocks. Use `///` doc comments on
items and `//!` at the top of a module, first letter capitalized. Use sparse `//` inline
comments, same brevity and capitalization, only where a doc comment cannot go. A comment
earns its place by saying what the code cannot: the invariant, the reason for a guard, the
platform quirk being worked around. Never narrate the obvious.

**4. Writing style for docs, comments and user-visible text.** The product is Cubetimer in
prose and cubetimer in code, paths and the command line. No em dashes anywhere. Write
fluid, professional, human prose: direct, specific, no filler, no marketing voice, no
throat-clearing.

**5. Build constraints.** The toolchain is `stable-x86_64-pc-windows-gnu`. The
`parking_lot = "=0.12.3"` and `parking_lot_core = "=0.9.10"` pins in `Cargo.toml` are load
bearing, and the comment above them explains exactly why; never remove or bump them
casually. Zero clippy warnings is the bar, not a goal.

**6. The persisted JSON is a compatibility contract.** The serde shape of `SaveFile`,
`Session`, `Solve`, `Settings`, `Puzzle` and `Penalty` in `types.rs` is what already sits on
users' disks. Adding a field means giving it `#[serde(default)]`, as every field of
`Settings` has. Renaming, removing or retyping one means bumping `SaveFile.version` and
writing the migration in the same change. `storage::load` deliberately refuses to parse a
file it does not understand rather than overwrite it, so a careless schema edit locks
people out of their own times.

`SAVE_VERSION` is 4 and `load` migrates v1 -> v2 -> v3 -> v4 as a chain, one step per bump.
Each step is written against constants frozen at that version (`V2_DEFAULT_ORDER`,
`V2_FIRST_USER_ID`, `V3_ID_SHIFT`, `V3_FIRST_USER_ID`, `V4_ID_SHIFT`), never against
`Puzzle::DEFAULT_ORDER` or `FIRST_USER_ID`, because a migration describes a historical
format and must not change meaning when a new event is added. Reserved session ids 1
through 12 are also part of the contract: a new event takes the next free id and needs a
migration shifting user ids past it, exactly as v3 and v4 did.

None of this covers `cstimer.rs`. That format is a contract with another program, it is
never loaded at startup, and it carries no version of Cubetimer's; changing one is never a
reason to touch the other.

## Before you call a change done

1. `cargo test` is fully green.
2. `cargo clippy --all-targets` is silent.
3. The docs match the code. Anything a user can see belongs in `docs/`, on the page for
   that topic, with `README.md` updated only when the change touches the front door: the
   keys table, the commands table or the quick start. Anything a contributor needs to know
   belongs in `claude-docs/*`.
