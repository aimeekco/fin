# fin

`fin` is the runtime for METL, a CLI live-coding environment for electronic music written in Rust and backed by SuperCollider over OSC.

## Current Status

This repo currently supports:

- parsing a small `.metl` subset
- printing one-bar schedules
- sending OSC trigger messages to SuperCollider
- triggering custom drum synthdefs in SuperCollider
- releasing notes cleanly after each hit
- continuous bar-by-bar playback with bar-boundary reloads

Implemented METL subset:

- `bpm = <number>`
- `[layer]`
- `/n`
- `# comments`

## What We Learned

- Booting SuperCollider through `s.boot;` is the simplest reliable setup for local development.
- The repo binary and an older globally installed `fin` can diverge. `cargo run -- ...` is the safest way to verify the latest local code.
- Silent OSC failure is hard to debug. `fin` now sends `/status` and requires `/status.reply` before playback.
- Reusing fixed node IDs causes repeat-run failures in SuperCollider. The runtime now uses unique node IDs per process.
- Triggering `default` with `/s_new` is not enough by itself. Notes must also be released, so the runtime sends `/n_set gate 0` after each hit.
- Pitching the stock `default` synth does not sound like drums. `bd`, `sd`, and `hh` now target custom synthdefs that must be loaded into the server first.
- `s.dumpOSC(1);` in SuperCollider is the fastest way to see whether `fin` is actually reaching the server.

## Setup

1. Install Rust.
2. Install SuperCollider from the official downloads page: <https://supercollider.github.io/downloads.html>
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
// then evaluate the contents of supercollider/fin_setup.scd
Synth(\fin_bd);
```

If `Synth(\fin_bd);` makes sound, the server and FIN synthdefs are working.

Useful debugging commands:

```supercollider
s.addr
s.freeAll;
s.dumpOSC(1);
```

The setup script is at [supercollider/fin_setup.scd](/Users/aimeeco/fin/supercollider/fin_setup.scd). Run it after `s.boot;` whenever you start a fresh SuperCollider session.

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
fin watch --host 127.0.0.1 --port 57110 examples/basic.metl
```

`watch` keeps the last good program loaded and re-reads the file at each bar boundary. If a new edit fails to parse, the previous good schedule keeps playing and the reload error is printed to stderr.

Current layer-to-synth mapping:

- `bd` -> `fin_bd`
- `sd` -> `fin_sd`
- `hh` -> `fin_hh`
- any other symbol -> `fin_tone`

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
