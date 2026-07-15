# Using the timer

## What you are looking at

The scramble sits across the top in bold, and the border above it tells you
which puzzle you are on, which session you are in, and whether inspection is
turned on. The big block digits in the middle are the timer. Underneath them is
a strip with this session's averages and bests, and the column on the right is
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

## Hiding the running time

Some people solve faster when they cannot see the clock racing. `/hidetime`
replaces the running digits with `...` while you solve, and the final time
appears as usual the moment you stop. Run it again to bring the running clock
back. Like inspection, the setting is remembered between runs.

Next: turn on the competition countdown in [inspection.md](inspection.md), or
go back over a solve you have already done in
[reviewing-solves.md](reviewing-solves.md).
