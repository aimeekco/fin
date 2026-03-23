use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Symbol(pub String);

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub bpm: Option<f32>,
    pub bars: Option<u32>,
    pub layers: Vec<Layer>,
}

impl Program {
    pub fn effective_bpm(&self) -> f32 {
        self.bpm.unwrap_or(120.0)
    }

    pub fn effective_bars(&self) -> u32 {
        self.bars.unwrap_or(4)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub name: Symbol,
    pub default_target: SoundTarget,
    pub modifiers: Vec<Modifier>,
    pub bars: BTreeMap<u32, BarPattern>,
    pub source_line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BarPattern {
    pub pattern: PatternSource,
    pub modifiers: Vec<Modifier>,
    pub source_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SoundTarget {
    pub name: String,
    pub index: Option<i32>,
}

impl SoundTarget {
    pub fn display_name(&self) -> String {
        match self.index {
            Some(index) => format!("{}:{index}", self.name),
            None => self.name.clone(),
        }
    }

    pub fn with_index(&self, index: Option<i32>) -> Self {
        Self {
            name: self.name.clone(),
            index,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatternSource {
    ImplicitSelf,
    Atom(PatternAtom),
    Group(Vec<PatternAtom>),
    Sequence(Vec<PatternValue>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatternValue {
    Hit,
    Rest,
    Atom(PatternAtom),
    Note(NoteValue),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternAtom {
    SampleIndex(i32),
    Sound(SoundTarget),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NoteValue {
    pub label: String,
    pub semitone: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Modifier {
    Divide(u32),
    Multiply(u32),
    Shift(f32),
    Gain(f32),
    Pan(f32),
    Speed(f32),
    Sustain(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Meter {
    pub beats_per_bar: u32,
    pub beat_unit: u32,
}

impl Default for Meter {
    fn default() -> Self {
        Self {
            beats_per_bar: 4,
            beat_unit: 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledEvent {
    pub layer: Symbol,
    pub sound: SoundTarget,
    pub bar_pos: f32,
    pub beat_pos: f32,
    pub params: EventParams,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EventParams {
    pub gain: Option<f32>,
    pub pan: Option<f32>,
    pub speed: Option<f32>,
    pub sustain: Option<f32>,
    pub note: Option<f32>,
    pub note_label: Option<String>,
}
