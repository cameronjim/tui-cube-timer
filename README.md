# cubetimer

A speedcube timer for your terminal — scrambles for 2x2 through 7x7, WCA-style
inspection, ao5/ao12/ao100, PB tracking, and persistent sessions. Built in Rust
with [ratatui](https://ratatui.rs).

## Run it

```
cargo run --release
```

or grab the built binary at `target\release\cubetimer.exe` and put it on your PATH.

## Using the timer

The flow matches csTimer, minus the inspection step — inspection is **off by
default**:

1. A scramble is waiting at the top. Scramble your cube.
2. **Hold space** — the timer turns red, then **green** after 300 ms.
3. **Release** to start. Digits run in cyan.
4. Hit **any key** to stop. The solve is saved to disk immediately. You can keep
   holding the key down afterwards — it stays dead until you let go, and space is
   ignored for another 300 ms after that, so you never re-trigger by accident.

### Inspection

WCA-style 15-second inspection is off until you turn it on with `/inspect` (the
header shows `inspection: on/off`). With it on, an extra step slots in at the
front: **tap space** to start the yellow countdown, then hold-and-release as
usual to start the solve. Going over 15s costs you a **+2**, over 17s is a
**DNF** — applied to the solve automatically. `Esc` cancels a countdown.

### Keys

| Key | Action |
|---|---|
| `space` | hold-and-release to start / stop (tap to start inspection when it's on) |
| `n` | new scramble |
| `/` | command mode |
| `↑` `↓` / `j` `k` / PgUp PgDn | scroll times |
| `h` or `?` | help overlay |
| `Esc` | cancel inspection / close popups |
| `q` | quit |

### Commands

Type `/` then:

| Command | Action |
|---|---|
| `/2x2` `/3x3` `/4x4` `/5x5` `/6x6` `/7x7` | switch puzzle — an empty session is retyped in place; one with solves stays put and you jump to your latest session for that puzzle (created if there isn't one) |
| `/new [name]` | new session for the current puzzle |
| `/sessions` | list sessions |
| `/session <id>` | switch session |
| `/rename <name>` | rename current session |
| `/dnf` `/+2` `/ok` | set / clear penalty on the last solve |
| `/del` | delete the last solve |
| `/inspect` | toggle 15s inspection (off by default) |
| `/help` | help overlay |
| `/quit` | quit |

## Where your times live

All sessions are saved as pretty-printed JSON at
`%APPDATA%\cubetimer\data\sessions.json` (override with the `CUBETIMER_DATA`
env var — full file path). Saves are atomic (temp file + rename) and happen
after every solve and every mutating command, so you can close the terminal
whenever; you'll pick up exactly where you left off.

## Stats rules

- Averages are WCA trimmed means: drop the best and worst (ao5/ao12), or the
  best 5 and worst 5 (ao100), and mean the rest. A DNF counts as worst; more
  DNFs than the trim allowance makes the whole average a DNF.
- `+2` adds two seconds to the raw time. Times display truncated to
  centiseconds, WCA style.
- PBs (single, ao5, ao12, ao100) are computed across **all** sessions of the
  current puzzle.

## Scrambles

Random-move scrambles with the standard constraints (no same face twice in a
row, no three moves on one axis): 2x2 uses U/R/F (9–11 moves), 3x3 is 20 moves,
4x4 adds Uw/Rw/Fw (44), 5x5 all wide moves (60), 6x6 adds 3-wide (80), 7x7 (100).

## Build notes (Windows)

Built with the `stable-x86_64-pc-windows-gnu` toolchain (self-contained, no
Visual Studio required). `parking_lot`/`parking_lot_core` are pinned in
`Cargo.toml` because newer `parking_lot_core` needs a mingw assembler
(`as.exe`) that rustup's bundled toolchain doesn't ship — see the comment in
`Cargo.toml` before unpinning.

## Architecture (if you're here to learn TUIs)

```
src/main.rs      event loop: poll input → update state → draw   (~60 fps tick)
src/app.rs       state machine: Idle → Inspecting → Armed → Timing, commands
src/ui.rs        pure rendering of App state (ratatui widgets, block-digit font)
src/scramble.rs  random-move generators with face/axis constraints
src/stats.rs     WCA averages, PBs — pure functions, heavily unit-tested
src/storage.rs   atomic JSON persistence
src/types.rs     shared vocabulary: Puzzle, Solve, Session, SaveFile
```

The key TUI ideas: the terminal is put in **raw mode** (keys arrive as events,
nothing is echoed), the app **redraws the whole screen every frame** from a
single `App` state struct (immediate-mode rendering), and the hold-space flow
works because Windows delivers key **release** events, which most terminals
don't. `INTERFACES.md` documents the full design contract the modules were
built against.
