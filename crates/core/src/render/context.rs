//! GPU context wrapper with capability detection.
//!
//! `GpuContext` wraps a `glow::Context` and queries for required
//! extensions at initialization. The rendering pipeline requires
//! `EXT_color_buffer_float` for RGBA16F framebuffer attachments.

/// Wraps a `glow::Context` with detected GPU capabilities.
///
/// Created once at initialization. Stores whether critical extensions
/// like `EXT_color_buffer_float` are available, allowing the pipeline
/// to fail fast or select fallback paths.
pub struct GpuContext {
    gl: glow::Context,
    supports_color_buffer_float: bool,
}

impl GpuContext {
    /// Creates a new `GpuContext` by wrapping the given GL context
    /// and querying for required extensions.
    ///
    /// Checks for `EXT_color_buffer_float` which is **required** for
    /// rendering to RGBA16F framebuffer attachments. All intermediate
    /// FBOs in the pipeline use RGBA16F for HDR range.
    ///
    /// # Errors
    ///
    /// Returns an error if `EXT_color_buffer_float` is not supported,
    /// since the rendering pipeline cannot function without it.
    pub fn new(gl: glow::Context) -> Result<Self, String> {
        use glow::HasContext;

        // Some GL implementations (notably Mesa via EGL/GLES on Linux) return
        // extension names with a `GL_` prefix from `glGetStringi(GL_EXTENSIONS)`,
        // while others return the bare name. WebGL contexts return the bare
        // name. Accept either form so the same code path works in browser
        // and native headless contexts.
        let exts = gl.supported_extensions();
        let supports_color_buffer_float =
            exts.contains("EXT_color_buffer_float") || exts.contains("GL_EXT_color_buffer_float");

        if !supports_color_buffer_float {
            return Err("required extension EXT_color_buffer_float is not supported".to_string());
        }

        Ok(Self {
            gl,
            supports_color_buffer_float,
        })
    }

    /// Returns a reference to the underlying `glow::Context`.
    pub fn gl(&self) -> &glow::Context {
        &self.gl
    }

    /// Consumes this wrapper and returns the underlying `glow::Context`.
    pub fn into_gl(self) -> glow::Context {
        self.gl
    }

    /// Returns whether the `EXT_color_buffer_float` extension is supported.
    ///
    /// This extension is required for rendering to RGBA16F framebuffer
    /// attachments. Without it, the pipeline must fall back to RGBA8.
    pub fn supports_color_buffer_float(&self) -> bool {
        self.supports_color_buffer_float
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // GpuContext requires a live GL context, so integration tests are ignored.

    #[test]
    fn gpu_context_struct_compiles_with_expected_api() {
        // Compile-time check that the public API exists.
        // This test passes if the module compiles.
        fn _assert_api(ctx: &GpuContext) {
            let _gl: &glow::Context = ctx.gl();
            let _flag: bool = ctx.supports_color_buffer_float();
        }
    }

    #[test]
    #[ignore = "requires GL context"]
    fn new_succeeds_with_valid_context() {
        // Would test: GpuContext::new(gl) returns Ok.
    }

    #[test]
    #[ignore = "requires GL context"]
    fn supports_color_buffer_float_returns_bool() {
        // Would test: the flag matches actual extension support.
    }
}
