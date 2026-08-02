# Cubetimer

Cubetimer is a speedcube timer that lives in your terminal. It hands you a
scramble, times the solve, keeps your averages, and remembers everything the
next time you open it. If you have used csTimer, the rhythm will feel familiar:
hold the space bar, wait for green, let go, solve, hit any key to stop.

Everything else (sessions, penalties, switching puzzles) happens from a small
command line at the bottom of the screen, so your hands never have to leave the
keyboard or reach for a mouse.

## Getting started

Build it once:

```
cargo build --release
```

Then run `target\release\cubetimer.exe`, or drop that file somewhere on your
PATH and just type `cubetimer`. During development, `cargo run --release` does
both steps at once.

Cubetimer is built for Windows first, because the hold-and-release space bar
flow needs a terminal that reports key releases and the Windows console does
that natively. On other platforms it works in terminals that speak the kitty
keyboard protocol, which Cubetimer asks for at startup.

## What you are looking at

The scramble sits across the top in bold, and the border above it tells you
which puzzle you are on, which session you are in, and whether inspection is
turned on. The big block digits in the middle are the timer. Underneath them is
a strip with your averages and personal bests, and the column on the right is
your times list, newest at the top, with the best of the session in green and
the worst in red. The line along the bottom is your status line: it shows a
hint most of the time, the result of your last command after you run one, and
whatever you are typing while you are in command mode.

The timer changes colour to tell you what it wants. White means idle, red means
you are holding but not there yet, green means let go, cyan means you are
solving, and yellow is the inspection countdown.

A marker and a highlighted row in the times list show which solve you have
selected, and the panel title counts it for you: `times 12/87` means the
selected row is solve number 12 of the 87 in the session. The list numbers
solves the way it stacks them, oldest as 1 and newest at the top.

On a narrow or short terminal the times column and the stats strip drop away so
the timer itself stays readable. Widen the window and they come back.

## Your first solve

Inspection is off by default, so the flow is as short as it gets:

1. Scramble your cube using the sequence at the top.
2. Hold the space bar. The digits turn red, then green after 300 ms.
3. Let go on green and the timer starts running in cyan.
4. Press any key at all to stop it. The solve is written to disk right away.

You do not have to be careful about how you stop. Whatever key you hit stays
completely dead until you physically let go of it, so you can keep leaning on
the space bar for as long as you like without arming the next attempt. On top
of that, space is ignored for another 300 ms after the timer stops. Between the
two, a bounced key or an over-enthusiastic slap cannot start a solve you did not
mean to start. Everything else keeps working through that window, so you can
still press `n`, open the help, or run a command immediately.

Releasing space too early is harmless. If you let go before the digits turn
green, nothing starts and you are back where you were.

## Inspection

Turn on WCA inspection with `/inspect`, and the top border will start reading
`inspection: on`. Running it again turns it back off. The setting is written to
your save file, so whichever way you leave it is how Cubetimer starts next time.

With inspection on, one step slots in ahead of everything else. Tap space and a
15 second countdown starts in yellow. Inspect the cube, then hold space and
release exactly as before to start the solve. `Esc` backs out of a countdown if
you change your mind.

A judge at a competition calls out "8 seconds" and "12 seconds" while you
inspect, and Cubetimer marks both silently. At 8 seconds the countdown turns
light magenta and `8s` appears in the small line under the digits; at 12 seconds
it turns light red and that line reads `12s`. The colour is the part you catch
without looking away from the cube, and the caption is there when you do look.

The penalties are applied for you, so there is nothing to remember afterwards.
Going past 15 seconds turns the solve into a +2, and going past 17 seconds makes
it a DNF. Either way the countdown turns red and the same small line switches
from the judge call to `+2` or `DNF`, so a penalty you have already earned is
never mistaken for a call, and the solve is recorded with that penalty already
attached.

## Looking back at a solve

The arrow keys move a selection through the times list, `PgUp` and `PgDn` jump
ten at a time, and `Home` takes you straight back to your newest solve. Press
`Enter` on the one you want and a popup opens with the solve in full: the time
with its penalty, the date it was set in UTC, and the whole scramble it was done
on, wrapped to fit and tall enough for all seven of Megaminx's lines.

Inside that popup, `r` loads its scramble back as your current one so you can
have another go at exactly the same case, and `Esc` or `Enter` closes it. The
popup opens from an idle timer only, so `Enter` can never interrupt a solve.

## Hiding the running time

Some people solve faster when they cannot see the clock racing. `/hidetime`
replaces the running digits with `...` while you solve, and the final time
appears as usual the moment you stop. Run it again to bring the running clock
back. Like inspection, the setting is remembered between runs.

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
| `/2x2` `/3x3` `/4x4` `/5x5` `/6x6` `/7x7` | switch to that puzzle's default session (see below) |
| `/pyraminx` `/skewb` `/megaminx` `/sq1` `/clock` | the same, for the five non-cube events |
| `/oh` | the same, for 3x3 one-handed |
| `/new [name]` | start a new session for the current puzzle |
| `/sessions` | open a picker listing every session with its id, puzzle and solve count |
| `/session <id>` | switch to a session by id |
| `/rename <name>` | rename the session you are in |
| `/delsession [id]` | delete a session and its solves, current one by default |
| `/dnf` `/+2` `/ok` | set or clear the penalty on your last solve |
| `/del [n]` | delete solve number `n`, or your newest solve if you leave `n` off |
| `/inspect` | turn 15 second inspection on or off |
| `/hidetime` | hide or show the running time while you solve |
| `/export [path]` | write every session out as a csTimer file |
| `/import <path>` | read a csTimer file in as new sessions |
| `/help` | open and close the help overlay |
| `/quit` or `/q` | quit |

The longer puzzle names have short forms: `/pyra` for `/pyraminx`, `/mega` for
`/megaminx`, `/square1` or `/square-1` for `/sq1`, and `/3x3oh` for `/oh`.

The number `/del` takes is the one beside the solve in the times list, so
`/del 1` throws away the oldest solve in the session and `/del` on its own takes
the newest. A number nothing matches, or something that is not a number at all,
just says so on the status line and changes nothing.

## Sessions and puzzles

A session is just an ordered list of solves for one puzzle, with a name and an
id. Every save file has twelve permanent ones called `default`, one per puzzle:

| id | puzzle | id | puzzle |
|---|---|---|---|
| 1 | 3x3 | 7 | pyraminx |
| 2 | 2x2 | 8 | skewb |
| 3 | 4x4 | 9 | megaminx |
| 4 | 5x5 | 10 | sq1 |
| 5 | 6x6 | 11 | clock |
| 6 | 7x7 | 12 | oh |

One-handed is its own event with its own session and its own personal bests,
even though `/oh` hands you an ordinary 3x3 scramble, because a one-handed 12
and a two-handed 12 are not the same achievement.

Their solves are yours to penalise and delete as usual, but the sessions
themselves cannot be renamed, retyped or deleted, so `/4x4` always has somewhere
to land. `/new` gives you as many sessions of your own as you want, with ids
from 13 up, and without a name they are numbered for you.

`/sessions` shows the lot in a popup you pick from, one row each, with the
twelve `default` names dimmed so your own stand out and a `>` beside the session
you are in. The cursor starts on that row. The arrow keys and `k` `j` move it one
row at a time, `PgUp` and `PgDn` ten, and `Home` goes to the top; `Enter`
switches to the session under the cursor and closes the popup, and `Esc` closes
it without changing anything. Choosing the session you are already in is a no-op,
so the scramble in front of you survives.

Nothing behind the popup responds while it is open, not even the space bar, so
there is no way to start a solve by accident while you are choosing. On a
terminal too short to hold every row the list scrolls under the cursor, and the
title counts your position, as in `sessions 14/20`.

A save file written by an older Cubetimer is brought up to this layout when it
is read, however far back it came from. The old `default` session keeps its
solves and becomes the default for its puzzle, and anything else you had made
keeps its name and times, moving to a new id if the one it held is now reserved
for one of the new events. Adding one-handed took id 12, for instance, so a
session of yours that used to sit there is now id 13, and nothing you recorded
in it is lost.

Puzzle switching is built around one rule: a session that has solves in it never
changes puzzle, because that would mix two events into one set of stats. So
`/4x4` takes you to the 4x4 default session, and that is the whole story unless
you are sitting in an empty session you made yourself, in which case Cubetimer
retypes that session in place and you keep the name you gave it. Either way you
land on a fresh scramble for the new puzzle.

Because the destination is a fixed session rather than whichever one you made
most recently, going back and forth between `/3x3` and `/4x4` all evening drops
you in the same two places every time.

`/delsession` throws a session away along with its solves. With no argument it
takes the one you are in, and with an id it takes that one, so you can clear out
a session without switching to it first. Deleting the session you are in leaves
you on the default for its puzzle. The twelve defaults are refused.

## Where your times live

Every solve is saved the moment you stop the timer, and so is every command that
changes something. There is no save step and nothing to remember before you
close the terminal.

Your data lives in a single file:

```
%APPDATA%\cubetimer\data\sessions.json
```

It is plain, pretty-printed JSON, so you can read it, back it up, or copy it to
another machine. Writes go to a temporary file and are then renamed into place,
which means a crash mid-save cannot leave you with half a file. Set the
`CUBETIMER_DATA` environment variable to a full file path if you want your times
somewhere else.

If that file ever does turn up unreadable, Cubetimer refuses to start and tells
you the path rather than starting fresh over the top of it.

## Taking your times with you

Cubetimer reads and writes csTimer's export format, so your history is not
trapped in either program. `/export` writes every session, solves, scrambles,
penalties and timestamps included, to `cubetimer-cstimer-export.json` in the
directory you started Cubetimer from, and the status line tells you the full
path it landed at. Give it an argument, as in `/export C:\backups\times.json`,
and it writes there instead. Nothing about your save file changes, so exporting
is also the quickest way to take a backup. What comes out is the shape csTimer's
own importer reads, and reading it back into Cubetimer returns every solve as it
was, give or take the fraction of a second in a timestamp that csTimer counts in
whole seconds.

`/import <path>` goes the other way and reads a file csTimer exported. Every
session in it arrives as a **new** session of your own with the next free id,
keeping the name csTimer had for it. An import never merges into a session that
already exists, never touches a solve you already had, and never moves you out
of the session you are in, so the worst an unwanted import can do is leave you
some sessions to `/delsession`. The status line counts what came in.

The twelve events line up in both directions, one-handed included. Events
csTimer has and Cubetimer does not, blindfolded and fewest moves among them, are
skipped rather than filed under the nearest match, because 3x3 blindfolded times
in a 3x3 average would be nonsense. The status line says how many were passed
over, as in `imported 4 sessions (2 skipped)`.

## How the stats work

Your averages follow WCA rules. An ao5 or ao12 throws out the best and the worst
solve and takes the mean of what is left; an ao100 throws out the best five and
the worst five, and an ao1000 throws out fifty at each end. A DNF always counts
as the worst solve in the window, so a single DNF in an ao5 is absorbed by the
trim, and a second one turns the whole average into a DNF. When there are not
enough solves yet, the average shows as a dash.

The mo3 is the odd one out and deliberately so. It is a plain mean of your last
three solves with nothing trimmed, which is how the WCA scores the big cubes, so
there is no discarded slot for a DNF to hide in: one DNF anywhere in those three
and the mo3 reads `DNF`.

A `+2` adds two seconds to the raw time, and everything downstream uses that
penalised time. Times are truncated to centiseconds rather than rounded, WCA
style, so 12.349 shows as `12.34`.

The stats strip reads in three rows, and the first and last say which is which:
`current` labels your rolling `mo3 ao5 ao12 ao100 ao1000`, and `best` labels the
same five windows at the best you have ever done them. The middle row, between
the two, is `best single worst single mean solves` for the session you are in. It
carries no label of its own but is still indented to the same column, so all
three rows start their numbers in the same place and a rolling average sits
directly above its personal best. The middle row's figures cover the current
session only and ignore DNFs, and its two singles are spelled out in full,
`best single` and `worst single`, so neither reads as a row label. The personal
bests are the best you have ever done across every session of the puzzle you are
currently on, and the rolling windows behind them are searched within each
session rather than across the seam between two of them. On a narrow terminal a
row drops entries from the right rather than wrapping, so the numbers you look at
most stay put.

When the window is tall enough, a two row sparkline labelled `trend` appears
under those three rows, plotting your last fifty solves with the newest at the
right. The bars are times, so a dip is a fast solve and a rising staircase is a
session going the wrong way. DNFs have no time to draw and are simply absent. It
is the first thing the left column gives up: the moment the terminal is too short
to hold both the bars and the big digits, the sparkline goes and the digits stay.
On a panel too narrow for fifty bars the oldest solves are dropped rather than
the newest.

Beating a personal best says so. When a solve is faster than your best ever
single, or its ao5 beats your best ever ao5, a green `new pb` line appears over
the digits for five seconds and the result underneath turns green with it. Both
at once are named on the same line. Only a record you actually beat counts:
matching one exactly is not beating it, and the first single or ao5 you ever
record had nothing to beat, so it passes quietly.

## Scrambles

Every puzzle scrambles in official WCA notation and follows the conventions of
its event, so what you read is what you turn: no move undoes the one before it,
Pyraminx tips come last in `u l r b` order, Megaminx uses Pochmann notation,
Square-1 alternates twists and slashes and never asks for a slash the puzzle
cannot make, and Clock walks the fourteen dials with the `y2` flip in the
middle. Press `n` any time you want a different scramble. Each solve is stored
alongside the one it was done on.

How close each event gets to a competition scramble varies, and it is worth
being plain about it:

- **Clock** is exactly official. Its dials commute, so drawing each one at
  random already gives a uniformly random state, which is all the official
  generator does.
- **Megaminx** matches the official generator move for move. It is a random-move
  event even in competition, and Cubetimer reproduces the same seven lines with
  the same rules.
- **5x5, 6x6 and 7x7** use the same random-move generation the official scramble
  program uses, with the same move pools and lengths.
- **2x2, 3x3, 4x4, Pyraminx, Skewb and Square-1** are honest practice
  approximations. Official scrambles for these are generated from a random
  state and solved back into moves; Cubetimer picks the moves and takes whatever
  state comes out. The lengths match real scrambles and they are perfectly good
  to train on, but they are not competition-legal.
- **One-handed** is 3x3, scrambled by the very same generator and inheriting
  exactly the same caveat. The WCA scrambles it the same way; the event is the
  hand you are not using, not the cube.

Megaminx scrambles run to seven lines, and the scramble area at the top of the
screen grows to fit them, up to a point: it will not eat so much of the window
that the timer loses its big digits.

If you last solved Clock a few years ago, note that official scrambles stopped
including pin states in January 2024. Cubetimer follows the current rules, so
you set the pins yourself.

## Under the hood

Cubetimer is written in Rust and drawn with [ratatui](https://ratatui.rs). It
has no runtime dependencies and no configuration file. Build it with
`cargo build --release` and the binary lands at `target\release\cubetimer.exe`.

Architecture notes and design documents live in `claude-docs/`, and if you are
planning to send a change, the ground rules are in `CLAUDE.md`.
