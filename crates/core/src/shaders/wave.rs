//! Wave — horizontal sinusoidal bands.
//!
//! Reads as signal, waveform, broadcast, ocean swell, breath. Best
//! paired with talk about transmission, frequency, communication,
//! or steady rhythm. Pairs nicely with low audio energy for a quiet
//! "this is a steady-state moment" feel.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Drives wave-phase drift. |
//! | `u_density` | `float` | 6.0 | Number of bands stacked vertically. |
//! | `u_freq` | `float` | 1.5 | Horizontal wave frequency (cycles across width). |
//! | `u_amplitude` | `float` | 0.4 | How much each band displaces vertically. |
//! | `u_speed` | `float` | 1.0 | Wave drift speed. |
//! | `u_intensity` | `float` | 1.0 | Overall brightness multiplier. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Background color. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Wave-crest color. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_density;
uniform float u_freq;
uniform float u_amplitude;
uniform float u_speed;
uniform float u_intensity;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
// Audio reactivity (optional; default 0.0 leaves the shader unchanged).
uniform float u_rms;
uniform float u_onset;

void main() {
    vec2 uv = v_uv;

    // On onset, push the amplitude up briefly — waves visibly heave on
    // the transient. rms also boosts amplitude as sustained loudness.
    float amp_eff = u_amplitude * (1.0 + 0.30 * u_rms + 0.50 * u_onset);

    // Per-x sinusoidal vertical displacement — each band rides this wave.
    float wave_y = sin(uv.x * u_freq * 6.2831 - u_time * u_speed) * amp_eff * 0.08;

    // Stack bands vertically.
    float bands = max(u_density, 1.0);
    float y_local = fract(uv.y * bands + wave_y * bands);
    float dy = abs(y_local - 0.5);

    // Each band reads as a soft horizontal stripe.
    float band = exp(-dy * dy / 0.05);

    // Slight phase variation per band so the stack doesn't feel rigid.
    float band_index = floor(uv.y * bands + wave_y * bands);
    float jitter = sin(band_index * 1.7 + u_time * 0.3) * 0.15;
    band *= 0.85 + jitter;

    band *= u_intensity * (1.0 + 0.20 * u_rms);
    vec3 c = mix(u_color_lo, u_color_hi, clamp(band, 0.0, 1.0));
    fragColor = vec4(c, 1.0);
}
"#;

pub const NAME: &str = "wave";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_wave_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_density"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_freq"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_amplitude"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_speed"));
    }

    #[test]
    fn fragment_source_displaces_bands_with_sin() {
        assert!(FRAGMENT_SOURCE.contains("wave_y"));
        assert!(FRAGMENT_SOURCE.contains("sin(uv.x"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
