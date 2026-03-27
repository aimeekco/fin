# METL Notation

METL is a layer-based live-coding language with a global phrase length and per-layer bar overrides.

## Current Implemented Subset

The current parser supports:

- `bpm = <number>`
- `bpm [intro] = <number>`, `bpm [intro2] = <number>`, `bpm [barN] = <number>`, and `bpm [bar%N] = <number>`
- `bars = <positive integer>` with a default of `4`
- layer headers like `[bd]` and `[sd:2]`
- optional layer-wide effect params: `.gain`, `.pan`, `.speed`, `.sustain`
- indented one-time intro entries like `[intro]`, `[intro2]`, ...
- indented per-layer fallback entries like `[default]`
- indented periodic entries like `[bar%4]`
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
  [bar%4] /4 <0 2 4 6>

[hh] .pan 0.2
  [default] *4 [hh hh:2]
```

## File Structure

Top-level assignments:

```ini
bpm = 128
bpm [intro] = 96
bpm [bar2] = 140
bars = 4
```

- `bpm` defaults to `120` when omitted
- `bpm = ...` sets the base tempo for the whole file
- scoped `bpm [...] = ...` lines override the tempo for matching intro bars or loop bars
- tempo selector precedence matches bar selection precedence: exact `[barN]` beats periodic `[bar%N]`
- intro tempo selectors must be contiguous when numbered: `[intro]`, `[intro2]`, `[intro3]`, ...
- use bare `bpm = ...` for the default tempo; `bpm [default] = ...` is not supported
- `bars` defaults to `4` when omitted

Layers are declared first, then given one or more indented bar definitions:

```ini
[bd]
  [intro] /1 <7>
  [intro2] /1 <3 5>
  [default] /4 <0 3 5 7>
  [bar%4] /8 <0 7 0 7 0 7 0 7>
  [bar2] /2 <0 5>
```

- `[bd]` declares the layer and its default sound target
- `[intro]` plays once before `bar1` on the first pass, then never loops
- `[intro2]`, `[intro3]`, ... play once in ascending order after `[intro]` and before `bar1`
- `[default]` plays on every bar unless a more specific bar selector exists
- `[bar%N]` plays on bars divisible by `N`, so `[bar%4]` applies on bars `4, 8, 12, ...`
- `[barN]` defines the pattern for that specific bar in the phrase
- startup order is `[intro]`, `[intro2]`, ..., then the loop begins at `bar1`
- loop precedence after the intro is `[barN]` > `[bar%N]` > `[default]`
- if multiple periodic selectors match, the largest `N` wins
- intro numbering must be contiguous per layer: `[intro]`, `[intro2]`, `[intro3]`, ...
- if a layer has no matching intro bar for a given startup step, or no matching `[barN]`, `[bar%N]`, or `[default]` during the loop, that layer is silent on that bar
- after the last bar, the phrase loops back to `bar1`

## Pattern Forms

Patterns live on the `[intro]`, `[introN]`, `[default]`, `[bar%N]`, or `[barN]` line.

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

- atom and hit/rest sequences step across the bar-local slots produced by `/n` and `*n`
- if an atom or hit/rest sequence omits `/n` and `*n`, it defaults to one slot per sequence value
- note sequences subdivide each slot evenly across the listed notes

## Timing Operators

Timing operators are bar-local. They belong on `[intro]`, `[introN]`, `[default]`, `[bar%N]`, or `[barN]`, not on the layer header.

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
