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
    pub layers: Vec<Layer>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub name: Symbol,
    pub pattern: PatternSource,
    pub modifiers: Vec<Modifier>,
    pub source_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternSource {
    ImplicitSelf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modifier {
    Divide(u32),
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
    pub bar_pos: f32,
    pub beat_pos: f32,
}
