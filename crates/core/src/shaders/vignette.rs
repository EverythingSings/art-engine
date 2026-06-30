//! Vignette darkening fragment shader.
//!
//! Multiplies the image by a smooth radial falloff that darkens the corners.
//! Subtle vignettes pull the eye toward the center; heavy ones evoke film
//! and lens-vintage looks.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_texture` | `sampler2D` | — | Composite to darken |
//! | `u_strength` | `float` | 0.4 | How dark the corners get (0 = off, 1 = black) |
//! | `u_radius` | `float` | 0.75 | Distance from center where falloff begins (0..√2/2) |
//! | `u_softness` | `float` | 0.45 | Width of the falloff transition |

/// GLSL ES 3.0 fragment shader for radial vignette darkening.
///
/// Computes `length(uv - 0.5) * sqrt(2)` to get a normalised radial
/// distance in `[0, 1]`, then `smoothstep(radius, radius + softness, …)`
/// for a smooth dark→light falloff which is multiplied against the
/// sampled image.
pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform sampler2D u_texture;
uniform float u_strength;
uniform float u_radius;
uniform float u_softness;

void main() {
    vec4 base = texture(u_texture, v_uv);

    // Normalised radial distance: 0 at center, ~1 at corners.
    float dist = length(v_uv - vec2(0.5)) * 1.41421356;
    float falloff = smoothstep(u_radius, u_radius + max(u_softness, 1e-4), dist);
    float dim = 1.0 - falloff * u_strength;

    fragColor = vec4(base.rgb * dim, base.a);
}
"#;

/// Name used in the shader registry.
pub const NAME: &str = "vignette";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_required_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform sampler2D u_texture"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_strength"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_radius"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_softness"));
    }

    #[test]
    fn fragment_source_outputs_frag_color() {
        assert!(FRAGMENT_SOURCE.contains("out vec4 fragColor"));
        assert!(FRAGMENT_SOURCE.contains("fragColor ="));
    }
}
