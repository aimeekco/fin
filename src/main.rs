use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use clap::{Parser, Subcommand};
use fin::model::{Meter, Program, ScheduledEvent};
use fin::osc::OscClient;
use fin::parser::parse_program;
use fin::scheduler::{format_events, schedule_bar};
use fin::sounds::{format_sounds_report, load_sounds_report};

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
    Sounds,
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
        Command::Sounds => list_sounds(),
    }
}

fn run_file(path: PathBuf, host: String, port: u16, no_play: bool) -> Result<(), String> {
    ensure_metl_extension(&path)?;
    let loaded = load_track(&path)?;
    let rendered = render_bar(&loaded.program, 0)?;

    print_schedule(&rendered.output);

    if !no_play && !rendered.events.is_empty() {
        let client = OscClient::connect(&host, port).map_err(|error| error.to_string())?;
        client
            .play_bar(&loaded.program, &rendered.events)
            .map_err(|error| error.to_string())?;
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
    let client = if no_play {
        None
    } else {
        Some(OscClient::connect(&host, port).map_err(|error| error.to_string())?)
    };
    let mut last_reload_error: Option<String> = None;

    println!("watch load {}", path.display());
    print_schedule(&render_bar(&loaded.program, 0)?.output);

    if bars == Some(0) {
        return Ok(());
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

        match load_track(&path) {
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

fn render_bar(program: &Program, bar_index: usize) -> Result<RenderedBar, String> {
    let events =
        schedule_bar(program, Meter::default(), bar_index).map_err(|error| error.to_string())?;
    let output = format_events(program, &events);
    Ok(RenderedBar { events, output })
}

struct LoadedTrack {
    source: String,
    program: Program,
}

struct RenderedBar {
    events: Vec<ScheduledEvent>,
    output: String,
}
