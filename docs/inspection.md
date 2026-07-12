# Inspection

Turn on WCA inspection with `/inspect`, and the top border will start reading
`inspection: on`. Running it again turns it back off. The setting is written to
your save file, so whichever way you leave it is how Cubetimer starts next time.

With inspection on, one step slots in ahead of everything else. Tap space and a
15 second countdown starts in yellow. Inspect the cube, then hold space and
release exactly as before to start the solve. `Esc` backs out of a countdown if
you change your mind.

## The judge calls

A judge at a competition calls out "8 seconds" and "12 seconds" while you
inspect, and Cubetimer marks both silently. At 8 seconds the countdown turns
light magenta and `8s` appears in the small line under the digits; at 12 seconds
it turns light red and that line reads `12s`. The colour is the part you catch
without looking away from the cube, and the caption is there when you do look.

## The penalties

The penalties are applied for you, so there is nothing to remember afterwards.
Going past 15 seconds turns the solve into a +2, and going past 17 seconds makes
it a DNF. Either way the countdown turns red and the same small line switches
from the judge call to `+2` or `DNF`, so a penalty you have already earned is
never mistaken for a call, and the solve is recorded with that penalty already
attached.

A `+2` from inspection behaves like any other, adding two seconds to the raw
time everywhere the number is used. See
[stats-and-trend.md](stats-and-trend.md) for what that does to your averages.
