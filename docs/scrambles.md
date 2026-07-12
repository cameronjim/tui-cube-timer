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

## Two things worth knowing

Megaminx scrambles run to seven lines, and the scramble area at the top of the
screen grows to fit them, up to a point: it will not eat so much of the window
that the timer loses its big digits.

If you last solved Clock a few years ago, note that official scrambles stopped
including pin states in January 2024. Cubetimer follows the current rules, so
you set the pins yourself.

Which puzzle you are scrambling for follows from the session you are in, which
[sessions.md](sessions.md) explains.
