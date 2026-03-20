use std::error::Error;
use std::fmt;

use crate::model::{
    EventParams, Meter, Modifier, PatternAtom, PatternSource, Program, ScheduledEvent, SoundTarget,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleError {
    message: String,
}

impl ScheduleError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}

impl Error for ScheduleError {}

pub fn schedule_bar(
    program: &Program,
    meter: Meter,
    bar_index: usize,
) -> Result<Vec<ScheduledEvent>, ScheduleError> {
    let mut events = Vec::new();

    for layer in &program.layers {
        let mut divide = 1u32;
        let mut multiply = 1u32;
        let mut shift = 0.0f32;
        let mut params = EventParams::default();

        for modifier in &layer.modifiers {
            match modifier {
                Modifier::Divide(value) => divide = *value,
                Modifier::Multiply(value) => {
                    multiply = multiply
                        .checked_mul(*value)
                        .ok_or_else(|| ScheduleError::new("density overflowed supported range"))?
                }
                Modifier::Shift(value) => shift += *value,
                Modifier::Gain(value) => params.gain = Some(*value),
                Modifier::Pan(value) => params.pan = Some(*value),
                Modifier::Speed(value) => params.speed = Some(*value),
                Modifier::Sustain(value) => params.sustain = Some(*value),
            }
        }

        if divide == 0 {
            return Err(ScheduleError::new("divide must be greater than zero"));
        }

        let slots = divide
            .checked_mul(multiply)
            .ok_or_else(|| ScheduleError::new("slot count overflowed supported range"))?;
        let targets = resolve_pattern_targets(layer, bar_index)?;

        for slot in 0..slots {
            let base_bar_pos = slot as f32 / slots as f32;
            let bar_pos = (base_bar_pos + shift).rem_euclid(1.0);
            let beat_pos = meter.beats_per_bar as f32 * bar_pos;
            for sound in &targets {
                events.push(ScheduledEvent {
                    layer: layer.name.clone(),
                    sound: sound.clone(),
                    bar_pos,
                    beat_pos,
                    params,
                });
            }
        }
    }

    events.sort_by(|left, right| {
        left.bar_pos
            .total_cmp(&right.bar_pos)
            .then_with(|| left.layer.cmp(&right.layer))
            .then_with(|| left.sound.cmp(&right.sound))
    });

    Ok(events)
}

pub fn format_events(program: &Program, events: &[ScheduledEvent]) -> String {
    let mut lines = Vec::new();

    if let Some(bpm) = program.bpm {
        if bpm.fract() == 0.0 {
            lines.push(format!("bpm={:.0}", bpm));
        } else {
            lines.push(format!("bpm={:.3}", bpm));
        }
    }

    lines.extend(events.iter().map(|event| {
        format!(
            "{}  beat={:.3}  bar={:.3}",
            event.sound.display_name(),
            event.beat_pos,
            event.bar_pos
        )
    }));

    lines.join("\n")
}

fn resolve_pattern_targets(
    layer: &crate::model::Layer,
    bar_index: usize,
) -> Result<Vec<SoundTarget>, ScheduleError> {
    match &layer.pattern {
        PatternSource::ImplicitSelf => Ok(vec![layer.default_target.clone()]),
        PatternSource::Atom(atom) => Ok(vec![resolve_atom(atom, &layer.default_target)?]),
        PatternSource::Group(atoms) => atoms
            .iter()
            .map(|atom| resolve_atom(atom, &layer.default_target))
            .collect(),
        PatternSource::Cycle(atoms) => {
            let Some(atom) = atoms.get(bar_index % atoms.len()) else {
                return Err(ScheduleError::new("cycle pattern cannot be empty"));
            };
            Ok(vec![resolve_atom(atom, &layer.default_target)?])
        }
    }
}

fn resolve_atom(
    atom: &PatternAtom,
    default_target: &SoundTarget,
) -> Result<SoundTarget, ScheduleError> {
    match atom {
        PatternAtom::SampleIndex(index) => Ok(default_target.with_index(Some(*index))),
        PatternAtom::Sound(sound) => Ok(sound.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Layer, Modifier, PatternAtom, PatternSource, Program, SoundTarget, Symbol};

    fn make_program(modifiers: Vec<Modifier>) -> Program {
        Program {
            bpm: Some(128.0),
            layers: vec![Layer {
                name: Symbol("bd".to_string()),
                default_target: SoundTarget {
                    name: "bd".to_string(),
                    index: None,
                },
                pattern: PatternSource::ImplicitSelf,
                modifiers,
                source_line: 1,
            }],
        }
    }

    #[test]
    fn schedules_quarter_notes_in_four_four() {
        let events = schedule_bar(
            &make_program(vec![Modifier::Divide(4)]),
            Meter::default(),
            0,
        )
        .expect("schedule should work");
        let beats: Vec<f32> = events.iter().map(|event| event.beat_pos).collect();
        assert_eq!(beats, vec![0.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn schedules_half_notes() {
        let events = schedule_bar(
            &make_program(vec![Modifier::Divide(2)]),
            Meter::default(),
            0,
        )
        .expect("schedule should work");
        let beats: Vec<f32> = events.iter().map(|event| event.beat_pos).collect();
        assert_eq!(beats, vec![0.0, 2.0]);
    }

    #[test]
    fn multiplies_density_within_the_bar() {
        let events = schedule_bar(
            &make_program(vec![Modifier::Divide(2), Modifier::Multiply(2)]),
            Meter::default(),
            0,
        )
        .expect("schedule should work");
        let beats: Vec<f32> = events.iter().map(|event| event.beat_pos).collect();
        assert_eq!(beats, vec![0.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn shifts_events_right_with_wraparound() {
        let events = schedule_bar(
            &make_program(vec![Modifier::Divide(2), Modifier::Shift(0.25)]),
            Meter::default(),
            0,
        )
        .expect("schedule should work");
        let beats: Vec<f32> = events.iter().map(|event| event.beat_pos).collect();
        assert_eq!(beats, vec![1.0, 3.0]);
    }

    #[test]
    fn shifts_events_left_with_wraparound() {
        let events = schedule_bar(
            &make_program(vec![Modifier::Divide(4), Modifier::Shift(-0.125)]),
            Meter::default(),
            0,
        )
        .expect("schedule should work");
        let beats: Vec<f32> = events.iter().map(|event| event.beat_pos).collect();
        assert_eq!(beats, vec![0.5, 1.5, 2.5, 3.5]);
    }

    #[test]
    fn formats_output_stably() {
        let program = make_program(vec![Modifier::Divide(2)]);
        let events = schedule_bar(&program, Meter::default(), 0).expect("schedule should work");
        let output = format_events(&program, &events);
        assert_eq!(
            output,
            "bpm=128\nbd  beat=0.000  bar=0.000\nbd  beat=2.000  bar=0.500"
        );
    }

    #[test]
    fn carries_effect_parameters_into_events() {
        let events = schedule_bar(
            &make_program(vec![
                Modifier::Divide(1),
                Modifier::Gain(0.5),
                Modifier::Pan(-0.25),
                Modifier::Speed(1.5),
                Modifier::Sustain(0.2),
            ]),
            Meter::default(),
            0,
        )
        .expect("schedule should work");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].params.gain, Some(0.5));
        assert_eq!(events[0].params.pan, Some(-0.25));
        assert_eq!(events[0].params.speed, Some(1.5));
        assert_eq!(events[0].params.sustain, Some(0.2));
    }

    #[test]
    fn cycle_pattern_advances_by_bar_index() {
        let program = Program {
            bpm: Some(128.0),
            layers: vec![Layer {
                name: Symbol("bd".to_string()),
                default_target: SoundTarget {
                    name: "bd".to_string(),
                    index: None,
                },
                pattern: PatternSource::Cycle(vec![
                    PatternAtom::SampleIndex(0),
                    PatternAtom::SampleIndex(3),
                ]),
                modifiers: vec![Modifier::Divide(1)],
                source_line: 1,
            }],
        };

        let first_bar = schedule_bar(&program, Meter::default(), 0).expect("schedule should work");
        let second_bar = schedule_bar(&program, Meter::default(), 1).expect("schedule should work");

        assert_eq!(first_bar[0].sound.display_name(), "bd:0");
        assert_eq!(second_bar[0].sound.display_name(), "bd:3");
    }

    #[test]
    fn group_pattern_creates_multiple_events_in_one_slot() {
        let program = Program {
            bpm: Some(128.0),
            layers: vec![Layer {
                name: Symbol("drum".to_string()),
                default_target: SoundTarget {
                    name: "drum".to_string(),
                    index: None,
                },
                pattern: PatternSource::Group(vec![
                    PatternAtom::Sound(SoundTarget {
                        name: "bd".to_string(),
                        index: None,
                    }),
                    PatternAtom::Sound(SoundTarget {
                        name: "sd".to_string(),
                        index: Some(2),
                    }),
                ]),
                modifiers: vec![Modifier::Divide(1)],
                source_line: 1,
            }],
        };

        let events = schedule_bar(&program, Meter::default(), 0).expect("schedule should work");
        let labels: Vec<String> = events
            .iter()
            .map(|event| event.sound.display_name())
            .collect();
        assert_eq!(labels, vec!["bd".to_string(), "sd:2".to_string()]);
    }
}
