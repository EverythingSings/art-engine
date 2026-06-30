//! Kaleidoscope (radial symmetry) fragment shader.
//!
//! Remaps UV coordinates through angular folding to create radial
//! mirror symmetry. Applied as a per-layer effect, it transforms
//! any content — particles, noise, other shader output — into
//! mandala-like symmetric patterns.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_texture` | `sampler2D` | — | Layer content to kaleidoscope |
//! | `u_segments` | `float` | 6.0 | Number of symmetry segments |
//! | `u_rotation` | `float` | 0.0 | Base rotation (radians) |
//! | `u_center` | `vec2` | (0.5, 0.5) | Center of symmetry |
//! | `u_zoom` | `float` | 1.0 | Radial zoom factor |

/// GLSL ES 3.0 fragment shader for kaleidoscope symmetry.
///
/// Converts the UV space to polar coordinates centered at `u_center`,
/// folds the angle into one segment of the kaleidoscope, mirrors it,
/// then converts back to cartesian for the texture lookup. The result
/// is N-fold radial symmetry.
pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform sampler2D u_texture;
uniform float u_segments;
uniform float u_rotation;
uniform vec2 u_center;
uniform float u_zoom;

const float TAU = 6.2831853;

void main() {
    // Shift to center-relative coordinates.
    vec2 uv = v_uv - u_center;

    // Polar conversion.
    float angle = atan(uv.y, uv.x) + u_rotation;
    float radius = length(uv) * u_zoom;

    // Fold angle into one segment, then mirror.
    float segment_angle = TAU / u_segments;
    angle = mod(angle, segment_angle);
    if (angle > segment_angle * 0.5) {
        angle = segment_angle - angle;
    }

    // Back to cartesian, re-center.
    vec2 folded_uv = vec2(cos(angle), sin(angle)) * radius + u_center;

    fragColor = texture(u_texture, folded_uv);
}
"#;

/// Name used in the shader registry.
pub const NAME: &str = "kaleidoscope";

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
        assert!(FRAGMENT_SOURCE.contains("uniform float u_segments"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rotation"));
        assert!(FRAGMENT_SOURCE.contains("uniform vec2 u_center"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_zoom"));
    }

    #[test]
    fn fragment_source_uses_polar_coordinates() {
        assert!(
            FRAGMENT_SOURCE.contains("atan("),
            "expected atan for polar angle conversion"
        );
        assert!(
            FRAGMENT_SOURCE.contains("length("),
            "expected length for polar radius"
        );
    }

    #[test]
    fn fragment_source_folds_angle_into_segment() {
        assert!(
            FRAGMENT_SOURCE.contains("mod(angle, segment_angle)"),
            "expected modulo for segment folding"
        );
    }

    #[test]
    fn fragment_source_mirrors_within_segment() {
        // The mirror creates true kaleidoscope symmetry (not just rotation).
        assert!(
            FRAGMENT_SOURCE.contains("segment_angle - angle"),
            "expected mirror reflection within segment"
        );
    }

    #[test]
    fn fragment_source_converts_back_to_cartesian() {
        assert!(
            FRAGMENT_SOURCE.contains("cos(angle)"),
            "expected cos for cartesian conversion"
        );
        assert!(
            FRAGMENT_SOURCE.contains("sin(angle)"),
            "expected sin for cartesian conversion"
        );
    }

    #[test]
    fn fragment_source_outputs_frag_color() {
        assert!(FRAGMENT_SOURCE.contains("out vec4 fragColor"));
        assert!(FRAGMENT_SOURCE.contains("fragColor ="));
    }
}
