# Where your times live

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

For a copy you can hand to another program, or read on a machine without
Cubetimer on it, see [import-export.md](import-export.md).
