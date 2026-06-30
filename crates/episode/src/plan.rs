//! Plan reports — what `art-engine-episode plan <storyboard>` produces.
//!
//! Defines the typed `PlanReport` struct that is the compatibility
//! surface for the `plan` capability's JSON output. The `schema_version`
//! field on the struct is part of that surface — incrementing it
//! requires a capability-contract update per LAW-012 (no spec drift).

use art_engine_storyboard::{schedule, Backdrop, Foreground, Storyboard, StoryboardError, Transition};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Stable schema version for the JSON output. Bump when adding/removing
/// fields or changing field semantics; document the bump in the
/// `plan-episode.yaml` capability contract.
pub const SCHEMA_VERSION: u32 = 1;

/// Top-level plan report. Serialised directly to JSON via `serde_json`.
#[derive(Debug, Serialize)]
pub struct PlanReport {
    pub schema_version: u32,
    pub storyboard_path: PathBuf,
    pub audio_path: PathBuf,
    pub subtitles_path: Option<PathBuf>,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    pub duration_s: f32,
    pub frame_count: u32,
    pub scene_count: usize,
    pub has_header: bool,
    pub has_sigil: bool,
    pub has_scene_pips: bool,
    pub scenes: Vec<ScenePlan>,
}

/// Per-scene summary embedded in `PlanReport.scenes`. Compact by design —
/// don't include every shader parameter or palette stop here. Agents
/// that want the full storyboard should read the `.ron` file directly.
#[derive(Debug, Serialize)]
pub struct ScenePlan {
    pub idx: u32,
    pub start_s: f32,
    pub end_s: f32,
    pub duration_s: f32,
    pub backdrop: String,        // short_backdrop output
    pub transition_in: String,   // short_transition output
    pub foreground_count: usize,
    pub foreground_kinds: Vec<String>,
    pub has_post: bool,
}

/// Build a `PlanReport` from a storyboard path. Mirrors the existing
/// `cmd_plan` behaviour but returns a typed value instead of printing.
pub fn build_plan(path: &Path) -> Result<PlanReport, StoryboardError> {
    let sb = Storyboard::load(path)?;
    let scenes = sb
        .scenes
        .iter()
        .enumerate()
        .map(|(i, sc)| ScenePlan {
            idx: i as u32,
            start_s: sc.start,
            end_s: sc.end,
            duration_s: sc.end - sc.start,
            backdrop: short_backdrop(&sc.backdrop).into(),
            transition_in: short_transition(&sc.transition_in).into(),
            foreground_count: sc.foreground.len(),
            foreground_kinds: sc.foreground.iter().map(short_foreground).map(Into::into).collect(),
            has_post: sc.post.grain.is_some()
                || sc.post.vignette.is_some()
                || sc.post.color_grade.is_some(),
        })
        .collect();
    Ok(PlanReport {
        schema_version: SCHEMA_VERSION,
        storyboard_path: path.to_path_buf(),
        audio_path: sb.audio.clone(),
        subtitles_path: sb.subtitles.clone(),
        fps: sb.fps,
        width: sb.width,
        height: sb.height,
        duration_s: sb.duration(),
        frame_count: schedule::frame_count(&sb),
        scene_count: sb.scenes.len(),
        has_header: sb.header.is_some(),
        has_sigil: sb.sigil.is_some(),
        has_scene_pips: sb.scene_pips.is_some(),
        scenes,
    })
}

/// Human-readable formatting of a plan report — the historical
/// `cmd_plan` text output, kept stable for documentation but treated
/// as advisory rather than a parseable contract.
pub fn human_format(r: &PlanReport) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(
        &mut out,
        "loaded {}: {} scenes, {:.2}s @ {}fps, {}x{}",
        r.storyboard_path.display(),
        r.scene_count,
        r.duration_s,
        r.fps,
        r.width,
        r.height
    )
    .unwrap();
    for sc in &r.scenes {
        writeln!(
            &mut out,
            "  [{:>3}] {:>6.2}s → {:>6.2}s ({:>5.2}s)  backdrop={:?}  fg={}",
            sc.idx, sc.start_s, sc.end_s, sc.duration_s, sc.backdrop, sc.foreground_count
        )
        .unwrap();
    }
    writeln!(&mut out, "frame_count={}", r.frame_count).unwrap();
    out
}

/// Stable short name for a `Backdrop` variant. The CLI exposes these
/// strings, so renaming requires a capability-contract bump.
pub fn short_backdrop(b: &Backdrop) -> &'static str {
    match b {
        Backdrop::Flow { .. } => "Flow",
        Backdrop::Solid { .. } => "Solid",
        Backdrop::Voronoi { .. } => "Voronoi",
        Backdrop::NoiseStatic { .. } => "NoiseStatic",
        Backdrop::Lattice { .. } => "Lattice",
        Backdrop::Mandala { .. } => "Mandala",
        Backdrop::Concentric { .. } => "Concentric",
        Backdrop::Strands { .. } => "Strands",
        Backdrop::Wave { .. } => "Wave",
        Backdrop::Spiral { .. } => "Spiral",
        Backdrop::Halftone { .. } => "Halftone",
        Backdrop::Crosshatch { .. } => "Crosshatch",
        Backdrop::Topo { .. } => "Topo",
        Backdrop::Aurora { .. } => "Aurora",
        Backdrop::Sun { .. } => "Sun",
        Backdrop::Particles { .. } => "Particles",
        Backdrop::Branch { .. } => "Branch",
        Backdrop::Caustics { .. } => "Caustics",
        Backdrop::Phyllotaxis { .. } => "Phyllotaxis",
        Backdrop::Constellation { .. } => "Constellation",
        Backdrop::VectorField { .. } => "VectorField",
        Backdrop::Crystal { .. } => "Crystal",
        Backdrop::Smoke { .. } => "Smoke",
        Backdrop::Moire { .. } => "Moire",
        Backdrop::Ripple { .. } => "Ripple",
        Backdrop::Plasma { .. } => "Plasma",
        Backdrop::Bokeh { .. } => "Bokeh",
        Backdrop::Mosaic { .. } => "Mosaic",
    }
}

/// Stable short name for a `Foreground` variant.
pub fn short_foreground(fg: &Foreground) -> &'static str {
    match fg {
        Foreground::KaraokeCaptions => "KaraokeCaptions",
        Foreground::TitleCard { .. } => "TitleCard",
        Foreground::PullQuote { .. } => "PullQuote",
        Foreground::EndCard { .. } => "EndCard",
        Foreground::Arrow { .. } => "Arrow",
        Foreground::Annotation { .. } => "Annotation",
        Foreground::Decomposition { .. } => "Decomposition",
        Foreground::Highlight { .. } => "Highlight",
        Foreground::Comparison { .. } => "Comparison",
    }
}

/// Stable short name for a `Transition` variant.
pub fn short_transition(t: &Transition) -> &'static str {
    match t {
        Transition::HardCut => "HardCut",
        Transition::Crossfade { .. } => "Crossfade",
        Transition::GlitchBreak { .. } => "GlitchBreak",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use art_engine_storyboard::{Backdrop, PaletteRef};
    use std::io::Write;

    fn write_temp_storyboard(contents: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".ron")
            .tempfile()
            .unwrap();
        f.write_all(contents.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    fn sample_ron() -> &'static str {
        r#"Storyboard(
            audio: "a.m4a",
            fps: 30,
            width: 1080,
            height: 1920,
            subtitles: Some("subs.ass"),
            header: Some(HeaderSpec(text: "Hook")),
            sigil: Some(SigilSpec(handle: "@x")),
            scene_pips: Some(ScenePipsSpec(position: Top)),
            scenes: [
                Scene(
                    start: 0.0, end: 5.0,
                    backdrop: Flow(palette: TealAmber, intensity: 1.0, seed: 11),
                    foreground: [KaraokeCaptions],
                    transition_in: HardCut,
                    post: (grain: Some(0.02), vignette: None, color_grade: None),
                ),
                Scene(
                    start: 5.0, end: 10.0,
                    backdrop: Solid(color: (0.0, 0.0, 0.0)),
                    foreground: [],
                    transition_in: Crossfade(dur: 0.5),
                    post: (grain: None, vignette: None, color_grade: None),
                ),
            ],
        )"#
    }

    #[test]
    fn schema_version_is_present_and_stable() {
        // If this assertion ever fires the team should have updated
        // plan-episode.yaml's compatibility section.
        assert_eq!(SCHEMA_VERSION, 1);
    }

    #[test]
    fn build_plan_roundtrips_scene_summary() {
        let f = write_temp_storyboard(sample_ron());
        let r = build_plan(f.path()).expect("plan");
        assert_eq!(r.scene_count, 2);
        assert_eq!(r.scenes[0].backdrop, "Flow");
        assert_eq!(r.scenes[0].transition_in, "HardCut");
        assert_eq!(r.scenes[0].foreground_kinds, vec!["KaraokeCaptions"]);
        assert!(r.scenes[0].has_post);
        assert_eq!(r.scenes[1].backdrop, "Solid");
        assert_eq!(r.scenes[1].transition_in, "Crossfade");
        assert_eq!(r.scenes[1].foreground_count, 0);
        assert!(!r.scenes[1].has_post);
        assert_eq!(r.fps, 30);
        assert_eq!(r.frame_count, 300); // 10s × 30fps
        assert!(r.has_header && r.has_sigil && r.has_scene_pips);
    }

    #[test]
    fn json_output_includes_schema_version_at_top_level() {
        let f = write_temp_storyboard(sample_ron());
        let r = build_plan(f.path()).unwrap();
        let json = serde_json::to_string_pretty(&r).unwrap();
        // schema_version is the first field in the struct definition,
        // so it appears near the top of the pretty-printed JSON.
        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.contains("\"backdrop\": \"Flow\""));
        assert!(json.contains("\"foreground_kinds\""));
    }

    #[test]
    fn short_helpers_cover_all_backdrop_variants() {
        // Synthesise one of every backdrop kind to ensure short_backdrop
        // doesn't accidentally drop a variant if someone adds one.
        let variants: Vec<Backdrop> = vec![
            Backdrop::Flow { palette: PaletteRef::TealAmber, intensity: 1.0, seed: 0 },
            Backdrop::Solid { color: [0.0; 3] },
            Backdrop::Voronoi { scale: 1.0, edge_width: 0.0, jitter: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::NoiseStatic { intensity: 0.0, density: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Lattice { density: 0.0, thickness: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Mandala { segments: 0.0, freq: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Concentric { freq: 0.0, speed: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Strands { density: 0.0, thickness: 0.0, jitter: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Wave { density: 0.0, freq: 0.0, amplitude: 0.0, speed: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Spiral { arms: 0.0, tightness: 0.0, speed: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Halftone { cell: 0.0, strength: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Crosshatch { spacing: 0.0, thickness: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Topo { scale: 0.0, density: 0.0, thickness: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Aurora { curtains: 0.0, speed: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Sun { radius: 0.0, rays: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Particles { count: 0.0, glow: 0.0, speed: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Branch { branches: 0.0, thickness: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Caustics { scale: 0.0, sharpness: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Phyllotaxis { count: 0.0, radius_scale: 0.0, seed_radius: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Constellation { node_glow: 0.0, edge_glow: 0.0, edge_strength: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::VectorField { scale: 0.0, freq: 0.0, density: 0.0, thickness: 0.0, dash_speed: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Crystal { scale: 0.0, levels: 0.0, edge_width: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Smoke { scale: 0.0, warp: 0.0, speed: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Moire { freq: 0.0, angle_delta: 0.0, thickness: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Ripple { freq: 0.0, speed: 0.0, decay: 0.0, sharpness: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Plasma { count: 0.0, radius: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Bokeh { count: 0.0, radius: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
            Backdrop::Mosaic { grid: 0.0, levels: 0.0, gap: 0.0, intensity: 0.0, palette: PaletteRef::TealAmber },
        ];
        let names: Vec<&str> = variants.iter().map(short_backdrop).collect();
        // All names non-empty and distinct.
        assert!(names.iter().all(|n| !n.is_empty()));
        let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
        assert_eq!(unique.len(), names.len(), "duplicate short_backdrop names");
    }
}
