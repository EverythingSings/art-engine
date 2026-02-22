#![deny(unsafe_code)]
//! CLI binary for the art-engine generative art system.
//!
//! Subcommands:
//! - `render <engine>` — run an engine N steps, write PNG
//! - `list` — print available engines and palettes

mod error;

use art_engine_core::{Engine, Palette};
use art_engine_engines::EngineKind;
use clap::{Parser, Subcommand};
use error::CliError;
use std::path::PathBuf;
use std::process;

#[derive(Parser)]
#[command(name = "art-engine", about = "Generative art engine CLI")]
struct Cli {
    /// Output as JSON instead of human-readable text.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run an engine for N steps and write a PNG snapshot.
    Render {
        /// Engine name (e.g. "gray-scott").
        engine: String,

        /// Canvas width in pixels.
        #[arg(short = 'W', long, default_value_t = 256)]
        width: usize,

        /// Canvas height in pixels.
        #[arg(short = 'H', long, default_value_t = 256)]
        height: usize,

        /// Number of simulation steps.
        #[arg(short, long, default_value_t = 1000)]
        steps: usize,

        /// PRNG seed for deterministic output.
        #[arg(long, default_value_t = 42)]
        seed: u64,

        /// Palette name (ocean, neon, earth, monochrome, vapor, fire).
        #[arg(short, long, default_value = "ocean")]
        palette: String,

        /// Output file path.
        #[arg(short, long, default_value = "output.png")]
        output: PathBuf,

        /// Engine parameters as a JSON string.
        #[arg(long, default_value = "{}")]
        params: String,
    },
    /// List available engines and palettes.
    List,
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::List => {
            let engines = EngineKind::list_engines();
            let palettes = Palette::list_names();
            if cli.json {
                let info = serde_json::json!({
                    "engines": engines,
                    "palettes": palettes,
                });
                println!("{}", serde_json::to_string_pretty(&info)?);
            } else {
                println!("Engines:");
                for name in engines {
                    println!("  {name}");
                }
                println!("Palettes:");
                println!("  {}", palettes.join(", "));
            }
        }
        Command::Render {
            engine,
            width,
            height,
            steps,
            seed,
            palette,
            output,
            params,
        } => {
            let params: serde_json::Value = serde_json::from_str(&params)
                .map_err(|e| CliError::Input(format!("invalid --params JSON: {e}")))?;

            let palette =
                Palette::from_name(&palette).map_err(|e| CliError::Input(e.to_string()))?;

            let mut eng = EngineKind::from_name(&engine, width, height, seed, &params)?;

            (0..steps).try_for_each(|_| eng.step())?;

            art_engine_engines::snapshot::write_png(eng.field(), &palette, &output)?;

            if cli.json {
                let info = serde_json::json!({
                    "engine": engine,
                    "width": width,
                    "height": height,
                    "steps": steps,
                    "seed": seed,
                    "output": output.display().to_string(),
                });
                println!("{}", serde_json::to_string_pretty(&info)?);
            } else {
                eprintln!(
                    "rendered {engine} ({width}x{height}, {steps} steps, seed {seed}) -> {}",
                    output.display()
                );
            }
        }
    }

    Ok(())
}

fn main() {
    let cli = Cli::parse();
    let json_mode = cli.json;
    if let Err(e) = run(cli) {
        if json_mode {
            let j = serde_json::json!({"error": e.to_string(), "exit_code": e.exit_code()});
            eprintln!("{}", serde_json::to_string_pretty(&j).unwrap_or_default());
        } else {
            eprintln!("error: {e}");
        }
        process::exit(e.exit_code());
    }
}
