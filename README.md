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
`inspection: on`. Running it again turns it back off.

With inspection on, one step slots in ahead of everything else. Tap space and a
15 second countdown starts in yellow. Inspect the cube, then hold space and
release exactly as before to start the solve. `Esc` backs out of a countdown if
you change your mind.

The penalties are applied for you, so there is nothing to remember afterwards.
Going past 15 seconds turns the solve into a +2, and going past 17 seconds makes
it a DNF. Either way the timer shows what you have earned in red while the
countdown is still on screen, and the solve is recorded with that penalty
already attached.

## Keys

| Key | What it does |
|---|---|
| `space` | hold until green, release to start (a tap starts inspection when inspection is on) |
| any key | stops a running timer |
| `n` | new scramble |
| `/` | opens the command line |
| `↑` `↓` or `k` `j` | scroll the times list one solve at a time |
| `PgUp` `PgDn` | scroll the times list ten at a time |
| `Home` | jump the times list back to your newest solve |
| `h` or `?` | open and close the help overlay |
| `Esc` | cancel inspection, close the help, leave the command line, clear the status line |
| `q` | quit |

`Ctrl+C` also quits, from anywhere, if you ever need it.

## Commands

Press `/` to open the command line, type, and press `Enter`. Backspacing past
the leading slash gets you out again, as does `Esc`. Capitalisation and extra
spaces do not matter.

| Command | What it does |
|---|---|
| `/2x2` `/3x3` `/4x4` `/5x5` `/6x6` `/7x7` | switch to that puzzle's default session (see below) |
| `/new [name]` | start a new session for the current puzzle |
| `/sessions` | list every session with its id, puzzle and solve count |
| `/session <id>` | switch to a session by id |
| `/rename <name>` | rename the session you are in |
| `/delsession [id]` | delete a session and its solves, current one by default |
| `/dnf` `/+2` `/ok` | set or clear the penalty on your last solve |
| `/del` | delete your last solve |
| `/inspect` | turn 15 second inspection on or off |
| `/help` | open and close the help overlay |
| `/quit` or `/q` | quit |

## Sessions and puzzles

A session is just an ordered list of solves for one puzzle, with a name and an
id. Every save file has six permanent ones called `default`, one per puzzle:
3x3 is id 1, then 2x2, 4x4, 5x5, 6x6 and 7x7 as ids 2 to 6. Their solves are
yours to penalise and delete as usual, but the sessions themselves cannot be
renamed, retyped or deleted, so `/4x4` always has somewhere to land. `/new`
gives you as many sessions of your own as you want, with ids from 7 up, and
without a name they are numbered for you.

A save file written by an older Cubetimer is brought up to this layout when it
is read: the old `default` session keeps its solves and becomes the default for
its puzzle, and anything else you had made keeps its name and times under a new
id.

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
you on the default for its puzzle. The six defaults are refused.

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

## How the stats work

Your averages follow WCA rules. An ao5 or ao12 throws out the best and the worst
solve and takes the mean of what is left; an ao100 throws out the best five and
the worst five. A DNF always counts as the worst solve in the window, so a single
DNF in an ao5 is absorbed by the trim, and a second one turns the whole average
into a DNF. When there are not enough solves yet, the average shows as a dash.

A `+2` adds two seconds to the raw time, and everything downstream uses that
penalised time. Times are truncated to centiseconds rather than rounded, WCA
style, so 12.349 shows as `12.34`.

The `best`, `worst` and `mean` figures in the stats strip cover the session you
are in and ignore DNFs. The personal bests are wider: PB single, PB ao5, PB ao12
and PB ao100 are the best you have ever done across every session of the puzzle
you are currently on, and the rolling averages behind them are searched within
each session rather than across the seam between two of them.

## Scrambles

Scrambles are random-move and follow the usual WCA conventions for each cube
size, with the standard constraints that stop a scramble from undoing itself, so
the move count you see is the move count you turn. On 5x5, 6x6 and 7x7 they come
out of the same generation the official scramble program uses. On 2x2 through
4x4 they are a close practice equivalent rather than a competition-legal
scramble. Press `n` any time you want a different one. Each solve is stored
alongside the scramble it was done on.

## Under the hood

Cubetimer is written in Rust and drawn with [ratatui](https://ratatui.rs). It
has no runtime dependencies and no configuration file. Build it with
`cargo build --release` and the binary lands at `target\release\cubetimer.exe`.

Architecture notes and design documents live in `claude-docs/`, and if you are
planning to send a change, the ground rules are in `CLAUDE.md`.
