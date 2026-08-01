# Cubetimer

Cubetimer is a speedcube timer that runs in the terminal: random-move scrambles in WCA
notation for 2x2 through 7x7, optional 15 second inspection, ao5/ao12/ao100, personal
bests and sessions persisted as JSON. It is written in Rust on top of ratatui and
crossterm, and it is Windows-first, because the hold-and-release timer flow depends on
key release events that the Windows console delivers natively.

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

**2. Separation of concerns, one responsibility per module.** The seven modules and their
single jobs:

| Module | Responsibility |
|---|---|
| `main.rs` | Process lifecycle and the event loop: load, init terminal, poll, draw, restore, final save |
| `app.rs` | State machine: timer states, key handling, `/commands`, and the derived fields the UI reads |
| `ui.rs` | Rendering only: turns `App` fields into ratatui widgets, mutates nothing |
| `scramble.rs` | Random-move scramble generation in WCA notation |
| `stats.rs` | Pure statistics: trimmed averages, session summaries, personal bests |
| `storage.rs` | Where the save file lives, and reading and writing it atomically |
| `types.rs` | Shared vocabulary and the serde shape of the persisted file |

No module reaches around another's API. `ui.rs` reads `App` fields and never touches
`Instant` or the filesystem. `app.rs` is the only caller of `storage::save`. Nothing
outside `storage.rs` decides where data lives. Files stay small: past roughly 500 lines of
non-test code, split along responsibility lines rather than appending. `app.rs` (about 660
non-test lines) and `ui.rs` (about 555) are already at that line, so treat any further
growth there as a prompt to split.

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
`Session`, `Solve`, `Puzzle` and `Penalty` in `types.rs` is what already sits on users'
disks. Adding a field means giving it `#[serde(default)]`. Renaming, removing or retyping
one means bumping `SaveFile.version` and writing the migration in the same change.
`storage::load` deliberately refuses to parse a file it does not understand rather than
overwrite it, so a careless schema edit locks people out of their own times.

## Before you call a change done

1. `cargo test` is fully green.
2. `cargo clippy --all-targets` is silent.
3. The docs match the code: `README.md` for anything a user can see, `claude-docs/*` for
   anything a contributor needs to know.
