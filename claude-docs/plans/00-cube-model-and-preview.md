# Plan 00: the cube model and the scramble preview

The foundation for everything that follows (random-state scrambles, solvers): Cubetimer
learns what a cube looks like. This phase ships a facelet-level state model for the NxN
cubes, a flat-net renderer in block glyphs, and a `/preview` overlay showing the cube the
current scramble produces. Seven of the twelve events are covered (2x2 through 7x7 and
3x3 one-handed); the other five show a polite message and wait for their own models.

## Why a facelet model

A cube state is 6*n*n stickers, each carrying the color of the face it belongs to when
solved. A move is nothing more than a fixed permutation of those stickers. That is the
whole model: no group theory yet, just bookkeeping precise enough to trust. Later phases
(solvers) will build coordinate encodings on top of this; they need this layer to be
exactly right, which is why the conventions below are spelled out to the sticker.

## Module boundaries

| File | Responsibility | Owner |
|---|---|---|
| `src/cube/mod.rs` | The facelet model: state, WCA move parsing, move application. Pure, no ratatui, no IO. | Agent A |
| `src/ui/net.rs` | Pure net geometry: a `Cube` into colored block-glyph lines. No `Frame`, no `App`. | Agent B |
| `src/app/inspection.rs` | The inspection cluster cut out of `app/mod.rs` (the seam CLAUDE.md names). | Agent C |
| `app` + `ui/overlay.rs` + `ui/mod.rs` wiring, docs | The `/preview` command, the overlay, the cached cube on `App`. | Agent C |

`cube/` sits beside `scramble/`: one produces strings, the other consumes them. `ui`
still reads only `App` fields; the cube is cached on `App` when the scramble changes,
never computed in the draw loop.

## Conventions (the contract; every agent builds against these)

Face order is net reading order: `U, L, F, R, B, D`. Stickers are stored face-major in
that order, row-major within a face: index = face*n*n + row*n + col.

Orientation is "as the face appears on the unfolded cross net":

```
        [U]
[L] [F] [R] [B]
        [D]
```

- L, F, R, B: viewed straight on with U above and D below. Row 0 is the top, col 0 the left.
- U: sits above F, so U's row n-1 borders F, row 0 borders B, col 0 borders L.
- D: sits below F, so D's row 0 borders F, row n-1 borders B, col 0 borders L.

Sticker colors in the UI (the map lives in `ui/net.rs`, never in `cube/`): U white,
D yellow, F green, B blue, R red, L magenta (the terminal's stand-in for orange).

## Move semantics

Token grammar, exactly what `scramble/cube.rs` emits: optional `3` prefix (only ever with
`w`), a face letter `U D L R F B`, optional `w`, optional suffix `'` or `2`. Width is 3
for `3Xw`, 2 for `Xw`, 1 otherwise. A width of n or more is `MoveError::TooWide` (an `Rw`
on a 2x2 is a whole-cube rotation, not a move). Anything else is `MoveError::BadToken`.

A plain move is one clockwise quarter turn viewed looking at that face from outside; `'`
is three, `2` is two. Applying a turn: the face's own stickers rotate 90 degrees
clockwise once, and for each layer depth d in 0..width the four adjacent strips cycle.

Ground truth for R, derived corner by corner (depth d, any row r, s = n-1-d):

```
F(r, s) -> U(r, s) -> B(n-1-r, d) -> D(r, s) -> F(r, s)
```

The row flip on B exists because B is viewed from behind on the net. Agent A derives the
other five faces the same way (each is a 4-cycle with at most two flipped legs) and pins
every one with tests. Do not guess the flips; reason them from a physical corner as the
R derivation above was.

## What must be tested (the definition of done for each module)

`cube/`:
- Every move applied four times is the identity; `X X'` is the identity; `X2 X2` is the identity. All faces, all widths, all sizes.
- Color counts are conserved: n*n stickers of each face value after any scramble.
- Direction pins from solved 3x3: after `R`, `sticker(U, r, 2) == F` and `sticker(F, r, 2) == D` for all r; after `U`, `sticker(F, 0, c) == R` for all c. Equivalent pins for the other four faces, derived not guessed.
- Wide pins: on 4x4 after `Uw`, rows 0 and 1 of F are all R; on 7x7 after `3Rw`, cols 4..6 behave as the R cycle at depths 0..2.
- The sexy move `R U R' U'` has order 6 on a solved 3x3.
- Property: for many seeds and all six cube sizes, scramble then `invert_scramble` returns to solved.
- Integration: every string `scramble::generate` produces for the seven covered events applies with `Ok`.

`ui/net.rs`:
- Size formulas hold for n = 2..=7, both modes.
- A solved cube renders each face solid in its color, faces in net positions.
- CP437 sweep: every character in every line is space, full block, or upper half block (mirror the existing trend-chart sweep test).
- Compact mode pairs rows correctly and pads the odd final row.

`app` wiring:
- `/preview` toggles; the overlays remain mutual alternatives (opening any closes the others, both directions); Esc closes it.
- `/preview` on an unsupported event sets a status message and does not open.
- The cached cube refreshes when the scramble changes and on session or puzzle switch.
- The inspection cut is behavior-neutral: the moved tests move with it, unchanged.

End-to-end tests that need the real model and the real wiring together (for example
"after `/preview` on 3x3 the overlay buffer contains green cells") belong to the
integrator, not to Agent C, so every worktree stays green in isolation.

## Net geometry

Full mode: one text row per sticker row, each sticker two full blocks (`██`) wide, one
blank gutter row/column between faces. Width = 8n+3 cells, height = 3n+2.

Compact mode: the upper half block (`▀`) carries two sticker rows per text row,
foreground the upper row, background the lower. Odd n leaves the final row's lower half
at the default background. Width = 8n+3, height = 3*ceil(n/2)+2. The overlay uses full
mode when it fits the frame and falls back to compact.

All three glyphs are CP437. Nothing else is permitted in the net.

## Docs shipped with this change (Agent C, verified by the integrator)

- `docs/scrambles.md`: a preview section (what it shows, which events, the command).
- `README.md`: the `/preview` row in the commands table.
- `CLAUDE.md`: rows for `cube/`, `ui/net.rs`, `app/inspection.rs` in the module tables; the integrator refreshes the line-count paragraph.
- `claude-docs/architecture.md`: the new modules in the module map and data flow.

## Gates

`cargo test` fully green, `cargo clippy --all-targets` silent, docs matching the code.
The scaffolding's two `#[allow(dead_code)]` markers are removed by the integrator once
the wiring makes them used.
