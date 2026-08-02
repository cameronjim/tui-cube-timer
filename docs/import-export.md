# Taking your times with you

Cubetimer reads and writes csTimer's export format, so your history is not
trapped in either program.

## Exporting

`/export` writes every session, solves, scrambles, penalties and timestamps
included, to a file named the way csTimer names its own,
`cstimer_20260802_141534.txt` for an export taken at that moment, in the
directory you started Cubetimer from. The date and time in the name are UTC. The
status line tells you the full path it landed at. Give it an argument, as in
`/export C:\backups\times.txt`, and it writes there instead. Nothing about your
save file changes, so exporting is also the quickest way to take a backup.

Keep the `.txt` ending on any name you choose. csTimer's import button opens a
file picker that only offers text files, and Windows calls a `.json` file
something else, so a `.json` export is one the picker will not show you.

What comes out is the shape csTimer's own importer reads: hand it to the
from-file import in csTimer's export panel and every session, every name and
every solve arrives. Reading it back into Cubetimer returns every solve as it
was, give or take the fraction of a second in a timestamp that csTimer counts in
whole seconds. One thing to know before you import into csTimer: it replaces all
of its own settings with whatever the file carries, and a Cubetimer export
carries session data and nothing else, so csTimer's preferences go back to their
defaults.

## Importing

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

An export is a copy, not the original. Where the original lives is in
[your-data.md](your-data.md).
