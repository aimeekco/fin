use std::collections::{BTreeMap, VecDeque};
use std::io::{self, stdout};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use clap::{Args, Parser, Subcommand};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use fin::dashboard::{DashboardRuntime, build_dashboard_state, render_dashboard};
use fin::model::{Meter, Program, ScheduledEvent};
use fin::osc::{OscClient, event_gain};
use fin::parser::parse_program;
use fin::scheduler::{format_events, schedule_bar, schedule_intro};
use fin::sounds::{format_sounds_report, load_sounds_report};
use fin::supercollider::{StartMode, start_superdirt, stop_superdirt, superdirt_status};
use fin::watcher::FileChangeWatcher;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use time::OffsetDateTime;
use time::format_description::parse;

const DASHBOARD_FRAME_TIME: Duration = Duration::from_millis(33);
const SCOPE_HISTORY_WIDTH: usize = 24;
const LEVEL_DECAY_SECONDS: f32 = 0.35;

#[derive(Debug, Parser)]
#[command(name = "fin")]
#[command(about = "Functional Instrument Notation CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Run {
        path: PathBuf,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 57120)]
        port: u16,
        #[arg(long)]
        no_play: bool,
    },
    Watch {
        path: PathBuf,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 57120)]
        port: u16,
        #[arg(long)]
        no_play: bool,
        #[arg(long)]
        bars: Option<usize>,
    },
    Dashboard {
        path: PathBuf,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 57120)]
        port: u16,
        #[arg(long)]
        no_play: bool,
        #[arg(long)]
        bars: Option<usize>,
    },
    Superdirt(SuperdirtArgs),
    Sounds,
}

#[derive(Debug, Args)]
struct SuperdirtArgs {
    #[command(subcommand)]
    action: Option<SuperdirtAction>,
    #[arg(long)]
    sclang: Option<PathBuf>,
    #[arg(long, default_value_t = 57120)]
    port: u16,
    #[arg(long)]
    foreground: bool,
}

#[derive(Debug, Subcommand)]
enum SuperdirtAction {
    Kill,
    Status,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();

    match cli.command {
        Command::Run {
            path,
            host,
            port,
            no_play,
        } => run_file(path, host, port, no_play),
        Command::Watch {
            path,
            host,
            port,
            no_play,
            bars,
        } => watch_file(path, host, port, no_play, bars),
        Command::Dashboard {
            path,
            host,
            port,
            no_play,
            bars,
        } => dashboard_file(path, host, port, no_play, bars),
        Command::Superdirt(args) => match args.action {
            Some(SuperdirtAction::Kill) => stop_superdirt(),
            Some(SuperdirtAction::Status) => superdirt_status(),
            None => {
                let mode = if args.foreground {
                    StartMode::Foreground
                } else {
                    StartMode::Background
                };
                start_superdirt(args.sclang, args.port, mode)
            }
        },
        Command::Sounds => list_sounds(),
    }
}

fn run_file(path: PathBuf, host: String, port: u16, no_play: bool) -> Result<(), String> {
    ensure_metl_extension(&path)?;
    let loaded = load_track(&path)?;
    let intro = render_intro(&loaded.program)?;
    let rendered = render_bar(&loaded.program, 0)?;

    if let Some(intro) = &intro {
        print_schedule(&intro.output);
    }
    print_schedule(&rendered.output);

    if !no_play {
        let client = OscClient::connect(&host, port).map_err(|error| error.to_string())?;
        if let Some(intro) = &intro {
            play_rendered_bar(&client, &loaded.program, intro).map_err(|error| error.to_string())?;
        }
        play_rendered_bar(&client, &loaded.program, &rendered).map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn watch_file(
    path: PathBuf,
    host: String,
    port: u16,
    no_play: bool,
    bars: Option<usize>,
) -> Result<(), String> {
    ensure_metl_extension(&path)?;
    let mut loaded = load_track(&path)?;
    let mut file_watcher = FileChangeWatcher::new(&path)?;
    let client = if no_play {
        None
    } else {
        Some(OscClient::connect(&host, port).map_err(|error| error.to_string())?)
    };
    let mut last_reload_error: Option<String> = None;
    let intro = render_intro(&loaded.program)?;

    println!("watch load {}", path.display());
    if let Some(intro) = &intro {
        print_schedule(&intro.output);
    }
    print_schedule(&render_bar(&loaded.program, 0)?.output);

    if bars == Some(0) {
        return Ok(());
    }

    if let Some(intro) = &intro {
        if let Some(client) = &client {
            play_rendered_bar(client, &loaded.program, intro).map_err(|error| error.to_string())?;
        } else {
            thread::sleep(bar_duration(&loaded.program, Meter::default()));
        }
    }

    let mut completed_bars = 0usize;
    loop {
        let rendered = render_bar(&loaded.program, completed_bars)?;
        if let Some(client) = &client {
            if rendered.events.is_empty() {
                thread::sleep(bar_duration(&loaded.program, Meter::default()));
            } else {
                client
                    .play_bar(&loaded.program, &rendered.events)
                    .map_err(|error| error.to_string())?;
            }
        } else {
            thread::sleep(bar_duration(&loaded.program, Meter::default()));
        }

        completed_bars += 1;
        if bars.is_some_and(|limit| completed_bars >= limit) {
            return Ok(());
        }

        let poll = file_watcher.poll();
        for error in poll.errors {
            eprintln!("watch error: {error}");
        }

        if poll.changed {
            match load_track(file_watcher.watched_path()) {
                Ok(next) => {
                    if next.source != loaded.source {
                        println!("watch reload {}", path.display());
                        print_schedule(&render_bar(&next.program, completed_bars)?.output);
                        loaded = next;
                    }
                    last_reload_error = None;
                }
                Err(error) => {
                    if last_reload_error.as_ref() != Some(&error) {
                        eprintln!("watch reload failed: {error}");
                        last_reload_error = Some(error);
                    }
                }
            }
        }
    }
}

fn dashboard_file(
    path: PathBuf,
    host: String,
    port: u16,
    no_play: bool,
    bars: Option<usize>,
) -> Result<(), String> {
    ensure_metl_extension(&path)?;
    let mut loaded = load_track(&path)?;
    let mut file_watcher = FileChangeWatcher::new(&path)?;
    let client = if no_play {
        None
    } else {
        Some(OscClient::connect(&host, port).map_err(|error| error.to_string())?)
    };
    let osc_status = if no_play {
        "DISABLED".to_string()
    } else {
        format!("READY (SuperDirt {host}:{port})")
    };
    let mut watcher_status = if file_watcher.is_active() {
        format!("ARMED ({})", path.display())
    } else {
        "INACTIVE".to_string()
    };
    let mut logs = VecDeque::new();
    push_log(&mut logs, format!("Loaded {}", path.display()));

    let mut terminal = setup_terminal().map_err(|error| error.to_string())?;
    let _restore = TerminalRestore;

    if bars == Some(0) {
        return Ok(());
    }

    let mut completed_bars = 0usize;
    let mut last_reload_error: Option<String> = None;
    let mut visual_state = DashboardVisualState::new(&loaded.program);
    let mut pending_reload = false;

    if let Some(intro) = render_intro(&loaded.program)? {
        visual_state.sync_layers(&loaded.program);
        if run_dashboard_bar(
            &mut terminal,
            &loaded.program,
            &intro,
            client.as_ref(),
            &osc_status,
            &mut watcher_status,
            &mut pending_reload,
            &mut file_watcher,
            &mut visual_state,
            0,
            &mut logs,
        )? {
            push_log(&mut logs, "Quit requested".to_string());
            return Ok(());
        }
    }

    loop {
        let rendered = render_bar(&loaded.program, completed_bars)?;
        visual_state.sync_layers(&loaded.program);
        if run_dashboard_bar(
            &mut terminal,
            &loaded.program,
            &rendered,
            client.as_ref(),
            &osc_status,
            &mut watcher_status,
            &mut pending_reload,
            &mut file_watcher,
            &mut visual_state,
            completed_bars,
            &mut logs,
        )? {
            push_log(&mut logs, "Quit requested".to_string());
            draw_dashboard(
                &mut terminal,
                &loaded.program,
                &rendered,
                DashboardRuntime {
                    osc_status: osc_status.clone(),
                    watcher_status: watcher_status.clone(),
                    bar_index: completed_bars,
                    bar_progress: 1.0,
                    pending_reload,
                    master_scope: visual_state.master_scope.clone(),
                    layer_visuals: visual_state.layer_visuals(),
                },
                &logs,
            )
            .map_err(|error| error.to_string())?;
            return Ok(());
        }

        completed_bars += 1;
        if bars.is_some_and(|limit| completed_bars >= limit) {
            push_log(&mut logs, format!("Completed {completed_bars} bar(s)"));
            draw_dashboard(
                &mut terminal,
                &loaded.program,
                &rendered,
                DashboardRuntime {
                    osc_status: osc_status.clone(),
                    watcher_status: watcher_status.clone(),
                    bar_index: completed_bars,
                    bar_progress: 1.0,
                    pending_reload,
                    master_scope: visual_state.master_scope.clone(),
                    layer_visuals: visual_state.layer_visuals(),
                },
                &logs,
            )
            .map_err(|error| error.to_string())?;
            return Ok(());
        }

        if pending_reload {
            match load_track(file_watcher.watched_path()) {
                Ok(next) => {
                    if next.source != loaded.source {
                        push_log(&mut logs, "File changed. Re-parsing... DONE.".to_string());
                        loaded = next;
                        watcher_status = "RELOADED".to_string();
                    } else {
                        watcher_status = "ARMED".to_string();
                    }
                    last_reload_error = None;
                    pending_reload = false;
                }
                Err(error) => {
                    if last_reload_error.as_ref() != Some(&error) {
                        push_log(&mut logs, format!("Reload failed: {error}"));
                        last_reload_error = Some(error);
                    }
                    watcher_status = "RELOAD FAILED".to_string();
                    pending_reload = false;
                }
            }
        } else if !watcher_status.starts_with("ERROR") {
            watcher_status = "ARMED".to_string();
        }
    }
}

fn ensure_metl_extension(path: &Path) -> Result<(), String> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("metl") => Ok(()),
        _ => Err("expected a `.metl` source file".to_string()),
    }
}

fn load_track(path: &Path) -> Result<LoadedTrack, String> {
    let source = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let program = parse_program(&source).map_err(|error| error.to_string())?;
    Ok(LoadedTrack { source, program })
}

fn print_schedule(output: &str) {
    if !output.is_empty() {
        println!("{output}");
    }
}

fn bar_duration(program: &Program, meter: Meter) -> Duration {
    let seconds = meter.beats_per_bar as f64 * 60.0 / program.effective_bpm() as f64;
    Duration::from_secs_f64(seconds)
}

fn list_sounds() -> Result<(), String> {
    let report = load_sounds_report().map_err(|error| error.to_string())?;
    println!("{}", format_sounds_report(&report));
    Ok(())
}

fn draw_dashboard(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    program: &Program,
    rendered: &RenderedBar,
    runtime: DashboardRuntime,
    logs: &VecDeque<String>,
) -> io::Result<()> {
    let state = build_dashboard_state(
        program,
        &rendered.events,
        runtime,
        logs.iter().cloned().collect(),
    );
    terminal.draw(|frame| render_dashboard(frame, frame.area(), &state))?;
    Ok(())
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

fn should_quit() -> io::Result<bool> {
    while event::poll(Duration::from_millis(1))? {
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind == KeyEventKind::Press && matches!(key.code, KeyCode::Char('q')) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn push_log(logs: &mut VecDeque<String>, message: String) {
    logs.push_back(format!("[{}] {}", time_stamp(), message));
    while logs.len() > 8 {
        logs.pop_front();
    }
}

fn time_stamp() -> String {
    let format = parse("[hour]:[minute]:[second]").expect("time format should be valid");
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    now.format(&format)
        .unwrap_or_else(|_| "??:??:??".to_string())
}

fn render_bar(program: &Program, bar_index: usize) -> Result<RenderedBar, String> {
    let events =
        schedule_bar(program, Meter::default(), bar_index).map_err(|error| error.to_string())?;
    let output = format_events(program, &events);
    Ok(RenderedBar { events, output })
}

fn render_intro(program: &Program) -> Result<Option<RenderedBar>, String> {
    if !program.layers.iter().any(|layer| layer.intro_bar().is_some()) {
        return Ok(None);
    }

    let events = schedule_intro(program, Meter::default()).map_err(|error| error.to_string())?;
    let output = format_events(program, &events);
    Ok(Some(RenderedBar { events, output }))
}

fn play_rendered_bar(
    client: &OscClient,
    program: &Program,
    rendered: &RenderedBar,
) -> Result<(), fin::osc::OscError> {
    if rendered.events.is_empty() {
        thread::sleep(bar_duration(program, Meter::default()));
        Ok(())
    } else {
        client.play_bar(program, &rendered.events)
    }
}

struct LoadedTrack {
    source: String,
    program: Program,
}

struct RenderedBar {
    events: Vec<ScheduledEvent>,
    output: String,
}

struct TerminalRestore;

impl Drop for TerminalRestore {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
    }
}

fn run_dashboard_bar(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    program: &Program,
    rendered: &RenderedBar,
    client: Option<&OscClient>,
    osc_status: &str,
    watcher_status: &mut String,
    pending_reload: &mut bool,
    file_watcher: &mut FileChangeWatcher,
    visual_state: &mut DashboardVisualState,
    bar_index: usize,
    logs: &mut VecDeque<String>,
) -> Result<bool, String> {
    let bar_duration = bar_duration(program, Meter::default());
    let bar_start = Instant::now();
    let mut next_frame = bar_start;
    let mut next_event = 0usize;

    loop {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(bar_start);
        let progress = (elapsed.as_secs_f32() / bar_duration.as_secs_f32()).clamp(0.0, 1.0);

        while next_event < rendered.events.len()
            && beat_to_duration(
                rendered.events[next_event].beat_pos,
                program.effective_bpm(),
            ) <= elapsed
        {
            let event = &rendered.events[next_event];
            if let Some(client) = client {
                client
                    .play_event(event)
                    .map_err(|error| error.to_string())?;
            }
            visual_state.push_trigger(event, now);
            next_event += 1;
        }

        let poll = file_watcher.poll();
        for error in poll.errors {
            push_log(logs, format!("Watcher error: {error}"));
            *watcher_status = format!("ERROR ({error})");
        }
        if poll.changed {
            *pending_reload = true;
            if !watcher_status.starts_with("ERROR") {
                *watcher_status = "CHANGE DETECTED".to_string();
            }
        } else if !*pending_reload && !watcher_status.starts_with("ERROR") {
            *watcher_status = "ARMED".to_string();
        }

        visual_state.update(program, now);
        draw_dashboard(
            terminal,
            program,
            rendered,
            DashboardRuntime {
                osc_status: osc_status.to_string(),
                watcher_status: watcher_status.clone(),
                bar_index,
                bar_progress: progress,
                pending_reload: *pending_reload,
                master_scope: visual_state.master_scope.clone(),
                layer_visuals: visual_state.layer_visuals(),
            },
            logs,
        )
        .map_err(|error| error.to_string())?;

        if should_quit().map_err(|error| error.to_string())? {
            return Ok(true);
        }

        if elapsed >= bar_duration {
            break;
        }

        next_frame += DASHBOARD_FRAME_TIME;
        let sleep_until = next_frame.min(bar_start + bar_duration);
        let now = Instant::now();
        if sleep_until > now {
            thread::sleep(sleep_until - now);
        }
    }

    Ok(false)
}

fn beat_to_duration(beat_pos: f32, bpm: f32) -> Duration {
    let seconds = beat_pos as f64 * 60.0 / bpm as f64;
    Duration::from_secs_f64(seconds)
}

#[derive(Debug, Clone)]
struct DashboardVisualState {
    pulses: Vec<LayerPulse>,
    scope_history: BTreeMap<String, String>,
    master_scope: String,
}

impl DashboardVisualState {
    fn new(program: &Program) -> Self {
        let mut state = Self {
            pulses: Vec::new(),
            scope_history: BTreeMap::new(),
            master_scope: " ".repeat(SCOPE_HISTORY_WIDTH),
        };
        state.sync_layers(program);
        state
    }

    fn sync_layers(&mut self, program: &Program) {
        let existing = &mut self.scope_history;
        for layer in &program.layers {
            existing
                .entry(layer.name.0.clone())
                .or_insert_with(|| " ".repeat(SCOPE_HISTORY_WIDTH));
        }
        existing.retain(|name, _| program.layers.iter().any(|layer| &layer.name.0 == name));
    }

    fn push_trigger(&mut self, event: &ScheduledEvent, now: Instant) {
        self.pulses.push(LayerPulse {
            layer: event.layer.0.clone(),
            at: now,
            gain: event_gain(event),
        });
    }

    fn update(&mut self, program: &Program, now: Instant) {
        self.pulses.retain(|pulse| {
            now.saturating_duration_since(pulse.at).as_secs_f32() <= LEVEL_DECAY_SECONDS * 4.0
        });
        let mut per_layer = BTreeMap::<String, f32>::new();
        let mut master = 0.0f32;

        for pulse in &self.pulses {
            let age = now.saturating_duration_since(pulse.at).as_secs_f32();
            let level = (1.0 - age / LEVEL_DECAY_SECONDS).clamp(0.0, 1.0) * pulse.gain;
            if level <= 0.0 {
                continue;
            }
            *per_layer.entry(pulse.layer.clone()).or_default() += level;
            master += level;
        }

        for layer in &program.layers {
            let level = per_layer
                .get(&layer.name.0)
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);
            let entry = self
                .scope_history
                .entry(layer.name.0.clone())
                .or_insert_with(|| " ".repeat(SCOPE_HISTORY_WIDTH));
            push_scope_sample(entry, level);
        }

        push_scope_sample(&mut self.master_scope, master.clamp(0.0, 1.0));
    }

    fn layer_visuals(&self) -> BTreeMap<String, fin::dashboard::LayerVisual> {
        self.scope_history
            .iter()
            .map(|(layer, scope)| {
                let level = scope_level(scope);
                (
                    layer.clone(),
                    fin::dashboard::LayerVisual {
                        level,
                        scope: scope.clone(),
                    },
                )
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
struct LayerPulse {
    layer: String,
    at: Instant,
    gain: f32,
}

fn push_scope_sample(scope: &mut String, level: f32) {
    if scope.chars().count() >= SCOPE_HISTORY_WIDTH {
        let mut chars = scope.chars();
        chars.next();
        *scope = chars.collect();
    }
    scope.push(level_char(level));
}

fn level_char(level: f32) -> char {
    match (level.clamp(0.0, 1.0) * 5.0).round() as u8 {
        0 => ' ',
        1 => '.',
        2 => ':',
        3 => '=',
        4 => '#',
        _ => '@',
    }
}

fn scope_level(scope: &str) -> f32 {
    scope
        .chars()
        .last()
        .map(|ch| match ch {
            ' ' => 0.0,
            '.' => 0.2,
            ':' => 0.4,
            '=' => 0.6,
            '#' => 0.8,
            '@' => 1.0,
            _ => 0.0,
        })
        .unwrap_or(0.0)
}
