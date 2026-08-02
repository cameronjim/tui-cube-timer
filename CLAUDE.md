# Cubetimer

Cubetimer is a speedcube timer that runs in the terminal: scrambles in WCA notation for
twelve events (2x2 through 7x7, Pyraminx, Skewb, Megaminx, Square-1, Clock, 3x3 one-handed),
optional 15 second inspection with the 8 and 12 second judge calls, mo3 through ao1000,
personal bests and sessions persisted as JSON, and csTimer import and export. It is
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
| `README.md` | User-facing behavior: keys, commands, where data lives |

## Non-negotiable rules

**1. Every code change ships with its tests.** A new feature arrives with new tests.
Changed behavior arrives with updated tests. Same change set, no follow-up promises, no
exceptions. The only change that may leave the suite untouched is one where nothing
observable changed. Details in `claude-docs/testing.md`.

**2. Separation of concerns, one responsibility per module.** The eight modules and their
single jobs:

| Module | Responsibility |
|---|---|
| `main.rs` | Process lifecycle and the event loop: load, init terminal, poll, draw, restore, final save |
| `app/` | State machine: timer states, key handling, `/commands`, and the derived fields the UI reads |
| `ui/` | Rendering only: turns `App` fields into ratatui widgets, mutates nothing |
| `scramble/` | Scramble generation in WCA notation, one generator per puzzle family |
| `stats.rs` | Pure statistics: trimmed averages, session summaries, personal bests |
| `cstimer.rs` | Pure conversion to and from csTimer's export format; no file IO, the commands do that |
| `storage.rs` | Where the save file lives, and reading and writing it atomically |
| `types.rs` | Shared vocabulary and the serde shape of the persisted file |

Three of those are directories, each split along its own internal seam:

| File | Responsibility |
|---|---|
| `app/mod.rs` | Timer state machine, key handling, tick, the derived fields the UI reads |
| `app/commands.rs` | Command mode: the `/command` parser and every `cmd_*` handler |
| `app/selection.rs` | Selection state: the times cursor, the solve-detail overlay, the sessions picker |
| `app/progress.rs` | How the session is going: the trend window and the personal-best celebration |
| `app/repair.rs` | Save-file structural repair: `sanitize` and the id bookkeeping under it |
| `app/testkit.rs` | Test scaffolding shared by the five, `#[cfg(test)]` only |
| `ui/mod.rs` | `draw`, the header, the stats strip, the times list and the status line |
| `ui/timer.rs` | The big countdown: `timer_view`, `draw_timer` and the block font |
| `ui/overlay.rs` | The three popups: help, the session picker, and one solve in full |
| `ui/layout.rs` | Pure geometry: panel heights, word wrap, popup placement. No `Frame`, no `App` |
| `scramble/mod.rs` | Dispatch on `Puzzle`, nothing else |
| `scramble/{cube,pyraminx,skewb,megaminx,square1,clock}.rs` | One puzzle family each |

No module reaches around another's API. `ui` reads `App` fields and never touches
`Instant`, the filesystem, or `stats` (statistics are cached on `App` by `refresh_derived`
because recomputing them in the 15 ms draw loop can hang on a large save file). `app` is
the only caller of `storage::save` and of `cstimer`, which opens no file of its own.
Nothing outside `storage.rs` decides where data lives. `App`'s public surface is the whole
of `app`: `crate::app::App` keeps every path it had before the directory split, and
`main.rs` and `ui` are unaware there is more than one file behind it.

Files stay small: past roughly 500 lines of non-test code, split along responsibility lines
rather than appending. Nothing in `src/` is over the line, but `app/mod.rs` sits at 497
non-test lines with nothing to spare, so the next thing added to the state machine needs a
cut first, not after. **The seam waiting there is the inspection cluster**: the five
`INSPECTION_*` constants with `start_inspection`, `cancel_inspection`, `refresh_inspection`
and `on_key_inspecting` behind them, which is the one part of the timer that has its own
vocabulary. Below it are `ui/mod.rs` at 450, `app/commands.rs` at 431, `cstimer.rs` at 372
and `storage.rs` at 345, none of which has an obvious seam left, so treat growth past
roughly 500 in any of them as the prompt to look for one. `app/progress.rs` is the most
recent cut and it shows the shape to aim for, as `app/selection.rs` and `ui/timer.rs` did
before it: the parent keeps one entry point per cluster (`note_pb`, `expire_pb_banner`,
`on_key_times`, `draw_timer`) and the child keeps every constant and helper behind it.

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
3. The docs match the code: `README.md` for anything a user can see, `claude-docs/*` for
   anything a contributor needs to know.
