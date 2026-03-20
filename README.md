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
- `[layer:index]`
- note sequences like `[g4 a4 a3 c3]`
- explicit pattern bodies with `<...>` and `[...]`
- `/n`
- `*n`
- `<< n` and `>> n`
- `.gain <number>`
- `.pan <number>`
- `.speed <number>`
- `.sustain <number>`
- `# comments`

## Setup

1. Install Rust.
2. Install SuperCollider from the official downloads page: <https://supercollider.github.io/downloads.html>
3. Install the SuperDirt quark in SuperCollider:

```supercollider
include("SuperDirt");
```

4. From this repo, build or run `fin`.

For direct shell use:

```bash
cargo install --path . --force
```

For development:

```bash
cargo run -- run examples/basic.metl
```

## SuperCollider Workflow

One-time install, in SuperCollider:

```supercollider
include("SuperDirt");
```

After that, you do not need the IDE to start SuperDirt.

Start it as a managed background process:

```bash
fin superdirt
```

Check it:

```bash
fin superdirt status
```

Stop it:

```bash
fin superdirt kill
```

If you want the old foreground behavior:

```bash
fin superdirt --foreground
```

There is also a direct shell wrapper from the repo:

```bash
./scripts/start-superdirt.sh
```

Both paths launch `sclang` directly and run [supercollider/superdirt_startup.scd](/Users/aimeeco/fin/supercollider/superdirt_startup.scd) headlessly.

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

Use a custom port if needed:

```bash
fin superdirt --port 57121
./scripts/start-superdirt.sh 57121
```

If `sclang` is not on your `PATH`, either pass it directly:

```bash
fin superdirt --sclang /Applications/SuperCollider.app/Contents/MacOS/sclang
```

or set:

```bash
export FIN_SCLANG_BIN=/Applications/SuperCollider.app/Contents/MacOS/sclang
```

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

Run the TUI dashboard:

```bash
fin dashboard examples/basic.metl
```

Start SuperDirt without the IDE:

```bash
fin superdirt
```

Check or stop the managed background process:

```bash
fin superdirt status
fin superdirt kill
```

Use a custom OSC target:

```bash
fin watch --host 127.0.0.1 --port 57120 examples/basic.metl
```

`watch` keeps the last good program loaded and re-reads the file at each bar boundary. If a new edit fails to parse, the previous good schedule keeps playing and the reload error is printed to stderr.

`dashboard` runs the same bar-by-bar engine in an alternate-screen TUI with layer meters and recent logs. Press `q` to quit between bar updates.

Current layer-to-sound mapping:

- `bd` -> SuperDirt `bd`
- `sd` -> SuperDirt `sd`
- `hh` -> SuperDirt `hh`
- any other symbol -> the same sound name is sent through to SuperDirt

## Example

[`examples/basic.metl`](/Users/aimeeco/fin/examples/basic.metl):

```ini
bpm = 128
[bass] [g4 a4 a3 c3]
[bd] <0 3 5 7> /1
[sd] /2 >> 0.25 .gain 0.8
[hh] [hh hh:2] *4 .pan 0.2 .speed 1.1 .sustain 0.15
```

## Documentation

Language notation is documented in [docs/metl-notation.md](/Users/aimeeco/fin/docs/metl-notation.md).

## Verification

Current verification command:

```bash
cargo test
```
