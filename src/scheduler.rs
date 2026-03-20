use std::error::Error;
use std::fmt;

use crate::model::{Meter, Modifier, Program, ScheduledEvent};

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

pub fn schedule_bar(program: &Program, meter: Meter) -> Result<Vec<ScheduledEvent>, ScheduleError> {
    let mut events = Vec::new();

    for layer in &program.layers {
        let divide = layer
            .modifiers
            .iter()
            .find_map(|modifier| match modifier {
                Modifier::Divide(value) => Some(*value),
            })
            .unwrap_or(1);

        if divide == 0 {
            return Err(ScheduleError::new("divide must be greater than zero"));
        }

        for slot in 0..divide {
            let bar_pos = slot as f32 / divide as f32;
            let beat_pos = meter.beats_per_bar as f32 * bar_pos;
            events.push(ScheduledEvent {
                layer: layer.name.clone(),
                bar_pos,
                beat_pos,
            });
        }
    }

    events.sort_by(|left, right| {
        left.bar_pos
            .total_cmp(&right.bar_pos)
            .then_with(|| left.layer.cmp(&right.layer))
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
            event.layer, event.beat_pos, event.bar_pos
        )
    }));

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Layer, Modifier, PatternSource, Program, Symbol};

    fn make_program(divide: u32) -> Program {
        Program {
            bpm: Some(128.0),
            layers: vec![Layer {
                name: Symbol("bd".to_string()),
                pattern: PatternSource::ImplicitSelf,
                modifiers: vec![Modifier::Divide(divide)],
                source_line: 1,
            }],
        }
    }

    #[test]
    fn schedules_quarter_notes_in_four_four() {
        let events =
            schedule_bar(&make_program(4), Meter::default()).expect("schedule should work");
        let beats: Vec<f32> = events.iter().map(|event| event.beat_pos).collect();
        assert_eq!(beats, vec![0.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn schedules_half_notes() {
        let events =
            schedule_bar(&make_program(2), Meter::default()).expect("schedule should work");
        let beats: Vec<f32> = events.iter().map(|event| event.beat_pos).collect();
        assert_eq!(beats, vec![0.0, 2.0]);
    }

    #[test]
    fn formats_output_stably() {
        let program = make_program(2);
        let events = schedule_bar(&program, Meter::default()).expect("schedule should work");
        let output = format_events(&program, &events);
        assert_eq!(
            output,
            "bpm=128\nbd  beat=0.000  bar=0.000\nbd  beat=2.000  bar=0.500"
        );
    }
}
