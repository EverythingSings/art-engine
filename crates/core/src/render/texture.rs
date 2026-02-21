//! Texture creation helpers for WebGL2 / OpenGL.
//!
//! Provides `TextureConfig` for specifying texture parameters and
//! `create_texture` for allocating GPU textures. All intermediate
//! framebuffer textures use RGBA16F for HDR range.

/// Configuration for creating a GPU texture.
///
/// Stores dimensions, internal format, and filter mode. Use the
/// convenience constructors (e.g. [`TextureConfig::rgba16f`]) for
/// common configurations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureConfig {
    /// Texture width in pixels.
    pub width: u32,
    /// Texture height in pixels.
    pub height: u32,
    /// GL internal format (e.g. `glow::RGBA16F`).
    pub internal_format: u32,
    /// GL texture filter mode (e.g. `glow::LINEAR`).
    pub filter: u32,
}

impl TextureConfig {
    /// Creates a config for an RGBA16F (half-float HDR) texture with LINEAR filtering.
    ///
    /// This is the standard format for all intermediate FBOs in the rendering
    /// pipeline, providing HDR range for bloom thresholding, additive blending,
    /// and banding prevention.
    pub fn rgba16f(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            internal_format: glow::RGBA16F,
            filter: glow::LINEAR,
        }
    }
}

/// Returns the GL pixel type that corresponds to a given internal format.
///
/// Derives the upload type from the internal format rather than always
/// assuming `HALF_FLOAT`, so that `RGBA8` textures use `UNSIGNED_BYTE`.
pub fn pixel_type_for_format(internal_format: u32) -> u32 {
    match internal_format {
        glow::RGBA16F | glow::RGB16F => glow::HALF_FLOAT,
        glow::RGBA32F | glow::RGB32F => glow::FLOAT,
        _ => glow::UNSIGNED_BYTE,
    }
}

/// Creates a GPU texture from the given configuration.
///
/// Sets wrap mode to `CLAMP_TO_EDGE` on both axes, applies the specified
/// filter for both min and mag, and allocates storage at the given size.
///
/// # Errors
///
/// Returns an error string if the GL context fails to create the texture.
#[allow(unsafe_code)]
pub fn create_texture(gl: &glow::Context, config: &TextureConfig) -> Result<glow::Texture, String> {
    use glow::HasContext;

    // SAFETY: glow wraps raw GL calls as unsafe. We create, configure,
    // and allocate a texture using valid parameters derived from TextureConfig.
    let texture = unsafe { gl.create_texture()? };

    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));

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
            config.filter as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            config.filter as i32,
        );

        // Allocate storage without initial data.
        let pixel_type = pixel_type_for_format(config.internal_format);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            config.internal_format as i32,
            config.width as i32,
            config.height as i32,
            0,
            glow::RGBA,
            pixel_type,
            glow::PixelUnpackData::Slice(None),
        );

        gl.bind_texture(glow::TEXTURE_2D, None);
    }

    Ok(texture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba16f_sets_correct_dimensions() {
        let config = TextureConfig::rgba16f(1024, 768);
        assert_eq!(config.width, 1024);
        assert_eq!(config.height, 768);
    }

    #[test]
    fn rgba16f_uses_half_float_internal_format() {
        let config = TextureConfig::rgba16f(512, 512);
        assert_eq!(
            config.internal_format,
            glow::RGBA16F,
            "expected RGBA16F internal format"
        );
    }

    #[test]
    fn rgba16f_uses_linear_filter() {
        let config = TextureConfig::rgba16f(256, 256);
        assert_eq!(config.filter, glow::LINEAR, "expected LINEAR filter");
    }

    #[test]
    fn texture_config_supports_custom_values() {
        let config = TextureConfig {
            width: 64,
            height: 64,
            internal_format: glow::RGBA8,
            filter: glow::NEAREST,
        };
        assert_eq!(config.width, 64);
        assert_eq!(config.height, 64);
        assert_eq!(config.internal_format, glow::RGBA8);
        assert_eq!(config.filter, glow::NEAREST);
    }

    #[test]
    fn texture_config_is_copy_and_clone() {
        let config = TextureConfig::rgba16f(128, 128);
        let copy = config;
        let clone = config.clone();
        assert_eq!(config, copy);
        assert_eq!(config, clone);
    }

    #[test]
    fn pixel_type_for_rgba16f_is_half_float() {
        assert_eq!(pixel_type_for_format(glow::RGBA16F), glow::HALF_FLOAT);
    }

    #[test]
    fn pixel_type_for_rgba32f_is_float() {
        assert_eq!(pixel_type_for_format(glow::RGBA32F), glow::FLOAT);
    }

    #[test]
    fn pixel_type_for_rgba8_is_unsigned_byte() {
        assert_eq!(pixel_type_for_format(glow::RGBA8), glow::UNSIGNED_BYTE);
    }

    #[test]
    fn texture_config_debug_format_is_readable() {
        let config = TextureConfig::rgba16f(100, 200);
        let debug = format!("{config:?}");
        assert!(debug.contains("100"), "missing width in debug: {debug}");
        assert!(debug.contains("200"), "missing height in debug: {debug}");
    }
}
