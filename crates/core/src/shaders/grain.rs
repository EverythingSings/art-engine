//! Film grain fragment shader.
//!
//! Adds high-frequency monochromatic noise to break up flat regions and
//! deliver a film-stock texture. The noise is hash-derived from `(uv, time)`
//! so it animates between frames when `u_time` advances.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_texture` | `sampler2D` | — | Composite to grain |
//! | `u_amount` | `float` | 0.02 | Noise amplitude (0 = off, 0.05 = heavy) |
//! | `u_time` | `float` | 0.0 | Frame counter or seconds, drives noise animation |

/// GLSL ES 3.0 fragment shader for film grain.
///
/// Uses a fract/dot/sin hash to produce per-pixel pseudo-random noise in
/// `[-1, 1]`, scaled by `u_amount` and added to all RGB channels equally
/// so the grain stays monochromatic and doesn't tint the image.
pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform sampler2D u_texture;
uniform float u_amount;
uniform float u_time;

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

void main() {
    vec4 base = texture(u_texture, v_uv);
    float n = hash(v_uv + vec2(u_time, -u_time)) * 2.0 - 1.0;
    fragColor = vec4(base.rgb + vec3(n * u_amount), base.a);
}
"#;

/// Name used in the shader registry.
pub const NAME: &str = "grain";

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
        assert!(FRAGMENT_SOURCE.contains("uniform float u_amount"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_time"));
    }

    #[test]
    fn fragment_source_outputs_frag_color() {
        assert!(FRAGMENT_SOURCE.contains("out vec4 fragColor"));
        assert!(FRAGMENT_SOURCE.contains("fragColor ="));
    }
}
