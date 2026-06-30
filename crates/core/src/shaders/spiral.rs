//! Spiral — logarithmic spiral winding from center outward.
//!
//! Reads as recursion, depth, "going down the rabbit hole", a question
//! that keeps unfolding. Best paired with moments that recurse onto
//! themselves: feedback loops, self-reference, infinite regress.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Spirals the pattern over time. |
//! | `u_arms` | `float` | 3.0 | Number of spiral arms (1 = single, 2 = double helix, …). |
//! | `u_tightness` | `float` | 1.0 | Logarithmic winding rate. Higher = tighter coil. |
//! | `u_speed` | `float` | 1.0 | Rotation speed. |
//! | `u_intensity` | `float` | 1.0 | Overall brightness multiplier. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Background. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Spiral arm color. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size for aspect correction. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_arms;
uniform float u_tightness;
uniform float u_speed;
uniform float u_intensity;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;
// Audio reactivity (optional; default 0.0 leaves the shader unchanged).
uniform float u_rms;
uniform float u_onset;

const float TAU = 6.2831853072;

void main() {
    // Center, aspect-correct.
    vec2 uv = v_uv * 2.0 - 1.0;
    float aspect = u_resolution.x / max(u_resolution.y, 1.0);
    uv.x *= aspect;

    float r = length(uv);
    float theta = atan(uv.y, uv.x);

    // Logarithmic spiral coordinate: arms * theta + tightness * log(r)
    // rotated by time. sin() then gives bright/dark stripes along
    // the spiral curve.
    float coord = u_arms * theta + u_tightness * log(r + 0.08) * 6.0 - u_time * u_speed;
    float spiral = sin(coord) * 0.5 + 0.5;
    spiral *= spiral; // sharpen

    // Soft fade-in at the center and fade-out at the edges so the
    // spiral doesn't read as a flat texture.
    float fade_in  = smoothstep(0.02, 0.08, r);
    float fade_out = 1.0 - smoothstep(0.85, 1.3, r);
    spiral *= fade_in * fade_out;

    // rms lifts the whole spiral; onset gives a brief flash as if the
    // recursion just snapped one layer tighter.
    spiral *= u_intensity;
    spiral = clamp(spiral * (1.0 + 0.35 * u_rms) + 0.25 * u_onset, 0.0, 1.0);
    vec3 c = mix(u_color_lo, u_color_hi, spiral);
    fragColor = vec4(c, 1.0);
}
"#;

pub const NAME: &str = "spiral";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_spiral_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_arms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_tightness"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_speed"));
    }

    #[test]
    fn fragment_source_uses_log_spiral_coord() {
        // Logarithmic spiral: arms*theta + tightness*log(r).
        assert!(FRAGMENT_SOURCE.contains("log(r"));
        assert!(FRAGMENT_SOURCE.contains("u_arms * theta"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
