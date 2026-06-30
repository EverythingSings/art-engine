//! Halftone — newsprint dot pattern.
//!
//! Each cell is a circle whose radius is driven by a smooth field; the
//! result reads as a CMYK halftone print. Industrial / spec-sheet
//! aesthetic; pairs well with talk about media, reproduction,
//! transmission, "this has been published".
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Slowly drifts the underlying field. |
//! | `u_cell` | `float` | 22.0 | Halftone cell size in pixels. |
//! | `u_strength` | `float` | 1.0 | Max dot radius as fraction of cell (0..1). |
//! | `u_intensity` | `float` | 1.0 | Overall brightness multiplier. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Paper / background. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Ink / dot fill. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size in pixels. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_cell;
uniform float u_strength;
uniform float u_intensity;
uniform float u_rms;
uniform float u_onset;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;

float hash21(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

void main() {
    vec2 px = v_uv * u_resolution;
    float cell = max(u_cell, 2.0);

    // Quantise into halftone cells.
    vec2 cell_idx = floor(px / cell);
    vec2 g = (px - cell_idx * cell) - cell * 0.5;
    float dist = length(g);

    // Underlying smooth field decides each cell's "fill" — a slow
    // sin/cos surface plus a touch of per-cell hash variance gives
    // a gentle wave rather than a uniform grid of identical dots.
    float wave = 0.5
        + 0.5 * sin(cell_idx.x * 0.22 + u_time * 0.5)
              * cos(cell_idx.y * 0.18 - u_time * 0.4);
    float jitter = (hash21(cell_idx) - 0.5) * 0.15;
    float val = clamp(wave + jitter, 0.0, 1.0);

    // Dot size grows with sustained loudness; onset adds a sharp punch.
    float audio_strength = u_strength * (1.0 + u_rms * 0.45 + u_onset * 0.4);
    float radius = val * cell * 0.5 * audio_strength;
    float dot = 1.0 - smoothstep(radius - 1.2, radius + 0.6, dist);
    dot *= u_intensity;

    vec3 c = mix(u_color_lo, u_color_hi, clamp(dot, 0.0, 1.0));
    fragColor = vec4(c, 1.0);
}
"#;

pub const NAME: &str = "halftone";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_halftone_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_cell"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_strength"));
    }

    #[test]
    fn fragment_source_uses_per_cell_dots() {
        // Halftone identity: quantise to cells, draw a dot per cell.
        assert!(FRAGMENT_SOURCE.contains("cell_idx"));
        assert!(FRAGMENT_SOURCE.contains("dist"));
    }
}
