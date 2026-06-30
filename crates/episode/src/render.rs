//! Frame-loop renderer: storyboard + features → ffmpeg-piped mp4.
//!
//! Walks the storyboard timeline at the configured fps. Per frame:
//!
//! 1. Resolve the active scene (the unique scene where `start <= t < end`).
//! 2. If we just crossed a scene boundary, rewrite the canvas's single
//!    "backdrop" effect to the new scene's shader + parameters.
//! 3. Update per-frame uniforms (`u_time`, audio features).
//! 4. Call `Pipeline::render_frame` → RGBA8 bytes.
//! 5. Pipe the bytes to ffmpeg's stdin.
//!
//! ffmpeg gets the original audio as a second input and applies its
//! `subtitles=` filter against a *combined* ASS file we generate from
//! the karaoke captions + the storyboard's `header` and `sigil` —
//! three overlay families burned in a single ASS pass.

#![cfg(feature = "gpu")]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use art_engine_core::canvas::{Canvas, ContentType, Layer, ShaderEffectDesc};
use art_engine_core::field::Field;
use art_engine_core::palette::Palette;
use art_engine_core::render::headless::create_headless_context;
use art_engine_core::render::pipeline::Pipeline;
use art_engine_core::Srgb;
use art_engine_storyboard::{design, schedule, Backdrop, PaletteRef, Scene, Storyboard};

use crate::audio_features::FeatureTrack;
use crate::error::RenderError;
use crate::meta_ass;

const BACKDROP_LAYER: &str = "backdrop";

/// Render the storyboard and audio into the given mp4 path.
pub fn render(
    storyboard_path: &Path,
    output_path: &Path,
    features_path: &Path,
) -> Result<(), RenderError> {
    let sb = Storyboard::load(storyboard_path)?;
    let features = FeatureTrack::load(features_path)?;

    if features.fps != sb.fps {
        return Err(RenderError::FpsMismatch {
            storyboard: sb.fps,
            features: features.fps,
        });
    }

    let storyboard_dir = storyboard_path.parent().unwrap_or_else(|| Path::new("."));
    let resolved_audio = resolve(storyboard_dir, &sb.audio);
    let resolved_karaoke = sb.subtitles.as_ref().map(|s| resolve(storyboard_dir, s));

    // Combine karaoke + header + sigil into one ass next to the input
    // karaoke (or next to the storyboard if no karaoke is given).
    let meta_ass_path = meta_ass_target(storyboard_dir, &sb, resolved_karaoke.as_deref())?;
    meta_ass::build_meta_ass(&sb, resolved_karaoke.as_deref(), &meta_ass_path)
        .map_err(RenderError::MetaAss)?;

    let n_frames = schedule::frame_count(&sb);
    let w = sb.width;
    let h = sb.height;

    eprintln!(
        "[episode] {} → {} ({} frames @ {}fps, {}x{}, {} scenes)",
        storyboard_path.display(),
        output_path.display(),
        n_frames,
        sb.fps,
        w,
        h,
        sb.scenes.len()
    );

    // Build a Canvas with one backdrop layer. The single effect on this
    // layer is rewritten in-place each time a new scene starts.
    let mut canvas = Canvas::new(
        w as usize,
        h as usize,
        Srgb { r: 0.0, g: 0.0, b: 0.0 },
    )
    .map_err(RenderError::Canvas)?;
    let initial_effect = effect_for_scene(&sb.scenes[0], &sb, 0.0);
    canvas
        .add_layer(Layer::new(BACKDROP_LAYER, ContentType::Field).with_effect(initial_effect))
        .map_err(RenderError::Canvas)?;

    let field = Field::filled(w as usize, h as usize, 0.0).map_err(RenderError::FieldAlloc)?;
    let palette = Palette::amber();

    let gpu = create_headless_context().map_err(RenderError::HeadlessGl)?;
    let mut pipeline = Pipeline::new(gpu.context(), w, h).map_err(RenderError::PipelineInit)?;
    pipeline
        .rebake_palette(gpu.context(), &palette)
        .map_err(RenderError::PaletteRebake)?;

    // ffmpeg child: pipe rgba8 video + mux audio + burn the combined ass.
    let mut cmd = Command::new("ffmpeg");
    let audio_str = resolved_audio
        .to_str()
        .ok_or_else(|| RenderError::NonUtf8Path {
            path: resolved_audio.clone(),
        })?;
    cmd.args([
        "-y",
        "-f", "rawvideo",
        "-pix_fmt", "rgba",
        "-s", &format!("{w}x{h}"),
        "-r", &sb.fps.to_string(),
        "-i", "pipe:0",
        "-i", audio_str,
    ]);

    // Always pass meta.ass — it always contains at least the karaoke
    // events plus whatever the storyboard's header/sigil declare.
    let subs_escaped = meta_ass_path
        .to_string_lossy()
        .replace('\\', "/")
        .replace(':', "\\:");
    cmd.args(["-vf", &format!("subtitles='{subs_escaped}'")]);

    let output_str = output_path
        .to_str()
        .ok_or_else(|| RenderError::NonUtf8Path {
            path: output_path.to_path_buf(),
        })?;
    cmd.args([
        "-map", "0:v",
        "-map", "1:a",
        "-c:v", "libx264",
        "-pix_fmt", "yuv420p",
        "-preset", "medium",
        // CRF 23 is YouTube-visually-transparent at 1080×1920 for
        // 30fps content and roughly halves file size vs CRF 20 on
        // high-entropy scenes (NoiseStatic, fast motion). Keeping
        // the dial here lets us tune per-episode if needed.
        "-crf", "23",
        // Cap the peak bitrate so a few noisy scenes can't dominate
        // the file size.
        "-maxrate", "10M",
        "-bufsize", "20M",
        "-c:a", "aac",
        "-b:a", "192k",
        "-shortest",
        "-movflags", "+faststart",
        output_str,
    ]);

    eprintln!("[episode] spawning ffmpeg");
    let mut ffmpeg = cmd
        .stdin(Stdio::piped())
        .spawn()
        .map_err(RenderError::FfmpegSpawn)?;
    let mut stdin = ffmpeg.stdin.take().ok_or(RenderError::FfmpegStdin)?;

    let mut active_scene_idx: usize = 0; // matches the initial_effect above
    let t_start = std::time::Instant::now();

    for frame_idx in 0..n_frames {
        let t = frame_idx as f32 / sb.fps as f32;
        let (rms, onset, centroid) = features.at_frame(frame_idx as usize);

        // Resolve current scene. If past the last scene end we keep
        // rendering the final scene's backdrop (better than crashing).
        let scene_idx = schedule::scene_at(&sb, t).unwrap_or(sb.scenes.len() - 1);
        if scene_idx != active_scene_idx {
            let scene = &sb.scenes[scene_idx];
            replace_backdrop_effect(&mut canvas, scene, &sb, t - scene.start)?;
            active_scene_idx = scene_idx;
        }
        let scene = &sb.scenes[active_scene_idx];
        update_dynamic_uniforms(&mut canvas, scene, t - scene.start, rms, onset, centroid)?;

        let bytes = pipeline
            .render_frame(gpu.context(), &canvas, &field)
            .map_err(|source| RenderError::RenderFrame {
                idx: frame_idx,
                source,
            })?;
        if stdin.write_all(&bytes).is_err() {
            break; // ffmpeg gone; wait() will surface the real cause.
        }

        if frame_idx % 60 == 0 && frame_idx > 0 {
            let elapsed = t_start.elapsed().as_secs_f32();
            let fps = frame_idx as f32 / elapsed;
            let eta = (n_frames - frame_idx) as f32 / fps.max(0.001);
            eprintln!(
                "[episode] frame {}/{}  scene {}/{}  {:.1} fps  eta {:.0}s",
                frame_idx,
                n_frames,
                active_scene_idx + 1,
                sb.scenes.len(),
                fps,
                eta
            );
        }
    }
    drop(stdin);

    let status = ffmpeg.wait().map_err(RenderError::FfmpegWait)?;
    if !status.success() {
        return Err(RenderError::FfmpegExit { status });
    }

    eprintln!(
        "[episode] done in {:.1}s -> {}",
        t_start.elapsed().as_secs_f32(),
        output_path.display()
    );
    Ok(())
}

// ── path helpers ─────────────────────────────────────────────────────

fn resolve(base: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}

/// Decide where to write the combined ass. Prefer next to the karaoke
/// file (so build/<ep>/meta.ass sits beside subs.ass); fall back to
/// next to the storyboard.
fn meta_ass_target(
    storyboard_dir: &Path,
    _sb: &Storyboard,
    karaoke: Option<&Path>,
) -> Result<PathBuf, crate::error::MetaAssError> {
    let dir = karaoke
        .and_then(|k| k.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| storyboard_dir.to_path_buf());
    std::fs::create_dir_all(&dir).map_err(|source| crate::error::MetaAssError::Mkdir {
        path: dir.clone(),
        source,
    })?;
    Ok(dir.join("meta.ass"))
}

// ── per-scene effect dispatch ────────────────────────────────────────

/// Build the initial `ShaderEffectDesc` for a scene.
fn effect_for_scene(scene: &Scene, sb: &Storyboard, t_in_scene: f32) -> ShaderEffectDesc {
    match &scene.backdrop {
        Backdrop::Flow {
            palette,
            intensity,
            seed,
        } => {
            let (lo, mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "flow",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_rms": 0.0,
                    "u_onset": 0.0,
                    "u_centroid": 0.5,
                    "u_intensity": *intensity,
                    "u_seed": *seed as f32,
                    "u_pal_low":  lo,
                    "u_pal_mid":  mid,
                    "u_pal_high": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Solid { color } => ShaderEffectDesc::new(
            "solid",
            serde_json::json!({ "u_color": color }),
        ),
        Backdrop::Voronoi {
            scale,
            edge_width,
            jitter,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "voronoi",
                serde_json::json!({
                    "u_scale": *scale,
                    "u_edge_width": *edge_width,
                    "u_jitter": *jitter,
                    "u_time": t_in_scene,
                    "u_color_a": lo,
                    "u_color_b": hi,
                    "u_edge_color": [0.95f32, 0.95, 0.93],
                }),
            )
        }
        Backdrop::NoiseStatic {
            intensity,
            density,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "noise_static",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_intensity": *intensity,
                    "u_density": *density,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Lattice {
            density,
            thickness,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "lattice",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_density": *density,
                    "u_thickness": *thickness,
                    "u_intensity": *intensity,
                    "u_color_bg": lo,
                    "u_color_line": hi,
                }),
            )
        }
        Backdrop::Mandala {
            segments,
            freq,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "mandala",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_segments": *segments,
                    "u_freq": *freq,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Concentric {
            freq,
            speed,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "concentric",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_freq": *freq,
                    "u_speed": *speed,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Strands {
            density,
            thickness,
            jitter,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "strands",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_density": *density,
                    "u_thickness": *thickness,
                    "u_jitter": *jitter,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                }),
            )
        }
        Backdrop::Wave {
            density,
            freq,
            amplitude,
            speed,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "wave",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_density": *density,
                    "u_freq": *freq,
                    "u_amplitude": *amplitude,
                    "u_speed": *speed,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                }),
            )
        }
        Backdrop::Spiral {
            arms,
            tightness,
            speed,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "spiral",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_arms": *arms,
                    "u_tightness": *tightness,
                    "u_speed": *speed,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Halftone {
            cell,
            strength,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "halftone",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_cell": *cell,
                    "u_strength": *strength,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Crosshatch {
            spacing,
            thickness,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "crosshatch",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_spacing": *spacing,
                    "u_thickness": *thickness,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Topo {
            scale,
            density,
            thickness,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "topo",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_scale": *scale,
                    "u_density": *density,
                    "u_thickness": *thickness,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Aurora {
            curtains,
            speed,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "aurora",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_curtains": *curtains,
                    "u_speed": *speed,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Sun {
            radius,
            rays,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "sun",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_radius": *radius,
                    "u_rays": *rays,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Particles {
            count,
            glow,
            speed,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "particles",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_count": *count,
                    "u_glow": *glow,
                    "u_speed": *speed,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Branch {
            branches,
            thickness,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "branch",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_branches": *branches,
                    "u_thickness": *thickness,
                    "u_intensity": *intensity,
                    "u_rms": 0.0f32,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Caustics {
            scale,
            sharpness,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "caustics",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_scale": *scale,
                    "u_sharpness": *sharpness,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Phyllotaxis {
            count,
            radius_scale,
            seed_radius,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "phyllotaxis",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_count": *count,
                    "u_radius_scale": *radius_scale,
                    "u_seed_radius": *seed_radius,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Constellation {
            node_glow,
            edge_glow,
            edge_strength,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "constellation",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_node_glow": *node_glow,
                    "u_edge_glow": *edge_glow,
                    "u_edge_strength": *edge_strength,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::VectorField {
            scale,
            freq,
            density,
            thickness,
            dash_speed,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "vector_field",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_scale": *scale,
                    "u_freq": *freq,
                    "u_density": *density,
                    "u_thickness": *thickness,
                    "u_dash_speed": *dash_speed,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Crystal {
            scale,
            levels,
            edge_width,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "crystal",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_scale": *scale,
                    "u_levels": *levels,
                    "u_edge_width": *edge_width,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Smoke {
            scale,
            warp,
            speed,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "smoke",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_scale": *scale,
                    "u_warp": *warp,
                    "u_speed": *speed,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Moire {
            freq,
            angle_delta,
            thickness,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "moire",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_freq": *freq,
                    "u_angle_delta": *angle_delta,
                    "u_thickness": *thickness,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Ripple {
            freq,
            speed,
            decay,
            sharpness,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "ripple",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_freq": *freq,
                    "u_speed": *speed,
                    "u_decay": *decay,
                    "u_sharpness": *sharpness,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Plasma {
            count,
            radius,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "plasma",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_count": *count,
                    "u_radius": *radius,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Bokeh {
            count,
            radius,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "bokeh",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_count": *count,
                    "u_radius": *radius,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
        Backdrop::Mosaic {
            grid,
            levels,
            gap,
            intensity,
            palette,
        } => {
            let (lo, _mid, hi) = palette_stops(palette);
            ShaderEffectDesc::new(
                "mosaic",
                serde_json::json!({
                    "u_time": t_in_scene,
                    "u_grid": *grid,
                    "u_levels": *levels,
                    "u_gap": *gap,
                    "u_intensity": *intensity,
                    "u_color_lo": lo,
                    "u_color_hi": hi,
                    "u_resolution": [sb.width as f32, sb.height as f32],
                }),
            )
        }
    }
}

/// Swap the canvas's backdrop effect for a new scene. Mutates in place
/// so we don't reallocate the layer.
fn replace_backdrop_effect(
    canvas: &mut Canvas,
    scene: &Scene,
    sb: &Storyboard,
    t_in_scene: f32,
) -> Result<(), RenderError> {
    let new_effect = effect_for_scene(scene, sb, t_in_scene);
    let layer = canvas
        .layer_mut(BACKDROP_LAYER)
        .map_err(|_| RenderError::BackdropLayerMissing)?;
    let fx = layer
        .effects_mut()
        .first_mut()
        .ok_or(RenderError::BackdropEffectMissing)?;
    fx.name = new_effect.name;
    fx.params = new_effect.params;
    Ok(())
}

/// Update per-frame dynamic uniforms (`u_time`, audio features) for the
/// active scene's effect.
///
/// **Convention:** every backdrop except `Solid` takes `u_time`, `u_rms`,
/// and `u_onset` as standard dynamic uniforms. They're set here once for
/// the active scene; the shader is free to ignore them (default 0.0 in
/// the schema means a shader that doesn't read them is unaffected).
/// Shader-specific overrides — e.g. Flow's `u_centroid`, NoiseStatic's
/// per-frame `u_density` bump — go in the match arm below.
fn update_dynamic_uniforms(
    canvas: &mut Canvas,
    scene: &Scene,
    t_in_scene: f32,
    rms: f32,
    onset: f32,
    centroid: f32,
) -> Result<(), RenderError> {
    let layer = canvas
        .layer_mut(BACKDROP_LAYER)
        .map_err(|_| RenderError::BackdropLayerMissing)?;
    let fx = layer
        .effects_mut()
        .first_mut()
        .ok_or(RenderError::BackdropEffectMissing)?;
    let obj = fx
        .params
        .as_object_mut()
        .ok_or(RenderError::BackdropParamsShape)?;

    // Standard dynamic uniforms for every animated backdrop. Solid opts
    // out — it has no time-varying uniforms by design.
    if !matches!(scene.backdrop, Backdrop::Solid { .. }) {
        obj.insert("u_time".into(), serde_json::json!(t_in_scene));
        obj.insert("u_rms".into(), serde_json::json!(rms));
        obj.insert("u_onset".into(), serde_json::json!(onset));
    }

    // Shader-specific overrides on top of the standard set.
    match &scene.backdrop {
        Backdrop::Flow { .. } => {
            // Flow additionally reads centroid for timbre-driven hue mix.
            obj.insert("u_centroid".into(), serde_json::json!(centroid));
        }
        Backdrop::NoiseStatic { density, .. } => {
            // The shader itself reads u_onset (set above) for tear-line
            // probability. The renderer-side density bump amplifies the
            // pattern at a frame granularity coarser than what the
            // shader can express alone. Computed off the *storyboard's*
            // base density value (NOT the previous frame) so the bump
            // doesn't compound across frames.
            let bumped = *density as f64 * (1.0 + 0.4 * onset as f64);
            obj.insert("u_density".into(), serde_json::json!(bumped));
        }
        // All other variants just need the standard set above.
        Backdrop::Solid { .. }
        | Backdrop::Voronoi { .. }
        | Backdrop::Lattice { .. }
        | Backdrop::Mandala { .. }
        | Backdrop::Concentric { .. }
        | Backdrop::Strands { .. }
        | Backdrop::Wave { .. }
        | Backdrop::Spiral { .. }
        | Backdrop::Halftone { .. }
        | Backdrop::Crosshatch { .. }
        | Backdrop::Topo { .. }
        | Backdrop::Aurora { .. }
        | Backdrop::Sun { .. }
        | Backdrop::Particles { .. }
        | Backdrop::Branch { .. }
        | Backdrop::Caustics { .. }
        | Backdrop::Phyllotaxis { .. }
        | Backdrop::Constellation { .. }
        | Backdrop::VectorField { .. }
        | Backdrop::Crystal { .. }
        | Backdrop::Smoke { .. }
        | Backdrop::Moire { .. }
        | Backdrop::Ripple { .. }
        | Backdrop::Plasma { .. }
        | Backdrop::Bokeh { .. }
        | Backdrop::Mosaic { .. } => {}
    }
    Ok(())
}

fn palette_stops(p: &PaletteRef) -> ([f32; 3], [f32; 3], [f32; 3]) {
    match p {
        PaletteRef::TealAmber => (design::COLOR_INK, design::COLOR_TEAL, design::COLOR_AMBER),
        PaletteRef::Custom(c) => (c[0], c[1], c[2]),
    }
}
