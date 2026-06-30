#![deny(unsafe_code)]
//! `art-engine-episode` — turn an audio file + a storyboard `.ron` into
//! a YouTube-Shorts-shaped mp4.
//!
//! Subcommands:
//!
//! - `plan`   — validate a storyboard and emit a summary
//! - `render` — render storyboard + audio to mp4
//!
//! Both surface typed exit codes; `plan --format json` emits a stable
//! `PlanReport` schema (see `plan.rs::SCHEMA_VERSION`).

mod audio_features;
mod doctor;
mod error;
#[cfg(feature = "gpu")]
mod meta_ass;
mod plan;
#[cfg(feature = "gpu")]
mod render;

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::process;

#[derive(Parser)]
#[command(
    name = "art-engine-episode",
    about = "Render a storyboard.ron + audio into an mp4 episode"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate a storyboard.ron and print a summary.
    ///
    /// `--format json` emits a stable `PlanReport` document (the
    /// `plan-episode` capability's machine-readable output surface);
    /// `--format human` (the default) prints a brief table on stdout.
    Plan {
        /// Path to the storyboard.ron file.
        storyboard: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
    /// Probe environment readiness: ffmpeg, libEGL, GL pipeline, tempdir.
    ///
    /// Exit 0 if everything's OK; exit 6 (unavailable dependency) if any
    /// probe fails. Useful when a render fails and you want to know
    /// whether the problem is your storyboard or your environment.
    Doctor {
        /// Output format.
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
    /// Render storyboard + audio to mp4.
    Render {
        /// Path to the storyboard.ron file.
        storyboard: PathBuf,
        /// Output mp4 path.
        #[arg(short, long, default_value = "episode.mp4")]
        output: PathBuf,
        /// Per-frame audio features (JSON written by extract_features.py).
        #[arg(long)]
        features: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
    /// Human-readable text on stdout (the default).
    Human,
    /// `PlanReport` JSON document on stdout, one complete object.
    Json,
}

fn main() {
    let cli = Cli::parse();
    let rc = match cli.command {
        Command::Plan { storyboard, format } => cmd_plan(&storyboard, format),
        Command::Doctor { format } => cmd_doctor(format),
        Command::Render {
            storyboard,
            output,
            features,
        } => cmd_render(&storyboard, &output, &features),
    };
    process::exit(rc);
}

/// Run every doctor probe, emit the report, and exit with the appropriate code.
fn cmd_doctor(format: OutputFormat) -> i32 {
    let report = doctor::run();
    match format {
        OutputFormat::Human => print!("{}", doctor::human_format(&report)),
        OutputFormat::Json => match serde_json::to_string_pretty(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("error: serialise doctor report: {e}");
                return 10;
            }
        },
    }
    doctor::exit_code(&report)
}

/// Spec exit codes:
///   0  success
///   2  CLI parser usage (handled by clap)
///   3  invalid input (StoryboardError)
///   10 internal error (JSON serialisation failure — should be unreachable)
fn cmd_plan(path: &std::path::Path, format: OutputFormat) -> i32 {
    match plan::build_plan(path) {
        Ok(report) => match format {
            OutputFormat::Human => {
                // Human output goes to stdout (it IS the command's result).
                // Diagnostics and progress go to stderr per spec.
                print!("{}", plan::human_format(&report));
                0
            }
            OutputFormat::Json => match serde_json::to_string_pretty(&report) {
                Ok(s) => {
                    println!("{s}");
                    0
                }
                Err(e) => {
                    eprintln!("error: serialise plan report: {e}");
                    10
                }
            },
        },
        Err(e) => {
            eprintln!("error: {e}");
            3
        }
    }
}

#[cfg(feature = "gpu")]
fn cmd_render(
    storyboard: &std::path::Path,
    output: &std::path::Path,
    features: &std::path::Path,
) -> i32 {
    match render::render(storyboard, output, features) {
        Ok(()) => 0,
        Err(e) => {
            // Typed exit codes per the foundation spec's exit-code
            // policy. See `episode/src/error.rs::RenderError::exit_code`.
            eprintln!("error: {e}");
            e.exit_code()
        }
    }
}

#[cfg(not(feature = "gpu"))]
fn cmd_render(
    _storyboard: &std::path::Path,
    _output: &std::path::Path,
    _features: &std::path::Path,
) -> i32 {
    eprintln!("error: built without `gpu` feature; rebuild with --features gpu");
    1
}
