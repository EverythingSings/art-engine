//! Feedback (trails/echo) fragment shader.
//!
//! Blends the current frame with a decayed copy of the previous frame,
//! creating trails, echoes, and motion persistence. The workhorse effect
//! for particle-based generative art — turns point clouds into flowing
//! rivers of light.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_texture` | `sampler2D` | — | Current layer content |
//! | `u_feedback` | `sampler2D` | — | Previous frame texture |
//! | `u_decay` | `float` | 0.92 | Persistence (0 = no trails, 1 = infinite) |
//! | `u_offset` | `vec2` | (0, 0) | UV offset for directional drift |

/// GLSL ES 3.0 fragment shader for frame feedback / trails.
///
/// Reads the current layer content and the previous frame, blending
/// them with a decay factor. Higher decay = longer trails. The offset
/// uniform displaces the feedback sample, creating directional drift.
///
/// Uses `max()` blending to preserve bright particles without additive
/// blowout, then mixes with the decayed feedback for smooth trails.
pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform sampler2D u_texture;
uniform sampler2D u_feedback;
uniform float u_decay;
uniform vec2 u_offset;

void main() {
    vec4 current = texture(u_texture, v_uv);
    vec4 previous = texture(u_feedback, v_uv + u_offset);

    // Decay the previous frame, then composite current on top.
    // max() preserves bright particles without additive blowout.
    vec4 decayed = previous * u_decay;
    fragColor = max(current, decayed);
}
"#;

/// Name used in the shader registry.
pub const NAME: &str = "feedback";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(
            FRAGMENT_SOURCE.contains("#version 300 es"),
            "expected GLSL ES 3.0 version directive"
        );
    }

    #[test]
    fn fragment_source_declares_required_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform sampler2D u_texture"));
        assert!(FRAGMENT_SOURCE.contains("uniform sampler2D u_feedback"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_decay"));
        assert!(FRAGMENT_SOURCE.contains("uniform vec2 u_offset"));
    }

    #[test]
    fn fragment_source_reads_both_textures() {
        assert!(FRAGMENT_SOURCE.contains("texture(u_texture"));
        assert!(FRAGMENT_SOURCE.contains("texture(u_feedback"));
    }

    #[test]
    fn fragment_source_uses_decay_factor() {
        assert!(
            FRAGMENT_SOURCE.contains("u_decay"),
            "expected decay uniform usage"
        );
    }

    #[test]
    fn fragment_source_outputs_frag_color() {
        assert!(FRAGMENT_SOURCE.contains("out vec4 fragColor"));
        assert!(FRAGMENT_SOURCE.contains("fragColor ="));
    }
}
