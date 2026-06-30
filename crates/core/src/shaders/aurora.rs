//! Aurora — vertical curtains of light with bottom-bright falloff.
//!
//! Reads as atmospheric, mysterious, "something larger than us is
//! happening". Best paired with awe-leaning moments or transitions
//! between dense reasoning beats.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Drives curtain drift. |
//! | `u_curtains` | `float` | 3.0 | Number of curtain bands (used as int). |
//! | `u_speed` | `float` | 1.0 | Curtain drift speed multiplier. |
//! | `u_intensity` | `float` | 1.0 | Overall brightness multiplier. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Sky / background. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Curtain glow. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size for aspect correction. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_curtains;
uniform float u_speed;
uniform float u_intensity;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;
// Audio reactivity (optional; default 0.0 leaves the shader unchanged).
uniform float u_rms;
uniform float u_onset;

void main() {
    vec2 uv = v_uv;
    float y_norm = 1.0 - uv.y;     // 0 = bottom (bright), 1 = top (dim)
    float t = u_time * u_speed;

    // Bottom-to-top falloff — curtains are luminous near the horizon.
    float base = clamp(1.0 - y_norm * 0.7, 0.0, 1.0);

    // Up to 4 curtains at different horizontal positions, each
    // drifting on its own slow sin and shimmering vertically.
    int n = int(clamp(u_curtains, 1.0, 4.0));
    float energy = 0.0;
    for (int i = 0; i < 4; i++) {
        if (i >= n) break;
        float fi = float(i);
        // Curtain centre x drifts back and forth + wobbles with y.
        float x_centre = 0.5
            + 0.32 * sin(t * 0.35 + fi * 1.9)
            + 0.10 * sin(y_norm * 4.0 + t * 0.6 + fi);
        float band = 0.06 + 0.04 * sin(t * 0.7 + fi * 1.3);
        float dx = abs(uv.x - x_centre);
        float curtain = exp(-(dx * dx) / (band * band));
        // Vertical shimmer
        curtain *= 0.7 + 0.3 * sin(y_norm * 30.0 + t * 2.0 + fi * 3.0);
        energy = max(energy, curtain);
    }

    // rms lifts the whole curtain energy; onset gives a brief shimmer
    // as if the aurora "breathes" with the speaker.
    float intensity = energy * base * u_intensity;
    intensity = clamp(intensity * (1.0 + 0.35 * u_rms) + 0.20 * u_onset * energy, 0.0, 1.0);
    vec3 c = mix(u_color_lo, u_color_hi, intensity);
    fragColor = vec4(c, 1.0);
}
"#;

pub const NAME: &str = "aurora";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_curtain_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_curtains"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_speed"));
    }

    #[test]
    fn fragment_source_uses_bottom_bright_falloff() {
        // Aurora curtain identity: bottom luminous, top fades.
        assert!(FRAGMENT_SOURCE.contains("y_norm"));
        assert!(FRAGMENT_SOURCE.contains("1.0 - y_norm"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
