//! Crosshatch — two intersecting families of diagonal lines.
//!
//! Reads as draftsman / blueprint / engraved-document texture. Pairs
//! well with talk about plans, blueprints, schematics, drawing
//! something out by hand before it's automated.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Slowly drifts the line phase. |
//! | `u_spacing` | `float` | 14.0 | Pixel spacing between adjacent lines. |
//! | `u_thickness` | `float` | 1.3 | Half-thickness of each line in pixels. |
//! | `u_intensity` | `float` | 1.0 | Overall brightness multiplier. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Paper / background. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Ink / line fill. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size in pixels. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_spacing;
uniform float u_thickness;
uniform float u_intensity;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;
// Audio reactivity (optional; default 0.0 leaves the shader unchanged).
uniform float u_rms;
uniform float u_onset;

float diag_line(float coord, float spacing, float thickness) {
    // Distance to the nearest line center in this diagonal family.
    float d = abs(mod(coord, spacing) - spacing * 0.5);
    return 1.0 - smoothstep(thickness, thickness + 0.7, d);
}

void main() {
    vec2 px = v_uv * u_resolution;

    // Two diagonal families: (x + y) and (x - y), slowly drifting.
    float drift = u_time * 6.0;
    float d_a = px.x + px.y + drift;
    float d_b = px.x - px.y - drift * 0.7;

    // On onset, push the pencil harder — thicker line strokes briefly.
    float thick_eff = u_thickness * (1.0 + 0.5 * u_onset);
    float a = diag_line(d_a, u_spacing, thick_eff);
    float b = diag_line(d_b, u_spacing, thick_eff);

    // Smooth max so the intersections add slightly rather than clip.
    float ink = clamp(a + b * 0.85, 0.0, 1.0);
    ink = clamp(ink * u_intensity * (1.0 + 0.35 * u_rms), 0.0, 1.0);

    vec3 c = mix(u_color_lo, u_color_hi, ink);
    fragColor = vec4(c, 1.0);
}
"#;

pub const NAME: &str = "crosshatch";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_crosshatch_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_spacing"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_thickness"));
    }

    #[test]
    fn fragment_source_uses_two_diagonal_families() {
        // Crosshatch is (x+y) and (x-y) line families.
        assert!(FRAGMENT_SOURCE.contains("px.x + px.y"));
        assert!(FRAGMENT_SOURCE.contains("px.x - px.y"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
