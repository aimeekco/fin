# METL Notation

METL is a layer-based live-coding language. Each layer is introduced by a header, and all layers are active at the same time.

## Current Implemented Subset

The current parser supports:

- `bpm = <number>`
- bare layer headers like `[bd]`
- division with `/n`
- line comments starting with `#`

Example:

```ini
bpm = 128
[bd] /4
[sd] /2
```

Current semantics:

- `[bd] /4` means "trigger `bd` four times across one 4/4 bar"
- `/4` produces beat positions `0, 1, 2, 3`
- `/2` produces beat positions `0, 2`
- if `bpm` is omitted, playback defaults to `120`
- runtime playback currently sends layer names directly to SuperDirt as sound names

## Layer Model

Layer headers use square brackets:

```ini
[bd]
[sd]
[bass]
```

Today, a bare layer name is treated as an implicit self-triggering pattern source. There is no separate inline pattern body yet.

Runtime voice mapping today:

- `bd` plays the SuperDirt `bd` sound
- `sd` plays the SuperDirt `sd` sound
- `hh` plays the SuperDirt `hh` sound
- unknown symbols are sent through unchanged so they can target other SuperDirt sounds

## Timing Operator

`/n` divides the bar into `n` evenly spaced trigger slots.

```ini
[bd] /4
[hh] /8
[bass] /1
```

Interpretation in 4/4:

- `/1` means one trigger at the start of the bar
- `/2` means two triggers, halfway apart
- `/4` means quarter-note triggers
- `/8` means eighth-note triggers

## Comments

Use `#` for comments:

```ini
bpm = 128
[bd] /4 # kick drum on quarter notes
```

## Planned Syntax

The design target for METL still includes the following syntax, but it is not implemented yet:

```ini
[sd] >> 0.25
[hh] *16 ~ 0.8 .gain 0.6
[bass] <0 3 5 7> /1 .lpf 400
```

Planned operators:

- `*n` for density multiplication
- `<< n` and `>> n` for bar-relative offsets
- `~ n` for probability
- `.method value` for effect-style parameter chaining
- `< >` for ordered cycles
- `[ ]` for subdivisions inside a slot

## Runtime Behavior Today

`fin run file.metl`:

- parses one file
- prints one bar of scheduled events
- optionally plays one bar through SuperCollider

`fin watch file.metl`:

- loads the file
- plays continuously bar by bar
- re-reads the file at the end of each bar
- applies valid changes on the next bar boundary
- keeps the last good schedule if a reload fails
