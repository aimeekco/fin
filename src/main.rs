use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use clap::{Parser, Subcommand};
use fin::model::{Meter, Program, ScheduledEvent};
use fin::osc::OscClient;
use fin::parser::parse_program;
use fin::scheduler::{format_events, schedule_bar};

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
    }
}

fn run_file(path: PathBuf, host: String, port: u16, no_play: bool) -> Result<(), String> {
    ensure_metl_extension(&path)?;
    let loaded = load_track(&path)?;

    print_schedule(&loaded.output);

    if !no_play && !loaded.events.is_empty() {
        let client = OscClient::connect(&host, port).map_err(|error| error.to_string())?;
        client
            .play_bar(&loaded.program, &loaded.events)
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
    print_schedule(&loaded.output);

    if bars == Some(0) {
        return Ok(());
    }

    let mut completed_bars = 0usize;
    loop {
        if let Some(client) = &client {
            if loaded.events.is_empty() {
                thread::sleep(bar_duration(&loaded.program, Meter::default()));
            } else {
                client
                    .play_bar(&loaded.program, &loaded.events)
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
                    print_schedule(&next.output);
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
    let events = schedule_bar(&program, Meter::default()).map_err(|error| error.to_string())?;
    let output = format_events(&program, &events);

    Ok(LoadedTrack {
        source,
        program,
        events,
        output,
    })
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

struct LoadedTrack {
    source: String,
    program: Program,
    events: Vec<ScheduledEvent>,
    output: String,
}
