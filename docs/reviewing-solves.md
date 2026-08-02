# Reviewing solves

## Looking back at a solve

The arrow keys move a selection through the times list, `PgUp` and `PgDn` jump
ten at a time, and `Home` takes you straight back to your newest solve. Press
`Enter` on the one you want and a popup opens with the solve in full: the time
with its penalty, the date it was set in UTC, and the whole scramble it was done
on, wrapped to fit and tall enough for all seven of Megaminx's lines.

Inside that popup, `r` loads its scramble back as your current one so you can
have another go at exactly the same case, and `Esc` or `Enter` closes it. The
popup opens from an idle timer only, so `Enter` can never interrupt a solve.

## Deleting a solve

The number `/del` takes is the one beside the solve in the times list, so
`/del 1` throws away the oldest solve in the session and `/del` on its own takes
the newest. A number nothing matches, or something that is not a number at all,
just says so on the status line and changes nothing.

Penalties work the same way on your last solve: `/dnf`, `/+2` and `/ok` set or
clear one. To throw away a whole session at a time, see
[sessions.md](sessions.md).
