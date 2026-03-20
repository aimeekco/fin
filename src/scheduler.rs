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
        let mut divide = 1u32;
        let mut multiply = 1u32;
        let mut shift = 0.0f32;

        for modifier in &layer.modifiers {
            match modifier {
                Modifier::Divide(value) => divide = *value,
                Modifier::Multiply(value) => {
                    multiply = multiply
                        .checked_mul(*value)
                        .ok_or_else(|| ScheduleError::new("density overflowed supported range"))?
                }
                Modifier::Shift(value) => shift += *value,
            }
        }

        if divide == 0 {
            return Err(ScheduleError::new("divide must be greater than zero"));
        }

        let slots = divide
            .checked_mul(multiply)
            .ok_or_else(|| ScheduleError::new("slot count overflowed supported range"))?;

        for slot in 0..slots {
            let base_bar_pos = slot as f32 / slots as f32;
            let bar_pos = (base_bar_pos + shift).rem_euclid(1.0);
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

    fn make_program(modifiers: Vec<Modifier>) -> Program {
        Program {
            bpm: Some(128.0),
            layers: vec![Layer {
                name: Symbol("bd".to_string()),
                pattern: PatternSource::ImplicitSelf,
                modifiers,
                source_line: 1,
            }],
        }
    }

    #[test]
    fn schedules_quarter_notes_in_four_four() {
        let events = schedule_bar(&make_program(vec![Modifier::Divide(4)]), Meter::default())
            .expect("schedule should work");
        let beats: Vec<f32> = events.iter().map(|event| event.beat_pos).collect();
        assert_eq!(beats, vec![0.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn schedules_half_notes() {
        let events = schedule_bar(&make_program(vec![Modifier::Divide(2)]), Meter::default())
            .expect("schedule should work");
        let beats: Vec<f32> = events.iter().map(|event| event.beat_pos).collect();
        assert_eq!(beats, vec![0.0, 2.0]);
    }

    #[test]
    fn multiplies_density_within_the_bar() {
        let events = schedule_bar(
            &make_program(vec![Modifier::Divide(2), Modifier::Multiply(2)]),
            Meter::default(),
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
        )
        .expect("schedule should work");
        let beats: Vec<f32> = events.iter().map(|event| event.beat_pos).collect();
        assert_eq!(beats, vec![0.5, 1.5, 2.5, 3.5]);
    }

    #[test]
    fn formats_output_stably() {
        let program = make_program(vec![Modifier::Divide(2)]);
        let events = schedule_bar(&program, Meter::default()).expect("schedule should work");
        let output = format_events(&program, &events);
        assert_eq!(
            output,
            "bpm=128\nbd  beat=0.000  bar=0.000\nbd  beat=2.000  bar=0.500"
        );
    }
}
