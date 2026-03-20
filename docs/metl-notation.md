# METL Notation

METL is a layer-based live-coding language. Each layer is introduced by a header, and all layers are active at the same time.

## Current Implemented Subset

The current parser supports:

- `bpm = <number>`
- bare layer headers like `[bd]`
- division with `/n`
- density multiplication with `*n`
- bar-relative offset with `<< n` and `>> n`
- line comments starting with `#`

Example:

```ini
bpm = 128
[bd] /4
[sd] /2 >> 0.25
[hh] *8
```

Current semantics:

- `[bd] /4` means "trigger `bd` four times across one 4/4 bar"
- `/4` produces beat positions `0, 1, 2, 3`
- `/2` produces beat positions `0, 2`
- `*n` multiplies the number of evenly spaced trigger slots in the bar
- `>> n` shifts a layer later by `n` bars
- `<< n` shifts a layer earlier by `n` bars
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

`*n` multiplies the density of those trigger slots.

```ini
[hh] *4
[bd] /2 *2
```

Interpretation in 4/4:

- `*4` on its own produces four evenly spaced events in the bar
- `/2 *2` produces four events because the two-slot pattern is doubled in density

`>> n` and `<< n` shift an entire layer within the bar with wraparound.

```ini
[sd] /2 >> 0.25
[hh] *4 << 0.125
```

Interpretation in 4/4:

- `>> 0.25` shifts events later by one beat
- `<< 0.125` shifts events earlier by half a beat
- wrapped events stay inside the current bar

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
