# Security policy

## Supported versions

Only the latest commit on `main` is supported. Cubetimer has no release branches, and
fixes land on `main` rather than being backported.

## Reporting a vulnerability

Use GitHub private vulnerability reporting on this repository: open the Security tab,
then "Report a vulnerability". That keeps the report private until a fix is out. Please
do not open a public issue for something exploitable.

A useful report says what an attacker controls, what they get, and how to reproduce it.
A save file or an environment value that triggers the problem is worth more than a
description of it.

## Scope

Cubetimer is a local, single-user terminal program. It makes no network connections of
any kind, has no server component, no auth, and no telemetry.

In scope:

- The save file. Cubetimer parses `sessions.json` as JSON at startup. Crafted contents
  reaching a memory-safety bug, a hang, or unbounded memory growth is a real finding.
  A plain error message and a refusal to start is the intended behavior, not a bug.
- The `CUBETIMER_DATA` environment variable, which names the save file path. Cubetimer
  writes a `.tmp` sibling and renames it over the target. A path traversal beyond what
  the variable already names is in scope.
- The dependency tree, as reported by `cargo audit` against the committed `Cargo.lock`.

Out of scope:

- Anything that requires the attacker to already have write access to the user's own
  data directory or the ability to set the user's environment. At that point they can
  run arbitrary code as the user anyway.
- Terminal escape sequence rendering by third party terminal emulators.
- Denial of service that only affects the reporter's own machine and own data.

## What to expect

Cubetimer is a hobby project maintained in spare time. Expect an acknowledgement within
about a week and a fix on a best-effort schedule after that. There is no bounty. Credit
in the commit and the release notes is offered unless you would rather stay anonymous.
