//! Concentric rings — outward-radiating waves from the centre.
//!
//! Reads as "calm resonance" — embodied, contemplative, the still
//! moment when a question hangs in the air. Use for reflective pauses
//! between dense scenes.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Drives outward ring motion. |
//! | `u_freq` | `float` | 18.0 | Ring frequency (more = tighter rings). |
//! | `u_speed` | `float` | 1.0 | Ring outflow speed. |
//! | `u_intensity` | `float` | 1.0 | Overall brightness multiplier. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Trough color. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Crest color. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size for aspect correction. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_freq;
uniform float u_speed;
uniform float u_intensity;
uniform float u_rms;
uniform float u_onset;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;

void main() {
    vec2 uv = v_uv * 2.0 - 1.0;
    float aspect = u_resolution.x / max(u_resolution.y, 1.0);
    uv.x *= aspect;

    float r = length(uv);

    // RMS speeds the rings — louder voice = faster resonance.
    float audio_speed = u_speed * (1.0 + u_rms * 0.6);
    float rings = sin(r * u_freq - u_time * audio_speed) * 0.5 + 0.5;
    rings *= rings;  // sharpen crests

    // Slow breathing pulse + onset gives a sharp crest brightening.
    float breath = 0.92 + 0.08 * sin(u_time * 0.4);

    float vig = clamp(1.0 - r * 0.65, 0.15, 1.0);
    float field = clamp(rings * breath * vig * u_intensity * (1.0 + u_onset * 0.5), 0.0, 1.0);

    vec3 c = mix(u_color_lo, u_color_hi, field);
    fragColor = vec4(c, 1.0);
}
"#;

pub const NAME: &str = "concentric";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_uses_radial_distance() {
        assert!(FRAGMENT_SOURCE.contains("length(uv)"));
    }

    #[test]
    fn fragment_source_has_breath_pulse() {
        assert!(FRAGMENT_SOURCE.contains("breath"));
    }
}
