use std::path::PathBuf;

use clap::{Parser, Subcommand};
use fin::model::Meter;
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
    Run { path: PathBuf },
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
        Command::Run { path } => run_file(path),
    }
}

fn run_file(path: PathBuf) -> Result<(), String> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("metl") => {}
        _ => return Err("expected a `.metl` source file".to_string()),
    }

    let source = std::fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let program = parse_program(&source).map_err(|error| error.to_string())?;
    let events = schedule_bar(&program, Meter::default()).map_err(|error| error.to_string())?;
    let output = format_events(&program, &events);

    if !output.is_empty() {
        println!("{output}");
    }

    Ok(())
}
