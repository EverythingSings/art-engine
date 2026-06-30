#![deny(unsafe_code)]
//! CLI binary for the art-engine generative art system.
//!
//! Subcommands:
//! - `render <engine>` — run an engine N steps, write PNG
//! - `render-sequence <engine>` — produce N frames in a directory
//! - `list` — print available engines and palettes

mod error;

use art_engine_core::canvas::{Canvas, ContentType, Layer, ShaderEffectDesc};
use art_engine_core::{Engine, Palette, Srgb};
use art_engine_engines::pixel::PostFx;
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

        /// Palette name (ocean, neon, earth, monochrome, vapor, fire, amber).
        #[arg(short, long, default_value = "ocean")]
        palette: String,

        /// Output file path.
        #[arg(short, long, default_value = "output.png")]
        output: PathBuf,

        /// Engine parameters as a JSON string.
        #[arg(long, default_value = "{}")]
        params: String,

        /// Apply CRT post-processing (scanlines + vignette + grain).
        #[arg(long)]
        postfx: bool,

        /// Render via the GPU pipeline (headless EGL + GLSL shaders).
        ///
        /// Required to apply --shader / --post effects. Without this flag,
        /// the engine output is rendered through the legacy CPU palette
        /// + PostFx (`--postfx`) path.
        #[arg(long)]
        gpu: bool,

        /// Apply a per-layer shader effect, format `name:json` (repeatable).
        ///
        /// Example: `--shader 'kaleidoscope:{"segments":6}'`.
        /// Names: feedback, voronoi, kaleidoscope.
        #[arg(long)]
        shader: Vec<String>,

        /// Append a post-processing effect, format `name:json` (repeatable).
        ///
        /// Example: `--post 'bloom:{"intensity":0.6}' --post 'grain:{"amount":0.02}'`.
        /// Names: bloom, vignette, grain, color_grade.
        #[arg(long)]
        post: Vec<String>,
    },
    /// Produce a sequence of frames suitable for ffmpeg-stitching.
    ///
    /// Two modes are selected by `--params-end`:
    ///
    /// 1. Evolve (default): initialise the engine once, then step it
    ///    `--steps-per-frame` times before writing each frame. Best for
    ///    diffusion / agent engines that have time-evolving state.
    ///
    /// 2. Param-sweep (when `--params-end` is set): re-initialise the
    ///    engine each frame with parameters linearly interpolated from
    ///    `--params` to `--params-end`. Best for purely-parameterised
    ///    engines (e.g. Mandelbrot zooms).
    RenderSequence {
        /// Engine name.
        engine: String,

        /// Canvas width in pixels.
        #[arg(short = 'W', long, default_value_t = 1280)]
        width: usize,

        /// Canvas height in pixels.
        #[arg(short = 'H', long, default_value_t = 720)]
        height: usize,

        /// Number of frames to render.
        #[arg(short, long, default_value_t = 120)]
        frames: usize,

        /// Simulation steps run between frames in evolve mode.
        #[arg(long, default_value_t = 5)]
        steps_per_frame: usize,

        /// Steps to run before the first frame (burnin, evolve mode only).
        #[arg(long, default_value_t = 0)]
        warmup: usize,

        /// PRNG seed for deterministic output.
        #[arg(long, default_value_t = 42)]
        seed: u64,

        /// Palette name.
        #[arg(short, long, default_value = "amber")]
        palette: String,

        /// Output directory; created if missing. Frames are written as
        /// zero-padded `<output>/<prefix>NNNNNN.png`.
        #[arg(short, long, default_value = "frames")]
        output: PathBuf,

        /// Filename prefix (e.g. "frame_" -> "frame_000001.png").
        #[arg(long, default_value = "frame_")]
        prefix: String,

        /// Engine parameters at frame 0 as a JSON object.
        #[arg(long, default_value = "{}")]
        params: String,

        /// Engine parameters at the final frame, as a JSON object.
        /// When set, switches to param-sweep mode and re-inits the engine
        /// each frame with parameters lerped between `--params` and this.
        /// Only numeric values are interpolated; non-numeric keys take
        /// the start value.
        #[arg(long)]
        params_end: Option<String>,

        /// Apply CRT post-processing to each frame (scanlines + vignette + grain).
        #[arg(long)]
        postfx: bool,

        /// Render every frame via the GPU pipeline. See `render --gpu` for details.
        #[arg(long)]
        gpu: bool,

        /// Per-layer shader effect, format `name:json` (repeatable).
        #[arg(long)]
        shader: Vec<String>,

        /// Post-processing effect, format `name:json` (repeatable).
        #[arg(long)]
        post: Vec<String>,
    },
    /// Compose two engines: feed engine A's field into engine B as influence.
    ///
    /// Both engines step every frame; A is stepped first, its `field()` is
    /// passed to B via `Engine::set_influence`, then B is stepped. The
    /// composite output is B's field (rendered through the chosen palette).
    ///
    /// This is the simplest form of cross-engine coupling — for richer
    /// chains write a JSON file and load it via `--chain` (TODO).
    Compose {
        /// Engine A name (the influencer).
        engine_a: String,
        /// Engine B name (the influenced — its field is the output).
        engine_b: String,

        /// Canvas width in pixels.
        #[arg(short = 'W', long, default_value_t = 720)]
        width: usize,
        /// Canvas height in pixels.
        #[arg(short = 'H', long, default_value_t = 720)]
        height: usize,
        /// Number of simulation steps.
        #[arg(short, long, default_value_t = 1000)]
        steps: usize,
        /// PRNG seed (passed to both engines).
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Palette name.
        #[arg(short, long, default_value = "amber")]
        palette: String,
        /// Output PNG path.
        #[arg(short, long, default_value = "compose.png")]
        output: PathBuf,
        /// Engine A parameters as a JSON string.
        #[arg(long, default_value = "{}")]
        params_a: String,
        /// Engine B parameters as a JSON string.
        #[arg(long, default_value = "{}")]
        params_b: String,
        /// Apply CRT post-processing.
        #[arg(long)]
        postfx: bool,
    },
    /// List available engines and palettes.
    List,
    /// Render one preview frame per backdrop shader to a directory.
    ///
    /// Walks every non-post-process [`BuiltinShader`] (skipping
    /// `feedback` + `kaleidoscope` since those need a source texture)
    /// and writes a single PNG per shader to the output directory.
    /// Used for *visual* storyboard authoring — open the directory in
    /// any image viewer and pick the shader whose look fits the beat,
    /// instead of guessing by name.
    Gallery {
        /// Output directory; created if missing.
        #[arg(short, long, default_value = "gallery")]
        output: PathBuf,

        /// Per-cell width in pixels.
        #[arg(short = 'W', long, default_value_t = 540)]
        width: usize,

        /// Per-cell height in pixels.
        #[arg(short = 'H', long, default_value_t = 960)]
        height: usize,

        /// Animation time (seconds) baked into `u_time` for each
        /// shader. Default 2.0 — most backdrops have settled into their
        /// steady-state pattern by then and don't show initial transients.
        #[arg(long, default_value_t = 2.0)]
        time: f32,
    },
}

/// Maximum canvas dimension (width or height) in pixels.
const MAX_DIMENSION: usize = 8192;
/// Maximum simulation steps.
const MAX_STEPS: usize = 1_000_000;
/// Maximum frames per render-sequence invocation.
const MAX_FRAMES: usize = 100_000;

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::List => cmd_list(cli.json),
        Command::Gallery {
            output,
            width,
            height,
            time,
        } => cmd_gallery(cli.json, output, width, height, time),
        Command::Render {
            engine,
            width,
            height,
            steps,
            seed,
            palette,
            output,
            params,
            postfx,
            gpu,
            shader,
            post,
        } => cmd_render(
            cli.json, engine, width, height, steps, seed, palette, output, params, postfx, gpu,
            shader, post,
        ),
        Command::Compose {
            engine_a,
            engine_b,
            width,
            height,
            steps,
            seed,
            palette,
            output,
            params_a,
            params_b,
            postfx,
        } => cmd_compose(
            cli.json, engine_a, engine_b, width, height, steps, seed, palette, output, params_a,
            params_b, postfx,
        ),
        Command::RenderSequence {
            engine,
            width,
            height,
            frames,
            steps_per_frame,
            warmup,
            seed,
            palette,
            output,
            prefix,
            params,
            params_end,
            postfx,
            gpu,
            shader,
            post,
        } => cmd_render_sequence(
            cli.json,
            engine,
            width,
            height,
            frames,
            steps_per_frame,
            warmup,
            seed,
            palette,
            output,
            prefix,
            params,
            params_end,
            postfx,
            gpu,
            shader,
            post,
        ),
    }
}

/// Backdrop shaders that aren't viable as standalone gallery cells
/// because they need a source texture to sample (the kaleidoscope mirrors
/// the previous layer; feedback echoes a previous frame). Without input
/// they render black, which isn't useful for shader selection.
const GALLERY_SKIP: &[&str] = &["feedback", "kaleidoscope"];

fn cmd_gallery(
    json: bool,
    output: PathBuf,
    width: usize,
    height: usize,
    time: f32,
) -> Result<(), CliError> {
    use art_engine_core::shaders::BuiltinShader;
    validate_dims(width, height)?;
    std::fs::create_dir_all(&output)
        .map_err(|e| CliError::Io(format!("create gallery dir: {e}")))?;

    // The backdrop shaders generate from uniforms only — they don't
    // sample the field. A zero-filled field at the canvas dimensions
    // just keeps the pipeline's palette-mapping stage happy.
    let field = art_engine_core::Field::filled(width, height, 0.0)
        .map_err(|e| CliError::Input(e.to_string()))?;
    // Palette is irrelevant for backdrops (each carries its own
    // `u_color_lo`/`u_color_hi`); we still need to pass one through
    // the snapshot path. "amber" matches the show's chrome accent.
    let palette = Palette::from_name("amber").map_err(|e| CliError::Input(e.to_string()))?;

    let mut written: Vec<(String, PathBuf)> = Vec::new();

    for &shader_name in BuiltinShader::list() {
        let shader = match BuiltinShader::from_name(shader_name) {
            Some(s) => s,
            None => continue,
        };
        if shader.is_post_process() || GALLERY_SKIP.contains(&shader_name) {
            continue;
        }

        // Only override u_time. Every other uniform comes from the
        // pipeline's default schema, so the gallery shows each backdrop
        // exactly as it appears with stock params in a storyboard.
        let effect_str = format!("{shader_name}:{{\"u_time\":{time}}}");
        let canvas = build_canvas(width, height, &[effect_str], &[])?;

        let out_path = output.join(format!("{shader_name}.png"));
        // Direct call to gpu_snapshot::render_to_png so the canvas we
        // built (with the effect attached) is preserved. render_gpu_one
        // would rebuild from the parse_effect path, which we'd have to
        // marshal back through a string round-trip.
        #[cfg(feature = "gpu")]
        {
            art_engine_engines::gpu_snapshot::render_to_png(
                &canvas, &field, &palette, &out_path,
            )
            .map_err(|e| CliError::Io(e.to_string()))?;
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = canvas;
            return Err(CliError::Input(
                "this CLI was built without the `gpu` feature; rebuild with --features gpu"
                    .to_string(),
            ));
        }
        written.push((shader_name.to_string(), out_path.clone()));

        if !json {
            eprintln!("  {shader_name:>14} → {}", out_path.display());
        }
    }

    if json {
        let info = serde_json::json!({
            "gallery_dir": output.display().to_string(),
            "cell_width": width,
            "cell_height": height,
            "time": time,
            "count": written.len(),
            "shaders": written
                .iter()
                .map(|(n, p)| serde_json::json!({"name": n, "path": p.display().to_string()}))
                .collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        eprintln!(
            "\nwrote {} backdrop previews to {}",
            written.len(),
            output.display()
        );
    }
    Ok(())
}

fn cmd_list(json: bool) -> Result<(), CliError> {
    let engines = EngineKind::list_engines();
    let palettes = Palette::list_names();
    if json {
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
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_render(
    json: bool,
    engine: String,
    width: usize,
    height: usize,
    steps: usize,
    seed: u64,
    palette: String,
    output: PathBuf,
    params: String,
    postfx: bool,
    gpu: bool,
    shader: Vec<String>,
    post: Vec<String>,
) -> Result<(), CliError> {
    validate_dims(width, height)?;
    if steps > MAX_STEPS {
        return Err(CliError::Input(format!(
            "steps must be <={MAX_STEPS}, got {steps}"
        )));
    }
    if !gpu && (!shader.is_empty() || !post.is_empty()) {
        return Err(CliError::Input(
            "--shader and --post require --gpu".to_string(),
        ));
    }

    let params_value: serde_json::Value = serde_json::from_str(&params)
        .map_err(|e| CliError::Input(format!("invalid --params JSON: {e}")))?;

    let palette_obj = Palette::from_name(&palette).map_err(|e| CliError::Input(e.to_string()))?;

    let mut eng = EngineKind::from_name(&engine, width, height, seed, &params_value)?;
    (0..steps).try_for_each(|_| eng.step())?;

    if gpu {
        render_gpu_one(
            &output,
            &palette_obj,
            eng.field(),
            width,
            height,
            &shader,
            &post,
        )?;
    } else {
        let postfx_cfg = if postfx {
            PostFx::crt_amber()
        } else {
            PostFx::default()
        };
        art_engine_engines::snapshot::write_png_with_postfx(
            eng.field(),
            &palette_obj,
            &postfx_cfg,
            &output,
        )?;
    }

    if json {
        let info = serde_json::json!({
            "engine": engine,
            "width": width,
            "height": height,
            "steps": steps,
            "seed": seed,
            "output": output.display().to_string(),
            "postfx": postfx,
            "gpu": gpu,
            "shader": shader,
            "post": post,
        });
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        let mode = if gpu { "GPU" } else { "CPU" };
        eprintln!(
            "rendered {engine} ({width}x{height}, {steps} steps, seed {seed}, {mode}) -> {}",
            output.display()
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_compose(
    json: bool,
    engine_a: String,
    engine_b: String,
    width: usize,
    height: usize,
    steps: usize,
    seed: u64,
    palette: String,
    output: PathBuf,
    params_a: String,
    params_b: String,
    postfx: bool,
) -> Result<(), CliError> {
    validate_dims(width, height)?;
    if steps > MAX_STEPS {
        return Err(CliError::Input(format!(
            "steps must be <={MAX_STEPS}, got {steps}"
        )));
    }
    let pa: serde_json::Value = serde_json::from_str(&params_a)
        .map_err(|e| CliError::Input(format!("invalid --params-a JSON: {e}")))?;
    let pb: serde_json::Value = serde_json::from_str(&params_b)
        .map_err(|e| CliError::Input(format!("invalid --params-b JSON: {e}")))?;
    let palette_obj = Palette::from_name(&palette).map_err(|e| CliError::Input(e.to_string()))?;

    let mut a = EngineKind::from_name(&engine_a, width, height, seed, &pa)?;
    let mut b = EngineKind::from_name(&engine_b, width, height, seed, &pb)?;

    for _ in 0..steps {
        a.step()?;
        // a.field() borrows a immutably; b.set_influence borrows b mutably.
        // Different objects so the borrows don't conflict.
        b.set_influence(a.field())?;
        b.step()?;
    }

    let postfx_cfg = if postfx {
        PostFx::crt_amber()
    } else {
        PostFx::default()
    };
    art_engine_engines::snapshot::write_png_with_postfx(
        b.field(),
        &palette_obj,
        &postfx_cfg,
        &output,
    )?;

    if json {
        let info = serde_json::json!({
            "engine_a": engine_a,
            "engine_b": engine_b,
            "width": width,
            "height": height,
            "steps": steps,
            "seed": seed,
            "output": output.display().to_string(),
            "postfx": postfx,
        });
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        eprintln!(
            "composed {engine_a} -> {engine_b} ({width}x{height}, {steps} steps) -> {}",
            output.display()
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_render_sequence(
    json: bool,
    engine: String,
    width: usize,
    height: usize,
    frames: usize,
    steps_per_frame: usize,
    warmup: usize,
    seed: u64,
    palette: String,
    output: PathBuf,
    prefix: String,
    params: String,
    params_end: Option<String>,
    postfx: bool,
    gpu: bool,
    shader: Vec<String>,
    post: Vec<String>,
) -> Result<(), CliError> {
    if !gpu && (!shader.is_empty() || !post.is_empty()) {
        return Err(CliError::Input(
            "--shader and --post require --gpu".to_string(),
        ));
    }
    validate_dims(width, height)?;
    if frames == 0 || frames > MAX_FRAMES {
        return Err(CliError::Input(format!(
            "frames must be 1..={MAX_FRAMES}, got {frames}"
        )));
    }
    if steps_per_frame > MAX_STEPS {
        return Err(CliError::Input(format!(
            "steps-per-frame must be <={MAX_STEPS}, got {steps_per_frame}"
        )));
    }
    if warmup > MAX_STEPS {
        return Err(CliError::Input(format!(
            "warmup must be <={MAX_STEPS}, got {warmup}"
        )));
    }

    let start_params: serde_json::Value = serde_json::from_str(&params)
        .map_err(|e| CliError::Input(format!("invalid --params JSON: {e}")))?;
    let end_params: Option<serde_json::Value> = match params_end {
        Some(s) => Some(
            serde_json::from_str(&s)
                .map_err(|e| CliError::Input(format!("invalid --params-end JSON: {e}")))?,
        ),
        None => None,
    };

    let palette_obj = Palette::from_name(&palette).map_err(|e| CliError::Input(e.to_string()))?;

    std::fs::create_dir_all(&output)
        .map_err(|e| CliError::Io(format!("creating {}: {e}", output.display())))?;

    let postfx_cfg = if postfx {
        PostFx::crt_amber()
    } else {
        PostFx::default()
    };

    // Width of the frame index zero-padding: at least 6 digits, more if
    // total frame count needs it. Six digits = 999_999 frames cap.
    let pad = frame_pad_width(frames);

    let canvas_for_gpu = if gpu {
        Some(build_canvas(width, height, &shader, &post)?)
    } else {
        None
    };

    let mut sequence_renderer = SequenceRenderer::new(
        gpu,
        canvas_for_gpu.as_ref(),
        &palette_obj,
        width,
        height,
        &postfx_cfg,
    )?;

    if let Some(end) = end_params.as_ref() {
        // Param-sweep mode: re-init engine each frame with lerped params.
        for i in 0..frames {
            let t = if frames == 1 {
                0.0
            } else {
                i as f64 / (frames - 1) as f64
            };
            let lerped = lerp_params(&start_params, end, t);
            let mut eng = EngineKind::from_name(&engine, width, height, seed, &lerped)?;
            (0..steps_per_frame).try_for_each(|_| eng.step())?;

            let path = output.join(format!("{prefix}{:0pad$}.png", i, pad = pad));
            sequence_renderer.write_frame(eng.field(), &path)?;
        }
    } else {
        // Evolve mode: init once, step between frames.
        let mut eng = EngineKind::from_name(&engine, width, height, seed, &start_params)?;
        (0..warmup).try_for_each(|_| eng.step())?;
        for i in 0..frames {
            // Only the first frame skips pre-step (since warmup already covered it).
            if i > 0 {
                (0..steps_per_frame).try_for_each(|_| eng.step())?;
            }
            let path = output.join(format!("{prefix}{:0pad$}.png", i, pad = pad));
            sequence_renderer.write_frame(eng.field(), &path)?;
        }
    }

    if json {
        let info = serde_json::json!({
            "engine": engine,
            "width": width,
            "height": height,
            "frames": frames,
            "steps_per_frame": steps_per_frame,
            "warmup": warmup,
            "seed": seed,
            "palette": palette,
            "output_dir": output.display().to_string(),
            "prefix": prefix,
            "mode": if end_params.is_some() { "param-sweep" } else { "evolve" },
            "postfx": postfx,
            "gpu": gpu,
            "shader": shader,
            "post": post,
        });
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        let mode = if gpu { "GPU" } else { "CPU" };
        eprintln!(
            "rendered {frames} frames of {engine} ({width}x{height}, {mode}) -> {}/{prefix}*.png",
            output.display()
        );
    }
    Ok(())
}

// ── Effect parsing + canvas/session helpers ──────────────────────────────

/// Parse a `name:json` flag into a `ShaderEffectDesc`. If the colon is absent,
/// the entire string is treated as the name and params default to `{}`.
fn parse_effect(s: &str) -> Result<ShaderEffectDesc, CliError> {
    let (name, params_s) = match s.find(':') {
        Some(idx) => (&s[..idx], &s[idx + 1..]),
        None => (s, "{}"),
    };
    if name.is_empty() {
        return Err(CliError::Input(
            "shader/post effect name cannot be empty".to_string(),
        ));
    }
    let trimmed = params_s.trim();
    let params: serde_json::Value = if trimmed.is_empty() {
        serde_json::Value::Object(serde_json::Map::new())
    } else {
        serde_json::from_str(trimmed).map_err(|e| {
            CliError::Input(format!(
                "invalid JSON for effect '{name}': {e} (input: {trimmed:?})"
            ))
        })?
    };
    Ok(ShaderEffectDesc::new(name.to_string(), params))
}

/// Build a Canvas with one Field-content layer carrying the per-layer
/// effect chain, and `post` populating the canvas's post-processing stack.
fn build_canvas(
    width: usize,
    height: usize,
    shader: &[String],
    post: &[String],
) -> Result<Canvas, CliError> {
    let mut canvas = Canvas::new(
        width,
        height,
        Srgb {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        },
    )?;
    let mut layer = Layer::new("content", ContentType::Field);
    for s in shader {
        layer = layer.with_effect(parse_effect(s)?);
    }
    canvas.add_layer(layer)?;
    for s in post {
        canvas.push_post(parse_effect(s)?);
    }
    Ok(canvas)
}

#[cfg(feature = "gpu")]
fn render_gpu_one(
    output: &std::path::Path,
    palette: &Palette,
    field: &art_engine_core::Field,
    width: usize,
    height: usize,
    shader: &[String],
    post: &[String],
) -> Result<(), CliError> {
    let canvas = build_canvas(width, height, shader, post)?;
    art_engine_engines::gpu_snapshot::render_to_png(&canvas, field, palette, output)
        .map_err(|e| CliError::Io(e.to_string()))
}

#[cfg(not(feature = "gpu"))]
fn render_gpu_one(
    _output: &std::path::Path,
    _palette: &Palette,
    _field: &art_engine_core::Field,
    _width: usize,
    _height: usize,
    _shader: &[String],
    _post: &[String],
) -> Result<(), CliError> {
    Err(CliError::Input(
        "this CLI was built without the `gpu` feature; rebuild with --features gpu".to_string(),
    ))
}

/// Helper that abstracts over CPU and GPU paths for `render-sequence` so the
/// per-frame loop body stays small. The GPU variant holds a long-lived
/// `GpuSession` to avoid re-initialising EGL per frame.
enum SequenceRenderer<'a> {
    Cpu {
        palette: &'a Palette,
        postfx: &'a PostFx,
    },
    #[cfg(feature = "gpu")]
    Gpu {
        canvas: &'a Canvas,
        // Boxed because GpuSession holds a heavy GL context + pipeline
        // (~hundred-byte Pipeline state); the enum stays compact.
        session: Box<art_engine_engines::gpu_snapshot::GpuSession>,
    },
}

impl<'a> SequenceRenderer<'a> {
    fn new(
        gpu: bool,
        canvas: Option<&'a Canvas>,
        palette: &'a Palette,
        _width: usize,
        _height: usize,
        postfx: &'a PostFx,
    ) -> Result<Self, CliError> {
        if !gpu {
            return Ok(Self::Cpu { palette, postfx });
        }
        #[cfg(feature = "gpu")]
        {
            let canvas = canvas.expect("gpu mode requires a canvas");
            let mut session = art_engine_engines::gpu_snapshot::GpuSession::new(
                canvas.width() as u32,
                canvas.height() as u32,
            )
            .map_err(|e| CliError::Io(e.to_string()))?;
            session
                .rebake_palette(palette)
                .map_err(|e| CliError::Io(e.to_string()))?;
            Ok(Self::Gpu {
                canvas,
                session: Box::new(session),
            })
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = canvas;
            Err(CliError::Input(
                "this CLI was built without the `gpu` feature; rebuild with --features gpu"
                    .to_string(),
            ))
        }
    }

    fn write_frame(
        &mut self,
        field: &art_engine_core::Field,
        path: &std::path::Path,
    ) -> Result<(), CliError> {
        match self {
            Self::Cpu { palette, postfx } => {
                art_engine_engines::snapshot::write_png_with_postfx(field, palette, postfx, path)?;
                Ok(())
            }
            #[cfg(feature = "gpu")]
            Self::Gpu { canvas, session } => session
                .render_to_png(canvas, field, path)
                .map_err(|e| CliError::Io(e.to_string())),
        }
    }
}

fn validate_dims(width: usize, height: usize) -> Result<(), CliError> {
    if width == 0 || width > MAX_DIMENSION {
        return Err(CliError::Input(format!(
            "width must be 1..={MAX_DIMENSION}, got {width}"
        )));
    }
    if height == 0 || height > MAX_DIMENSION {
        return Err(CliError::Input(format!(
            "height must be 1..={MAX_DIMENSION}, got {height}"
        )));
    }
    Ok(())
}

fn frame_pad_width(frames: usize) -> usize {
    let digits = (frames.saturating_sub(1)).to_string().len();
    digits.max(6)
}

/// Lerps numeric values between `start` and `end` JSON objects.
///
/// For each key present in both `start` and `end` whose values are both
/// numeric, the result contains `(1 - t) * start + t * end`.
/// Non-numeric or missing keys take the value from `start`. Keys present
/// only in `end` are ignored (they have no animation curve).
fn lerp_params(start: &serde_json::Value, end: &serde_json::Value, t: f64) -> serde_json::Value {
    let mut out = start.clone();
    let (Some(start_obj), Some(end_obj), Some(out_obj)) =
        (start.as_object(), end.as_object(), out.as_object_mut())
    else {
        return out;
    };

    for (k, sv) in start_obj.iter() {
        if let (Some(sf), Some(ev)) = (sv.as_f64(), end_obj.get(k).and_then(|v| v.as_f64())) {
            let v = sf + (ev - sf) * t;
            // Preserve integer-ness if both endpoints were ints and the
            // lerped value is integer at this t.
            if sv.is_i64() && end_obj[k].is_i64() && v.fract() == 0.0 {
                out_obj.insert(k.clone(), serde_json::Value::from(v as i64));
            } else {
                out_obj.insert(k.clone(), serde_json::json!(v));
            }
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frame_pad_width_min_six() {
        assert_eq!(frame_pad_width(1), 6);
        assert_eq!(frame_pad_width(120), 6);
        // Indices 0..999_999 still fit in 6 digits.
        assert_eq!(frame_pad_width(1_000_000), 6);
        // 10M frames -> max index is 9_999_999 -> 7 digits.
        assert_eq!(frame_pad_width(10_000_000), 7);
    }

    #[test]
    fn lerp_params_numeric_keys() {
        let s = json!({"feed_rate": 0.04, "kill_rate": 0.06, "name": "x"});
        let e = json!({"feed_rate": 0.06, "kill_rate": 0.05, "name": "y"});
        let mid = lerp_params(&s, &e, 0.5);
        assert!((mid["feed_rate"].as_f64().unwrap() - 0.05).abs() < 1e-12);
        assert!((mid["kill_rate"].as_f64().unwrap() - 0.055).abs() < 1e-12);
        // String key falls back to start value.
        assert_eq!(mid["name"].as_str().unwrap(), "x");
    }

    #[test]
    fn lerp_params_t_zero_returns_start() {
        let s = json!({"a": 1.0, "b": 2.0});
        let e = json!({"a": 10.0, "b": 20.0});
        let r = lerp_params(&s, &e, 0.0);
        assert!((r["a"].as_f64().unwrap() - 1.0).abs() < 1e-12);
        assert!((r["b"].as_f64().unwrap() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn lerp_params_t_one_returns_end_for_shared_keys() {
        let s = json!({"a": 1.0});
        let e = json!({"a": 10.0});
        let r = lerp_params(&s, &e, 1.0);
        assert!((r["a"].as_f64().unwrap() - 10.0).abs() < 1e-12);
    }

    #[test]
    fn lerp_params_keeps_integer_when_both_ints_and_t_lands_on_integer() {
        let s = json!({"max_iter": 100});
        let e = json!({"max_iter": 200});
        let r = lerp_params(&s, &e, 0.5);
        assert!(r["max_iter"].is_i64(), "expected int, got {r:?}");
        assert_eq!(r["max_iter"].as_i64().unwrap(), 150);
    }

    #[test]
    fn lerp_params_handles_non_object_gracefully() {
        let s = json!(42);
        let e = json!(100);
        let r = lerp_params(&s, &e, 0.5);
        assert_eq!(r, json!(42));
    }
}
