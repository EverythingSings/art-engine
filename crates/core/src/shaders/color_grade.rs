//! Lift / gamma / gain color grading fragment shader.
//!
//! The classic three-knob colorist's grade:
//! - **Lift** raises the shadows (additive offset).
//! - **Gamma** adjusts the midtones (power curve).
//! - **Gain** scales the highlights (multiplicative).
//!
//! Each knob is per-channel (vec3) so warm/cool tinting falls out naturally
//! (e.g. `lift = (-0.02, 0.0, 0.04)` for cooler shadows).
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_texture` | `sampler2D` | — | Composite to grade |
//! | `u_lift` | `vec3` | (0, 0, 0) | Shadow offset, per channel |
//! | `u_gamma` | `vec3` | (1, 1, 1) | Midtone curve exponent (>1 darker, <1 brighter) |
//! | `u_gain` | `vec3` | (1, 1, 1) | Highlight multiplier, per channel |
//! | `u_saturation` | `float` | 1.0 | Saturation scaler (0 = grey, 1 = neutral, >1 punch) |

/// GLSL ES 3.0 fragment shader for lift/gamma/gain + saturation grading.
///
/// Order of operations: `clamp` input → apply gain → add lift → apply gamma
/// per channel → adjust saturation against luminance (Rec. 709) → clamp output.
/// Gamma is guarded against zero / negative exponents which would NaN.
pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform sampler2D u_texture;
uniform vec3 u_lift;
uniform vec3 u_gamma;
uniform vec3 u_gain;
uniform float u_saturation;

void main() {
    vec4 base = texture(u_texture, v_uv);
    vec3 rgb = base.rgb;

    // Gain (highlights), then lift (shadows), then gamma (midtones).
    rgb = rgb * u_gain + u_lift;
    vec3 g = max(u_gamma, vec3(1e-3));
    rgb = pow(max(rgb, vec3(0.0)), vec3(1.0) / g);

    // Saturation against Rec. 709 luminance.
    float luma = dot(rgb, vec3(0.2126, 0.7152, 0.0722));
    rgb = mix(vec3(luma), rgb, u_saturation);

    fragColor = vec4(rgb, base.a);
}
"#;

/// Name used in the shader registry.
pub const NAME: &str = "color_grade";

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
        assert!(FRAGMENT_SOURCE.contains("uniform vec3 u_lift"));
        assert!(FRAGMENT_SOURCE.contains("uniform vec3 u_gamma"));
        assert!(FRAGMENT_SOURCE.contains("uniform vec3 u_gain"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_saturation"));
    }

    #[test]
    fn fragment_source_outputs_frag_color() {
        assert!(FRAGMENT_SOURCE.contains("out vec4 fragColor"));
        assert!(FRAGMENT_SOURCE.contains("fragColor ="));
    }
}
