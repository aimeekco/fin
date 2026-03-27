use std::error::Error;
use std::fmt;

use crate::model::{
    BarPattern, EventParams, Layer, Meter, Modifier, PatternAtom, PatternSource, PatternValue,
    Program, ScheduledEvent, SoundTarget,
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
    let phrase_bar = (bar_index as u32 % program.effective_bars()) + 1;
    schedule_selected_bars(program, meter, |layer| layer.bar_for_phrase(phrase_bar))
}

pub fn schedule_intro(
    program: &Program,
    meter: Meter,
    intro_index: u32,
) -> Result<Vec<ScheduledEvent>, ScheduleError> {
    schedule_selected_bars(program, meter, |layer| layer.intro_bar(intro_index))
}

fn schedule_selected_bars<'a, F>(
    program: &'a Program,
    meter: Meter,
    mut select_bar: F,
) -> Result<Vec<ScheduledEvent>, ScheduleError>
where
    F: FnMut(&'a Layer) -> Option<&'a BarPattern>,
{
    let mut events = Vec::new();

    for layer in &program.layers {
        let Some(bar) = select_bar(layer) else {
            continue;
        };

        let mut divide = 1u32;
        let mut multiply = 1u32;
        let mut shift = 0.0f32;
        let mut has_explicit_density = false;
        let mut params = collect_layer_params(&layer.modifiers)?;

        for modifier in &bar.modifiers {
            match modifier {
                Modifier::Divide(value) => {
                    divide = *value;
                    has_explicit_density = true;
                }
                Modifier::Multiply(value) => {
                    has_explicit_density = true;
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

        let slots = divide
            .checked_mul(multiply)
            .ok_or_else(|| ScheduleError::new("slot count overflowed supported range"))?;

        match &bar.pattern {
            PatternSource::ImplicitSelf | PatternSource::Atom(_) | PatternSource::Group(_) => {
                let targets = resolve_pattern_targets(layer, bar)?;
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
                            params: params.clone(),
                        });
                    }
                }
            }
            PatternSource::Sequence(values) => schedule_sequence(
                &mut events,
                layer,
                values,
                slots,
                shift,
                params.clone(),
                meter,
                has_explicit_density,
            )?,
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

pub fn format_events(program: &Program, bpm: f32, events: &[ScheduledEvent]) -> String {
    let mut lines = Vec::new();

    if program.has_explicit_tempo() {
        if bpm.fract() == 0.0 {
            lines.push(format!("bpm={:.0}", bpm));
        } else {
            lines.push(format!("bpm={:.3}", bpm));
        }
    }

    if let Some(bars) = program.bars {
        lines.push(format!("bars={bars}"));
    }

    lines.extend(events.iter().map(|event| {
        format!(
            "{}  beat={:.3}  bar={:.3}",
            event_label(event),
            event.beat_pos,
            event.bar_pos
        )
    }));

    lines.join("\n")
}

fn event_label(event: &ScheduledEvent) -> String {
    match &event.params.note_label {
        Some(note) => format!("{}@{note}", event.sound.display_name()),
        None => event.sound.display_name(),
    }
}

fn collect_layer_params(modifiers: &[Modifier]) -> Result<EventParams, ScheduleError> {
    let mut params = EventParams::default();
    for modifier in modifiers {
        match modifier {
            Modifier::Gain(value) => params.gain = Some(*value),
            Modifier::Pan(value) => params.pan = Some(*value),
            Modifier::Speed(value) => params.speed = Some(*value),
            Modifier::Sustain(value) => params.sustain = Some(*value),
            Modifier::Divide(_) | Modifier::Multiply(_) | Modifier::Shift(_) => {
                return Err(ScheduleError::new(
                    "rhythmic modifiers are only allowed inside bar entries",
                ));
            }
        }
    }
    Ok(params)
}

fn resolve_pattern_targets(
    layer: &Layer,
    bar: &BarPattern,
) -> Result<Vec<SoundTarget>, ScheduleError> {
    match &bar.pattern {
        PatternSource::ImplicitSelf => Ok(vec![layer.default_target.clone()]),
        PatternSource::Atom(atom) => Ok(vec![resolve_atom(atom, &layer.default_target)?]),
        PatternSource::Group(atoms) => atoms
            .iter()
            .map(|atom| resolve_atom(atom, &layer.default_target))
            .collect(),
        PatternSource::Sequence(_) => Err(ScheduleError::new(
            "sequence patterns are expanded directly during scheduling",
        )),
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

fn schedule_sequence(
    events: &mut Vec<ScheduledEvent>,
    layer: &Layer,
    values: &[PatternValue],
    default_slots: u32,
    shift: f32,
    params: EventParams,
    meter: Meter,
    has_explicit_density: bool,
) -> Result<(), ScheduleError> {
    if values.is_empty() {
        return Err(ScheduleError::new("sequence pattern cannot be empty"));
    }

    let all_notes = values
        .iter()
        .all(|value| matches!(value, PatternValue::Note(_) | PatternValue::Rest));
    let all_atoms = values.iter().all(|value| {
        matches!(
            value,
            PatternValue::Atom(_) | PatternValue::Hit | PatternValue::Rest
        )
    });

    if all_notes {
        return schedule_note_sequence(events, layer, values, default_slots, shift, params, meter);
    }

    if all_atoms {
        let slots = if has_explicit_density {
            default_slots
        } else {
            values.len() as u32
        };

        for slot in 0..slots {
            let base_bar_pos = slot as f32 / slots as f32;
            let bar_pos = (base_bar_pos + shift).rem_euclid(1.0);
            let beat_pos = meter.beats_per_bar as f32 * bar_pos;
            let sound = match &values[slot as usize % values.len()] {
                PatternValue::Hit => layer.default_target.clone(),
                PatternValue::Rest => continue,
                PatternValue::Atom(atom) => resolve_atom(atom, &layer.default_target)?,
                PatternValue::Note(_) => unreachable!("all values are atoms"),
            };
            events.push(ScheduledEvent {
                layer: layer.name.clone(),
                sound,
                bar_pos,
                beat_pos,
                params: params.clone(),
            });
        }
        return Ok(());
    }

    Err(ScheduleError::new(
        "sequence pattern cannot mix note values and sample targets",
    ))
}

fn schedule_note_sequence(
    events: &mut Vec<ScheduledEvent>,
    layer: &Layer,
    values: &[PatternValue],
    slots: u32,
    shift: f32,
    params: EventParams,
    meter: Meter,
) -> Result<(), ScheduleError> {
    if values.is_empty() {
        return Err(ScheduleError::new("note sequence cannot be empty"));
    }

    for slot in 0..slots {
        let slot_start = slot as f32 / slots as f32;
        let slot_width = 1.0 / slots as f32;

        for (index, value) in values.iter().enumerate() {
            let subdivision = index as f32 / values.len() as f32;
            let bar_pos = (slot_start + subdivision * slot_width + shift).rem_euclid(1.0);
            let beat_pos = meter.beats_per_bar as f32 * bar_pos;
            let PatternValue::Note(note) = value else {
                continue;
            };
            let mut note_params = params.clone();
            note_params.note = Some(note.semitone);
            note_params.note_label = Some(note.label.clone());

            events.push(ScheduledEvent {
                layer: layer.name.clone(),
                sound: layer.default_target.clone(),
                bar_pos,
                beat_pos,
                params: note_params,
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::model::{
        BarPattern, BarSelector, NoteValue, PatternValue, Program, SoundTarget, Symbol,
    };

    fn bar(pattern: PatternSource, modifiers: Vec<Modifier>) -> BarPattern {
        BarPattern {
            pattern,
            modifiers,
            source_line: 1,
        }
    }

    fn layer(name: &str, bars: &[(BarSelector, BarPattern)]) -> Layer {
        Layer {
            name: Symbol(name.to_string()),
            default_target: SoundTarget {
                name: name.to_string(),
                index: None,
            },
            modifiers: Vec::new(),
            bars: bars.iter().cloned().collect::<BTreeMap<_, _>>(),
            source_line: 1,
        }
    }

    #[test]
    fn schedules_atom_sequence_across_bar_slots() {
        let program = Program {
            bpm: Some(128.0),
            tempo_changes: BTreeMap::new(),
            bars: Some(4),
            layers: vec![layer(
                "bd",
                &[(
                    BarSelector::Exact(1),
                    bar(
                        PatternSource::Sequence(vec![
                            PatternValue::Atom(PatternAtom::SampleIndex(0)),
                            PatternValue::Atom(PatternAtom::SampleIndex(3)),
                            PatternValue::Atom(PatternAtom::SampleIndex(5)),
                            PatternValue::Atom(PatternAtom::SampleIndex(7)),
                        ]),
                        vec![Modifier::Divide(4)],
                    ),
                )],
            )],
        };

        let events = schedule_bar(&program, Meter::default(), 0).expect("schedule should work");
        let labels: Vec<String> = events
            .iter()
            .map(|event| event.sound.display_name())
            .collect();
        assert_eq!(labels, vec!["bd:0", "bd:3", "bd:5", "bd:7"]);
    }

    #[test]
    fn infers_slot_count_for_atom_sequence_without_density_modifier() {
        let program = Program {
            bpm: Some(128.0),
            tempo_changes: BTreeMap::new(),
            bars: Some(4),
            layers: vec![layer(
                "bd",
                &[(
                    BarSelector::Exact(1),
                    bar(
                        PatternSource::Sequence(vec![
                            PatternValue::Atom(PatternAtom::SampleIndex(0)),
                            PatternValue::Atom(PatternAtom::SampleIndex(3)),
                            PatternValue::Atom(PatternAtom::SampleIndex(5)),
                            PatternValue::Atom(PatternAtom::SampleIndex(7)),
                        ]),
                        vec![],
                    ),
                )],
            )],
        };

        let events = schedule_bar(&program, Meter::default(), 0).expect("schedule should work");
        let beats: Vec<f32> = events.iter().map(|event| event.beat_pos).collect();
        let labels: Vec<String> = events
            .iter()
            .map(|event| event.sound.display_name())
            .collect();

        assert_eq!(beats, vec![0.0, 1.0, 2.0, 3.0]);
        assert_eq!(labels, vec!["bd:0", "bd:3", "bd:5", "bd:7"]);
    }

    #[test]
    fn schedules_note_sequence_within_selected_bar() {
        let program = Program {
            bpm: Some(120.0),
            tempo_changes: BTreeMap::new(),
            bars: Some(4),
            layers: vec![layer(
                "bass",
                &[(
                    BarSelector::Exact(1),
                    bar(
                        PatternSource::Sequence(vec![
                            PatternValue::Note(NoteValue {
                                label: "g4".to_string(),
                                semitone: -5.0,
                            }),
                            PatternValue::Note(NoteValue {
                                label: "a4".to_string(),
                                semitone: -3.0,
                            }),
                        ]),
                        vec![Modifier::Divide(1)],
                    ),
                )],
            )],
        };

        let events = schedule_bar(&program, Meter::default(), 0).expect("schedule should work");
        let beats: Vec<f32> = events.iter().map(|event| event.beat_pos).collect();
        assert_eq!(beats, vec![0.0, 2.0]);
    }

    #[test]
    fn schedules_compact_hit_rest_grid() {
        let program = Program {
            bpm: Some(120.0),
            tempo_changes: BTreeMap::new(),
            bars: Some(1),
            layers: vec![layer(
                "bd",
                &[(
                    BarSelector::Exact(1),
                    bar(
                        PatternSource::Sequence(vec![
                            PatternValue::Rest,
                            PatternValue::Rest,
                            PatternValue::Rest,
                            PatternValue::Hit,
                            PatternValue::Rest,
                            PatternValue::Rest,
                            PatternValue::Rest,
                            PatternValue::Rest,
                            PatternValue::Rest,
                            PatternValue::Rest,
                            PatternValue::Rest,
                            PatternValue::Hit,
                            PatternValue::Hit,
                            PatternValue::Rest,
                            PatternValue::Rest,
                            PatternValue::Rest,
                            PatternValue::Hit,
                        ]),
                        vec![Modifier::Divide(17)],
                    ),
                )],
            )],
        };

        let events = schedule_bar(&program, Meter::default(), 0).expect("schedule should work");
        let beats: Vec<f32> = events.iter().map(|event| event.beat_pos).collect();
        assert_eq!(
            beats,
            vec![0.705_882_4, 2.588_235_4, 2.823_529_5, 3.764_706]
        );
    }

    #[test]
    fn skips_rests_in_note_sequences_without_collapsing_timing() {
        let program = Program {
            bpm: Some(120.0),
            tempo_changes: BTreeMap::new(),
            bars: Some(1),
            layers: vec![layer(
                "bass",
                &[(
                    BarSelector::Exact(1),
                    bar(
                        PatternSource::Sequence(vec![
                            PatternValue::Note(NoteValue {
                                label: "g4".to_string(),
                                semitone: -5.0,
                            }),
                            PatternValue::Rest,
                            PatternValue::Note(NoteValue {
                                label: "a4".to_string(),
                                semitone: -3.0,
                            }),
                            PatternValue::Rest,
                        ]),
                        vec![Modifier::Divide(1)],
                    ),
                )],
            )],
        };

        let events = schedule_bar(&program, Meter::default(), 0).expect("schedule should work");
        let beats: Vec<f32> = events.iter().map(|event| event.beat_pos).collect();
        let labels: Vec<String> = events
            .iter()
            .map(|event| event.params.note_label.clone().unwrap_or_default())
            .collect();
        assert_eq!(beats, vec![0.0, 2.0]);
        assert_eq!(labels, vec!["g4", "a4"]);
    }

    #[test]
    fn missing_bar_definition_is_silent() {
        let program = Program {
            bpm: Some(128.0),
            tempo_changes: BTreeMap::new(),
            bars: Some(4),
            layers: vec![layer(
                "bd",
                &[(
                    BarSelector::Exact(1),
                    bar(PatternSource::ImplicitSelf, vec![Modifier::Divide(1)]),
                )],
            )],
        };

        let events = schedule_bar(&program, Meter::default(), 1).expect("schedule should work");
        assert!(events.is_empty());
    }

    #[test]
    fn intro_bar_definition_applies_only_during_intro_schedule() {
        let program = Program {
            bpm: Some(128.0),
            tempo_changes: BTreeMap::new(),
            bars: Some(4),
            layers: vec![layer(
                "bd",
                &[(
                    BarSelector::Intro(1),
                    bar(
                        PatternSource::Atom(PatternAtom::SampleIndex(8)),
                        vec![Modifier::Divide(1)],
                    ),
                )],
            )],
        };

        let intro = schedule_intro(&program, Meter::default(), 1).expect("schedule should work");
        let first_bar = schedule_bar(&program, Meter::default(), 0).expect("schedule should work");
        let looped_bar = schedule_bar(&program, Meter::default(), 4).expect("schedule should work");

        assert_eq!(intro.len(), 1);
        assert_eq!(intro[0].sound.display_name(), "bd:8");
        assert!(first_bar.is_empty());
        assert!(looped_bar.is_empty());
    }

    #[test]
    fn numbered_intro_bar_definition_applies_only_for_matching_intro_index() {
        let program = Program {
            bpm: Some(128.0),
            tempo_changes: BTreeMap::new(),
            bars: Some(4),
            layers: vec![layer(
                "bd",
                &[
                    (
                        BarSelector::Intro(1),
                        bar(
                            PatternSource::Atom(PatternAtom::SampleIndex(8)),
                            vec![Modifier::Divide(1)],
                        ),
                    ),
                    (
                        BarSelector::Intro(2),
                        bar(
                            PatternSource::Atom(PatternAtom::SampleIndex(9)),
                            vec![Modifier::Divide(1)],
                        ),
                    ),
                ],
            )],
        };

        let intro1 = schedule_intro(&program, Meter::default(), 1).expect("schedule should work");
        let intro2 = schedule_intro(&program, Meter::default(), 2).expect("schedule should work");

        assert_eq!(intro1[0].sound.display_name(), "bd:8");
        assert_eq!(intro2[0].sound.display_name(), "bd:9");
    }

    #[test]
    fn exact_bar_definition_applies_after_intro_schedule() {
        let program = Program {
            bpm: Some(128.0),
            tempo_changes: BTreeMap::new(),
            bars: Some(4),
            layers: vec![layer(
                "bd",
                &[
                    (
                        BarSelector::Intro(1),
                        bar(
                            PatternSource::Atom(PatternAtom::SampleIndex(8)),
                            vec![Modifier::Divide(1)],
                        ),
                    ),
                    (
                        BarSelector::Exact(1),
                        bar(
                            PatternSource::Atom(PatternAtom::SampleIndex(1)),
                            vec![Modifier::Divide(1)],
                        ),
                    ),
                ],
            )],
        };

        let intro = schedule_intro(&program, Meter::default(), 1).expect("schedule should work");
        let first_bar = schedule_bar(&program, Meter::default(), 0).expect("schedule should work");
        let second_cycle_first_bar =
            schedule_bar(&program, Meter::default(), 4).expect("schedule should work");

        assert_eq!(intro[0].sound.display_name(), "bd:8");
        assert_eq!(first_bar[0].sound.display_name(), "bd:1");
        assert_eq!(second_cycle_first_bar[0].sound.display_name(), "bd:1");
    }

    #[test]
    fn default_bar_definition_applies_when_specific_bar_is_missing() {
        let program = Program {
            bpm: Some(128.0),
            tempo_changes: BTreeMap::new(),
            bars: Some(4),
            layers: vec![layer(
                "bd",
                &[(
                    BarSelector::Default,
                    bar(
                        PatternSource::Atom(PatternAtom::SampleIndex(7)),
                        vec![Modifier::Divide(1)],
                    ),
                )],
            )],
        };

        let events = schedule_bar(&program, Meter::default(), 2).expect("schedule should work");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sound.display_name(), "bd:7");
    }

    #[test]
    fn specific_bar_overrides_default_bar_definition() {
        let program = Program {
            bpm: Some(128.0),
            tempo_changes: BTreeMap::new(),
            bars: Some(4),
            layers: vec![layer(
                "bd",
                &[
                    (
                        BarSelector::Default,
                        bar(
                            PatternSource::Atom(PatternAtom::SampleIndex(7)),
                            vec![Modifier::Divide(1)],
                        ),
                    ),
                    (
                        BarSelector::Exact(2),
                        bar(
                            PatternSource::Atom(PatternAtom::SampleIndex(3)),
                            vec![Modifier::Divide(1)],
                        ),
                    ),
                ],
            )],
        };

        let events = schedule_bar(&program, Meter::default(), 1).expect("schedule should work");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sound.display_name(), "bd:3");
    }

    #[test]
    fn periodic_bar_definition_applies_on_matching_phrase_bars() {
        let program = Program {
            bpm: Some(128.0),
            tempo_changes: BTreeMap::new(),
            bars: Some(8),
            layers: vec![layer(
                "bd",
                &[(
                    BarSelector::Every(4),
                    bar(
                        PatternSource::Atom(PatternAtom::SampleIndex(7)),
                        vec![Modifier::Divide(1)],
                    ),
                )],
            )],
        };

        let non_match = schedule_bar(&program, Meter::default(), 2).expect("schedule should work");
        let match_bar = schedule_bar(&program, Meter::default(), 3).expect("schedule should work");

        assert!(non_match.is_empty());
        assert_eq!(match_bar.len(), 1);
        assert_eq!(match_bar[0].sound.display_name(), "bd:7");
    }

    #[test]
    fn more_specific_periodic_bar_definition_wins() {
        let program = Program {
            bpm: Some(128.0),
            tempo_changes: BTreeMap::new(),
            bars: Some(8),
            layers: vec![layer(
                "bd",
                &[
                    (
                        BarSelector::Every(2),
                        bar(
                            PatternSource::Atom(PatternAtom::SampleIndex(2)),
                            vec![Modifier::Divide(1)],
                        ),
                    ),
                    (
                        BarSelector::Every(4),
                        bar(
                            PatternSource::Atom(PatternAtom::SampleIndex(4)),
                            vec![Modifier::Divide(1)],
                        ),
                    ),
                ],
            )],
        };

        let events = schedule_bar(&program, Meter::default(), 3).expect("schedule should work");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sound.display_name(), "bd:4");
    }

    #[test]
    fn exact_bar_definition_overrides_periodic_bar_definition() {
        let program = Program {
            bpm: Some(128.0),
            tempo_changes: BTreeMap::new(),
            bars: Some(8),
            layers: vec![layer(
                "bd",
                &[
                    (
                        BarSelector::Every(4),
                        bar(
                            PatternSource::Atom(PatternAtom::SampleIndex(4)),
                            vec![Modifier::Divide(1)],
                        ),
                    ),
                    (
                        BarSelector::Exact(4),
                        bar(
                            PatternSource::Atom(PatternAtom::SampleIndex(9)),
                            vec![Modifier::Divide(1)],
                        ),
                    ),
                ],
            )],
        };

        let events = schedule_bar(&program, Meter::default(), 3).expect("schedule should work");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sound.display_name(), "bd:9");
    }

    #[test]
    fn loops_back_to_first_phrase_bar() {
        let program = Program {
            bpm: Some(128.0),
            tempo_changes: BTreeMap::new(),
            bars: Some(4),
            layers: vec![layer(
                "bd",
                &[
                    (
                        BarSelector::Exact(1),
                        bar(PatternSource::Atom(PatternAtom::SampleIndex(0)), vec![]),
                    ),
                    (
                        BarSelector::Exact(2),
                        bar(PatternSource::Atom(PatternAtom::SampleIndex(3)), vec![]),
                    ),
                ],
            )],
        };

        let first_phrase =
            schedule_bar(&program, Meter::default(), 0).expect("schedule should work");
        let second_phrase =
            schedule_bar(&program, Meter::default(), 4).expect("schedule should work");

        assert_eq!(first_phrase[0].sound.display_name(), "bd:0");
        assert_eq!(second_phrase[0].sound.display_name(), "bd:0");
    }

    #[test]
    fn layer_level_effects_apply_to_bar_events() {
        let mut layer = layer(
            "hh",
            &[(
                BarSelector::Exact(1),
                bar(PatternSource::ImplicitSelf, vec![Modifier::Divide(1)]),
            )],
        );
        layer.modifiers = vec![Modifier::Gain(0.5), Modifier::Pan(-0.25)];
        let program = Program {
            bpm: Some(128.0),
            tempo_changes: BTreeMap::new(),
            bars: Some(4),
            layers: vec![layer],
        };

        let events = schedule_bar(&program, Meter::default(), 0).expect("schedule should work");
        assert_eq!(events[0].params.gain, Some(0.5));
        assert_eq!(events[0].params.pan, Some(-0.25));
    }
}
