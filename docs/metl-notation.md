# METL Notation

METL is a layer-based live-coding language with a global phrase length and per-layer bar overrides.

## Current Implemented Subset

The current parser supports:

- `bpm = <number>`
- `bars = <positive integer>` with a default of `4`
- layer headers like `[bd]` and `[sd:2]`
- optional layer-wide effect params: `.gain`, `.pan`, `.speed`, `.sustain`
- indented per-layer fallback entries like `[default]`
- indented per-bar entries like `[bar1]`
- bar-local timing operators: `/n`, `*n`, `<< n`, `>> n`
- atom patterns like `hh` or `sd:2`
- grouped sound patterns like `[bd sd:2]`
- sequence patterns like `<0 3 5 7>` and `<g4 a4 a3 c3>`
- compact hit/rest grids like `<xxxoxxxxxxxooxxxo>`
- line comments starting with `#`

Example:

```ini
bpm = 128
bars = 4

[bass] .sustain 0.2
  [bar1] /1 <g4 a4 a3 c3>
  [bar2] /1 <a4 c5 e5 c5>

[bd]
  [bar1] /4 <0 3 5 7>
  [bar2] /4 <0 0 5 7>

[sd] .gain 0.8
  [default] /2 >> 0.25
  [bar2] /1

[hh] .pan 0.2
  [default] *4 [hh hh:2]
```

## File Structure

Top-level assignments:

```ini
bpm = 128
bars = 4
```

- `bpm` defaults to `120` when omitted
- `bars` defaults to `4` when omitted

Layers are declared first, then given one or more indented bar definitions:

```ini
[bd]
  [default] /4 <0 3 5 7>
  [bar2] /2 <0 5>
```

- `[bd]` declares the layer and its default sound target
- `[default]` plays on every bar unless a more specific `[barN]` exists
- `[barN]` defines the pattern for that specific bar in the phrase
- if a layer omits both `[default]` and a specific bar, that layer is silent on that bar
- after the last bar, the phrase loops back to `bar1`

## Pattern Forms

Patterns live on the `[barN]` line.

Atom patterns:

```ini
[hh]
  [bar1] /8 hh
```

Group patterns trigger multiple sounds in the same slot:

```ini
[drum]
  [bar1] /1 [bd sd:2]
```

Sequence patterns use angle brackets:

```ini
[bd]
  [bar1] /4 <0 3 5 7>

[bass]
  [bar1] /1 <g4 a4 a3 c3>

[bd]
  [bar1] /16 <xxxoxxxxxxxooxxxo>
```

- numeric sequence values become sample indices on the current layer sound
- sound names like `hh` or `sd:2` override the layer sound for that step
- note names like `g4`, `bf3`, and `cs5` are sent as SuperDirt `note` values
- compact grids use `o` for a hit on the layer's default target and `x` for a rest

Sequence semantics:

- atom sequences step across the bar-local slots produced by `/n` and `*n`
- note sequences subdivide each slot evenly across the listed notes

## Timing Operators

Timing operators are bar-local. They belong on `[barN]`, not on the layer header.

```ini
[bd]
  [bar1] /4 <0 3 5 7>

[hh]
  [bar1] *8 hh

[sd]
  [bar1] /2 >> 0.25
```

- `/n` divides the bar into `n` evenly spaced trigger slots
- `*n` multiplies the slot density
- `>> n` shifts later within the bar with wraparound
- `<< n` shifts earlier within the bar with wraparound

Layer-wide effect params still live on the layer line:

```ini
[hh] .gain 0.6 .pan 0.2 .speed 1.1 .sustain 0.15
  [bar1] *8 hh
```

Bar-level effect params can override the layer defaults:

```ini
[sd] .gain 0.7
  [bar1] /2 >> 0.25 .gain 0.9
```

## Runtime Voice Mapping

- `bd` plays the SuperDirt `bd` sound
- `sd` plays the SuperDirt `sd` sound
- `hh` plays the SuperDirt `hh` sound
- unknown symbols are sent through unchanged

## Comments

Use `#` for comments:

```ini
bpm = 128
[bd]
  [bar1] /4 <0 3 5 7> # kick on quarter notes
```

## Runtime Behavior Today

`fin run file.metl`:

- parses one file
- renders the current bar
- optionally plays one bar through SuperCollider

`fin watch file.metl`:

- loads the file
- plays continuously bar by bar
- applies valid edits on the next bar boundary
- keeps the last good schedule if a reload fails
