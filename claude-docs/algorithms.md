# Cubetimer algorithms

This document covers the maths Cubetimer implements: WCA trimmed averages, untrimmed means,
rolling bests and personal bests (`src/stats.rs`), scramble generation (`src/scramble/`),
the inspection penalty thresholds and judge calls (`src/app/mod.rs`), and the csTimer
interchange format (`src/cstimer.rs`). Every claim here describes the code as it stands,
including the places where Cubetimer approximates the official rules rather than matching
them.

For how these pieces fit into the program, see [architecture.md](architecture.md).
Project-level guidance is in [../CLAUDE.md](../CLAUDE.md); code style rules are in
[code-style.md](code-style.md) and the testing policy is in [testing.md](testing.md).

---

## Times, penalties and effective values

Every solve stores its **raw** duration in milliseconds. Penalties are applied on read,
never baked into the stored value, so `/ok` can undo a `/+2` losslessly:

```rust
// src/types.rs
pub fn effective_millis(&self) -> Option<u64> {
    match self.penalty {
        Penalty::None => Some(self.millis),
        Penalty::Plus2 => Some(self.millis + 2000),
        Penalty::Dnf => None,
    }
}
```

`None` means DNF, and that `Option` is the single representation of a DNF everywhere in
`stats.rs`. Nothing downstream needs to know about `Penalty` again.

Display truncates rather than rounds, which is what the WCA specifies for results
recorded in hundredths. `types::format_millis` divides by 10 to get centiseconds and then
splits out minutes, so 12,349 ms displays as `12.34`, not `12.35`. Truncation happens
twice in an average: once when the integer mean is computed in milliseconds, and again
when that mean is formatted. Both directions are downward, so a displayed average is
never optimistic.

---

## WCA trimmed averages

### The trim count

```rust
fn trim_count(n: usize) -> usize {
    n.div_ceil(20)
}
```

Trim is `ceil(n / 20)`, which is 5 percent of the window rounded up, taken from *each*
end. That gives 1 for everything up to ao20, 2 for ao21 through ao40, 3 for ao41 through
ao60, 5 for ao100 and 50 for ao1000. The four trimmed averages Cubetimer displays are ao5
and ao12 (trim 1 each end, matching WCA Regulation 9f exactly), ao100 (trim 5 each end, the
standard practice-timer convention) and ao1000 (trim 50 each end, the same convention
carried up). The formula is general, so `average_of` works for any window size and none of
the four is special-cased.

### Ordering with DNFs

A DNF has no time, but it still has to sort somewhere. It sorts as the worst possible
result:

```rust
fn cmp_effective(a: &Option<u64>, b: &Option<u64>) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => x.cmp(y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}
```

This is the key to the whole algorithm. Because DNFs sort last, a single DNF in an ao5 is
automatically the value that gets trimmed off the slow end, exactly as the WCA intends,
without any special-casing.

### The window computation

`average_window` is the core, and it runs in four steps:

1. **Reject degenerate windows.** If `n == 0`, or `n <= 2 * trim` so that trimming would
   leave nothing behind, the result is `AvgResult::NotEnough`. In practice this only bites
   for `n <= 2`; an ao3 trims one from each end and averages the single survivor.

2. **Count DNFs.** If there are strictly more DNFs than the trim count, the trim cannot
   absorb them all, at least one DNF would have to enter the mean, and there is no
   sensible number to report. The result is `AvgResult::Dnf`. For an ao5 or ao12 that
   means one DNF is survivable and two are not; for an ao100 it takes six, and for an
   ao1000 it takes fifty-one.

3. **Sort and trim.** Sort the effective times with `cmp_effective` and keep the middle
   slice `times[trim .. n - trim]`. Because step 2 guarantees at most `trim` DNFs and
   DNFs sort last, every DNF is inside the trimmed tail. The `unwrap_or(0)` in the sum is
   therefore unreachable, and is written that way rather than as an `unwrap` so a future
   change to the guard degrades into a wrong number instead of a panic.

4. **Mean.** Accumulate into a `u128` and integer-divide by the kept count:

   ```rust
   let sum: u128 = kept.iter().map(|t| t.unwrap_or(0) as u128).sum();
   AvgResult::Time((sum / kept.len() as u128) as u64)
   ```

   The `u128` accumulator is what makes the sum provably overflow-free. Summing 90 `u64`
   values overflows `u64` in the general case, and while no real solve time comes close,
   solve times can be hand-edited in the JSON file. Widening to `u128` removes the
   question entirely at no measurable cost, since these windows are at most a few hundred
   elements. The final cast back to `u64` is safe because the mean of a set of `u64`
   values cannot exceed the largest of them.

The outcome is a three-state enum rather than an `Option`, because "not enough solves yet"
and "too many DNFs" are different facts and the UI shows them differently:

```rust
pub enum AvgResult { Time(u64), Dnf, NotEnough }
```

`AvgResult::display()` renders them as `"12.34"`, `"DNF"` and `"-"`.

### Which solves count

`average_of(n, solves)` takes the **last** `n` solves, the tail of the slice:

```rust
pub fn average_of(n: usize, solves: &[Solve]) -> AvgResult {
    if n == 0 || solves.len() < n {
        return AvgResult::NotEnough;
    }
    average_window(&solves[solves.len() - n..])
}
```

Sessions store solves in chronological order, so the tail is the most recent window. Fewer
than `n` solves is `NotEnough`; there is no partial average.

### Worked example

Five solves, in order: `10.000`, `11.000`, `DNF (9.000 raw)`, `12.000`, `13.000`.

- Effective values: `Some(10000)`, `Some(11000)`, `None`, `Some(12000)`, `Some(13000)`.
- `trim_count(5)` is 1. One DNF, which is not more than 1, so the average survives.
- Sorted with DNF last: `10000, 11000, 12000, 13000, None`.
- Trim one from each end, keeping `11000, 12000, 13000`.
- Mean: 36000 / 3 = 12000, displayed as `12.00`.

Add a second DNF anywhere in that window and the result becomes `AvgResult::Dnf`.

---

## The untrimmed mean: mo3

An moN is not a small aoN. It is the plain arithmetic mean of the window with nothing
trimmed, which is how the WCA scores 6x6, 7x7 and the blindfolded events, and Cubetimer
shows the mo3 of your last three solves beside the trimmed averages. The implementation is
a separate function precisely so the difference is impossible to blur:

```rust
fn mean_window(window: &[Solve]) -> AvgResult {
    if window.is_empty() {
        return AvgResult::NotEnough;
    }
    let mut sum: u128 = 0;
    for solve in window {
        match solve.effective_millis() {
            Some(ms) => sum += ms as u128,
            None => return AvgResult::Dnf,
        }
    }
    AvgResult::Time((sum / window.len() as u128) as u64)
}
```

The whole distinction is in that `None` arm. There is no trim, so there is no discarded
slot for a DNF to fall into and no `cmp_effective` ordering to exploit: **one DNF anywhere
in the window makes the entire mean a DNF**, and the function returns the moment it sees
one. Compare the ao5, where the single DNF of the worked example above is trimmed away and
the average survives. Same three solves with a DNF among them: the ao5 reports a time, the
mo3 reports `DNF`. Both are correct, because they are answering different questions.

Two things are shared with the trimmed path rather than reimplemented. Penalties still
arrive through `effective_millis`, so a `+2` in an mo3 costs its two seconds. The
accumulator is still `u128` for the same overflow reason, and the mean is still an integer
division, so it truncates downward like everything else.

`mean_of_last(n, solves)` is the tail wrapper, mirroring `average_of`: fewer than `n`
solves is `NotEnough`, never a partial mean. `best_mean_of(n, solves)` is the rolling
counterpart of `best_average_of` and skips DNF windows rather than scoring them as
infinitely slow, exactly as the trimmed version does.

The three-wide window is what makes any of this cheap. `n` of 3 means `mean_window` is a
three-element loop with no sort at all, so mo3 is the one statistic on the strip whose cost
never depends on the size of the average.

---

## Rolling bests and personal bests

`best_average_of(n, solves)` slides an `n`-wide window across the whole session and keeps
the smallest valid average:

```rust
for window in solves.windows(n) {
    if let AvgResult::Time(t) = average_window(window) {
        best = Some(match best { Some(b) if b <= t => b, _ => t });
    }
}
```

Windows that come back `Dnf` are skipped rather than treated as infinitely slow, so a
session that is all DNFs except for one clean stretch reports that stretch's average
instead of nothing. If no window ever produced a time, the answer is `None`.

Note that this calls `average_window` directly rather than `average_of`, so each window is
averaged on its own terms; the "last n solves" tail logic does not apply. Complexity is
`O((L - n + 1) · n log n)` for `L` solves, dominated by re-sorting each window, and
`personal_bests` pays it once per session of the puzzle.

That cost is why the result is cached on `App` rather than computed in the renderer.
An earlier version called `personal_bests` from `draw`, which put an unbounded walk over
every solve the user has ever done inside a loop that runs every 15 ms; a large enough save
file made the frame budget the binding constraint and the app unresponsive. `App::stats`
and `App::pbs` now hold the answers and `App::refresh_derived` recomputes them at the
handful of moments a number can change, which is at most once per solve. See
[architecture.md](architecture.md) for where those calls sit. If a session ever grows large
enough that even that is slow, the fix is memoisation keyed on solve count, not a cleverer
sort.

`session_stats` bundles what the stats strip needs, all in one pass over the effective
times: `count` (including DNFs), `valid_count` (excluding them), `best` and `worst` as the
min and max of the non-DNF effective times, a plain truncated `mean` of the non-DNF times,
`mo3` from `mean_of_last`, and `ao5` / `ao12` / `ao100` / `ao1000` from `average_of`. A DNF
can never be the `worst` single, because it is filtered out before the max is taken. Note
that `mean` and `mo3` are different things despite both being untrimmed: `mean` covers the
whole session and simply skips DNFs, while `mo3` covers the last three solves and is
poisoned by one.

`personal_bests` computes all-time bests across a set of sessions:

```rust
for session in sessions {
    keep_min(&mut pb.single, session.solves.iter().filter_map(|s| s.effective_millis()).min());
    keep_min(&mut pb.mo3,    best_mean_of(3, &session.solves));
    keep_min(&mut pb.ao5,    best_average_of(5, &session.solves));
    keep_min(&mut pb.ao12,   best_average_of(12, &session.solves));
    keep_min(&mut pb.ao100,  best_average_of(100, &session.solves));
    keep_min(&mut pb.ao1000, best_average_of(1000, &session.solves));
}
```

`pb.mo3` is the one entry that goes through `best_mean_of` rather than `best_average_of`,
so a personal best mo3 is a stretch of three clean solves and can never contain a DNF. The
ao1000 entry is the expensive one, which is why `best_average_of` returns early when the
session is shorter than the window: most sessions never reach a thousand solves and pay
nothing for the row being on screen.

Two properties are worth stating explicitly. **Rolling windows never span a session
boundary**, because `best_average_of` is called once per session; five solves spread across
two sessions do not form an ao5. **Filtering by puzzle is the caller's job.** `stats.rs`
takes whatever slice it is handed. `App::refresh_derived` supplies only the sessions
matching the active session's puzzle, which is what makes the cached PBs event-specific.
Combined with the puzzle-retype rule in [architecture.md](architecture.md), which prevents
a session that already has solves from changing puzzle, this guarantees no personal best
ever mixes events.

### What the trend plots

`app::progress::trend_of` is the one derived series that is not a statistic. It takes the
last 50 solves of the active session and keeps their `effective_millis()`, so what the
sparkline draws is the time each solve actually cost: a `+2` plots two seconds higher than
the stopwatch said, and a DNF, having no effective time, is not plotted at all. Nothing is
substituted for it, so the bars are shorter than the window whenever a DNF is in range and
a run of them simply leaves fewer bars. The series is oldest first and unsmoothed, and the
bar height is a time rather than a score, which makes a dip a fast solve.

---

## Scramble generation

`scramble::generate(puzzle)` returns a scramble in WCA notation. `generate_with_rng(puzzle,
rng)` is the same function with an injectable generator, which is how the tests get
reproducible scrambles from `StdRng::seed_from_u64`. `mod.rs` does nothing but match on
`Puzzle` and hand off; each family lives in its own file, because the events share almost
no notation and almost no structure. Every generator is a `fn scramble<R: Rng>(rng: &mut R)
-> String` and every one is total, meaning no retry loops and no path that can fail.

Twelve events, eleven generators. `Puzzle::Oh` is 3x3 with one hand behind your back, so
the dispatch maps it onto the 3x3 generator verbatim:

```rust
Puzzle::Oh => cube::scramble(Puzzle::Cube3, rng),
```

That is the whole of one-handed as far as scrambling is concerned, and mapping it here
rather than inside `cube.rs` is also what keeps `cube::scramble` a function of cube
variants only. The event is separate everywhere it matters, meaning its own session and its
own personal bests, and identical everywhere it does not. A seeded test asserts the two
generators agree scramble for scramble on the same seed, so the identity cannot quietly
drift.

Two things are true across all of them. The output is a single string, space separated,
with no leading or trailing whitespace; Megaminx is the only one that contains a newline.
And every scramble is a fixed length for its event, so nothing about the move count is
randomised.

### How close each event is to official

TNoodle, the official WCA scramble program, uses two methods. **Random-state** generation
samples a position uniformly from all reachable states and runs a solver backwards to
produce moves for it. **Random-move** generation samples a sequence and takes whatever
state falls out. Where TNoodle uses random-move, matching its emission rules is enough to
be equivalent. Where it uses random-state, a random-move generator is an approximation, and
saying otherwise would be dishonest.

| Event | TNoodle | Cubetimer | Verdict |
| --- | --- | --- | --- |
| Clock | random-state | random-move | **Exact.** Clock's moves commute, so uniform amounts already give a uniform state |
| Megaminx | random-move | random-move | **Emission-identical.** Same distribution, only the PRNG differs |
| 5x5, 6x6, 7x7 | random-move | random-move | **Equivalent.** Same pools, lengths and legality rule |
| 2x2, 3x3, 4x4 | random-state | random-move | Approximation |
| One-handed | random-state (as 3x3) | random-move (as 3x3) | Approximation, identical to 3x3 |
| Pyraminx, Skewb | random-state | random-move | Approximation |
| Square-1 | random-state | random-move, shape-aware | Approximation, close |

The three approximation rows are covered in detail at the end of their own sections. The
general shape of the gap is the same in each case: the state distribution is not uniform,
because some positions are reachable by many more legal sequences than others, so a
random-move scramble can occasionally land somewhere far easier than typical. These are
good practice scrambles and they are what most cubers train on. They are not
competition-legal, and closing the gap would mean shipping a solver and pruning tables per
puzzle, which is a large amount of machinery for a terminal timer. The trade-off is
deliberate.

---

## Cubes: 2x2 through 7x7

`src/scramble/cube.rs`.

### The move model

A move is a **move type** plus a **suffix**. A move type is a face and a layer width:

```rust
struct MoveType {
    name: &'static str,  // "R", "Rw", "3Rw"
    face: u8,            // b'U' b'D' b'L' b'R' b'F' b'B'
    width: u8,           // 1 = outer, 2 = w, 3 = 3..w
}
```

Two derived notions drive every constraint. `axis()` maps a face to `0` for U/D, `1` for
L/R and `2` for F/B, since turns on the same axis commute and are interchangeable in
ordering. `same_layer()` is true when two move types share both face *and* width, so `R`
and `Rw` are distinct move types even though they turn the same face.

The suffix is drawn uniformly from `["", "'", "2"]`, independently of everything else. The
suffix never affects legality, which is why the constraint logic only ever deals in move
types.

### Pools and lengths

| Puzzle | Move pool | Moves |
| --- | --- | --- |
| 2x2 | `U R F` | 11 |
| 3x3 | `U D L R F B` | 20 |
| 4x4 | `U D L R F B` + `Uw Rw Fw` | 44 |
| 5x5 | `U D L R F B` + `Uw Dw Lw Rw Fw Bw` | 60 |
| 6x6 | 5x5 pool + `3Uw 3Rw 3Fw` | 80 |
| 7x7 | 5x5 pool + `3Uw 3Dw 3Lw 3Rw 3Fw 3Bw` | 100 |

One principle explains every pool. A turn that moves **exactly half** the cube's layers is
the same permutation as the complementary turn on the opposite face plus a whole-cube
rotation, so keeping both would be pure redundancy. Only three of those six are included,
one per axis. Where a width is not half the cube, all six are kept:

- 2x2: `U` turns 1 of 2 layers, which is half, so the pool is `U R F` and `D L B` are
  dropped.
- 4x4: `Uw` turns 2 of 4 layers, which is half, so only `Uw Rw Fw` appear.
- 5x5: `Uw` turns 2 of 5 layers, not half, so all six wide moves appear.
- 6x6: `Uw` is 2 of 6, not half, so all six appear; `3Uw` is 3 of 6, which is half, so
  only `3Uw 3Rw 3Fw` appear.
- 7x7: neither 2 of 7 nor 3 of 7 is half, so all six of each width appear.

Every length is fixed, and each one is the count TNoodle emits for that event. Nothing about
the move count is randomised, including 2x2, which is always exactly 11 moves.

### The constraint rule

There is one rule, and it is TNoodle's: **inside a maximal run of consecutive same-axis
moves, no face and width may repeat.** `is_legal(candidate, run)` takes the trailing run and
applies it:

```rust
fn is_legal(candidate: MoveType, run: &[MoveType]) -> bool {
    match run.first() {
        Some(first) if first.axis() == candidate.axis() => {
            !run.iter().any(|m| m.same_layer(candidate))
        }
        // A move on a fresh axis starts a new block and is always legal.
        _ => true,
    }
}
```

`run` is a small buffer the generator carries alongside the output. A chosen move whose axis
differs from the run's clears the buffer before being pushed, so `run` always holds exactly
the trailing block of moves sharing one axis, and the first move of a scramble sees an empty
run and is unconstrained.

The rule is stated over the whole run rather than over the previous move, and that is the
point. Same-axis turns commute, so a run can be reordered freely; if any face and width
appeared twice in one run, the two occurrences could be brought together and merged, and the
scramble would be shorter than advertised. Checking the whole run catches `R L R` as well as
`R R'`, because both put `R` twice in the same block. Different widths on one face are
distinct move types, so both `R Rw` and `Uw U 3Uw` are legal.

What this rule deliberately does *not* forbid is a long same-axis run. `Uw U 3Uw` is three
consecutive moves on the U/D axis, all on distinct layers, nothing cancels, and real WCA
scrambles contain sequences exactly like it. An earlier version of this generator capped
same-axis runs at two, which is stricter than TNoodle and skewed the distribution on the big
cubes; `big_cubes_do_produce_runs_of_three_on_one_axis` in `cube.rs` now pins the
looser behavior down.

### Filtered sampling, not rejection sampling

Each position filters the pool down to the legal continuations and then picks uniformly
from what remains:

```rust
candidates.clear();
candidates.extend(pool.iter().copied().filter(|m| is_legal(*m, &run)));
let chosen = candidates[rng.gen_range(0..candidates.len())];
let suffix = SUFFIXES[rng.gen_range(0..SUFFIXES.len())];
```

This yields the same distribution as drawing from the full pool and retrying on an illegal
draw, since both are uniform over the legal set, but it does so in a fixed `O(pool)` per
move with no retry loop and no worst-case blowup. The `candidates` buffer is allocated once
outside the loop and reused.

The candidate list can never be empty. Every pool spans at least two axes, and the rule only
ever excludes move types that share the current run's axis, so every move type on any other
axis always survives the filter. The smallest pool, 2x2's `U R F`, puts its three move types
on three different axes, so at most one is ever excluded. The code carries a
`debug_assert!(!candidates.is_empty())` to catch a future pool that breaks this, plus a
`break` so a release build truncates the scramble rather than panicking.

### How close the cubes are to official

**5x5, 6x6 and 7x7 are equivalent to official output.** TNoodle generates the big cubes as
random-move sequences, and Cubetimer matches it on all three inputs that define the
generator: the same move pools, the same lengths of 60, 80 and 100, and the same legality
rule. A scramble from Cubetimer is drawn from the same distribution as a scramble from
TNoodle for these events. This is what the audit against TNoodle's source changed, and it is
why the constraint rule was relaxed to the same-axis-run form.

**2x2, 3x3 and 4x4 are approximations**, because TNoodle generates them from a random
state. The lengths of 11, 20 and 44 are matched to what real TNoodle scrambles for these
events look like, but matching the length does not make the method the same. The bias is
mild on 4x4 and most visible on 2x2, where the state space is tiny. One point that is easy
to miss: Cubetimer's 11 moves on 2x2 and 20 on 3x3 are sequence lengths, not the optimal
solution depth a random-state scramble is measured by.

---

## Pyraminx

`src/scramble/pyraminx.rs`. Approximation of a random-state event.

A Pyraminx scramble is eleven layer turns followed by up to four tip turns.

**Layer turns.** Exactly eleven, drawn from `U L R B` with an optional `'`. There is no `2`
suffix anywhere in Pyraminx notation, because a Pyraminx layer turns 120 degrees and two of
those is the inverse of one. The only constraint is that **no layer turns twice in a row**.
That is weaker than a cube's rule, and deliberately so: all four Pyraminx axes intersect, so
there are no parallel layers the way `L` is parallel to `R`, and the only redundancy to
prune is the immediate repeat, which would collapse into a single turn and make the
scramble shorter than eleven moves. The successor is picked by offsetting rather than
retrying, `(prev + rng.gen_range(1..4)) % 4`, which keeps the three legal successors equally
likely in one draw.

**Tips.** The four tips are emitted after every layer turn, in the fixed order `u l r b`,
each appearing at most once. A tip is a corner trivially rotatable in isolation, so a
uniformly random state leaves each one solved exactly one time in three, and a solved tip
contributes no move. The generator draws `0..3` per tip and skips on 0, giving 0 to 4 tip
moves per scramble with the same distribution the official generator produces.

This is the shape TNoodle emits: it solves a uniformly random state in exactly eleven layer
turns, then appends one move per unsolved tip. Cubetimer reproduces the *shape* and the tip
distribution, but samples the eleven layer turns instead of solving for them, so the layer
state is not uniform.

---

## Skewb

`src/scramble/skewb.rs`. Approximation of a random-state event.

Exactly eleven moves drawn from `R U L B` with an optional `'`, and no `2`, since a Skewb
turn is also 120 degrees. Those four are the fixed-corner scheme: Skewb scrambling holds one
corner still, the same trick 2x2 uses to drop from six faces to three, so four of the eight
corner axes reach every state and the other four never appear in notation.

**The only constraint is that no axis repeats consecutively**, and there is nothing stronger
to add. On a cube, `R` and `L` are parallel and commute, which is what makes the same-axis
run rule necessary. No two Skewb axes are parallel, so no two moves commute, and the only
sequence that collapses is the immediate repeat. Same successor arithmetic as Pyraminx: draw
from the three survivors and step over the excluded index, so no successor is starved. A
test asserts every ordered pair of distinct axes actually occurs, which guards both against
the constraint drifting stricter than TNoodle's and against that arithmetic biasing a
successor.

TNoodle generates Skewb from a random state, rejecting anything solvable in fewer than
seven moves and padding the result out to eleven. Cubetimer matches the length and the
notation, not the state distribution, and has no lower bound on solution depth.

---

## Megaminx

`src/scramble/megaminx.rs`. Emission-identical to the official generator.

Megaminx uses Pochmann notation and is laid out as seven lines. Each line is ten moves
alternating `R` and `D`, starting on `R` and ending on `D`, each carrying `++` or `--` from
an independent coin flip, and the line is closed by a single `U`:

```
R++ D-- R-- D++ R++ D++ R-- D-- R++ D++ U
```

The closing `U` is **not an independent draw**. TNoodle reuses the direction of the line's
last `D`, so a line ending `D++` closes with `U` and one ending `D--` closes with `U'`. The
implementation gets this for free by letting the `clockwise` flag outlive the inner loop and
reading it once more after it. Getting this wrong is the easiest way to produce Megaminx
output that looks right and is not, so it is worth stating plainly: 70 moves means 70 random
bits per scramble, not 77.

Megaminx is a random-move event officially, so there is no state distribution to
approximate. Reproducing the emission rules is the whole of what "matches the WCA scrambler"
means here, and Cubetimer's output is drawn from the same distribution as TNoodle's. Only
the PRNG behind it differs.

This is also the generator that drove the adaptive header: seven lines joined with `\n` is
the one scramble that does not fit a fixed-height panel. See
[architecture.md](architecture.md).

---

## Square-1

`src/scramble/square1.rs`. Approximation, but a shape-aware one.

Square-1 notation alternates twist groups and slashes, with a space on **both** sides of
every slash:

```
(1,0) / (-3,0) / (0,3) / ... / (6,-2)
```

Cubetimer emits a fixed 12 slashes and therefore 13 twist groups, ending on a twist. Each
twist amount is in `-5..=6`, which covers all twelve rotations of a layer exactly once, and
`(0,0)` is excluded because it is a no-op.

### Why the generator simulates the puzzle

A twist is never blocked. Each layer of a Square-1 spins freely, so any `(top,bottom)` is
always physical. A **slash** is different: it swaps the two front halves, which is only
possible when no corner straddles the slice plane. A generator that picked twists blindly
would emit sequences that cannot be turned.

So the generator carries a shape simulator, TNoodle's 24-slot piece model:

```rust
struct Shape { slots: [u8; 24] }
```

Slots 0 through 11 are the top layer, 12 through 23 the bottom, each slot a half-hour of the
dial. A corner fills two adjacent slots with the same piece id and an edge fills one, so two
neighbouring slots hold the same id exactly when a corner spans them. The slice plane cuts
each layer between slots 11 and 0 and between slots 5 and 6, which makes the legality test
four comparisons:

```rust
fn can_slash(self) -> bool {
    self.slots[0] != self.slots[11]
        && self.slots[5] != self.slots[6]
        && self.slots[12] != self.slots[23]
        && self.slots[17] != self.slots[18]
}
```

`twisted` rotates each layer's slots and `slashed` swaps the two front half-layers. With
those three operations, each step of the generator enumerates all 143 twists, keeps the ones
whose resulting shape satisfies `can_slash`, and draws uniformly from that set. This is
filtered sampling, the same technique the cube generator uses: uniform over the legal set,
in fixed work per step, with no retry loop. The final twist is unconstrained, because
nothing follows it. Every shape can be rotated into a slashable position, so the candidate
set is never empty; a `debug_assert` catches a future change that breaks that and a `break`
keeps a release build from panicking.

### What this buys and what it does not

Filtering to slashable twists has a second, non-obvious effect: it makes the twist amounts
**deliberately non-uniform**. Amounts of 0, ±3 and 6 dominate the output, because those are
the rotations most likely to leave the slice plane clear. That is not a defect to be
corrected. Real WCA Square-1 scrambles show the same skew, for the same reason, and a
generator that forced uniform amounts would produce output visibly unlike the real thing.

The gap to official is narrower here than for the other approximations. TNoodle generates
Square-1 from a random state and emits a variable 9 to 13 slashes; Cubetimer's 12 is
TNoodle's median. Every scramble Cubetimer emits is turnable and reaches a genuinely
scrambled shape, so the resulting states are close to uniform without being exactly
uniform.

---

## Clock

`src/scramble/clock.rs`. **Exactly official.**

A Clock scramble is exactly fifteen space-separated tokens, in exactly this order:

```
UR DR DL UL U R D L ALL y2 U R D L ALL
```

Nine front dials, the `y2` flip, then five back dials. Each dial carries an amount drawn
uniformly from twelve values, rendered `0+` through `6+` clockwise and `1-` through `5-`
anticlockwise. Note the asymmetry, which is correct and not a bug: TNoodle draws
`nextInt(12) - 5`, giving `-5..=6`, and calls every non-negative amount clockwise. So `0-`
never appears in an official scramble, and `6+` has no negative twin.

There are **no pin tokens**. The WCA removed pin states from official Clock scrambles on
1 January 2024, so a scramble ending in something like `UUdd` is pre-2024 output and
Cubetimer does not produce it.

Clock is the one event where a random-move generator is not an approximation. Its dial
turns commute: turning `UR` by 3 and then `DL` by 2 reaches the same state as doing them in
the other order, and every reachable state corresponds to exactly one assignment of amounts
to dials. Drawing each amount independently and uniformly therefore samples the state space
uniformly, which is precisely what random-state generation means. Cubetimer's Clock
scrambles are official scrambles, not an equivalent of them.

---

## Inspection penalties

WCA inspection is 15 seconds. Overrunning it costs 2 seconds; overrunning it badly is a
DNF. `App::refresh_inspection` evaluates both on every tick while `Inspecting`, and also
while `Armed { from_inspection: true }`, so the penalty keeps accruing right up to the
moment the timer starts:

```rust
const INSPECTION_SECS: i64 = 15;
const INSPECTION_PLUS2_MS: u128 = 15_000;
const INSPECTION_DNF_MS: u128 = 17_000;

let elapsed = started.elapsed().as_millis();
self.inspection_remaining = Some(INSPECTION_SECS - (elapsed / 1000) as i64);
self.pending_inspection_penalty = if elapsed > INSPECTION_DNF_MS {
    Penalty::Dnf
} else if elapsed > INSPECTION_PLUS2_MS {
    Penalty::Plus2
} else {
    Penalty::None
};
```

The comparisons are strict, so the boundaries are:

| Elapsed inspection | Penalty |
| --- | --- |
| 0 to 15,000 ms inclusive | `Penalty::None` |
| 15,001 to 17,000 ms inclusive | `Penalty::Plus2` |
| 17,001 ms and beyond | `Penalty::Dnf` |

Two design points follow from where this lives. First, the penalty is *pending*: it sits in
`app.pending_inspection_penalty` and is only committed when `finish_solve` copies it onto
the `Solve`. Cancelling inspection with `Esc` throws it away. Second, because the DNF branch
is checked first, an inspection that runs past 17 seconds is a DNF and never a `+2`, no
matter how long the cuber then stares at the cube.

`inspection_remaining` is a separate, purely cosmetic value: `15 - floor(elapsed / 1000)`,
counting down 15, 14, 13 and then going negative. `ui::timer::timer_view` maps it to the on-screen
warning, showing `+2` in red once `remaining <= 0` and `DNF` once `remaining <= -2`. That
second-granularity mapping is an approximation of the authoritative millisecond rule above
and differs from it in a one-millisecond sliver at each boundary: at exactly 15,000 ms the
display already says `+2` while the recorded penalty is still `None`. The number written to
disk always comes from `refresh_inspection`, never from the display.

A +2 earned during inspection is stored as `Penalty::Plus2` on the solve, so it flows
through `effective_millis` like any other +2: two seconds added when the solve is read, and
`format_solve` renders it as `14.02+` with the penalty already included in the number.

### The 8 and 12 second judge calls

A WCA judge calls "8 seconds" and "12 seconds" while a competitor inspects, and those two
calls are half of how anyone paces the last third of an inspection. `refresh_inspection`
reproduces them from the same `elapsed` it uses for the penalties:

```rust
const INSPECTION_CALL_8_MS: u128 = 8_000;
const INSPECTION_CALL_12_MS: u128 = 12_000;

// The judge calls are silent: `ui` reads the stage for the countdown's colour and caption.
self.inspection_stage = if elapsed >= INSPECTION_CALL_12_MS {
    2
} else if elapsed >= INSPECTION_CALL_8_MS {
    1
} else {
    0
};
```

Two properties are worth naming.

**The comparisons are inclusive**, unlike the strictly-greater penalty thresholds above. A
call announces that a mark has been reached, so it belongs at 8,000 ms exactly; a penalty
punishes overrunning one, so it belongs strictly after. The asymmetry is deliberate.

**The stage is assigned, not accumulated.** It is a function of `elapsed` and nothing else,
so a tick recomputes it rather than advancing it, and there is no bookkeeping about which
calls have already happened. That is only sound because the calls are silent: the stage
feeds a colour and a caption, both of which are redrawn every frame anyway, so re-deriving
the same value 66 times a second costs nothing and cannot get out of step with the clock.
`start_inspection`, `cancel_inspection`, `start_timing` and `finish_solve` set it back to 0,
which is what keeps the calls scoped to the inspection that earned them.

`ui::timer::timer_view` matches on `inspection_stage` once, and that single match yields
both the countdown colour, yellow at 0, light magenta at 1, light red at 2, and the caption
in the small line under the digits, nothing at 0, `8s` at 1 and `12s` at 2. Deriving both
from one value is what stops the colour and the text from disagreeing about which call is
current.

The penalty display still wins over the stage. Once `inspection_remaining` reaches 0 or
below, `timer_view` paints the countdown plain red and puts `+2` or `DNF` in the caption
slot instead of the call, so an earned penalty is never mistaken for a warning that has cost
nothing yet.

---

## The csTimer interchange format

`src/cstimer.rs` converts between `SaveFile` and the JSON csTimer exports, in both
directions and with no file handling of its own. The shape below was read off csTimer's
source and checked against a file csTimer wrote, because the two places it is
counter-intuitive, the penalty encoding and the string-inside-string nesting, are exactly
the places a guess would be wrong.

**The export is named `cstimer_YYYYMMDD_HHMMSS.txt` and the extension is not cosmetic.**
csTimer's import is `<input type="file" accept="text/*">`, and Windows calls a `.json` file
`application/json`, so a `.json` export is one the picker refuses to show. `default_file_name`
builds the name from `types::format_timestamp`, which is UTC and prints no seconds, so it
reduces that to its digits and takes the seconds off the clock itself; a clock far enough
off to print a year outside four digits falls back to `cstimer_export.txt` rather than a
malformed name.

The top level is an object holding one `session<n>` key per session, numbered from 1, beside
a `properties` object:

```json
{
  "session1": [ [[0, 12340], "R U R' U'", "", 1700000000] ],
  "properties": {
    "sessionN": 1,
    "session": 1,
    "sessionData": "{\"1\":{\"name\":\"main\",\"opt\":{\"scrType\":\"333\"},\"rank\":1,\"stat\":[1,0,12340],\"date\":[1700000000,1700000000]}}"
  }
}
```

`properties` is csTimer's whole settings object and an import replaces it wholesale, which
is why an export of ours puts csTimer's own preferences back to their defaults: only the
three session keys above are written, and inventing values for the rest would be worse than
leaving them out. `sessionN` is the one that has to be right, because csTimer's importer
reads `session1` through `sessionN` and nothing past it. A file csTimer wrote can carry
neither `sessionN` nor `session`, because it omits any property still holding its default,
so their absence is not a sign they are optional for us.

**One solve is a four-element tuple**, `[[penalty, millis], scramble, comment, timestamp]`:

| Slot | Holds |
| --- | --- |
| `penalty` | 0 clean, 2000 for a `+2`, -1 for a DNF |
| `millis` | The **raw** time, with no penalty folded into it |
| `scramble` | The scramble string, or an empty one |
| `comment` | A per-solve note csTimer supports and Cubetimer does not; exported empty, ignored on import |
| `timestamp` | Unix time in **seconds**, not milliseconds |

The penalty column is the subtlety. The 2000 of a `+2` is a marker rather than an addend:
csTimer stores the raw time beside it and adds the two seconds when it displays the result,
which is exactly what `Solve::effective_millis` does, so `export_solve` writes `solve.millis`
untouched and `import_solve` reads it back untouched. Folding the 2000 in would double it on
the next read. A DNF keeps the time it would have been under a `-1`, so nothing is lost by
marking a solve DNF in one program and clearing it in the other.

Two smaller rules follow from the format being another program's: any positive penalty is
legal in csTimer, so an unfamiliar one is added into the time on import and the total still
reads correctly, and a time written as a float, which converters that build these files
from text exports produce, is rounded rather than refused. Timestamps are divided by 1000
on the way out and multiplied by 1000 on the way in, so a round trip is exact to the
second and loses only the milliseconds beneath it.

**`sessionData` is a JSON-encoded string, not an object.** It maps each session index to
its `name`, its `rank` and its `opt.scrType`, and `properties` itself is sometimes stored
the same way. A session csTimer has opened also carries `stat`, which is
`[solves, DNFs, mean]`, and `date`, which is the first and last solve in Unix seconds.
Neither is load bearing, csTimer recomputes both the moment the session is opened, but its
session manager lists a session by them before anything opens it, so `session_summary`
writes both for any session with solves and neither for an empty one, exactly as csTimer
does. The mean is over `effective_millis` truncated to hundredths, which is how csTimer
averages what it displays, and it is -1 when every solve is a DNF. Both are decoded through one `as_object` helper that accepts either form, as
are the `session<n>` solve lists, so files written by older csTimer versions and by
third-party converters all parse. `scr` is accepted as a fallback for `opt.scrType`,
which is where the scramble type sat before csTimer moved it.

`scrType` is the event, and the mapping is asymmetric on purpose. Export writes the one WCA
type per event; import accepts that type and the whole-puzzle variants beside it, so a
session someone kept on a non-WCA scrambler still lands on the right event:

| Event | Exported as | Also imported from |
| --- | --- | --- |
| 3x3 | `333` | `333o`, `333noob` |
| 3x3 one-handed | `333oh` | |
| 2x2 | `222so` | `222o`, `2223`, `222nb` |
| 4x4 | `444wca` | `444m`, `444`, `444yj` |
| 5x5 | `555wca` | `555` |
| 6x6 | `666wca` | `666si`, `666p`, `666s` |
| 7x7 | `777wca` | `777si`, `777p`, `777s` |
| Pyraminx | `pyrso` | `pyro`, `pyrm`, `pyrnb` |
| Skewb | `skbso` | `skbo`, `skb`, `skbnb` |
| Megaminx | `mgmp` | `mgmc`, `mgmo`, `mgmso` |
| Square-1 | `sqrs` | `sq1h`, `sq1t` |
| Clock | `clkwca` | `clkwcab`, `clknf`, `clk`, `clko`, `clkc`, `clke` |

One-handed is `333oh` rather than `333`, which is what keeps a one-handed session from
merging into two-handed times when the file travels. A test walks `Puzzle::ALL` and asserts
every event survives its own scramble type, so a thirteenth event cannot be added without
one.

**Anything else is skipped, not guessed.** Every case trainer, every blindfolded type,
fewest moves, and every puzzle Cubetimer has no event for produce no session and increment
`Import::skipped`, which the status line reports. Filing 3x3 blindfolded solves under 3x3
would put minute-long times into an ao12 that never meant to hold them, and a session
quietly absorbed into the wrong event is harder to notice, and to undo, than one that never
arrived. A session whose solve list cannot be read at all is skipped the same way, while a
single unreadable solve inside a readable list is dropped and the rest of the session is
kept. A session naming no scramble type at all is 3x3, which is csTimer's own default.

---

## See also

- [architecture.md](architecture.md) for module layout, the timer state machine, the input
  pipeline and persistence.
- [../CLAUDE.md](../CLAUDE.md) for project-level guidance.
- [code-style.md](code-style.md) for code style rules.
- [testing.md](testing.md) for the testing policy.
