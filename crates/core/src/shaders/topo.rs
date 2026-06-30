//! Topo — topographic contour lines of a slow noise field.
//!
//! Reads as cartographic / geological / data-visualisation. For moments
//! when the spoken content is "looking at it from outside" — surveys,
//! maps, distance, the bird's-eye view.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Slowly drifts the underlying field. |
//! | `u_scale` | `float` | 3.0 | Spatial frequency of the underlying field. |
//! | `u_density` | `float` | 8.0 | Number of contour levels visible. |
//! | `u_thickness` | `float` | 0.04 | Line thickness as fraction of one level. |
//! | `u_intensity` | `float` | 1.0 | Overall brightness multiplier. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Background / valley. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Contour-line ink. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size in pixels. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_scale;
uniform float u_density;
uniform float u_thickness;
uniform float u_intensity;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;
// Audio reactivity (optional; default 0.0 leaves the shader unchanged).
uniform float u_rms;
uniform float u_onset;

void main() {
    // Aspect-corrected coords scaled to spatial frequency.
    vec2 uv = (v_uv - 0.5) * u_scale;
    float aspect = u_resolution.x / max(u_resolution.y, 1.0);
    uv.x *= aspect;

    float t = u_time * 0.18;

    // Smooth scalar field — two crossed sinusoids plus a slow drift —
    // gives plausible "terrain" without needing real noise textures.
    float field = sin(uv.x * 2.1 + t) * cos(uv.y * 1.7 - t * 1.3)
                + 0.6 * sin((uv.x + uv.y) * 1.3 + t * 0.7);
    field = 0.5 + 0.3 * field;  // re-centre roughly to [0..1]

    // On onset, fatten the contour lines briefly — the map "sharpens".
    float thick_eff = u_thickness * (1.0 + 0.4 * u_onset);

    // Quantise the field into level curves: distance to the nearest
    // integer of (field * density) yields a line mask.
    float lvl = field * u_density;
    float d = abs(fract(lvl) - 0.5);
    float line = 1.0 - smoothstep(thick_eff, thick_eff + 0.15, d);

    // Edge fade so the map doesn't read as a flat tile.
    vec2 e = v_uv - 0.5;
    float vig = 1.0 - smoothstep(0.45, 0.85, length(e));

    // rms lifts the whole ink — sustained loudness brightens the map.
    float ink = clamp(line * vig * u_intensity * (1.0 + 0.35 * u_rms), 0.0, 1.0);
    vec3 c = mix(u_color_lo, u_color_hi, ink);
    fragColor = vec4(c, 1.0);
}
"#;

pub const NAME: &str = "topo";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_topo_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_density"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_thickness"));
    }

    #[test]
    fn fragment_source_uses_level_curves() {
        // Topo identity: a contour appears where `fract(field*density)`
        // crosses a half-integer threshold.
        assert!(FRAGMENT_SOURCE.contains("fract(lvl)"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
