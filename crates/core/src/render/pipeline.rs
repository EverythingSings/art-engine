//! Per-frame rendering pipeline orchestration.
//!
//! Connects the engine's CPU-side `Field` and `Palette` to the GPU shader
//! library: uploads the field as a texture, looks it up through a baked
//! palette LUT, applies each layer's effect chain via ping-pong, composites
//! the layers onto a running composite target according to their blend
//! modes, applies the canvas's post-processing stack via a second
//! ping-pong, saves the post output into a feedback texture for next
//! frame, then tonemaps to RGBA8 and reads the pixels back to the CPU.
//!
//! # Render order
//!
//! For each visible layer (bottom-to-top):
//! 1. Draw content into the layer ping-pong (palette LUT of the engine field).
//! 2. Apply the layer's effect chain. The `feedback` effect uniquely binds
//!    [`Pipeline::feedback_target`] (the previous frame's post output) to
//!    texture unit 1.
//! 3. Composite the layer through the composite ping-pong: a shader pass
//!    keyed by `Layer::blend_mode` reads the previous composite (src) and the
//!    layer, writing the new composite (dst), then swaps. Reading and writing
//!    distinct FBOs avoids a per-layer copy.
//!
//! After all layers are composited the post-processing stack runs on the
//! composite, the result is saved into the feedback target (only when a
//! `feedback` effect will sample it), and finally tonemapped to RGBA8 for
//! readback.

use std::collections::HashMap;

use glow::HasContext;
use serde_json::Value;
use thiserror::Error;

use crate::canvas::{BlendMode, Canvas, Layer, ShaderEffectDesc};
use crate::field::Field;
use crate::palette::Palette;
use crate::render::{
    apply_params, compile_program, GpuContext, PingPong, RenderTarget, ShaderError, TextureConfig,
    UniformDefault, UniformSchema, Uniforms, FULLSCREEN_VERTEX_SHADER,
};
use crate::shaders::{bloom_sources, composite, BuiltinShader};

/// Errors produced while constructing or running the pipeline.
#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("GL error: {0}")]
    Gl(String),
    #[error("shader: {0}")]
    Shader(#[from] ShaderError),
    #[error("unknown shader '{0}'")]
    UnknownShader(String),
}

impl From<String> for PipelineError {
    fn from(s: String) -> Self {
        PipelineError::Gl(s)
    }
}

/// One frame's worth of GPU rendering state.
///
/// Constructed once per render session (or per `render-sequence` run).
/// `render_frame` consumes the engine output for one frame and returns
/// an RGBA8 byte buffer in row-major top-to-bottom order suitable for
/// passing to the `image` crate.
pub struct Pipeline {
    width: u32,
    height: u32,

    field_tex: glow::Texture,
    palette_tex: glow::Texture,

    palette_program: glow::Program,
    tonemap_program: glow::Program,

    layer_targets: [RenderTarget; 2],
    layer_pp: PingPong,

    /// Ping-pong pair the layers blend through, bottom-to-top. Each layer
    /// reads the current composite (src) and writes the next (dst), then
    /// swaps — avoiding a per-layer read-write copy of a single target.
    composite_targets: [RenderTarget; 2],
    composite_pp: PingPong,
    /// One compiled program per blend mode, populated up front in
    /// `Pipeline::new` so per-frame compositing never pays a compile cost.
    blend_programs: HashMap<BlendMode, glow::Program>,

    /// Holds the previous frame's post-stack output for the `feedback`
    /// shader to sample. Cleared to black on construction so the first
    /// frame sees no trails.
    feedback_target: RenderTarget,

    bloom_threshold: glow::Program,
    bloom_blur: glow::Program,
    bloom_combine: glow::Program,
    bloom_targets: [RenderTarget; 2],

    post_targets: [RenderTarget; 2],
    post_pp: PingPong,

    final_target: RenderTarget,

    /// Cache of compiled per-effect programs keyed by canonical shader name.
    effect_programs: HashMap<&'static str, glow::Program>,

    /// Empty VAO required by core profiles for `glDrawArrays`. Bound once
    /// in `new` and kept alive for the pipeline's lifetime.
    #[allow(dead_code)]
    vao: glow::VertexArray,

    /// Scratch buffer reused between frames for f64→f32 field upload.
    field_scratch: Vec<f32>,

    /// Animation clock pushed to every shader's `u_time` uniform for the
    /// current frame. Zero for still renders.
    frame_time: f32,
}

impl Pipeline {
    /// Creates a new pipeline at the given canvas dimensions.
    ///
    /// Compiles the always-needed programs (palette lookup, tonemap, bloom,
    /// and one blend program per [`BlendMode`]) and allocates the RGBA16F
    /// render targets for the layer ping-pong, composite + copy, feedback,
    /// bloom, and post-process ping-pong, plus a final RGBA8 readback target.
    pub fn new(gpu: &GpuContext, width: u32, height: u32) -> Result<Self, PipelineError> {
        let gl = gpu.gl();

        // SAFETY: glow GL calls are unsafe-wrapped. All handles below are
        // produced and immediately validated by glow itself.
        #[allow(unsafe_code)]
        let vao = unsafe { gl.create_vertex_array().map_err(PipelineError::Gl)? };
        #[allow(unsafe_code)]
        unsafe {
            gl.bind_vertex_array(Some(vao));
        }

        // Single-channel R32F texture for the field. Filled per frame.
        #[allow(unsafe_code)]
        let field_tex = unsafe { gl.create_texture().map_err(PipelineError::Gl)? };
        #[allow(unsafe_code)]
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(field_tex));
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::R32F as i32,
                width as i32,
                height as i32,
                0,
                glow::RED,
                glow::FLOAT,
                glow::PixelUnpackData::Slice(None),
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
        }

        let palette_tex = create_palette_lut(gl)?;

        let palette_program =
            compile_program(gl, FULLSCREEN_VERTEX_SHADER, PALETTE_FRAGMENT_SOURCE)?;
        let tonemap_program =
            compile_program(gl, FULLSCREEN_VERTEX_SHADER, TONEMAP_FRAGMENT_SOURCE)?;

        let layer_targets = [
            RenderTarget::new(gl, width, height)?,
            RenderTarget::new(gl, width, height)?,
        ];
        let composite_targets = [
            RenderTarget::new(gl, width, height)?,
            RenderTarget::new(gl, width, height)?,
        ];
        let feedback_target = RenderTarget::new(gl, width, height)?;
        // Clear feedback to opaque black so frame 0's feedback effect
        // samples a clean slate rather than uninitialised GPU memory.
        clear_target_to(gl, &feedback_target, [0.0, 0.0, 0.0, 1.0]);

        let bloom_targets = [
            RenderTarget::new(gl, width, height)?,
            RenderTarget::new(gl, width, height)?,
        ];
        let post_targets = [
            RenderTarget::new(gl, width, height)?,
            RenderTarget::new(gl, width, height)?,
        ];
        // Final target is RGBA8 so `glReadPixels(RGBA, UNSIGNED_BYTE)` is
        // guaranteed valid by the GLES 3 spec (it is not, from RGBA16F).
        let final_target = RenderTarget::with_config(gl, &TextureConfig::rgba8(width, height))?;

        let bloom = bloom_sources();
        let bloom_threshold = compile_program(gl, FULLSCREEN_VERTEX_SHADER, bloom.threshold)?;
        let bloom_blur = compile_program(gl, FULLSCREEN_VERTEX_SHADER, bloom.blur)?;
        let bloom_combine = compile_program(gl, FULLSCREEN_VERTEX_SHADER, bloom.combine)?;

        let mut blend_programs = HashMap::new();
        for (mode, source) in [
            (BlendMode::Normal, composite::NORMAL_SOURCE),
            (BlendMode::Additive, composite::ADDITIVE_SOURCE),
            (BlendMode::Multiply, composite::MULTIPLY_SOURCE),
            (BlendMode::Screen, composite::SCREEN_SOURCE),
            (BlendMode::Overlay, composite::OVERLAY_SOURCE),
        ] {
            blend_programs.insert(mode, compile_program(gl, FULLSCREEN_VERTEX_SHADER, source)?);
        }

        Ok(Self {
            width,
            height,
            field_tex,
            palette_tex,
            palette_program,
            tonemap_program,
            layer_targets,
            layer_pp: PingPong::new(),
            composite_targets,
            composite_pp: PingPong::new(),
            blend_programs,
            feedback_target,
            bloom_threshold,
            bloom_blur,
            bloom_combine,
            bloom_targets,
            post_targets,
            post_pp: PingPong::new(),
            final_target,
            effect_programs: HashMap::new(),
            vao,
            field_scratch: Vec::with_capacity((width * height) as usize),
            frame_time: 0.0,
        })
    }

    /// Renders one frame. `field` is the engine output; `palette` is the
    /// CPU palette already baked into the LUT (rebake by calling
    /// [`Self::rebake_palette`] if the palette changes mid-session).
    /// Returns an RGBA8 buffer of size `width * height * 4` in row-major,
    /// top-to-bottom order (suitable for `image::ImageBuffer::from_raw`).
    ///
    /// Layers without `visible() == true` are skipped. An empty canvas (no
    /// layers, or all hidden) renders the background colour with the post
    /// stack applied over it.
    pub fn render_frame(
        &mut self,
        gpu: &GpuContext,
        canvas: &Canvas,
        field: &Field,
    ) -> Result<Vec<u8>, PipelineError> {
        self.render_frame_at(gpu, canvas, field, 0.0)
    }

    /// Like [`Self::render_frame`] but drives every shader's `u_time` uniform
    /// with `time`, for rendering animation frames. `time` is in arbitrary
    /// units; shaders scale it by their own `u_speed`.
    pub fn render_frame_at(
        &mut self,
        gpu: &GpuContext,
        canvas: &Canvas,
        field: &Field,
        time: f32,
    ) -> Result<Vec<u8>, PipelineError> {
        self.frame_time = time;
        let gl = gpu.gl();
        self.upload_field(gl, field)?;

        // Seed the composite ping-pong's source with the background; layers
        // read it and write the partner, swapping each time.
        let bg = canvas.background();
        clear_target_to(
            gl,
            &self.composite_targets[self.composite_pp.src_index()],
            [bg.r as f32, bg.g as f32, bg.b as f32, 1.0],
        );

        for layer in canvas.layers().iter().filter(|l| l.visible()) {
            self.render_layer(gl, layer)?;
        }

        // Hand the composite to the post ping-pong, run the post stack.
        self.copy(gl, self.composite_targets[self.composite_pp.src_index()].texture())?;
        self.post_pp.swap();
        for effect in canvas.post_stack() {
            self.apply_effect(gl, effect, RenderStage::Post)?;
        }

        // Retain this frame's output only if a `feedback` effect will sample
        // it next frame — otherwise the copy is wasted work.
        if canvas_uses_feedback(canvas) {
            self.save_feedback(gl);
        }

        self.tonemap_into_final(gl);
        self.read_pixels_rgba8(gl)
    }

    /// Renders one layer end-to-end: content, effects, composite.
    fn render_layer(&mut self, gl: &glow::Context, layer: &Layer) -> Result<(), PipelineError> {
        // 1. Render content (palette lookup of the engine field) into
        //    layer_dst. draw_content() clears the target so leftover bits
        //    from the previous layer's effect chain are discarded.
        self.draw_content(gl);
        self.layer_pp.swap();

        // 2. Apply per-layer effect chain on the layer ping-pong.
        for effect in layer.effects() {
            self.apply_effect(gl, effect, RenderStage::Layer)?;
        }

        // 3. Composite the layer's final src onto the running composite.
        self.composite_layer(gl, layer.blend_mode(), layer.opacity() as f32)
    }

    /// Blends the current `layer_src` over the composite source into the
    /// composite destination, then swaps the composite ping-pong. The blend
    /// shader reads the previous composite (`u_composite`) and the layer
    /// (`u_layer`) — distinct FBOs, so no read-write copy is needed.
    fn composite_layer(
        &mut self,
        gl: &glow::Context,
        mode: BlendMode,
        opacity: f32,
    ) -> Result<(), PipelineError> {
        let program = *self.blend_programs.get(&mode).ok_or_else(|| {
            PipelineError::Gl(format!("missing blend program for mode {mode:?}"))
        })?;
        let layer_tex = self.layer_src().texture();
        let composite_src = self.composite_targets[self.composite_pp.src_index()].texture();

        self.composite_targets[self.composite_pp.dst_index()].bind(gl);
        begin_pass(gl, program, layer_tex);
        // SAFETY: composite_src is a valid texture handle owned by this pipeline.
        #[allow(unsafe_code)]
        unsafe {
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(composite_src));
        }
        let u = Uniforms::new(gl, program);
        u.try_set_sampler("u_layer", 0);
        u.try_set_sampler("u_composite", 1);
        u.try_set_f32("u_opacity", opacity);
        draw_fullscreen(gl);
        self.composite_pp.swap();
        Ok(())
    }

    /// Identity-copies `src_tex` into `dst` using the tonemap passthrough
    /// program. Used for the layer→post hand-off and for saving feedback.
    fn copy_target(&self, gl: &glow::Context, src_tex: glow::Texture, dst: &RenderTarget) {
        dst.bind(gl);
        begin_pass(gl, self.tonemap_program, src_tex);
        let u = Uniforms::new(gl, self.tonemap_program);
        u.try_set_sampler("u_texture", 0);
        // passthrough = 1.0: identity copy, no clamp/encode.
        u.try_set_f32("u_passthrough", 1.0);
        draw_fullscreen(gl);
    }

    /// Saves the current post_src into the feedback target so the next
    /// frame's `feedback` shader can sample it.
    fn save_feedback(&self, gl: &glow::Context) {
        self.copy_target(gl, self.post_src().texture(), &self.feedback_target);
    }

    /// Re-bakes the palette LUT. Call this when the palette changes between
    /// frames within a single pipeline lifetime.
    pub fn rebake_palette(
        &mut self,
        gpu: &GpuContext,
        palette: &Palette,
    ) -> Result<(), PipelineError> {
        let gl = gpu.gl();
        upload_palette_lut(gl, self.palette_tex, palette);
        Ok(())
    }

    /// Bakes the given palette into the GPU LUT. Convenience for one-shot
    /// pipelines that don't need to swap palettes.
    pub fn with_palette(
        mut self,
        gpu: &GpuContext,
        palette: &Palette,
    ) -> Result<Self, PipelineError> {
        self.rebake_palette(gpu, palette)?;
        Ok(self)
    }

    fn upload_field(&mut self, gl: &glow::Context, field: &Field) -> Result<(), PipelineError> {
        if field.width() as u32 != self.width || field.height() as u32 != self.height {
            return Err(PipelineError::Gl(format!(
                "field dimensions {}x{} do not match pipeline {}x{}",
                field.width(),
                field.height(),
                self.width,
                self.height,
            )));
        }
        self.field_scratch.clear();
        self.field_scratch
            .extend(field.data().iter().map(|&v| v as f32));
        // SAFETY: scratch length == width*height; texture allocated for that
        // size + format in `Pipeline::new`.
        #[allow(unsafe_code)]
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.field_tex));
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                self.width as i32,
                self.height as i32,
                glow::RED,
                glow::FLOAT,
                glow::PixelUnpackData::Slice(Some(bytemuck_cast(&self.field_scratch))),
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
        Ok(())
    }

    fn draw_content(&self, gl: &glow::Context) {
        self.layer_dst().bind(gl);
        begin_pass(gl, self.palette_program, self.field_tex);
        // SAFETY: clear the freshly-bound layer FBO and bind the palette LUT
        // to unit 1 for the lookup.
        #[allow(unsafe_code)]
        unsafe {
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.palette_tex));
        }
        let uniforms = Uniforms::new(gl, self.palette_program);
        uniforms.try_set_sampler("u_field", 0);
        uniforms.try_set_sampler("u_palette", 1);
        draw_fullscreen(gl);
    }

    fn apply_effect(
        &mut self,
        gl: &glow::Context,
        effect: &ShaderEffectDesc,
        stage: RenderStage,
    ) -> Result<(), PipelineError> {
        let shader = BuiltinShader::from_name(&effect.name)
            .ok_or_else(|| PipelineError::UnknownShader(effect.name.clone()))?;

        match shader {
            BuiltinShader::Bloom => self.apply_bloom(gl, &effect.params, stage)?,
            _ => self.apply_single_pass(gl, shader, &effect.params, stage)?,
        }
        Ok(())
    }

    fn apply_single_pass(
        &mut self,
        gl: &glow::Context,
        shader: BuiltinShader,
        params: &Value,
        stage: RenderStage,
    ) -> Result<(), PipelineError> {
        let program = self.compile_or_get_effect(gl, shader)?;
        let (src_tex, dst) = match stage {
            RenderStage::Layer => (
                self.layer_targets[self.layer_pp.src_index()].texture(),
                &self.layer_targets[self.layer_pp.dst_index()],
            ),
            RenderStage::Post => (
                self.post_targets[self.post_pp.src_index()].texture(),
                &self.post_targets[self.post_pp.dst_index()],
            ),
        };
        dst.bind(gl);
        begin_pass(gl, program, src_tex);
        let uniforms = Uniforms::new(gl, program);
        uniforms.try_set_sampler("u_texture", 0);
        if matches!(shader, BuiltinShader::Feedback) {
            // SAFETY: feedback_target is a valid texture handle.
            #[allow(unsafe_code)]
            unsafe {
                gl.active_texture(glow::TEXTURE1);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.feedback_target.texture()));
            }
            uniforms.try_set_sampler("u_feedback", 1);
        }
        apply_params(&uniforms, params, default_uniform_schema(shader));
        // u_resolution is universally the canvas dimensions — override any
        // schema default or JSON value so shaders that depend on aspect
        // (flow, plasma, branch, …) render correctly at non-9:16 sizes.
        uniforms.try_set_vec2("u_resolution", [self.width as f32, self.height as f32]);
        // u_time is driven by the pipeline's animation clock (0 for stills),
        // so frames of one composition share a single coherent timeline.
        uniforms.try_set_f32("u_time", self.frame_time);
        draw_fullscreen(gl);
        match stage {
            RenderStage::Layer => self.layer_pp.swap(),
            RenderStage::Post => self.post_pp.swap(),
        }
        Ok(())
    }

    fn apply_bloom(
        &mut self,
        gl: &glow::Context,
        params: &Value,
        stage: RenderStage,
    ) -> Result<(), PipelineError> {
        let intensity = crate::params::param_f64(params, "intensity", 0.6) as f32;
        let threshold = crate::params::param_f64(params, "threshold", 0.7) as f32;
        let soft_knee = crate::params::param_f64(params, "soft_knee", 0.5) as f32;
        let radius = crate::params::param_f64(params, "radius", 4.0) as f32;

        // Pass A: threshold src → bloom_targets[0]
        let src_tex = match stage {
            RenderStage::Layer => self.layer_targets[self.layer_pp.src_index()].texture(),
            RenderStage::Post => self.post_targets[self.post_pp.src_index()].texture(),
        };
        self.bloom_targets[0].bind(gl);
        begin_pass(gl, self.bloom_threshold, src_tex);
        let u = Uniforms::new(gl, self.bloom_threshold);
        u.try_set_sampler("u_texture", 0);
        u.try_set_f32("u_threshold", threshold);
        u.try_set_f32("u_soft_knee", soft_knee);
        draw_fullscreen(gl);

        // Pass B: blur horizontal bloom_targets[0] → bloom_targets[1]
        self.bloom_targets[1].bind(gl);
        begin_pass(gl, self.bloom_blur, self.bloom_targets[0].texture());
        let u = Uniforms::new(gl, self.bloom_blur);
        u.try_set_sampler("u_texture", 0);
        u.try_set_vec2("u_direction", [radius / self.width as f32, 0.0]);
        draw_fullscreen(gl);

        // Pass C: blur vertical bloom_targets[1] → bloom_targets[0]
        self.bloom_targets[0].bind(gl);
        begin_pass(gl, self.bloom_blur, self.bloom_targets[1].texture());
        let u = Uniforms::new(gl, self.bloom_blur);
        u.try_set_sampler("u_texture", 0);
        u.try_set_vec2("u_direction", [0.0, radius / self.height as f32]);
        draw_fullscreen(gl);

        // Pass D: combine src + blurred → stage dst
        let dst = match stage {
            RenderStage::Layer => &self.layer_targets[self.layer_pp.dst_index()],
            RenderStage::Post => &self.post_targets[self.post_pp.dst_index()],
        };
        dst.bind(gl);
        begin_pass(gl, self.bloom_combine, src_tex);
        // SAFETY: blurred bloom result is a valid texture handle.
        #[allow(unsafe_code)]
        unsafe {
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.bloom_targets[0].texture()));
        }
        let u = Uniforms::new(gl, self.bloom_combine);
        u.try_set_sampler("u_texture", 0);
        u.try_set_sampler("u_bloom", 1);
        u.try_set_f32("u_intensity", intensity);
        draw_fullscreen(gl);

        match stage {
            RenderStage::Layer => self.layer_pp.swap(),
            RenderStage::Post => self.post_pp.swap(),
        }
        Ok(())
    }

    fn copy(&self, gl: &glow::Context, src_tex: glow::Texture) -> Result<(), PipelineError> {
        // Render `src_tex` into the current post_dst via the tonemap
        // identity passthrough. Caller is responsible for swapping the
        // post ping-pong after this returns.
        self.copy_target(gl, src_tex, &self.post_targets[self.post_pp.dst_index()]);
        Ok(())
    }

    fn tonemap_into_final(&self, gl: &glow::Context) {
        self.final_target.bind(gl);
        begin_pass(gl, self.tonemap_program, self.post_src().texture());
        let u = Uniforms::new(gl, self.tonemap_program);
        u.try_set_sampler("u_texture", 0);
        u.try_set_f32("u_passthrough", 0.0); // apply clamp + sRGB encode
        draw_fullscreen(gl);
    }

    fn read_pixels_rgba8(&self, gl: &glow::Context) -> Result<Vec<u8>, PipelineError> {
        let mut buf = vec![0u8; (self.width * self.height * 4) as usize];
        self.final_target.bind(gl);
        // SAFETY: buffer length matches pixel count.
        #[allow(unsafe_code)]
        unsafe {
            gl.read_pixels(
                0,
                0,
                self.width as i32,
                self.height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut buf)),
            );
        }
        // GL origin is bottom-left, image crate / PNG convention is top-left.
        // Flip rows in place.
        flip_rows_rgba8(&mut buf, self.width as usize, self.height as usize);
        Ok(buf)
    }

    fn compile_or_get_effect(
        &mut self,
        gl: &glow::Context,
        shader: BuiltinShader,
    ) -> Result<glow::Program, PipelineError> {
        let name = shader.name();
        if let Some(&program) = self.effect_programs.get(name) {
            return Ok(program);
        }
        let program = compile_program(gl, FULLSCREEN_VERTEX_SHADER, shader.fragment_source())?;
        self.effect_programs.insert(name, program);
        Ok(program)
    }

    fn layer_src(&self) -> &RenderTarget {
        &self.layer_targets[self.layer_pp.src_index()]
    }

    fn layer_dst(&self) -> &RenderTarget {
        &self.layer_targets[self.layer_pp.dst_index()]
    }

    fn post_src(&self) -> &RenderTarget {
        &self.post_targets[self.post_pp.src_index()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderStage {
    Layer,
    Post,
}

/// Returns the per-shader uniform schema used by [`apply_params`].
fn default_uniform_schema(shader: BuiltinShader) -> UniformSchema {
    use UniformDefault::*;
    match shader {
        BuiltinShader::Feedback => &[("u_decay", F32(0.92)), ("u_offset", Vec2([0.0, 0.0]))],
        BuiltinShader::Voronoi => &[
            ("u_scale", F32(8.0)),
            ("u_edge_width", F32(0.05)),
            ("u_time", F32(0.0)),
            ("u_jitter", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_edge_color", Vec3([1.0, 1.0, 1.0])),
            ("u_color_a", Vec3([0.1, 0.05, 0.2])),
            ("u_color_b", Vec3([0.0, 0.5, 0.8])),
        ],
        BuiltinShader::Kaleidoscope => &[
            ("u_segments", F32(6.0)),
            ("u_rotation", F32(0.0)),
            ("u_center", Vec2([0.5, 0.5])),
            ("u_zoom", F32(1.0)),
        ],
        BuiltinShader::Flow => &[
            ("u_time", F32(0.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_centroid", F32(0.5)),
            ("u_intensity", F32(1.0)),
            ("u_seed", F32(11.0)),
            ("u_pal_low", Vec3([0.04, 0.05, 0.10])),
            ("u_pal_mid", Vec3([0.10, 0.32, 0.40])),
            ("u_pal_high", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Solid => &[("u_color", Vec3([0.04, 0.05, 0.10]))],
        BuiltinShader::NoiseStatic => &[
            ("u_time", F32(0.0)),
            ("u_intensity", F32(1.0)),
            ("u_density", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Lattice => &[
            ("u_time", F32(0.0)),
            ("u_density", F32(12.0)),
            ("u_thickness", F32(0.06)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_bg", Vec3([0.04, 0.05, 0.10])),
            ("u_color_line", Vec3([0.96, 0.74, 0.36])),
        ],
        BuiltinShader::Mandala => &[
            ("u_time", F32(0.0)),
            ("u_segments", F32(8.0)),
            ("u_freq", F32(12.0)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Concentric => &[
            ("u_time", F32(0.0)),
            ("u_freq", F32(18.0)),
            ("u_speed", F32(1.0)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Strands => &[
            ("u_time", F32(0.0)),
            ("u_density", F32(48.0)),
            ("u_thickness", F32(0.18)),
            ("u_jitter", F32(0.6)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
        ],
        BuiltinShader::Wave => &[
            ("u_time", F32(0.0)),
            ("u_density", F32(6.0)),
            ("u_freq", F32(1.5)),
            ("u_amplitude", F32(0.4)),
            ("u_speed", F32(1.0)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
        ],
        BuiltinShader::Spiral => &[
            ("u_time", F32(0.0)),
            ("u_arms", F32(3.0)),
            ("u_tightness", F32(1.0)),
            ("u_speed", F32(1.0)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Halftone => &[
            ("u_time", F32(0.0)),
            ("u_cell", F32(22.0)),
            ("u_strength", F32(1.0)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Crosshatch => &[
            ("u_time", F32(0.0)),
            ("u_spacing", F32(14.0)),
            ("u_thickness", F32(1.3)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Topo => &[
            ("u_time", F32(0.0)),
            ("u_scale", F32(3.0)),
            ("u_density", F32(8.0)),
            ("u_thickness", F32(0.04)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Aurora => &[
            ("u_time", F32(0.0)),
            ("u_curtains", F32(3.0)),
            ("u_speed", F32(1.0)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Sun => &[
            ("u_time", F32(0.0)),
            ("u_radius", F32(0.18)),
            ("u_rays", F32(24.0)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Particles => &[
            ("u_time", F32(0.0)),
            ("u_count", F32(16.0)),
            ("u_glow", F32(0.025)),
            ("u_speed", F32(1.0)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Branch => &[
            ("u_time", F32(0.0)),
            ("u_branches", F32(4.0)),
            ("u_thickness", F32(0.012)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Caustics => &[
            ("u_time", F32(0.0)),
            ("u_scale", F32(3.0)),
            ("u_sharpness", F32(7.0)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Phyllotaxis => &[
            ("u_time", F32(0.0)),
            ("u_count", F32(140.0)),
            ("u_radius_scale", F32(0.030)),
            ("u_seed_radius", F32(90.0)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Constellation => &[
            ("u_time", F32(0.0)),
            ("u_node_glow", F32(240.0)),
            ("u_edge_glow", F32(620.0)),
            ("u_edge_strength", F32(0.55)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::VectorField => &[
            ("u_time", F32(0.0)),
            ("u_scale", F32(2.5)),
            ("u_freq", F32(1.3)),
            ("u_density", F32(6.0)),
            ("u_thickness", F32(0.06)),
            ("u_dash_speed", F32(4.0)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Crystal => &[
            ("u_time", F32(0.0)),
            ("u_scale", F32(7.0)),
            ("u_levels", F32(5.0)),
            ("u_edge_width", F32(0.03)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Smoke => &[
            ("u_time", F32(0.0)),
            ("u_scale", F32(2.2)),
            ("u_warp", F32(0.7)),
            ("u_speed", F32(1.0)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Moire => &[
            ("u_time", F32(0.0)),
            ("u_freq", F32(80.0)),
            ("u_angle_delta", F32(0.18)),
            ("u_thickness", F32(0.35)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Ripple => &[
            ("u_time", F32(0.0)),
            ("u_freq", F32(18.0)),
            ("u_speed", F32(1.2)),
            ("u_decay", F32(2.0)),
            ("u_sharpness", F32(3.0)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Plasma => &[
            ("u_time", F32(0.0)),
            ("u_count", F32(6.0)),
            ("u_radius", F32(0.20)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Bokeh => &[
            ("u_time", F32(0.0)),
            ("u_count", F32(9.0)),
            ("u_radius", F32(0.18)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Mosaic => &[
            ("u_time", F32(0.0)),
            ("u_grid", F32(14.0)),
            ("u_levels", F32(5.0)),
            ("u_gap", F32(0.06)),
            ("u_intensity", F32(1.0)),
            ("u_rms", F32(0.0)),
            ("u_onset", F32(0.0)),
            ("u_color_lo", Vec3([0.04, 0.05, 0.10])),
            ("u_color_hi", Vec3([0.96, 0.74, 0.36])),
            ("u_resolution", Vec2([1080.0, 1920.0])),
        ],
        BuiltinShader::Bloom => &[
            ("u_intensity", F32(0.6)),
            ("u_threshold", F32(0.7)),
            ("u_soft_knee", F32(0.5)),
            ("u_radius", F32(4.0)),
        ],
        BuiltinShader::Vignette => &[
            ("u_strength", F32(0.4)),
            ("u_radius", F32(0.75)),
            ("u_softness", F32(0.45)),
        ],
        BuiltinShader::Grain => &[("u_amount", F32(0.02)), ("u_time", F32(0.0))],
        BuiltinShader::ColorGrade => &[
            ("u_lift", Vec3([0.0, 0.0, 0.0])),
            ("u_gamma", Vec3([1.0, 1.0, 1.0])),
            ("u_gain", Vec3([1.0, 1.0, 1.0])),
            ("u_saturation", F32(1.0)),
        ],
    }
}

/// Bakes a 256-pixel palette LUT (256x1, RGBA8).
fn create_palette_lut(gl: &glow::Context) -> Result<glow::Texture, PipelineError> {
    // SAFETY: standard glow texture creation; handle is validated below.
    #[allow(unsafe_code)]
    let tex = unsafe { gl.create_texture().map_err(PipelineError::Gl)? };
    // SAFETY: handle is valid; format/dimensions are canonical.
    #[allow(unsafe_code)]
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        // Allocate empty 256x1; data uploaded in upload_palette_lut.
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            256,
            1,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        gl.bind_texture(glow::TEXTURE_2D, None);
    }
    Ok(tex)
}

fn upload_palette_lut(gl: &glow::Context, tex: glow::Texture, palette: &Palette) {
    let mut data = Vec::with_capacity(256 * 4);
    for i in 0..256 {
        let t = i as f64 / 255.0;
        let c = palette.sample(t);
        data.push(srgb_to_u8(c.r));
        data.push(srgb_to_u8(c.g));
        data.push(srgb_to_u8(c.b));
        data.push(255);
    }
    // SAFETY: tex is valid; data length == 256*4.
    #[allow(unsafe_code)]
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_sub_image_2d(
            glow::TEXTURE_2D,
            0,
            0,
            0,
            256,
            1,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(&data)),
        );
        gl.bind_texture(glow::TEXTURE_2D, None);
    }
}

fn srgb_to_u8(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Starts a fullscreen pass: activates `program`, disables blending, and
/// binds `tex0` to texture unit 0. Callers bind any additional textures and
/// set uniforms afterward, then call [`draw_fullscreen`]. Centralises the
/// `unsafe` GL state-setup shared by every pass.
fn begin_pass(gl: &glow::Context, program: glow::Program, tex0: glow::Texture) {
    // SAFETY: program and tex0 are valid handles owned by the pipeline.
    #[allow(unsafe_code)]
    unsafe {
        gl.use_program(Some(program));
        gl.disable(glow::BLEND);
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(tex0));
    }
}

/// True if any layer effect or post effect is the `feedback` shader, meaning
/// this frame's output must be retained for the next frame to sample.
fn canvas_uses_feedback(canvas: &Canvas) -> bool {
    let in_layers = canvas
        .layers()
        .iter()
        .flat_map(|l| l.effects())
        .any(|e| e.name.eq_ignore_ascii_case("feedback"));
    in_layers
        || canvas
            .post_stack()
            .iter()
            .any(|e| e.name.eq_ignore_ascii_case("feedback"))
}

/// Clears a render target to the given RGBA value.
///
/// Used to initialise the composite target with the canvas background and
/// to seed the feedback target with opaque black at construction.
fn clear_target_to(gl: &glow::Context, target: &RenderTarget, rgba: [f32; 4]) {
    target.bind(gl);
    // SAFETY: target.bind set up a valid framebuffer; clear writes the
    // bound color attachment.
    #[allow(unsafe_code)]
    unsafe {
        gl.disable(glow::BLEND);
        gl.clear_color(rgba[0], rgba[1], rgba[2], rgba[3]);
        gl.clear(glow::COLOR_BUFFER_BIT);
    }
}

fn draw_fullscreen(gl: &glow::Context) {
    // SAFETY: empty VAO is bound globally in Pipeline::new; arrays draw
    // pulls 3 vertices entirely from gl_VertexID.
    #[allow(unsafe_code)]
    unsafe {
        gl.draw_arrays(glow::TRIANGLES, 0, 3);
    }
}

fn flip_rows_rgba8(buf: &mut [u8], width: usize, height: usize) {
    let row_bytes = width * 4;
    for y in 0..height / 2 {
        let other = height - 1 - y;
        // Split-borrow trick: split_at_mut so we can swap two rows safely.
        let (top, bot) = buf.split_at_mut((y + 1) * row_bytes);
        let top_row = &mut top[y * row_bytes..(y + 1) * row_bytes];
        let bot_row_start = (other - (y + 1)) * row_bytes;
        let bot_row = &mut bot[bot_row_start..bot_row_start + row_bytes];
        top_row.swap_with_slice(bot_row);
    }
}

/// Reinterpret a `&[f32]` as `&[u8]` without allocating. Used for the
/// per-frame field upload.
fn bytemuck_cast(v: &[f32]) -> &[u8] {
    // SAFETY: f32 is plain-old-data with no padding; reinterpreting as
    // bytes is safe and used solely for an immediate `glTexSubImage2D`.
    #[allow(unsafe_code)]
    unsafe {
        std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v))
    }
}

/// Fragment shader: sample a R32F field by UV, look up a 256x1 RGBA8 palette
/// LUT keyed by the field value clamped to `[0, 1]`. Used to render an
/// engine's `Field` to the layer's RGBA16F target.
const PALETTE_FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;
in vec2 v_uv;
out vec4 fragColor;
uniform sampler2D u_field;
uniform sampler2D u_palette;
void main() {
    float t = clamp(texture(u_field, v_uv).r, 0.0, 1.0);
    vec3 rgb = texture(u_palette, vec2(t, 0.5)).rgb;
    fragColor = vec4(rgb, 1.0);
}
"#;

/// Fragment shader: HDR → SDR. When `u_passthrough = 1.0` it is an identity
/// copy (used for stage hand-off). Otherwise it clamps to `[0, 1]`. sRGB
/// encoding happens implicitly because the engine palettes already produce
/// sRGB values and the LUT is RGBA8 sRGB-like.
const TONEMAP_FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;
in vec2 v_uv;
out vec4 fragColor;
uniform sampler2D u_texture;
uniform float u_passthrough;
void main() {
    vec4 c = texture(u_texture, v_uv);
    vec3 rgb = mix(clamp(c.rgb, 0.0, 1.0), c.rgb, u_passthrough);
    fragColor = vec4(rgb, c.a);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flip_rows_rgba8_swaps_top_and_bottom() {
        // 2 rows, 1 pixel each. 4 bytes per pixel.
        let mut buf: Vec<u8> = vec![1, 2, 3, 4, 5, 6, 7, 8];
        flip_rows_rgba8(&mut buf, 1, 2);
        assert_eq!(buf, vec![5, 6, 7, 8, 1, 2, 3, 4]);
    }

    #[test]
    fn flip_rows_rgba8_handles_odd_height_no_op_middle_row() {
        // 3 rows, 1 pixel each.
        let mut buf: Vec<u8> = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        flip_rows_rgba8(&mut buf, 1, 3);
        // Top and bottom swap; middle stays.
        assert_eq!(buf, vec![9, 10, 11, 12, 5, 6, 7, 8, 1, 2, 3, 4]);
    }

    #[test]
    fn srgb_to_u8_clamps_and_rounds() {
        assert_eq!(srgb_to_u8(-1.0), 0);
        assert_eq!(srgb_to_u8(0.0), 0);
        assert_eq!(srgb_to_u8(0.5), 128);
        assert_eq!(srgb_to_u8(1.0), 255);
        assert_eq!(srgb_to_u8(2.0), 255);
    }

    #[test]
    fn default_uniform_schema_returns_at_least_one_uniform_per_shader() {
        for name in BuiltinShader::list() {
            let s = BuiltinShader::from_name(name).unwrap();
            assert!(!default_uniform_schema(s).is_empty(), "{name}");
        }
    }
}
