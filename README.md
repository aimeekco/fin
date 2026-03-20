# fin

`fin` is the runtime for METL, a CLI live-coding environment for electronic music written in Rust and backed by SuperCollider over OSC.

## Current Status

This repo currently supports:

- parsing a small `.metl` subset
- printing one-bar schedules
- sending OSC trigger messages to SuperDirt
- using SuperDirt's sample library for drum playback
- continuous bar-by-bar playback with bar-boundary reloads

Implemented METL subset:

- `bpm = <number>`
- `[layer]`
- `/n`
- `# comments`

## What We Learned

- Booting SuperCollider through `s.boot;` is the simplest reliable setup for local development.
- The repo binary and an older globally installed `fin` can diverge. `cargo run -- ...` is the safest way to verify the latest local code.
- SuperDirt is a better fit than ad hoc synthdefs for this stage because METL layer names like `bd`, `sd`, and `hh` can map directly onto an existing sample library.
- The active OSC target for SuperDirt is `127.0.0.1:57120`, not the raw `scsynth` port `57110`.
- SuperDirt expects OSC trigger messages on its own listening port after `SuperDirt.start`, rather than direct `/s_new` traffic to the audio server.
- `s.dumpOSC(1);` in SuperCollider is the fastest way to see whether `fin` is actually reaching the server.

## Setup

1. Install Rust.
2. Install SuperCollider from the official downloads page: <https://supercollider.github.io/downloads.html>
3. Install the SuperDirt quark in SuperCollider:

```supercollider
include("SuperDirt");
```

3. From this repo, build or run `fin`.

For direct shell use:

```bash
cargo install --path . --force
```

For development:

```bash
cargo run -- run examples/basic.metl
```

## SuperCollider Workflow

In the SuperCollider IDE:

```supercollider
s.boot;
// evaluate supercollider/superdirt_startup.scd
```

You should see:

```text
SuperDirt: listening on port 57120
```

Useful debugging commands:

```supercollider
s.addr
s.freeAll;
s.dumpOSC(1);
```

The startup script is at [supercollider/superdirt_startup.scd](/Users/aimeeco/fin/supercollider/superdirt_startup.scd). Run it after installing SuperDirt and whenever you start a fresh SuperCollider session.

## Commands

Play one bar once:

```bash
fin run examples/basic.metl
```

Print scheduling only:

```bash
fin run --no-play examples/basic.metl
```

Run a continuous live-reload loop:

```bash
fin watch examples/basic.metl
```

Use a custom OSC target:

```bash
fin watch --host 127.0.0.1 --port 57120 examples/basic.metl
```

`watch` keeps the last good program loaded and re-reads the file at each bar boundary. If a new edit fails to parse, the previous good schedule keeps playing and the reload error is printed to stderr.

Current layer-to-sound mapping:

- `bd` -> SuperDirt `bd`
- `sd` -> SuperDirt `sd`
- `hh` -> SuperDirt `hh`
- any other symbol -> the same sound name is sent through to SuperDirt

## Example

[`examples/basic.metl`](/Users/aimeeco/fin/examples/basic.metl):

```ini
bpm = 128
[bd] /4
[sd] /2
```

## Documentation

Language notation is documented in [docs/metl-notation.md](/Users/aimeeco/fin/docs/metl-notation.md).

## Verification

Current verification command:

```bash
cargo test
```
