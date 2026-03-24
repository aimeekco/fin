use std::collections::BTreeMap;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier as StyleModifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap};

use crate::model::{
    BarPattern, Layer, Modifier, NoteValue, PatternAtom, PatternSource, PatternValue, Program,
    ScheduledEvent, DEFAULT_BAR_INDEX,
};
use crate::osc::event_gain;

const METER_WIDTH: usize = 30;
const TRANSPORT_WIDTH: usize = 32;
const SCOPE_WIDTH: usize = 24;

#[derive(Debug, Clone, PartialEq)]
pub struct DashboardState {
    pub status: String,
    pub bpm: String,
    pub clip_percent: u8,
    pub osc_status: String,
    pub watcher_status: String,
    pub master_scope: String,
    pub transport: TransportRow,
    pub layers: Vec<LayerRow>,
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DashboardRuntime {
    pub osc_status: String,
    pub watcher_status: String,
    pub bar_index: usize,
    pub bar_progress: f32,
    pub pending_reload: bool,
    pub master_scope: String,
    pub layer_visuals: BTreeMap<String, LayerVisual>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransportRow {
    pub label: String,
    pub phase_bar: String,
    pub playhead_bar: String,
    pub hits: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerVisual {
    pub level: f32,
    pub scope: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerRow {
    pub label: String,
    pub meter: String,
    pub scope: String,
    pub hits: usize,
    pub detail: String,
}

pub fn build_dashboard_state(
    program: &Program,
    events: &[ScheduledEvent],
    runtime: DashboardRuntime,
    logs: Vec<String>,
) -> DashboardState {
    let clip_percent = estimate_clip_percent(events);
    DashboardState {
        status: if runtime.pending_reload {
            "RELOAD PENDING".to_string()
        } else {
            "RUNNING".to_string()
        },
        bpm: format_bpm(program),
        clip_percent,
        osc_status: runtime.osc_status,
        watcher_status: runtime.watcher_status,
        master_scope: runtime.master_scope,
        transport: build_transport_row(runtime.bar_index, runtime.bar_progress, events),
        layers: build_layer_rows(program, events, &runtime.layer_visuals),
        logs,
    }
}

pub fn build_layer_rows(
    program: &Program,
    events: &[ScheduledEvent],
    layer_visuals: &BTreeMap<String, LayerVisual>,
) -> Vec<LayerRow> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut weighted_energy: BTreeMap<&str, f32> = BTreeMap::new();
    for event in events {
        *counts.entry(&event.layer.0).or_default() += 1;
        *weighted_energy.entry(&event.layer.0).or_default() += event_gain(event);
    }
    let max_energy = weighted_energy
        .values()
        .copied()
        .fold(0.0, f32::max)
        .max(1.0);

    program
        .layers
        .iter()
        .map(|layer| {
            let hits = counts.get(layer.name.0.as_str()).copied().unwrap_or(0);
            let energy = weighted_energy
                .get(layer.name.0.as_str())
                .copied()
                .unwrap_or(0.0);
            let ratio = if max_energy > 0.0 {
                energy / max_energy
            } else {
                0.0
            };
            let visual = layer_visuals
                .get(layer.name.0.as_str())
                .cloned()
                .unwrap_or_else(empty_visual);
            LayerRow {
                label: format!("[{}]", layer.name),
                meter: meter_bar(ratio, visual.level, hits),
                scope: visual.scope,
                hits,
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
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Min(8),
        Constraint::Min(6),
        Constraint::Length(1),
    ])
    .split(inner);

    let header = Paragraph::new(vec![
        Line::from(format!(
            "BPM: {} | CLIP: {}% | OSC: {}",
            state.bpm, state.clip_percent, state.osc_status
        )),
        Line::from(format!(
            "WATCH: {} | MASTER {} {}",
            state.watcher_status, state.master_scope, state.transport.playhead_bar
        )),
    ]);
    frame.render_widget(header, sections[0]);

    render_transport(frame, sections[1], &state.transport);
    render_layers(frame, sections[2], &state.layers);
    render_logs(frame, sections[3], &state.logs);
    frame.render_widget(Paragraph::new("q quit"), sections[4]);
}

fn render_transport(frame: &mut Frame<'_>, area: Rect, transport: &TransportRow) {
    let block = Block::default().borders(Borders::TOP).title(" TRANSPORT ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let line = Line::from(format!(
        "{} | {} | {} | hits {}",
        transport.label, transport.phase_bar, transport.playhead_bar, transport.hits
    ));
    frame.render_widget(Paragraph::new(line), inner);
}

fn render_layers(frame: &mut Frame<'_>, area: Rect, layers: &[LayerRow]) {
    let block = Block::default().borders(Borders::TOP).title(" LAYERS ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = layers.iter().map(|layer| {
        Row::new(vec![
            Cell::from(layer.label.clone()),
            Cell::from(layer.meter.clone()),
            Cell::from(layer.scope.clone()),
            Cell::from(format!("{:>3}", layer.hits)),
            Cell::from(layer.detail.clone()),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length((METER_WIDTH + 2) as u16),
            Constraint::Length((SCOPE_WIDTH + 2) as u16),
            Constraint::Length(5),
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

fn meter_bar(ratio: f32, live_level: f32, hits: usize) -> String {
    let filled = (ratio.clamp(0.0, 1.0) * METER_WIDTH as f32).round() as usize;
    let peak = (live_level.clamp(0.0, 1.0) * METER_WIDTH as f32).round() as usize;
    let mut meter = String::with_capacity(METER_WIDTH);
    let accent = match hits {
        0 => ' ',
        1..=2 => '.',
        3..=4 => ':',
        5..=8 => '=',
        _ => '#',
    };
    for index in 0..METER_WIDTH {
        let ch = if index < filled && index < peak {
            '#'
        } else if index < peak {
            '|'
        } else if index < filled {
            accent
        } else {
            ' '
        };
        meter.push(ch);
    }
    meter
}

fn build_transport_row(
    bar_index: usize,
    bar_progress: f32,
    events: &[ScheduledEvent],
) -> TransportRow {
    let mut cells = vec!['.'; TRANSPORT_WIDTH];
    for event in events {
        let index = ((event.bar_pos.clamp(0.0, 0.999_9)) * TRANSPORT_WIDTH as f32).floor() as usize;
        let cell = cells
            .get_mut(index.min(TRANSPORT_WIDTH - 1))
            .expect("index should be in range");
        *cell = match *cell {
            '.' => ':',
            ':' => '=',
            '=' => '#',
            '#' => '#',
            _ => '#',
        };
    }
    let mut playhead = vec![' '; TRANSPORT_WIDTH];
    let playhead_index =
        ((bar_progress.clamp(0.0, 0.999_9)) * TRANSPORT_WIDTH as f32).floor() as usize;
    playhead[playhead_index.min(TRANSPORT_WIDTH - 1)] = '^';

    TransportRow {
        label: format!("BAR {:03}", bar_index + 1),
        phase_bar: format!("[{}]", cells.into_iter().collect::<String>()),
        playhead_bar: format!("[{}]", playhead.into_iter().collect::<String>()),
        hits: events.len(),
    }
}

fn estimate_clip_percent(events: &[ScheduledEvent]) -> u8 {
    let mut peaks: BTreeMap<i32, f32> = BTreeMap::new();
    for event in events {
        let slot = (event.bar_pos * 1000.0).round() as i32;
        *peaks.entry(slot).or_default() += event_gain(event);
    }

    let max_peak = peaks.values().copied().fold(0.0, f32::max);
    if max_peak <= 1.0 {
        0
    } else {
        ((max_peak - 1.0) * 100.0).round().clamp(0.0, 100.0) as u8
    }
}

fn empty_visual() -> LayerVisual {
    LayerVisual {
        level: 0.0,
        scope: " ".repeat(SCOPE_WIDTH),
    }
}

fn layer_detail(layer: &Layer) -> String {
    let mut parts = Vec::new();

    if !layer.modifiers.is_empty() {
        parts.push(
            layer
                .modifiers
                .iter()
                .map(modifier_label)
                .collect::<Vec<_>>()
                .join(" "),
        );
    }

    if layer.bars.is_empty() {
        parts.push("silent".to_string());
    } else {
        parts.extend(
            layer
                .bars
                .iter()
                .map(|(bar_index, pattern)| {
                    format!("{} {}", bar_label(*bar_index), bar_pattern_label(pattern))
                }),
        );
    }

    parts.join(" | ")
}

fn bar_label(bar_index: u32) -> String {
    if bar_index == DEFAULT_BAR_INDEX {
        "[default]".to_string()
    } else {
        format!("bar{bar_index}")
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

fn pattern_label(pattern: &PatternSource) -> String {
    match pattern {
        PatternSource::ImplicitSelf => "self".to_string(),
        PatternSource::Atom(atom) => atom_label(atom),
        PatternSource::Group(atoms) => format!(
            "[{}]",
            atoms.iter().map(atom_label).collect::<Vec<_>>().join(" ")
        ),
        PatternSource::Sequence(values) => format!(
            "<{}>",
            values
                .iter()
                .map(|value| match value {
                    PatternValue::Hit => "o".to_string(),
                    PatternValue::Rest => "x".to_string(),
                    PatternValue::Atom(atom) => atom_label(atom),
                    PatternValue::Note(note) => note_label(note),
                })
                .collect::<Vec<_>>()
                .join(" ")
        ),
    }
}

fn modifier_label(modifier: &Modifier) -> String {
    match modifier {
        Modifier::Divide(value) => format!("/{value}"),
        Modifier::Multiply(value) => format!("*{value}"),
        Modifier::Shift(value) if *value >= 0.0 => format!(">> {:.3}", value),
        Modifier::Shift(value) => format!("<< {:.3}", value.abs()),
        Modifier::Gain(value) => format!(".gain {:.2}", value),
        Modifier::Pan(value) => format!(".pan {:.2}", value),
        Modifier::Speed(value) => format!(".speed {:.2}", value),
        Modifier::Sustain(value) => format!(".sustain {:.2}", value),
    }
}

fn bar_pattern_label(pattern: &BarPattern) -> String {
    let mut parts = pattern
        .modifiers
        .iter()
        .map(modifier_label)
        .collect::<Vec<_>>();
    parts.push(pattern_label(&pattern.pattern));
    parts.join(" ")
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
    use std::collections::BTreeMap;

    use super::*;
    use crate::model::{
        BarPattern, EventParams, Layer, NoteValue, PatternSource, PatternValue, Program,
        ScheduledEvent, SoundTarget, Symbol,
    };

    fn bar(pattern: PatternSource, modifiers: Vec<Modifier>) -> BarPattern {
        BarPattern {
            pattern,
            modifiers,
            source_line: 1,
        }
    }

    #[test]
    fn builds_layer_rows_with_density_bars() {
        let program = Program {
            bpm: Some(132.0),
            bars: Some(4),
            layers: vec![
                Layer {
                    name: Symbol("fin".to_string()),
                    default_target: SoundTarget {
                        name: "fin".to_string(),
                        index: None,
                    },
                    modifiers: vec![Modifier::Gain(0.8)],
                    bars: BTreeMap::from([(
                        1,
                        bar(PatternSource::ImplicitSelf, vec![Modifier::Divide(4)]),
                    )]),
                    source_line: 1,
                },
                Layer {
                    name: Symbol("splash".to_string()),
                    default_target: SoundTarget {
                        name: "splash".to_string(),
                        index: None,
                    },
                    modifiers: Vec::new(),
                    bars: BTreeMap::from([(
                        2,
                        bar(PatternSource::ImplicitSelf, vec![Modifier::Multiply(16)]),
                    )]),
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

        let rows = build_layer_rows(&program, &events, &BTreeMap::new());
        assert_eq!(rows[0].label, "[fin]");
        assert!(rows[0].detail.contains("bar1 /4 self"));
        assert_eq!(rows[0].hits, 1);
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
            modifiers: vec![Modifier::Pan(0.2), Modifier::Sustain(0.15)],
            bars: BTreeMap::from([(
                1,
                bar(
                    PatternSource::Group(vec![
                        PatternAtom::Sound(SoundTarget {
                            name: "hh".to_string(),
                            index: None,
                        }),
                        PatternAtom::SampleIndex(2),
                    ]),
                    vec![Modifier::Multiply(4)],
                ),
            )]),
            source_line: 1,
        };

        let detail = layer_detail(&layer);
        assert!(detail.contains(".pan 0.20"));
        assert!(detail.contains(".sustain 0.15"));
        assert!(detail.contains("bar1"));
        assert!(detail.contains("[hh 2]"));
        assert!(detail.contains("*4"));
    }

    #[test]
    fn layer_detail_shows_note_sequence() {
        let layer = Layer {
            name: Symbol("bass".to_string()),
            default_target: SoundTarget {
                name: "bass".to_string(),
                index: None,
            },
            modifiers: Vec::new(),
            bars: BTreeMap::from([(
                1,
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
            )]),
            source_line: 1,
        };

        let detail = layer_detail(&layer);
        assert!(detail.contains("<g4 a4>"));
        assert!(detail.contains("/1"));
    }

    #[test]
    fn layer_detail_labels_default_bar() {
        let layer = Layer {
            name: Symbol("bd".to_string()),
            default_target: SoundTarget {
                name: "bd".to_string(),
                index: None,
            },
            modifiers: Vec::new(),
            bars: BTreeMap::from([(
                DEFAULT_BAR_INDEX,
                bar(PatternSource::ImplicitSelf, vec![Modifier::Divide(4)]),
            )]),
            source_line: 1,
        };

        let detail = layer_detail(&layer);
        assert!(detail.contains("[default] /4 self"));
    }

    #[test]
    fn transport_row_marks_event_positions() {
        let transport = build_transport_row(
            2,
            0.5,
            &[ScheduledEvent {
                layer: Symbol("bd".to_string()),
                sound: SoundTarget {
                    name: "bd".to_string(),
                    index: None,
                },
                bar_pos: 0.5,
                beat_pos: 2.0,
                params: EventParams::default(),
            }],
        );

        assert_eq!(transport.label, "BAR 003");
        assert!(transport.phase_bar.contains(':'));
        assert!(transport.playhead_bar.contains('^'));
        assert_eq!(transport.hits, 1);
    }

    #[test]
    fn clip_percent_estimates_overlap_risk() {
        let clip = estimate_clip_percent(&[
            ScheduledEvent {
                layer: Symbol("bd".to_string()),
                sound: SoundTarget {
                    name: "bd".to_string(),
                    index: None,
                },
                bar_pos: 0.0,
                beat_pos: 0.0,
                params: EventParams {
                    gain: Some(0.8),
                    ..EventParams::default()
                },
            },
            ScheduledEvent {
                layer: Symbol("sd".to_string()),
                sound: SoundTarget {
                    name: "sd".to_string(),
                    index: None,
                },
                bar_pos: 0.0,
                beat_pos: 0.0,
                params: EventParams {
                    gain: Some(0.6),
                    ..EventParams::default()
                },
            },
        ]);

        assert_eq!(clip, 40);
    }
}
