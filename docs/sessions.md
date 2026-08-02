# Sessions and puzzles

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

## The session picker

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

## Older save files

A save file written by an older Cubetimer is brought up to this layout when it
is read, however far back it came from. The old `default` session keeps its
solves and becomes the default for its puzzle, and anything else you had made
keeps its name and times, moving to a new id if the one it held is now reserved
for one of the new events. Adding one-handed took id 12, for instance, so a
session of yours that used to sit there is now id 13, and nothing you recorded
in it is lost.

## Switching puzzles

Puzzle switching is built around one rule: a session that has solves in it never
changes puzzle, because that would mix two events into one set of stats. So
`/4x4` takes you to the 4x4 default session, and that is the whole story unless
you are sitting in an empty session you made yourself, in which case Cubetimer
retypes that session in place and you keep the name you gave it. Either way you
land on a fresh scramble for the new puzzle.

Because the destination is a fixed session rather than whichever one you made
most recently, going back and forth between `/3x3` and `/4x4` all evening drops
you in the same two places every time.

## Deleting sessions

`/delsession` throws a session away along with its solves. With no argument it
takes the one you are in, and with an id it takes that one, so you can clear out
a session without switching to it first. Deleting the session you are in leaves
you on the default for its puzzle. The twelve defaults are refused.

A range takes several at once: `/delsession 15-20` deletes every session from id
15 through id 20, both ends included. Ids in the range that belong to one of the
twelve defaults are refused and ids nothing sits on are passed over, both counted
rather than named, so the status reads `deleted 4 sessions (2 skipped)` and
`/delsession 13-9999` is a way to clear out every session you ever made. If the
one you are in goes with them you land on the default for its puzzle, exactly as
deleting it on its own would. A backwards range like `20-15` is refused.

Before you delete anything you might miss, [import-export.md](import-export.md)
covers taking a backup.
