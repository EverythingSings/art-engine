//! Closed typed-error enums for the episode crate.
//!
//! Replaces `Result<_, String>` boundaries with `thiserror`-derived
//! enums per spec LAW-005 (typed boundaries) and LAW-010 (no silent
//! failure). Each enum maps to a CLI exit code via `exit_code()` so
//! `main.rs` can surface failures with the spec's documented
//! error categories.

use std::path::PathBuf;
use thiserror::Error;

/// Errors loading or validating the per-frame audio-feature track
/// produced by `extract_features.py`.
#[derive(Debug, Error)]
pub enum FeatureError {
    /// The features file couldn't be read from disk.
    #[error("read features file {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The features file isn't valid JSON, or the schema is wrong.
    #[error("parse features json: {0}")]
    Parse(#[from] serde_json::Error),
    /// The three arrays must be the same length; one is wrong.
    #[error(
        "feature track lengths disagree: rms={rms} onset={onset} centroid={centroid}"
    )]
    LengthMismatch {
        rms: usize,
        onset: usize,
        centroid: usize,
    },
}

/// Errors building the combined ASS subtitle file (`meta.ass`).
#[derive(Debug, Error)]
pub enum MetaAssError {
    /// Couldn't read the karaoke ASS we merge events from.
    #[error("read karaoke ass {path}: {source}")]
    ReadKaraoke {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Couldn't create the parent directory for the meta.ass output.
    #[error("create meta.ass directory {path}: {source}")]
    Mkdir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Couldn't write the resulting meta.ass.
    #[error("write meta.ass {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Top-level errors from `episode render`. Every internal `String`
/// boundary previously used here is now one of these variants.
///
/// Each variant has a documented exit code via [`Self::exit_code`],
/// aligned with the spec's exit-code policy in
/// `engineering/rust_core_cli_iced_spec_seed_v0_3_stable.yaml`.
#[derive(Debug, Error)]
pub enum RenderError {
    /// Storyboard parse / validation upstream from us.
    #[error("storyboard: {0}")]
    Storyboard(#[from] art_engine_storyboard::StoryboardError),
    /// Per-frame audio features track.
    #[error("features: {0}")]
    Features(#[from] FeatureError),
    /// Building the combined ASS subtitle file.
    #[error("meta_ass: {0}")]
    MetaAss(#[from] MetaAssError),
    /// Storyboard fps doesn't match the features file's fps.
    #[error("fps mismatch: storyboard.fps={storyboard} features.fps={features}")]
    FpsMismatch { storyboard: u32, features: u32 },
    /// Canvas allocation failed (typically: zero-size canvas).
    #[error("canvas init: {0}")]
    Canvas(#[source] art_engine_core::EngineError),
    /// Couldn't create the headless GL context — usually libEGL missing
    /// or no DRI3 device available.
    #[error("headless GL init: {0}")]
    HeadlessGl(art_engine_core::render::headless::HeadlessError),
    /// Pipeline (shaders + FBOs) failed to initialise.
    #[error("pipeline init: {0}")]
    PipelineInit(art_engine_core::render::pipeline::PipelineError),
    /// Palette LUT couldn't be rebaked.
    #[error("palette rebake: {0}")]
    PaletteRebake(art_engine_core::render::pipeline::PipelineError),
    /// Path argument can't be passed to ffmpeg because it's not utf-8.
    #[error("path not utf-8: {path:?}")]
    NonUtf8Path { path: PathBuf },
    /// ffmpeg binary couldn't be spawned — typically missing from PATH.
    #[error("spawn ffmpeg: {0}")]
    FfmpegSpawn(#[source] std::io::Error),
    /// We asked for stdin but ffmpeg didn't give us one.
    #[error("ffmpeg stdin handle missing")]
    FfmpegStdin,
    /// Waiting for ffmpeg to exit itself failed.
    #[error("ffmpeg wait: {0}")]
    FfmpegWait(#[source] std::io::Error),
    /// ffmpeg ran but returned non-zero — usually a bad encode or
    /// input issue. Inspect stderr (we don't capture it).
    #[error("ffmpeg exited with {status}")]
    FfmpegExit { status: std::process::ExitStatus },
    /// Failed to render a specific frame through the GL pipeline.
    #[error("render_frame[{idx}]: {source}")]
    RenderFrame {
        idx: u32,
        #[source]
        source: art_engine_core::render::pipeline::PipelineError,
    },
    /// The "backdrop" layer disappeared from the canvas mid-render —
    /// internal bug (we own the canvas).
    #[error("backdrop layer missing from canvas (internal)")]
    BackdropLayerMissing,
    /// The backdrop layer has no effect attached — internal bug.
    #[error("backdrop effect missing from layer (internal)")]
    BackdropEffectMissing,
    /// The backdrop effect's params aren't a JSON object — internal bug.
    #[error("backdrop params not a JSON object (internal)")]
    BackdropParamsShape,
    /// Allocating the dummy Field for the GL pipeline failed.
    #[error("field allocation: {0}")]
    FieldAlloc(#[source] art_engine_core::EngineError),
}

impl RenderError {
    /// CLI exit code per the foundation spec's exit-code policy.
    pub fn exit_code(&self) -> i32 {
        use RenderError::*;
        match self {
            // Invalid input — recoverable, the user fixes the args.
            FpsMismatch { .. }
            | NonUtf8Path { .. }
            | Storyboard(_)
            | Features(_) => 3,
            // Unavailable dependency — ffmpeg or libEGL missing.
            FfmpegSpawn(_) | HeadlessGl(_) => 6,
            // ffmpeg exited non-zero or other general failure.
            FfmpegExit { .. } | FfmpegWait(_) => 1,
            // Anything else is an internal error.
            MetaAss(_) | Canvas(_) | PipelineInit(_) | PaletteRebake(_)
            | FfmpegStdin | RenderFrame { .. } | BackdropLayerMissing
            | BackdropEffectMissing | BackdropParamsShape | FieldAlloc(_) => 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_categories_match_spec() {
        // A representative variant per category — guards against the
        // policy drifting silently if someone adds a new variant.
        assert_eq!(
            RenderError::FpsMismatch {
                storyboard: 30,
                features: 24,
            }
            .exit_code(),
            3,
            "invalid input should be exit 3"
        );
        assert_eq!(
            RenderError::FfmpegStdin.exit_code(),
            10,
            "internal failure should be exit 10"
        );
    }

    #[test]
    fn feature_error_length_mismatch_formats_helpfully() {
        let msg = format!(
            "{}",
            FeatureError::LengthMismatch {
                rms: 100,
                onset: 99,
                centroid: 100,
            }
        );
        assert!(msg.contains("100"));
        assert!(msg.contains("99"));
        assert!(msg.contains("centroid"));
    }
}
