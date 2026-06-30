//! Mandala / radial-symmetry backdrop.
//!
//! Polar coordinates folded into N-fold symmetry. Reads as "structured
//! emergence" — a pattern with rules you can almost see, opening
//! outward. Best paired with moments about systems-as-pattern,
//! capability chasing capability, or symmetric scaling.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Drives outward radial pulse + slow rotation. |
//! | `u_segments` | `float` | 8.0 | Number of mirrored petals/segments. |
//! | `u_freq` | `float` | 12.0 | Radial pattern frequency. |
//! | `u_intensity` | `float` | 1.0 | Overall brightness multiplier. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Dark anchor. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Bright anchor. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size for aspect correction. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_segments;
uniform float u_freq;
uniform float u_intensity;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;
// Audio reactivity (optional; default 0.0 leaves the shader unchanged).
uniform float u_rms;
uniform float u_onset;

const float PI  = 3.1415926535;
const float TAU = 6.2831853072;

void main() {
    // Center, aspect-correct so the symmetry isn't squashed on portrait.
    vec2 uv = v_uv * 2.0 - 1.0;
    float aspect = u_resolution.x / max(u_resolution.y, 1.0);
    uv.x *= aspect;

    float r = length(uv);
    float theta = atan(uv.y, uv.x) + u_time * 0.08; // slow rotation

    // Fold the angle into one segment, mirror around its midpoint to
    // get clean N-fold symmetry.
    float seg = TAU / max(u_segments, 1.0);
    float a = mod(theta, seg);
    a = abs(a - seg * 0.5);

    // Radial waves + angular modulation for petal density.
    float wave = sin(r * u_freq - u_time * 1.4) * 0.5 + 0.5;
    wave *= 0.5 + 0.5 * cos(a * u_segments * 2.0);

    // Soft center bloom — mandalas read better with a luminous core.
    float bloom = exp(-r * r * 5.0);

    // Vignette + intensity master. rms lifts the whole field; onset adds
    // a brief radial flash that reads as the pattern *intensifying* on
    // the transient.
    float vig = clamp(1.0 - r * 0.7, 0.1, 1.0);
    float field = (wave * 0.85 + bloom * 0.4) * vig * u_intensity;
    field = clamp(field * (1.0 + 0.35 * u_rms) + 0.25 * u_onset, 0.0, 1.0);

    vec3 c = mix(u_color_lo, u_color_hi, field);
    fragColor = vec4(c, 1.0);
}
"#;

pub const NAME: &str = "mandala";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_segments_and_freq() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_segments"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_freq"));
    }

    #[test]
    fn fragment_source_folds_angle_into_segment() {
        // Symmetry depends on mod(theta, seg) + mirror around midpoint.
        assert!(FRAGMENT_SOURCE.contains("mod(theta, seg)"));
        assert!(FRAGMENT_SOURCE.contains("seg * 0.5"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
