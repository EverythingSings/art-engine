//! GPU-rendered PNG snapshot via the headless EGL pipeline.
//!
//! Mirrors [`crate::snapshot::write_png_with_postfx`] but routes through
//! `art-engine-core`'s GL pipeline rather than the CPU palette + PostFx
//! path. The engine's `step()` is the caller's responsibility — this
//! module only handles the field → texture → effects → PNG path.
//!
//! Available only when the `gpu` feature is enabled.

#![cfg(feature = "gpu")]

use std::path::Path;

use art_engine_core::canvas::Canvas;
use art_engine_core::field::Field;
use art_engine_core::palette::Palette;
use art_engine_core::render::headless::{create_headless_context, HeadlessError, HeadlessGpu};
use art_engine_core::render::pipeline::{Pipeline, PipelineError};
use thiserror::Error;

/// Errors produced by the GPU snapshot path.
#[derive(Debug, Error)]
pub enum GpuSnapshotError {
    /// Headless GL context could not be created.
    #[error("headless GL: {0}")]
    Headless(#[from] HeadlessError),
    /// The render pipeline rejected the frame.
    #[error("pipeline: {0}")]
    Pipeline(#[from] PipelineError),
    /// PNG encoding or filesystem write failed.
    #[error("io: {0}")]
    Io(String),
}

/// One-shot GPU render: open a headless context, render one frame, write
/// the PNG, drop the context. Convenient for CLI `render` invocations
/// that produce a single image.
pub fn render_to_png(
    canvas: &Canvas,
    field: &Field,
    palette: &Palette,
    path: &Path,
) -> Result<(), GpuSnapshotError> {
    let mut session = GpuSession::new(canvas.width() as u32, canvas.height() as u32)?;
    session.rebake_palette(palette)?;
    session.render_to_png(canvas, field, path)
}

/// A long-lived GPU rendering session: keeps the EGL context and pipeline
/// alive across many frames so `render-sequence` doesn't pay the
/// initialisation cost per frame.
pub struct GpuSession {
    gpu: HeadlessGpu,
    pipeline: Pipeline,
    width: u32,
    height: u32,
}

impl GpuSession {
    /// Allocates a headless GL context + pipeline at the given dimensions.
    pub fn new(width: u32, height: u32) -> Result<Self, GpuSnapshotError> {
        let gpu = create_headless_context()?;
        let pipeline = Pipeline::new(gpu.context(), width, height)?;
        Ok(Self {
            gpu,
            pipeline,
            width,
            height,
        })
    }

    /// Rebakes the palette LUT. Call once per session, or whenever the
    /// palette changes between frames.
    pub fn rebake_palette(&mut self, palette: &Palette) -> Result<(), GpuSnapshotError> {
        self.pipeline.rebake_palette(self.gpu.context(), palette)?;
        Ok(())
    }

    /// Renders one frame and returns the raw RGBA8 buffer, row-major top-to-bottom.
    ///
    /// Useful for in-memory consumers that want pixels without a filesystem
    /// round-trip — the explorer GUI, vision-API callouts, etc. The returned
    /// buffer is exactly `width * height * 4` bytes.
    pub fn render_to_rgba8(
        &mut self,
        canvas: &Canvas,
        field: &Field,
    ) -> Result<Vec<u8>, GpuSnapshotError> {
        Ok(self
            .pipeline
            .render_frame(self.gpu.context(), canvas, field)?)
    }

    /// Like [`Self::render_to_rgba8`] but at animation time `time`, driving
    /// every shader's `u_time` uniform. Used to render GIF/animation frames.
    pub fn render_to_rgba8_at(
        &mut self,
        canvas: &Canvas,
        field: &Field,
        time: f32,
    ) -> Result<Vec<u8>, GpuSnapshotError> {
        Ok(self
            .pipeline
            .render_frame_at(self.gpu.context(), canvas, field, time)?)
    }

    /// Renders one frame and writes it as a PNG to `path`.
    pub fn render_to_png(
        &mut self,
        canvas: &Canvas,
        field: &Field,
        path: &Path,
    ) -> Result<(), GpuSnapshotError> {
        let bytes = self.render_to_rgba8(canvas, field)?;
        let img = image::RgbaImage::from_raw(self.width, self.height, bytes)
            .ok_or_else(|| GpuSnapshotError::Io("RGBA buffer size mismatch".into()))?;
        img.save(path)
            .map_err(|e| GpuSnapshotError::Io(e.to_string()))?;
        Ok(())
    }

    /// Returns the session dimensions.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use art_engine_core::canvas::{BlendMode, ContentType, Layer, ShaderEffectDesc};
    use art_engine_core::Srgb;

    fn black() -> Srgb {
        Srgb {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        }
    }

    fn skip_if_no_gl(e: &GpuSnapshotError) -> bool {
        matches!(e, GpuSnapshotError::Headless(_))
            && std::env::var("ART_ENGINE_REQUIRE_GL").is_err()
    }

    /// Smoke-test the full pipeline end-to-end. Skipped if no headless GL
    /// is available (e.g. CI without WSLg/Mesa); set `ART_ENGINE_REQUIRE_GL=1`
    /// to upgrade this to a hard failure.
    #[test]
    fn renders_field_to_png_with_post_stack() {
        let mut canvas = Canvas::new(64, 64, black()).unwrap();
        canvas
            .add_layer(Layer::new("content", ContentType::Field).with_blend_mode(BlendMode::Normal))
            .unwrap();
        canvas.push_post(ShaderEffectDesc::new(
            "vignette",
            serde_json::json!({"strength": 0.5}),
        ));
        canvas.push_post(ShaderEffectDesc::new(
            "grain",
            serde_json::json!({"amount": 0.02}),
        ));
        canvas.push_post(ShaderEffectDesc::new(
            "color_grade",
            serde_json::json!({"saturation": 1.2}),
        ));

        // Diagonal gradient — a known signal so we can sanity-check the output.
        let mut field = Field::new(64, 64).unwrap();
        for y in 0..64isize {
            for x in 0..64isize {
                let v = (x + y) as f64 / 126.0;
                field.set(x, y, v.clamp(0.0, 1.0));
            }
        }

        let palette = Palette::amber();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gpu.png");

        match render_to_png(&canvas, &field, &palette, &path) {
            Ok(()) => {
                let img = image::open(&path).unwrap().to_rgba8();
                assert_eq!(img.width(), 64);
                assert_eq!(img.height(), 64);
                // Result must contain at least some non-black pixels.
                let any_color = img.pixels().any(|p| p[0] > 5 || p[1] > 5 || p[2] > 5);
                assert!(any_color, "GPU output is entirely black — pipeline broken");
            }
            Err(e) if skip_if_no_gl(&e) => {
                eprintln!("skipping GPU snapshot test: {e}");
            }
            Err(e) => panic!("GPU snapshot failed: {e}"),
        }
    }

    #[test]
    fn render_to_png_with_no_effects_still_works() {
        let mut canvas = Canvas::new(32, 32, black()).unwrap();
        canvas
            .add_layer(Layer::new("c", ContentType::Field))
            .unwrap();
        let field = Field::filled(32, 32, 0.5).unwrap();
        let palette = Palette::ocean();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.png");

        match render_to_png(&canvas, &field, &palette, &path) {
            Ok(()) => {
                let img = image::open(&path).unwrap().to_rgba8();
                assert_eq!(img.dimensions(), (32, 32));
                let any_color = img.pixels().any(|p| p[0] > 5 || p[1] > 5 || p[2] > 5);
                assert!(
                    any_color,
                    "no-effects GPU output is black — pipeline broken"
                );
            }
            Err(e) if skip_if_no_gl(&e) => {
                eprintln!("skipping: {e}");
            }
            Err(e) => panic!("{e}"),
        }
    }

    /// Convenience for the multi-layer tests: a layer that paints a solid
    /// colour by overriding its content via the `solid` shader effect.
    /// This bypasses the palette LUT entirely so the test asserts against
    /// known RGB values regardless of palette.
    fn solid_layer(name: &str, color: [f64; 3], mode: BlendMode) -> Layer {
        Layer::new(name, ContentType::Field)
            .with_blend_mode(mode)
            .with_effect(ShaderEffectDesc::new(
                "solid",
                serde_json::json!({"u_color": [color[0], color[1], color[2]]}),
            ))
    }

    /// Two opaque normal layers: the top layer fully overwrites the bottom.
    #[test]
    fn normal_blend_top_layer_replaces_bottom() {
        let mut canvas = Canvas::new(8, 8, black()).unwrap();
        canvas
            .add_layer(solid_layer("bot", [1.0, 0.0, 0.0], BlendMode::Normal))
            .unwrap();
        canvas
            .add_layer(solid_layer("top", [0.0, 1.0, 0.0], BlendMode::Normal))
            .unwrap();
        let field = Field::filled(8, 8, 0.0).unwrap();
        let palette = Palette::ocean();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("normal.png");

        match render_to_png(&canvas, &field, &palette, &path) {
            Ok(()) => {
                let img = image::open(&path).unwrap().to_rgba8();
                let p = img.get_pixel(4, 4);
                assert!(p[0] < 30, "red channel should be ~0, got {}", p[0]);
                assert!(p[1] > 200, "green channel should be ~255, got {}", p[1]);
                assert!(p[2] < 30, "blue channel should be ~0, got {}", p[2]);
            }
            Err(e) if skip_if_no_gl(&e) => eprintln!("skipping: {e}"),
            Err(e) => panic!("{e}"),
        }
    }

    /// Additive of two half-bright layers should sum to a brighter colour
    /// than either alone.
    #[test]
    fn additive_blend_sums_layers() {
        let mut canvas = Canvas::new(8, 8, black()).unwrap();
        canvas
            .add_layer(solid_layer("bot", [0.4, 0.0, 0.0], BlendMode::Normal))
            .unwrap();
        canvas
            .add_layer(solid_layer("top", [0.4, 0.0, 0.0], BlendMode::Additive))
            .unwrap();
        let field = Field::filled(8, 8, 0.0).unwrap();
        let palette = Palette::ocean();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("additive.png");

        match render_to_png(&canvas, &field, &palette, &path) {
            Ok(()) => {
                let img = image::open(&path).unwrap().to_rgba8();
                let p = img.get_pixel(4, 4);
                // 0.4 + 0.4 = 0.8 ≈ 204 in 8-bit. Allow a wide tolerance for
                // tonemap clamp / rounding.
                assert!(
                    p[0] > 180,
                    "additive red should be > single-layer 0.4 (~102), got {}",
                    p[0]
                );
                assert!(p[1] < 20 && p[2] < 20);
            }
            Err(e) if skip_if_no_gl(&e) => eprintln!("skipping: {e}"),
            Err(e) => panic!("{e}"),
        }
    }

    /// Multiply of white over mid-grey should darken to ~mid-grey.
    /// (White × grey = grey; multiplying a non-trivial colour by white is
    /// identity, so this verifies the blend math runs at all.)
    #[test]
    fn multiply_blend_darkens() {
        let mut canvas = Canvas::new(8, 8, black()).unwrap();
        canvas
            .add_layer(solid_layer("bot", [1.0, 1.0, 1.0], BlendMode::Normal))
            .unwrap();
        canvas
            .add_layer(solid_layer("top", [0.4, 0.4, 0.4], BlendMode::Multiply))
            .unwrap();
        let field = Field::filled(8, 8, 0.0).unwrap();
        let palette = Palette::ocean();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("multiply.png");

        match render_to_png(&canvas, &field, &palette, &path) {
            Ok(()) => {
                let img = image::open(&path).unwrap().to_rgba8();
                let p = img.get_pixel(4, 4);
                // 1.0 * 0.4 = 0.4 ≈ 102, tolerant.
                assert!(p[0] > 60 && p[0] < 160, "multiply red ≈ 102, got {}", p[0]);
                assert!(p[1] > 60 && p[1] < 160);
                assert!(p[2] > 60 && p[2] < 160);
            }
            Err(e) if skip_if_no_gl(&e) => eprintln!("skipping: {e}"),
            Err(e) => panic!("{e}"),
        }
    }

    /// Hidden layers are skipped during compositing.
    #[test]
    fn hidden_layer_does_not_render() {
        let mut canvas = Canvas::new(8, 8, black()).unwrap();
        canvas
            .add_layer(solid_layer("bot", [1.0, 0.0, 0.0], BlendMode::Normal))
            .unwrap();
        // Top layer would otherwise overwrite with green, but is hidden.
        canvas
            .add_layer(
                solid_layer("top", [0.0, 1.0, 0.0], BlendMode::Normal).with_visible(false),
            )
            .unwrap();
        let field = Field::filled(8, 8, 0.0).unwrap();
        let palette = Palette::ocean();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hidden.png");

        match render_to_png(&canvas, &field, &palette, &path) {
            Ok(()) => {
                let img = image::open(&path).unwrap().to_rgba8();
                let p = img.get_pixel(4, 4);
                assert!(p[0] > 200, "hidden top did not let bottom red show through");
                assert!(p[1] < 30 && p[2] < 30);
            }
            Err(e) if skip_if_no_gl(&e) => eprintln!("skipping: {e}"),
            Err(e) => panic!("{e}"),
        }
    }

    /// Empty canvas (no layers, just background) should render as the
    /// background colour.
    #[test]
    fn empty_canvas_renders_background() {
        let bg = Srgb {
            r: 0.0,
            g: 0.0,
            b: 0.8,
        };
        let canvas = Canvas::new(8, 8, bg).unwrap();
        let field = Field::filled(8, 8, 0.0).unwrap();
        let palette = Palette::ocean();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.png");

        match render_to_png(&canvas, &field, &palette, &path) {
            Ok(()) => {
                let img = image::open(&path).unwrap().to_rgba8();
                let p = img.get_pixel(4, 4);
                assert!(p[2] > 180, "background blue not visible, got {:?}", p);
                assert!(p[0] < 30 && p[1] < 30);
            }
            Err(e) if skip_if_no_gl(&e) => eprintln!("skipping: {e}"),
            Err(e) => panic!("{e}"),
        }
    }

    /// Two consecutive `render_frame` calls with a feedback effect on a
    /// shrinking-content layer should produce a brighter centre on the
    /// second frame than a single frame would (the previous frame's
    /// content persists with decay).
    #[test]
    fn feedback_persists_between_frames() {
        // A single layer that paints solid red, with a feedback effect
        // applied on top. Frame 1 paints red; the post stage saves the
        // composite into feedback_target. Frame 2 paints red again, but
        // the feedback effect now max-blends with the previous frame —
        // since the previous frame was already red, the result is at
        // least as bright. With u_decay = 0.95 (the test value), feedback
        // sampling preserves most of the prior frame.
        let mut canvas = Canvas::new(8, 8, black()).unwrap();
        canvas
            .add_layer(
                Layer::new("c", ContentType::Field)
                    .with_blend_mode(BlendMode::Normal)
                    .with_effect(ShaderEffectDesc::new(
                        "solid",
                        serde_json::json!({"u_color": [0.6, 0.0, 0.0]}),
                    ))
                    .with_effect(ShaderEffectDesc::new(
                        "feedback",
                        serde_json::json!({"u_decay": 0.95}),
                    )),
            )
            .unwrap();
        let field = Field::filled(8, 8, 0.0).unwrap();
        let palette = Palette::ocean();

        match GpuSession::new(8, 8) {
            Ok(mut session) => {
                session.rebake_palette(&palette).unwrap();
                let dir = tempfile::tempdir().unwrap();
                let p1 = dir.path().join("fb1.png");
                let p2 = dir.path().join("fb2.png");
                session.render_to_png(&canvas, &field, &p1).unwrap();
                session.render_to_png(&canvas, &field, &p2).unwrap();

                let img1 = image::open(&p1).unwrap().to_rgba8();
                let img2 = image::open(&p2).unwrap().to_rgba8();
                let r1 = img1.get_pixel(4, 4)[0];
                let r2 = img2.get_pixel(4, 4)[0];
                // Frame 1 sees a black feedback texture, so it's
                // max(red, decay*black) = red.
                // Frame 2 sees frame 1's output (≈ same red) decayed by
                // 0.95, max-blended with current red. With current ≥
                // decayed, frame 2 should produce ≥ frame 1's red,
                // bounded above by the same red. Just assert both are
                // strongly red and frame 2 is not weaker than frame 1.
                assert!(r1 > 100, "frame 1 should be red, got {r1}");
                assert!(r2 >= r1.saturating_sub(2), "frame 2 ({r2}) lost frame 1 ({r1})");
            }
            Err(e) if skip_if_no_gl(&e) => eprintln!("skipping: {e}"),
            Err(e) => panic!("{e}"),
        }
    }

    /// A canvas that isn't 9:16 should still render — verifies that
    /// `u_resolution` is being pushed from the canvas dimensions rather
    /// than the hardcoded 1080×1920 default.
    ///
    /// Uses the `flow` shader which divides by `u_resolution` to compute
    /// aspect-corrected coordinates; with the wrong resolution the output
    /// collapses or stripes. We just check that the output isn't black.
    #[test]
    fn u_resolution_propagates_to_non_default_aspect() {
        let mut canvas = Canvas::new(64, 32, black()).unwrap(); // 2:1 aspect
        canvas
            .add_layer(
                Layer::new("c", ContentType::Field)
                    .with_effect(ShaderEffectDesc::new(
                        "flow",
                        serde_json::json!({"u_intensity": 1.0}),
                    )),
            )
            .unwrap();
        let field = Field::filled(64, 32, 0.5).unwrap();
        let palette = Palette::ocean();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("flow_wide.png");

        match render_to_png(&canvas, &field, &palette, &path) {
            Ok(()) => {
                let img = image::open(&path).unwrap().to_rgba8();
                assert_eq!(img.dimensions(), (64, 32));
                let any_color = img.pixels().any(|p| p[0] > 5 || p[1] > 5 || p[2] > 5);
                assert!(any_color, "flow at 2:1 aspect rendered black");
            }
            Err(e) if skip_if_no_gl(&e) => eprintln!("skipping: {e}"),
            Err(e) => panic!("{e}"),
        }
    }
}
