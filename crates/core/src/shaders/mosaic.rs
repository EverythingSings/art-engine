//! Mosaic — quantised tile grid with slowly-varying per-tile colour.
//!
//! Reads as finite-resolution representation — a low-bit sketch of the
//! thing, not the thing itself. For beats about approximation, models,
//! "this is just what I can see at *this* resolution," or any moment
//! where the speaker is naming that what they have is a coarse map of
//! a continuous territory. Distinct from Halftone (newsprint dots
//! sized by underlying value), Crystal (irregular polygonal facets),
//! and Lattice (continuous rectilinear grid) — Mosaic is a *regular
//! square tile grid* with *discrete tone steps per tile* and visible
//! gaps between tiles, immediately legible as a mosaic / pixel sketch.
//!
//! Audio-reactive (see [`super`] doc): u_rms multiplies overall
//! brightness; u_onset injects a brief phase nudge so the tile field
//! re-shuffles momentarily — the sketch "redraws."
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Slow drift of the per-tile field. |
//! | `u_grid` | `float` | 14.0 | Tiles across the short axis. |
//! | `u_levels` | `float` | 5.0 | Discrete tone levels per tile. |
//! | `u_gap` | `float` | 0.06 | Border gap as fraction of one tile. |
//! | `u_intensity` | `float` | 1.0 | Overall brightness. |
//! | `u_rms` | `float` | 0.0 | Audio loudness (optional). |
//! | `u_onset` | `float` | 0.0 | Audio onset (optional). |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Empty tile / gap. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Bright tile. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size; sets tile aspect. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_grid;
uniform float u_levels;
uniform float u_gap;
uniform float u_intensity;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;
// Audio reactivity (optional; default 0.0 leaves the shader unchanged).
uniform float u_rms;
uniform float u_onset;

float hash21(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

void main() {
    // Tile dimensions: u_grid tiles across the shorter axis, aspect-
    // corrected so tiles are visually square instead of stretched.
    float aspect = u_resolution.x / max(u_resolution.y, 1.0);
    vec2 grid = vec2(u_grid * aspect, u_grid);

    vec2 cell = floor(v_uv * grid);
    vec2 cell_local = fract(v_uv * grid);

    // Per-tile slow-varying scalar: hash gives a phase, time + onset
    // shifts it so the field re-shuffles on transients.
    float h = hash21(cell);
    float phase = h * 6.28318 + u_time * 0.30 + u_onset * 2.0;
    float v = 0.5 + 0.5 * sin(phase);

    // Quantise into discrete tone levels — the "mosaic" effect comes
    // from this step. Continuous v would read as a soft cell grid.
    float L = max(u_levels, 2.0);
    float lvl = floor(v * L) / (L - 1.0);

    // Edge gap: dark border around each tile so they read as discrete
    // tesserae rather than a continuous grid.
    float g = clamp(u_gap, 0.0, 0.49);
    float inside = step(g, cell_local.x)
                 * step(g, 1.0 - cell_local.x)
                 * step(g, cell_local.y)
                 * step(g, 1.0 - cell_local.y);

    // rms lifts the whole brightness.
    float ink = clamp(lvl * inside * u_intensity * (1.0 + 0.35 * u_rms), 0.0, 1.0);
    vec3 col = mix(u_color_lo, u_color_hi, ink);
    fragColor = vec4(col, 1.0);
}
"#;

pub const NAME: &str = "mosaic";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_mosaic_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_grid"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_levels"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_gap"));
    }

    #[test]
    fn fragment_source_quantises_tile_value() {
        // Mosaic identity: per-tile value snapped to discrete levels.
        assert!(FRAGMENT_SOURCE.contains("floor(v * L)"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
