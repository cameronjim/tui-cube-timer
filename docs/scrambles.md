# Scrambles

Every puzzle scrambles in official WCA notation and follows the conventions of
its event, so what you read is what you turn: no move undoes the one before it,
Pyraminx tips come last in `u l r b` order, Megaminx uses Pochmann notation,
Square-1 alternates twists and slashes and never asks for a slash the puzzle
cannot make, and Clock walks the fourteen dials with the `y2` flip in the
middle. Press `n` any time you want a different scramble. Each solve is stored
alongside the one it was done on.

## How official each event is

How close each event gets to a competition scramble varies, and it is worth
being plain about it:

- **3x3, one-handed, 2x2, Pyraminx and Skewb** are random-state, which is the
  method the WCA uses. See below for what that means.
- **Clock** is exactly official. Its dials commute, so drawing each one at
  random already gives a uniformly random state, which is all the official
  generator does.
- **Megaminx** matches the official generator move for move. It is a random-move
  event even in competition, and Cubetimer reproduces the same seven lines with
  the same rules.
- **5x5, 6x6 and 7x7** use the same random-move generation the official scramble
  program uses, with the same move pools and lengths.
- **4x4 and Square-1** are honest practice approximations. Official scrambles for
  these are generated from a random state and solved back into moves; Cubetimer
  picks the moves and takes whatever state comes out. The lengths match real
  scrambles and they are perfectly good to train on, but they are not
  competition-legal.

One-handed is the 3x3 event with a hand behind your back, so it takes the very
same scrambles, which is how the WCA treats it too: the event is the hand you are
not using, not the cube.

## What random-state means

A random-move scramble picks a fixed number of legal moves and hands you whatever
position falls out. That sounds fair and is not, because some positions can be
reached by far more sequences than others, so the easy ones come up more often
than they should. A random-state scramble works the other way round: it picks the
position first, uniformly, so **every legal state of the puzzle is exactly as
likely as every other**, and only then works out the moves that produce it.

### 2x2, Pyraminx and Skewb

Cubetimer can do this for these three events because they are small enough to
solve completely. It knows the exact number of moves every position of a 2x2,
Pyraminx and Skewb needs, all 3,674,160, 933,120 and 3,149,280 of them, so it
draws a position, solves it, and writes the solution backwards.

What you see follows from that:

- **Every scramble is exactly eleven moves**, for all three events, whatever the
  position actually needs. Most positions can be solved in eight or nine, so the
  solution is deliberately searched for at eleven instead. That is what official
  scrambles do too, and it is why real 2x2 scrambles are always eleven moves
  long.
- **Notation is unchanged.** 2x2 is still `U R F` with `'` and `2`, Skewb is
  still `R U L B` with `'`, and Pyraminx is still `U L R B` with `'`.
- **Pyraminx tips still come last**, in `u l r b` order, one token per unsolved
  tip. A tip turns on its own and does not affect anything else, so it is drawn
  separately: a random tip is already solved one time in three and contributes no
  move, which is why a scramble carries anywhere from zero to four of them.

### 3x3 and one-handed

The 3x3 has 43 quintillion positions, so nothing can list them, and the method
above is not available. Cubetimer uses the one every scramble program has used
since 1992 instead: draw a random position, then solve it in two stages, first
into the family of positions that need no quarter turn of the four side faces,
then home from inside that family. Neither stage searches the whole cube, so
neither needs a table of the whole cube. The scramble is still that solution
written backwards, and the position is still drawn uniformly, which is the part
that matters.

**Scrambles run 16 to 21 moves rather than a fixed length**, and nearly always 20
or 21. That is what real 3x3 scrambles look like, and it is the shape the official
generator produces too: it asks for the shortest two-stage solution it can find up
to 21 moves and emits whatever that turns out to be. The 2x2 is fixed at eleven
because eleven moves always suffice and a shorter solution can be padded out to
exactly eleven; on a 3x3 there is no length that is always enough and always
reachable, so the number of moves varies with the position. Very occasionally, on
about one scramble in two thousand, the two-stage split cannot be squeezed under 21
and a scramble comes out a few moves longer, 23 to 25 in the batch this was measured
on. It is a genuine random-state scramble either way.

Notation is the plain six faces, `U D L R F B` with `'` and `2`, and no two
moves in a row turn the same face.

The remaining seven events stay random-move for now. Clock and Megaminx are
already equivalent to official output and need nothing; the rest each want a
solver written for their own shape, which is a piece of work per puzzle.

### The pause on the first scramble of an event

The tables behind all of this are built the first time you scramble that event,
not when Cubetimer starts, so the cost is paid once per run and only for the
events you actually use. It is a fraction of a second for Pyraminx, about four
tenths of a second for the 3x3, about half a second for 2x2, and a little over a
second for Skewb, whose table covers nine million positions. Nothing is written
to disk, so the next run pays it again, and every scramble after the first is
instant.

The 3x3 is the event a new save file opens on, so unless you have switched
sessions its scramble is the one waiting when the window appears, and that is
where its four tenths of a second lands: Cubetimer takes about half a second to
draw its first frame. Switching to 3x3 later pays the same pause once instead.

## Seeing the cube first

`/preview` opens the scramble as a cube: the six faces unfolded into a flat net,
each sticker the colour it would be once you finished turning. It is the state
the scramble in front of you leads to, not the state you are looking at, so it
is a way to check your turning after the fact or to plan a cross before you
start the timer. The net opens against the right side of the window, over the
stats and times panels, so the big timer stays visible beside it. `/preview`
again closes it, and so does `Esc`. It shares its slot with the help, the trend
graph and the session picker: opening any of those closes it.

The colours are the standard scheme, white on top and yellow underneath, green in
front and blue behind, red on the right and orange on the left. Orange is the one
colour a terminal's basic palette does not carry, so Cubetimer asks for it by its
RGB value; a terminal without RGB support approximates it with the nearest colour
it has.

The preview covers 2x2 through 7x7 and one-handed, which are the events
Cubetimer has a cube model for. Pyraminx, Skewb, Megaminx, Square-1 and Clock
say `no preview for <event> yet` and keep their scrambles unchanged; each is
waiting on a model of its own shape.

On a terminal too short for a full sized net, the preview halves its height by
drawing two sticker rows per line, which is the same picture at half scale. It
needs the same width either way, so on a window narrower than the net, or shorter
than even the half scale one, it says the terminal is too small rather than
drawing a cube you could not read.

## Two things worth knowing

Megaminx scrambles run to seven lines, and the scramble area at the top of the
screen grows to fit them, up to a point: it will not eat so much of the window
that the timer loses its big digits.

If you last solved Clock a few years ago, note that official scrambles stopped
including pin states in January 2024. Cubetimer follows the current rules, so
you set the pins yourself.

Which puzzle you are scrambling for follows from the session you are in, which
[sessions.md](sessions.md) explains.
