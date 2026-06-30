//! Strands — vertical glowing filaments.
//!
//! Reads as circuits, threads, fibers, "connections between things".
//! Best paired with talk about networks, neural connections, lines of
//! influence, or anything quietly running in the background.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Drives each strand's pulse phase. |
//! | `u_density` | `float` | 48.0 | Number of strands across the frame. |
//! | `u_thickness` | `float` | 0.18 | Strand glow width as a fraction of the strand cell. |
//! | `u_jitter` | `float` | 0.6 | Random horizontal offset per strand (0 = perfectly aligned, 1 = full chaos). |
//! | `u_intensity` | `float` | 1.0 | Overall brightness multiplier. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Background between strands. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Strand glow color. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_density;
uniform float u_thickness;
uniform float u_jitter;
uniform float u_intensity;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
// Audio reactivity (optional; default 0.0 leaves the shader unchanged).
uniform float u_rms;
uniform float u_onset;

float hash11(float x) {
    return fract(sin(x * 127.1 + 311.7) * 43758.5453);
}

void main() {
    vec2 uv = v_uv;

    // Quantise horizontal axis into strand cells. Each cell has its
    // own jittered centre + pulse phase + amplitude.
    float xs = uv.x * u_density;
    float cell = floor(xs);
    float local = fract(xs);
    float h = hash11(cell);

    // Jitter each strand's center position based on per-cell hash.
    float jit = (h - 0.5) * u_jitter;
    float dist = abs(local - 0.5 - jit);

    // Strand glow: gaussian-like falloff from the strand center.
    // On onset, fatten the thickness term so strands momentarily flare.
    float thick_eff = u_thickness * (1.0 + 0.5 * u_onset);
    float t2 = max(thick_eff * thick_eff * 0.04, 1e-5);
    float glow = exp(-dist * dist / t2);

    // Vertical modulation — each strand has its own slow flicker.
    float pulse = 0.5 + 0.5 * sin(uv.y * 18.0 + h * 6.2831 + u_time * (0.4 + h * 0.8));

    // Soft top/bottom fade so the strands don't read as rigid bars.
    float vfade = 1.0 - 0.35 * abs(uv.y - 0.5) * 2.0;

    // rms lifts the whole glow envelope.
    float intensity = clamp(glow * pulse * vfade * u_intensity * (1.0 + 0.35 * u_rms), 0.0, 1.0);
    vec3 c = mix(u_color_lo, u_color_hi, intensity);
    fragColor = vec4(c, 1.0);
}
"#;

pub const NAME: &str = "strands";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_strand_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_density"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_thickness"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_jitter"));
    }

    #[test]
    fn fragment_source_uses_per_strand_hash() {
        assert!(FRAGMENT_SOURCE.contains("hash11(cell)"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
