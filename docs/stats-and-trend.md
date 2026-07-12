# Stats and the trend graph

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

## Reading the stats strip

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

## The trend graph

`/trend` graphs your last fifty solves in a popup, as a line with time up the
side and the solve's place in the window along the bottom, so a dip is a fast
solve and a climb is a session going the wrong way. The y axis is labelled with
three times, the x axis with the first and last solve numbers, and `Esc` or
`/trend` again closes it. It shares its slot with the help and the session
picker, so opening one closes the other, and a solve detail popup covers all
three.

The time axis spans the window itself rather than counting up from zero, because
solve times cluster in a band far away from zero and starting there would draw
every session as one flat line across the top. It also stops at the window's
95th percentile rather than its slowest solve: a single sixty second disaster
among twelve second solves would otherwise own the whole scale and squash
everything else onto the bottom row, so the disaster is drawn pinned to the top
edge instead and the rest of the window keeps the height. That means the top
label is a ceiling, not always your slowest time. A window with nothing to
separate draws flat across the middle. DNFs have no time to plot and are simply
absent, so a session with a run of them graphs fewer than fifty points.

The graph is drawn out of `▀`, `▄` and `█` and nothing else, all of them
characters the classic Windows console fonts carry, so it renders as a line
rather than a row of empty boxes there. On a terminal too small to read it,
under 44 columns or 16 rows, `/trend` draws nothing at all.

## Beating a personal best

Beating a personal best says so. When a solve is faster than your best ever
single, or its ao5 beats your best ever ao5, a green `new pb` line appears over
the digits for five seconds and the result underneath turns green with it. Both
at once are named on the same line. Only a record you actually beat counts:
matching one exactly is not beating it, and the first single or ao5 you ever
record had nothing to beat, so it passes quietly.

Personal bests are per puzzle and span every session on it, which is part of why
sessions work the way [sessions.md](sessions.md) describes.
