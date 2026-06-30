//! Noise static — animated high-frequency hash noise evoking TV static,
//! opacity, signal breakdown.
//!
//! Used by `art-engine-storyboard`'s `Backdrop::NoiseStatic` variant.
//! Drives a single-channel hash field through a colorize step so the
//! result matches the show palette instead of looking like raw white
//! noise.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Animation seed; flips noise each frame. |
//! | `u_intensity` | `float` | 1.0 | Master gain on noise field. |
//! | `u_density` | `float` | 1.0 | Noise frequency multiplier. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Dark anchor color. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Bright anchor color. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size in pixels. |

/// GLSL ES 3.0 fragment shader source.
pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_intensity;
uniform float u_density;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;
// Audio reactivity (optional; default 0.0 leaves the shader unchanged).
// Renderer also bumps u_density in Rust on onset — these are additional
// in-shader effects that pulse contrast + tear frequency.
uniform float u_rms;
uniform float u_onset;

float hash21(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

void main() {
    vec2 px = v_uv * u_resolution * u_density;

    // Per-pixel hash for true high-frequency static (no coarse cells).
    // Temporal seed quantised to 12 Hz so the field still "fizzes"
    // perceptually but adjacent rendered frames share enough pixels
    // for x264 to compress them. Without this each frame would be
    // entirely new noise and the encoder would balloon to >50 Mbps.
    float t_noise = floor(u_time * 12.0);
    float n = hash21(px + vec2(t_noise * 137.0, t_noise * 213.0));

    // Punch contrast so the pattern reads as binary-ish noise instead
    // of uniform mid-gray (which previously looked like sand).
    float bright = pow(n, 1.8);

    // Occasional horizontal "tear" lines — every few frames a band of
    // rows flips toward the hi color. Gives signal-loss feeling.
    // On onset, lower the tear threshold so more tears appear briefly
    // — the static visibly breaks up on the transient.
    float row = floor(v_uv.y * u_resolution.y * 0.25);
    float t_chunk = floor(u_time * 6.0);
    float tear_thresh = 0.97 - 0.10 * u_onset;
    float tear = step(tear_thresh, hash21(vec2(row, t_chunk))) * 0.8;
    bright = mix(bright, 1.0, tear);

    // Slow horizontal banding mimics a CRT roll.
    float roll = 0.85 + 0.15 * sin(v_uv.y * u_resolution.y * 0.04 + u_time * 1.2);

    // rms lifts overall brightness for sustained-loudness sections.
    float field = clamp(bright * roll * u_intensity * (1.0 + 0.30 * u_rms), 0.0, 1.0);
    vec3 rgb = mix(u_color_lo, u_color_hi, field);

    fragColor = vec4(rgb, 1.0);
}
"#;

/// Name used in the shader registry.
pub const NAME: &str = "noise_static";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_required_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_time"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_intensity"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_density"));
        assert!(FRAGMENT_SOURCE.contains("uniform vec3  u_color_lo"));
        assert!(FRAGMENT_SOURCE.contains("uniform vec3  u_color_hi"));
    }

    #[test]
    fn fragment_source_has_tear_lines() {
        assert!(FRAGMENT_SOURCE.contains("tear"));
    }

    #[test]
    fn fragment_source_does_per_pixel_hash_not_coarse_cells() {
        // The earlier version quantized to 2px cells and read as sand;
        // the current shader must hash off raw `px`, not `floor(px*0.5)`.
        assert!(
            !FRAGMENT_SOURCE.contains("floor(px * 0.5)"),
            "noise_static regressed to coarse-cell quantization — would look like sand"
        );
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
