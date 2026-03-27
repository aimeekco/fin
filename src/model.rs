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
    pub tempo_changes: BTreeMap<BarSelector, TempoChange>,
    pub bars: Option<u32>,
    pub layers: Vec<Layer>,
}

impl Program {
    pub fn effective_bpm(&self) -> f32 {
        self.bpm.unwrap_or(120.0)
    }

    pub fn has_explicit_tempo(&self) -> bool {
        self.bpm.is_some() || !self.tempo_changes.is_empty()
    }

    pub fn bpm_for_bar(&self, bar_index: usize) -> f32 {
        let phrase_bar = (bar_index as u32 % self.effective_bars()) + 1;
        self.tempo_changes
            .get(&BarSelector::Exact(phrase_bar))
            .map(|change| change.bpm)
            .or_else(|| {
                self.tempo_changes
                    .iter()
                    .filter_map(|(selector, change)| match selector {
                        BarSelector::Every(divisor) if phrase_bar.is_multiple_of(*divisor) => {
                            Some((divisor, change.bpm))
                        }
                        _ => None,
                    })
                    .max_by_key(|(divisor, _)| *divisor)
                    .map(|(_, bpm)| bpm)
            })
            .unwrap_or_else(|| self.effective_bpm())
    }

    pub fn bpm_for_intro(&self, intro_index: u32) -> f32 {
        self.tempo_changes
            .get(&BarSelector::Intro(intro_index))
            .map(|change| change.bpm)
            .unwrap_or_else(|| self.effective_bpm())
    }

    pub fn effective_bars(&self) -> u32 {
        self.bars.unwrap_or(4)
    }

    pub fn intro_bar_count(&self) -> u32 {
        self.layers
            .iter()
            .map(Layer::intro_bar_count)
            .max()
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TempoChange {
    pub bpm: f32,
    pub source_line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub name: Symbol,
    pub default_target: SoundTarget,
    pub modifiers: Vec<Modifier>,
    pub bars: BTreeMap<BarSelector, BarPattern>,
    pub source_line: usize,
}

impl Layer {
    pub fn bar_for_phrase(&self, phrase_bar: u32) -> Option<&BarPattern> {
        self.bars
            .get(&BarSelector::Exact(phrase_bar))
            .or_else(|| {
                self.bars
                    .iter()
                    .filter_map(|(selector, pattern)| match selector {
                        BarSelector::Every(divisor) if phrase_bar.is_multiple_of(*divisor) => {
                            Some((divisor, pattern))
                        }
                        _ => None,
                    })
                    .max_by_key(|(divisor, _)| *divisor)
                    .map(|(_, pattern)| pattern)
            })
            .or_else(|| self.bars.get(&BarSelector::Default))
    }

    pub fn intro_bar(&self, intro_index: u32) -> Option<&BarPattern> {
        self.bars.get(&BarSelector::Intro(intro_index))
    }

    pub fn intro_bar_count(&self) -> u32 {
        self.bars
            .keys()
            .filter_map(|selector| match selector {
                BarSelector::Intro(index) => Some(*index),
                _ => None,
            })
            .max()
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum BarSelector {
    Intro(u32),
    Default,
    Every(u32),
    Exact(u32),
}

impl BarSelector {
    pub fn header_label(&self) -> String {
        match self {
            Self::Intro(1) => "[intro]".to_string(),
            Self::Intro(value) => format!("[intro{value}]"),
            Self::Default => "[default]".to_string(),
            Self::Every(value) => format!("[bar%{value}]"),
            Self::Exact(value) => format!("[bar{value}]"),
        }
    }

    pub fn detail_label(&self) -> String {
        match self {
            Self::Intro(1) => "[intro]".to_string(),
            Self::Intro(value) => format!("[intro{value}]"),
            Self::Default => "[default]".to_string(),
            Self::Every(value) => format!("[bar%{value}]"),
            Self::Exact(value) => format!("bar{value}"),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn program_with_tempo_changes(changes: &[(BarSelector, f32)]) -> Program {
        Program {
            bpm: Some(120.0),
            tempo_changes: changes
                .iter()
                .cloned()
                .map(|(selector, bpm)| {
                    (
                        selector,
                        TempoChange {
                            bpm,
                            source_line: 1,
                        },
                    )
                })
                .collect(),
            bars: Some(4),
            layers: Vec::new(),
        }
    }

    #[test]
    fn resolves_bar_bpm_with_exact_and_periodic_overrides() {
        let program = program_with_tempo_changes(&[
            (BarSelector::Every(2), 90.0),
            (BarSelector::Exact(4), 140.0),
        ]);

        assert_eq!(program.bpm_for_bar(0), 120.0);
        assert_eq!(program.bpm_for_bar(1), 90.0);
        assert_eq!(program.bpm_for_bar(3), 140.0);
        assert_eq!(program.bpm_for_bar(4), 120.0);
    }

    #[test]
    fn resolves_intro_bpm_without_affecting_loop_bars() {
        let program = program_with_tempo_changes(&[(BarSelector::Intro(1), 72.0)]);

        assert_eq!(program.bpm_for_intro(1), 72.0);
        assert_eq!(program.bpm_for_bar(0), 120.0);
    }
}
