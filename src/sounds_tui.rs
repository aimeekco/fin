use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier as StyleModifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::model::SoundTarget;
use crate::sounds::SoundsReport;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundsTab {
    Samples,
    Synths,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneFocus {
    List,
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundsBrowserState {
    active_tab: SoundsTab,
    sample_focus: PaneFocus,
    synth_focus: PaneFocus,
    selected_sample_bank: usize,
    selected_sample_entry: usize,
    selected_synth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BrowserLayout {
    inner: Rect,
    header: Rect,
    tabs: Rect,
    list: Rect,
    detail: Rect,
    footer: Rect,
}

impl SoundsBrowserState {
    pub fn new(report: &SoundsReport) -> Self {
        let mut state = Self {
            active_tab: if report.sample_banks.is_empty() && !report.synths.is_empty() {
                SoundsTab::Synths
            } else {
                SoundsTab::Samples
            },
            sample_focus: PaneFocus::List,
            synth_focus: PaneFocus::List,
            selected_sample_bank: 0,
            selected_sample_entry: 0,
            selected_synth: 0,
        };
        state.clamp(report);
        state
    }

    pub fn next_tab(&mut self) {
        self.active_tab = match self.active_tab {
            SoundsTab::Samples => SoundsTab::Synths,
            SoundsTab::Synths => SoundsTab::Samples,
        };
    }

    pub fn previous_tab(&mut self) {
        self.next_tab();
    }

    pub fn next_focus(&mut self) {
        match self.active_tab {
            SoundsTab::Samples => {
                self.sample_focus = toggle_focus(self.sample_focus);
            }
            SoundsTab::Synths => {
                self.synth_focus = toggle_focus(self.synth_focus);
            }
        }
    }

    pub fn previous_focus(&mut self) {
        self.next_focus();
    }

    pub fn move_up(&mut self, report: &SoundsReport) {
        match self.active_tab {
            SoundsTab::Samples if self.sample_focus == PaneFocus::List => {
                if self.selected_sample_bank > 0 {
                    self.selected_sample_bank -= 1;
                }
            }
            SoundsTab::Samples => {
                if self.selected_sample_entry > 0 {
                    self.selected_sample_entry -= 1;
                }
            }
            SoundsTab::Synths => {
                if self.selected_synth > 0 {
                    self.selected_synth -= 1;
                }
            }
        }
        self.clamp(report);
    }

    pub fn move_down(&mut self, report: &SoundsReport) {
        match self.active_tab {
            SoundsTab::Samples if self.sample_focus == PaneFocus::List => {
                if self.selected_sample_bank + 1 < report.sample_banks.len() {
                    self.selected_sample_bank += 1;
                }
            }
            SoundsTab::Samples => {
                if let Some(bank) = report.sample_banks.get(self.selected_sample_bank) {
                    if self.selected_sample_entry + 1 < bank.samples.len() {
                        self.selected_sample_entry += 1;
                    }
                }
            }
            SoundsTab::Synths => {
                if self.selected_synth + 1 < report.synths.len() {
                    self.selected_synth += 1;
                }
            }
        }
        self.clamp(report);
    }

    pub fn activate_selected(&self, report: &SoundsReport) -> Option<SoundTarget> {
        match self.active_tab {
            SoundsTab::Samples if self.sample_focus == PaneFocus::Detail => {
                self.selected_sample_sound(report)
            }
            SoundsTab::Samples => None,
            SoundsTab::Synths => self.selected_synth_sound(report),
        }
    }

    pub fn jump_to_query(&mut self, report: &SoundsReport, query: &str) -> bool {
        if query.is_empty() {
            return false;
        }
        let needle = query.to_ascii_lowercase();

        let matched = match self.active_tab {
            SoundsTab::Samples if self.sample_focus == PaneFocus::List => report
                .sample_banks
                .iter()
                .position(|bank| bank.name.to_ascii_lowercase().contains(&needle))
                .map(|index| {
                    self.selected_sample_bank = index;
                    true
                })
                .unwrap_or(false),
            SoundsTab::Samples => report
                .sample_banks
                .get(self.selected_sample_bank)
                .and_then(|bank| {
                    bank.samples.iter().position(|sample| {
                        sample.description.to_ascii_lowercase().contains(&needle)
                            || sample.file_name.to_ascii_lowercase().contains(&needle)
                            || sample.index.to_string().starts_with(&needle)
                    })
                })
                .map(|index| {
                    self.selected_sample_entry = index;
                    true
                })
                .unwrap_or(false),
            SoundsTab::Synths => report
                .synths
                .iter()
                .position(|synth| synth.name.to_ascii_lowercase().contains(&needle))
                .map(|index| {
                    self.selected_synth = index;
                    true
                })
                .unwrap_or(false),
        };

        self.clamp(report);
        matched
    }

    pub fn handle_click(
        &mut self,
        report: &SoundsReport,
        area: Rect,
        column: u16,
        row: u16,
    ) -> Option<SoundTarget> {
        let layout = compute_layout(area);
        if !contains(layout.inner, column, row) {
            return None;
        }

        if contains(layout.tabs, column, row) {
            let relative_x = column.saturating_sub(layout.tabs.x);
            self.active_tab = if relative_x < layout.tabs.width / 2 {
                SoundsTab::Samples
            } else {
                SoundsTab::Synths
            };
            self.clamp(report);
            return None;
        }

        match self.active_tab {
            SoundsTab::Samples => self.handle_sample_click(report, layout, column, row),
            SoundsTab::Synths => self.handle_synth_click(report, layout, column, row),
        }
    }

    pub fn active_tab(&self) -> SoundsTab {
        self.active_tab
    }

    pub fn active_focus(&self) -> PaneFocus {
        match self.active_tab {
            SoundsTab::Samples => self.sample_focus,
            SoundsTab::Synths => self.synth_focus,
        }
    }

    fn handle_sample_click(
        &mut self,
        report: &SoundsReport,
        layout: BrowserLayout,
        column: u16,
        row: u16,
    ) -> Option<SoundTarget> {
        if contains(layout.list, column, row) {
            let list_inner = bordered_inner(layout.list);
            let clicked_row = row.saturating_sub(list_inner.y) as usize;
            let visible_banks = visible_window(
                report.sample_banks.len(),
                self.selected_sample_bank,
                list_inner.height as usize,
            );
            let bank_index = visible_banks.start + clicked_row;
            if bank_index < report.sample_banks.len() {
                self.selected_sample_bank = bank_index;
                self.sample_focus = PaneFocus::List;
                self.clamp(report);
            }
            return None;
        }

        if contains(layout.detail, column, row) {
            let detail_inner = bordered_inner(layout.detail);
            let header_rows = 5usize;
            let clicked_row = row.saturating_sub(detail_inner.y) as usize;
            if clicked_row < header_rows {
                return None;
            }
            let Some(bank) = report.sample_banks.get(self.selected_sample_bank) else {
                return None;
            };
            let visible_entries = visible_window(
                bank.samples.len(),
                self.selected_sample_entry,
                detail_inner.height.saturating_sub(header_rows as u16) as usize,
            );
            let entry_index = visible_entries.start + (clicked_row - header_rows);
            if entry_index < bank.samples.len() {
                self.selected_sample_entry = entry_index;
                self.sample_focus = PaneFocus::Detail;
                return self.selected_sample_sound(report);
            }
        }

        None
    }

    fn handle_synth_click(
        &mut self,
        report: &SoundsReport,
        layout: BrowserLayout,
        column: u16,
        row: u16,
    ) -> Option<SoundTarget> {
        if contains(layout.list, column, row) {
            let list_inner = bordered_inner(layout.list);
            let clicked_row = row.saturating_sub(list_inner.y) as usize;
            let visible_synths = visible_window(
                report.synths.len(),
                self.selected_synth,
                list_inner.height as usize,
            );
            let synth_index = visible_synths.start + clicked_row;
            if synth_index < report.synths.len() {
                self.selected_synth = synth_index;
                self.synth_focus = PaneFocus::List;
                return self.selected_synth_sound(report);
            }
        }
        None
    }

    fn selected_sample_sound(&self, report: &SoundsReport) -> Option<SoundTarget> {
        let bank = report.sample_banks.get(self.selected_sample_bank)?;
        let sample = bank.samples.get(self.selected_sample_entry)?;
        Some(SoundTarget {
            name: bank.name.clone(),
            index: Some(sample.index as i32),
        })
    }

    fn selected_synth_sound(&self, report: &SoundsReport) -> Option<SoundTarget> {
        let synth = report.synths.get(self.selected_synth)?;
        Some(SoundTarget {
            name: synth.name.clone(),
            index: None,
        })
    }

    fn clamp(&mut self, report: &SoundsReport) {
        if report.sample_banks.is_empty() && !report.synths.is_empty() {
            self.active_tab = SoundsTab::Synths;
        } else if report.synths.is_empty() && !report.sample_banks.is_empty() {
            self.active_tab = SoundsTab::Samples;
        }

        if report.sample_banks.is_empty() {
            self.selected_sample_bank = 0;
            self.selected_sample_entry = 0;
        } else {
            self.selected_sample_bank = self
                .selected_sample_bank
                .min(report.sample_banks.len().saturating_sub(1));
            let bank = &report.sample_banks[self.selected_sample_bank];
            if bank.samples.is_empty() {
                self.selected_sample_entry = 0;
            } else {
                self.selected_sample_entry = self
                    .selected_sample_entry
                    .min(bank.samples.len().saturating_sub(1));
            }
        }

        if report.synths.is_empty() {
            self.selected_synth = 0;
        } else {
            self.selected_synth = self.selected_synth.min(report.synths.len().saturating_sub(1));
        }
    }
}

pub fn render_sounds_browser(
    frame: &mut Frame<'_>,
    area: Rect,
    report: &SoundsReport,
    state: &SoundsBrowserState,
    preview_status: &str,
    search_query: &str,
) {
    let outer = Block::default().borders(Borders::ALL).title(Line::from(vec![
        Span::raw(" SOUNDS "),
        Span::raw(" "),
        Span::styled(
            format!(
                "[ samples {} | synths {} ]",
                report.sample_banks.len(),
                report.synths.len()
            ),
            Style::default().add_modifier(StyleModifier::BOLD),
        ),
    ]));
    let layout = compute_layout(area);
    frame.render_widget(outer, area);

    let header = Paragraph::new(vec![
        Line::from(format!(
            "samples {}",
            report
                .samples_root
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "not found".to_string())
        )),
        Line::from(format!(
            "superdirt {} | {}{}",
            report
                .superdirt_root
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "not found".to_string()),
            preview_status,
            if search_query.is_empty() {
                String::new()
            } else {
                format!(" | jump {search_query}")
            }
        )),
    ]);
    frame.render_widget(header, layout.header);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            tab_span(" Samples ", state.active_tab() == SoundsTab::Samples),
            Span::raw(" "),
            tab_span(" Synths ", state.active_tab() == SoundsTab::Synths),
        ])),
        layout.tabs,
    );

    match state.active_tab() {
        SoundsTab::Samples => {
            render_sample_list(frame, layout.list, report, state);
            render_sample_detail(frame, layout.detail, report, state);
        }
        SoundsTab::Synths => {
            render_synth_list(frame, layout.list, report, state);
            render_synth_detail(frame, layout.detail, report, state);
        }
    }

    frame.render_widget(
        Paragraph::new(
            "tab panes  left/right tabs  up/down move  type jump  enter preview  click item preview  q quit",
        ),
        layout.footer,
    );
}

fn render_sample_list(
    frame: &mut Frame<'_>,
    area: Rect,
    report: &SoundsReport,
    state: &SoundsBrowserState,
) {
    let block = Block::default()
        .borders(Borders::TOP)
        .title(if state.active_tab() == SoundsTab::Samples && state.active_focus() == PaneFocus::List
        {
            " > BANKS "
        } else {
            " BANKS "
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let visible = visible_window(
        report.sample_banks.len(),
        state.selected_sample_bank,
        inner.height as usize,
    );
    let lines = report.sample_banks[visible.start..visible.end]
        .iter()
        .enumerate()
        .map(|(offset, bank)| {
            browser_line(
                visible.start + offset == state.selected_sample_bank,
                &format!("{}  {}", bank.name, bank.description),
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_sample_detail(
    frame: &mut Frame<'_>,
    area: Rect,
    report: &SoundsReport,
    state: &SoundsBrowserState,
) {
    let block = Block::default()
        .borders(Borders::TOP)
        .title(if state.active_tab() == SoundsTab::Samples
            && state.active_focus() == PaneFocus::Detail
        {
            " > SAMPLES "
        } else {
            " SAMPLES "
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(bank) = report.sample_banks.get(state.selected_sample_bank) else {
        frame.render_widget(Paragraph::new("no sample banks found"), inner);
        return;
    };

    let entry_height = inner.height.saturating_sub(5) as usize;
    let visible_entries = visible_window(bank.samples.len(), state.selected_sample_entry, entry_height);

    let mut lines = vec![
        Line::from(Span::styled(
            bank.name.clone(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(StyleModifier::BOLD),
        )),
        Line::from(format!("description {}", bank.description)),
        Line::from(format!("samples {}", bank.samples.len())),
        Line::from(format!("use [{}] /1 <0 1 2>", bank.name)),
        Line::from(""),
    ];
    lines.extend(
        bank.samples[visible_entries.start..visible_entries.end]
            .iter()
            .enumerate()
            .map(|(offset, sample)| {
                browser_line(
                    visible_entries.start + offset == state.selected_sample_entry,
                    &format!(
                        "{:>3}  {:<24}  {}",
                        sample.index, sample.description, sample.file_name
                    ),
                )
            }),
    );

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_synth_list(
    frame: &mut Frame<'_>,
    area: Rect,
    report: &SoundsReport,
    state: &SoundsBrowserState,
) {
    let block = Block::default()
        .borders(Borders::TOP)
        .title(if state.active_tab() == SoundsTab::Synths && state.active_focus() == PaneFocus::List
        {
            " > SYNTHS "
        } else {
            " SYNTHS "
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let visible = visible_window(report.synths.len(), state.selected_synth, inner.height as usize);
    let lines = report.synths[visible.start..visible.end]
        .iter()
        .enumerate()
        .map(|(offset, synth)| {
            browser_line(
                visible.start + offset == state.selected_synth,
                &format!("{}  {}", synth.name, synth.description),
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_synth_detail(
    frame: &mut Frame<'_>,
    area: Rect,
    report: &SoundsReport,
    state: &SoundsBrowserState,
) {
    let block = Block::default()
        .borders(Borders::TOP)
        .title(if state.active_tab() == SoundsTab::Synths
            && state.active_focus() == PaneFocus::Detail
        {
            " > DETAIL "
        } else {
            " DETAIL "
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(synth) = report.synths.get(state.selected_synth) else {
        frame.render_widget(Paragraph::new("no synths found"), inner);
        return;
    };

    let lines = vec![
        Line::from(Span::styled(
            synth.name.clone(),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(StyleModifier::BOLD),
        )),
        Line::from(format!("description {}", synth.description)),
        Line::from(format!("preview {}", synth.name)),
        Line::from("click or press enter to trigger a preview".to_string()),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

fn compute_layout(area: Rect) -> BrowserLayout {
    let outer = Block::default().borders(Borders::ALL);
    let inner = outer.inner(area);
    let sections = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Min(10),
        Constraint::Length(1),
    ])
    .split(inner);
    let body = Layout::horizontal([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(sections[2]);

    BrowserLayout {
        inner,
        header: sections[0],
        tabs: sections[1],
        list: body[0],
        detail: body[1],
        footer: sections[3],
    }
}

fn toggle_focus(focus: PaneFocus) -> PaneFocus {
    match focus {
        PaneFocus::List => PaneFocus::Detail,
        PaneFocus::Detail => PaneFocus::List,
    }
}

fn tab_span(label: &str, active: bool) -> Span<'static> {
    let style = if active {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(StyleModifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Span::styled(label.to_string(), style)
}

fn browser_line(selected: bool, text: &str) -> Line<'static> {
    let prefix = if selected { "> " } else { "  " };
    let style = if selected {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(StyleModifier::BOLD)
    } else {
        Style::default()
    };
    Line::from(Span::styled(format!("{prefix}{text}"), style))
}

fn visible_window(total: usize, selected: usize, height: usize) -> std::ops::Range<usize> {
    if total == 0 || height == 0 {
        return 0..0;
    }
    if total <= height {
        return 0..total;
    }

    let start = selected.saturating_sub(height / 2).min(total.saturating_sub(height));
    start..(start + height)
}

fn bordered_inner(area: Rect) -> Rect {
    Block::default().borders(Borders::TOP).inner(area)
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x + area.width
        && row >= area.y
        && row < area.y + area.height
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sounds::{SampleBank, SampleEntry, SynthSound};

    fn report() -> SoundsReport {
        SoundsReport {
            samples_root: None,
            sample_banks: vec![SampleBank {
                name: "ab".to_string(),
                description: "drum kit / percussion bank".to_string(),
                samples: vec![
                    SampleEntry {
                        index: 0,
                        file_name: "000_ab2closedhh.wav".to_string(),
                        description: "closed hi-hat".to_string(),
                    },
                    SampleEntry {
                        index: 1,
                        file_name: "004_ab2kick1.wav".to_string(),
                        description: "kick 1".to_string(),
                    },
                ],
            }],
            superdirt_root: None,
            synths: vec![SynthSound {
                name: "from".to_string(),
                description: "registered SuperDirt synth `from`".to_string(),
            }],
        }
    }

    #[test]
    fn can_switch_tabs() {
        let report = report();
        let mut state = SoundsBrowserState::new(&report);
        assert_eq!(state.active_tab(), SoundsTab::Samples);

        state.next_tab();
        assert_eq!(state.active_tab(), SoundsTab::Synths);
    }

    #[test]
    fn tab_switches_focus_within_active_tab() {
        let report = report();
        let mut state = SoundsBrowserState::new(&report);
        assert_eq!(state.active_focus(), PaneFocus::List);

        state.next_focus();
        assert_eq!(state.active_focus(), PaneFocus::Detail);
    }

    #[test]
    fn clicking_sample_entry_returns_preview_target() {
        let report = report();
        let mut state = SoundsBrowserState::new(&report);
        let layout = compute_layout(Rect::new(0, 0, 120, 40));
        let target = state.handle_click(
            &report,
            Rect::new(0, 0, 120, 40),
            layout.detail.x + 2,
            layout.detail.y + 6,
        );

        assert_eq!(
            target,
            Some(SoundTarget {
                name: "ab".to_string(),
                index: Some(0),
            })
        );
        assert_eq!(state.active_focus(), PaneFocus::Detail);
    }

    #[test]
    fn typing_query_jumps_within_focused_pane() {
        let report = report();
        let mut state = SoundsBrowserState::new(&report);
        state.next_focus();

        assert!(state.jump_to_query(&report, "kick"));
        assert_eq!(state.activate_selected(&report).unwrap().index, Some(1));
    }
}
