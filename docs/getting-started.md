# Getting started

Cubetimer ships as source code, so you build it once on your machine and end up
with a single self-contained `.exe`. Here is the whole journey from a bare
computer to a running timer.

## What you need first

Two tools, both free:

1. **The Rust toolchain**, installed through [rustup](https://rustup.rs). This
   gives you `cargo`, Rust's build tool, which handles everything else below.
   On Windows the installer offers two flavours: the default MSVC flavour asks
   to install Microsoft's Visual Studio Build Tools alongside it, and the GNU
   flavour needs no Visual Studio but wants MinGW binutils on your PATH for
   some dependencies. If you have no opinion, take the default and let it
   install what it asks for.
2. **Git**, to fetch the code, though the green Code button on GitHub offers a
   ZIP download that works just as well.

## Build and run

```
git clone https://github.com/cameronjim/tui-cube-timer.git
cd tui-cube-timer
cargo build --release
```

The first build takes a few minutes: cargo downloads the libraries Cubetimer
uses and compiles everything, once. Builds after that take seconds. The
finished program lands at `target\release\cubetimer.exe`; run it from there using:

```
target\release\cubetimer.exe
```

or drop that one file anywhere on your PATH and just type `cubetimer`. During
development, `cargo run --release` builds and runs in one step.

## There is nothing else to install

There is nothing else to install, ever. The libraries are declared in
`Cargo.toml`, pinned to exact versions in `Cargo.lock`, and fetched by cargo on
that first build: [ratatui](https://ratatui.rs) draws the interface, serde and
serde_json read and write the save file, rand feeds the scramblers, and
directories finds your data folder. The compiled `.exe` carries all of them
inside it, so it needs no runtime, no framework and no installer on any machine
it is copied to.

## Which terminals work

Cubetimer is built for Windows first, because the hold-and-release space bar
flow needs a terminal that reports key releases and the Windows console does
that natively. On other platforms it works in terminals that speak the kitty
keyboard protocol, which Cubetimer asks for at startup.

Once it is running, [using-the-timer.md](using-the-timer.md) takes you through
your first solve.
