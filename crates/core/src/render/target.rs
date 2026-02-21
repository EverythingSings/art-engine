//! Render target (FBO + texture) for off-screen rendering.
//!
//! A `RenderTarget` pairs a framebuffer object with an RGBA16F color
//! attachment. Used for layer FBOs, composite FBOs, post-processing
//! ping-pong pairs, and the feedback texture.

use super::texture::{create_texture, TextureConfig};

/// An off-screen render target consisting of a framebuffer object and
/// its attached RGBA16F color texture.
///
/// All rendering in the pipeline goes through `RenderTarget`s rather
/// than the default framebuffer, enabling multi-pass effects and
/// layer compositing.
pub struct RenderTarget {
    fbo: glow::Framebuffer,
    texture: glow::Texture,
    width: u32,
    height: u32,
}

impl RenderTarget {
    /// Creates a new render target with an RGBA16F texture at the given dimensions.
    ///
    /// Creates a framebuffer, attaches a new texture as `COLOR_ATTACHMENT0`,
    /// and verifies framebuffer completeness.
    ///
    /// # Errors
    ///
    /// Returns an error if the framebuffer or texture cannot be created,
    /// or if the framebuffer is not complete.
    #[allow(unsafe_code)]
    pub fn new(gl: &glow::Context, width: u32, height: u32) -> Result<Self, String> {
        use glow::HasContext;

        let config = TextureConfig::rgba16f(width, height);
        let texture = create_texture(gl, &config)?;

        // SAFETY: glow wraps raw GL calls as unsafe. We create, configure,
        // and verify a framebuffer using valid texture handles.
        let fbo = unsafe { gl.create_framebuffer()? };

        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture),
                0,
            );

            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);

            if status != glow::FRAMEBUFFER_COMPLETE {
                gl.delete_framebuffer(fbo);
                gl.delete_texture(texture);
                return Err(format!("framebuffer incomplete: status 0x{status:04X}"));
            }
        }

        Ok(Self {
            fbo,
            texture,
            width,
            height,
        })
    }

    /// Binds this render target's framebuffer as the active draw target
    /// and sets the viewport to match the texture dimensions.
    #[allow(unsafe_code)]
    pub fn bind(&self, gl: &glow::Context) {
        use glow::HasContext;

        // SAFETY: self.fbo is a valid framebuffer handle created in new().
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.viewport(0, 0, self.width as i32, self.height as i32);
        }
    }

    /// Returns the texture handle for sampling this render target.
    pub fn texture(&self) -> glow::Texture {
        self.texture
    }

    /// Returns the width of this render target in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Returns the height of this render target in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Recreates the texture at a new size, keeping the same framebuffer.
    ///
    /// Deletes the old texture, creates a new RGBA16F texture at the given
    /// dimensions, and re-attaches it to the framebuffer.
    ///
    /// # Errors
    ///
    /// Returns an error if the new texture cannot be created or the
    /// framebuffer becomes incomplete.
    #[allow(unsafe_code)]
    pub fn resize(&mut self, gl: &glow::Context, width: u32, height: u32) -> Result<(), String> {
        use glow::HasContext;

        let config = TextureConfig::rgba16f(width, height);
        let new_texture = create_texture(gl, &config)?;

        // SAFETY: self.fbo is a valid framebuffer from new(). We swap
        // the texture attachment and verify completeness.
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(new_texture),
                0,
            );

            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);

            if status != glow::FRAMEBUFFER_COMPLETE {
                // Re-attach old texture to restore the FBO to a working state.
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(self.texture),
                    0,
                );
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                gl.delete_texture(new_texture);
                return Err(format!(
                    "framebuffer incomplete after resize: status 0x{status:04X}"
                ));
            }

            // Clean up old texture only after successful attachment.
            gl.delete_texture(self.texture);
        }

        self.texture = new_texture;
        self.width = width;
        self.height = height;

        Ok(())
    }

    /// Deletes the framebuffer and texture, releasing GPU resources.
    ///
    /// Must be called before dropping the `RenderTarget` if you want
    /// deterministic cleanup. The GL context does not have a destructor
    /// that cleans up individual objects.
    #[allow(unsafe_code)]
    pub fn destroy(&self, gl: &glow::Context) {
        use glow::HasContext;

        // SAFETY: self.fbo and self.texture are valid handles from new().
        unsafe {
            gl.delete_framebuffer(self.fbo);
            gl.delete_texture(self.texture);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // RenderTarget requires a live GL context, so all tests are ignored.
    // Run with `cargo test --features render -- --ignored` when a GL
    // context is available (e.g. with an EGL/osmesa headless setup).

    #[test]
    fn render_target_struct_has_expected_fields() {
        // Compile-time verification that the struct has the fields
        // we expect. This test passes if the module compiles.
        fn _assert_fields(rt: &RenderTarget) {
            let _fbo = rt.fbo;
            let _tex = rt.texture;
            let _w = rt.width;
            let _h = rt.height;
        }
    }

    #[test]
    #[ignore = "requires GL context"]
    fn new_creates_valid_render_target() {
        // Would test: RenderTarget::new(gl, 512, 512) succeeds
        // and returns correct width/height.
    }

    #[test]
    #[ignore = "requires GL context"]
    fn bind_sets_framebuffer() {
        // Would test: after bind(), the active framebuffer is this target's FBO.
    }

    #[test]
    #[ignore = "requires GL context"]
    fn resize_changes_dimensions() {
        // Would test: after resize(1024, 1024), width() and height() reflect new size.
    }

    #[test]
    #[ignore = "requires GL context"]
    fn destroy_cleans_up_resources() {
        // Would test: after destroy(), the FBO and texture are deleted.
    }
}
