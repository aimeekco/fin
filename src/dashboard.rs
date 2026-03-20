use std::collections::BTreeMap;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier as StyleModifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap};

use crate::model::{
    Layer, Modifier, NoteValue, PatternAtom, PatternSource, Program, ScheduledEvent,
};

const METER_WIDTH: usize = 30;

#[derive(Debug, Clone, PartialEq)]
pub struct DashboardState {
    pub status: String,
    pub bpm: String,
    pub clip_percent: u8,
    pub osc_status: String,
    pub layers: Vec<LayerRow>,
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerRow {
    pub label: String,
    pub meter: String,
    pub detail: String,
}

pub fn build_dashboard_state(
    program: &Program,
    events: &[ScheduledEvent],
    osc_status: impl Into<String>,
    logs: Vec<String>,
) -> DashboardState {
    DashboardState {
        status: "RUNNING".to_string(),
        bpm: format_bpm(program),
        clip_percent: 0,
        osc_status: osc_status.into(),
        layers: build_layer_rows(program, events),
        logs,
    }
}

pub fn build_layer_rows(program: &Program, events: &[ScheduledEvent]) -> Vec<LayerRow> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for event in events {
        *counts.entry(&event.layer.0).or_default() += 1;
    }
    let max_events = counts.values().copied().max().unwrap_or(1) as f32;

    program
        .layers
        .iter()
        .map(|layer| {
            let count = counts.get(layer.name.0.as_str()).copied().unwrap_or(0) as f32;
            let ratio = if max_events > 0.0 {
                count / max_events
            } else {
                0.0
            };
            LayerRow {
                label: format!("[{}]", layer.name),
                meter: meter_bar(ratio),
                detail: layer_detail(layer),
            }
        })
        .collect()
}

pub fn render_dashboard(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let outer = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(vec![
            Span::raw(" METL v0.1.0 "),
            Span::raw(" "),
            Span::styled(
                format!("[ {} ]", state.status),
                Style::default().add_modifier(StyleModifier::BOLD),
            ),
        ]));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let sections = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(8),
        Constraint::Min(6),
        Constraint::Length(1),
    ])
    .split(inner);

    let header = Paragraph::new(Line::from(format!(
        "BPM: {} | CLIP: {}% | OSC: {}",
        state.bpm, state.clip_percent, state.osc_status
    )));
    frame.render_widget(header, sections[0]);

    render_layers(frame, sections[1], &state.layers);
    render_logs(frame, sections[2], &state.logs);
    frame.render_widget(Paragraph::new("q quit"), sections[3]);
}

fn render_layers(frame: &mut Frame<'_>, area: Rect, layers: &[LayerRow]) {
    let block = Block::default().borders(Borders::TOP).title(" LAYERS ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = layers.iter().map(|layer| {
        Row::new(vec![
            Cell::from(layer.label.clone()),
            Cell::from(layer.meter.clone()),
            Cell::from(layer.detail.clone()),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length((METER_WIDTH + 2) as u16),
            Constraint::Min(10),
        ],
    );
    frame.render_widget(table, inner);
}

fn render_logs(frame: &mut Frame<'_>, area: Rect, logs: &[String]) {
    let block = Block::default().borders(Borders::TOP).title(" LOG ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines: Vec<Line<'_>> = logs
        .iter()
        .map(|entry| Line::from(entry.as_str()))
        .collect();
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

fn meter_bar(ratio: f32) -> String {
    let filled = (ratio.clamp(0.0, 1.0) * METER_WIDTH as f32).round() as usize;
    let mut meter = String::with_capacity(METER_WIDTH);
    meter.push_str(&"|".repeat(filled));
    meter.push_str(&" ".repeat(METER_WIDTH.saturating_sub(filled)));
    meter
}

fn layer_detail(layer: &Layer) -> String {
    let mut parts = Vec::new();
    match &layer.pattern {
        PatternSource::ImplicitSelf => {}
        PatternSource::Atom(atom) => parts.push(atom_label(atom)),
        PatternSource::Cycle(atoms) => {
            parts.push(format!(
                "<{}>",
                atoms.iter().map(atom_label).collect::<Vec<_>>().join(" ")
            ));
        }
        PatternSource::Group(atoms) => {
            parts.push(format!(
                "[{}]",
                atoms.iter().map(atom_label).collect::<Vec<_>>().join(" ")
            ));
        }
        PatternSource::NoteSequence(notes) => {
            parts.push(format!(
                "[{}]",
                notes.iter().map(note_label).collect::<Vec<_>>().join(" ")
            ));
        }
    }

    for modifier in &layer.modifiers {
        match modifier {
            Modifier::Divide(value) => parts.push(format!("/{value}")),
            Modifier::Multiply(value) => parts.push(format!("*{value}")),
            Modifier::Shift(value) if *value >= 0.0 => parts.push(format!(">> {:.3}", value)),
            Modifier::Shift(value) => parts.push(format!("<< {:.3}", value.abs())),
            Modifier::Gain(value) => parts.push(format!(".gain {:.2}", value)),
            Modifier::Pan(value) => parts.push(format!(".pan {:.2}", value)),
            Modifier::Speed(value) => parts.push(format!(".speed {:.2}", value)),
            Modifier::Sustain(value) => parts.push(format!(".sustain {:.2}", value)),
        }
    }

    if parts.is_empty() {
        "/1".to_string()
    } else {
        parts.join(" ")
    }
}

fn atom_label(atom: &PatternAtom) -> String {
    match atom {
        PatternAtom::SampleIndex(index) => index.to_string(),
        PatternAtom::Sound(sound) => sound.display_name(),
    }
}

fn note_label(note: &NoteValue) -> String {
    note.label.clone()
}

fn format_bpm(program: &Program) -> String {
    let bpm = program.effective_bpm();
    if bpm.fract() == 0.0 {
        format!("{:.0}", bpm)
    } else {
        format!("{:.2}", bpm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        EventParams, Layer, NoteValue, PatternSource, Program, ScheduledEvent, SoundTarget, Symbol,
    };

    #[test]
    fn builds_layer_rows_with_density_bars() {
        let program = Program {
            bpm: Some(132.0),
            layers: vec![
                Layer {
                    name: Symbol("fin".to_string()),
                    default_target: SoundTarget {
                        name: "fin".to_string(),
                        index: None,
                    },
                    pattern: PatternSource::ImplicitSelf,
                    modifiers: vec![Modifier::Divide(4)],
                    source_line: 1,
                },
                Layer {
                    name: Symbol("splash".to_string()),
                    default_target: SoundTarget {
                        name: "splash".to_string(),
                        index: None,
                    },
                    pattern: PatternSource::ImplicitSelf,
                    modifiers: vec![Modifier::Multiply(16)],
                    source_line: 2,
                },
            ],
        };
        let events = vec![
            ScheduledEvent {
                layer: Symbol("fin".to_string()),
                sound: SoundTarget {
                    name: "fin".to_string(),
                    index: None,
                },
                bar_pos: 0.0,
                beat_pos: 0.0,
                params: EventParams::default(),
            },
            ScheduledEvent {
                layer: Symbol("splash".to_string()),
                sound: SoundTarget {
                    name: "splash".to_string(),
                    index: None,
                },
                bar_pos: 0.0,
                beat_pos: 0.0,
                params: EventParams::default(),
            },
            ScheduledEvent {
                layer: Symbol("splash".to_string()),
                sound: SoundTarget {
                    name: "splash".to_string(),
                    index: None,
                },
                bar_pos: 0.25,
                beat_pos: 1.0,
                params: EventParams::default(),
            },
        ];

        let rows = build_layer_rows(&program, &events);
        assert_eq!(rows[0].label, "[fin]");
        assert!(rows[0].detail.contains("/4"));
        assert!(rows[1].meter.len() == METER_WIDTH);
    }

    #[test]
    fn layer_detail_includes_pattern_and_params() {
        let layer = Layer {
            name: Symbol("hh".to_string()),
            default_target: SoundTarget {
                name: "hh".to_string(),
                index: None,
            },
            pattern: PatternSource::Group(vec![
                PatternAtom::Sound(SoundTarget {
                    name: "hh".to_string(),
                    index: None,
                }),
                PatternAtom::SampleIndex(2),
            ]),
            modifiers: vec![
                Modifier::Multiply(4),
                Modifier::Pan(0.2),
                Modifier::Sustain(0.15),
            ],
            source_line: 1,
        };

        let detail = layer_detail(&layer);
        assert!(detail.contains("[hh 2]"));
        assert!(detail.contains("*4"));
        assert!(detail.contains(".pan 0.20"));
        assert!(detail.contains(".sustain 0.15"));
    }

    #[test]
    fn layer_detail_shows_note_sequence() {
        let layer = Layer {
            name: Symbol("bass".to_string()),
            default_target: SoundTarget {
                name: "bass".to_string(),
                index: None,
            },
            pattern: PatternSource::NoteSequence(vec![
                NoteValue {
                    label: "g4".to_string(),
                    semitone: -5.0,
                },
                NoteValue {
                    label: "a4".to_string(),
                    semitone: -3.0,
                },
            ]),
            modifiers: vec![Modifier::Divide(1)],
            source_line: 1,
        };

        let detail = layer_detail(&layer);
        assert!(detail.contains("[g4 a4]"));
        assert!(detail.contains("/1"));
    }
}
