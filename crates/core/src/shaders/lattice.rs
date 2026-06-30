//! Orthogonal lattice / mechanical grid backdrop.
//!
//! Reads as "transparent classical machine" — discrete parts, regular
//! geometry. Best paired with sentences about levers, clocks, gears,
//! engines, or anything Newton would recognise.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Animates a slow drift across the grid. |
//! | `u_density` | `float` | 12.0 | Grid frequency (number of cells across). |
//! | `u_thickness` | `float` | 0.06 | Line width as a fraction of cell size. |
//! | `u_intensity` | `float` | 1.0 | Overall brightness multiplier. |
//! | `u_color_bg` | `vec3` | (0.04, 0.05, 0.10) | Cell-interior color. |
//! | `u_color_line` | `vec3` | (0.96, 0.74, 0.36) | Line color. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_density;
uniform float u_thickness;
uniform float u_intensity;
uniform float u_rms;
uniform float u_onset;
uniform vec3  u_color_bg;
uniform vec3  u_color_line;

// Smooth grid line mask: 1.0 on a gridline, 0.0 in the cell interior.
float grid_lines(vec2 uv, float thickness) {
    vec2 g = abs(fract(uv) - 0.5);
    float d = min(g.x, g.y);
    return 1.0 - smoothstep(0.5 - thickness * 0.5, 0.5 + thickness * 0.5, 1.0 - d);
}

void main() {
    // Centre the grid so animation drifts symmetrically.
    vec2 uv = (v_uv - 0.5) * u_density + u_time * 0.06;

    // Two grids at different frequencies — main + faint sub-grid — give
    // a more "drafting paper" feel than a single regular lattice.
    float main_g = grid_lines(uv, u_thickness);
    float sub_g  = grid_lines(uv * 4.0, u_thickness * 0.6) * 0.25;

    float lines = clamp(main_g + sub_g, 0.0, 1.0);
    // Subtle row pulse — slow vertical wave, helps the grid breathe.
    // Onset gives sharp line brightening — like a tick on a meter.
    float pulse = 0.85 + 0.15 * sin(v_uv.y * 6.2831853 - u_time * 0.6);
    pulse += u_rms * 0.15;
    lines *= pulse * u_intensity * (1.0 + u_onset * 0.6);

    vec3 c = mix(u_color_bg, u_color_line, lines);
    fragColor = vec4(c, 1.0);
}
"#;

pub const NAME: &str = "lattice";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_grid_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_density"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_thickness"));
        assert!(FRAGMENT_SOURCE.contains("uniform vec3  u_color_bg"));
        assert!(FRAGMENT_SOURCE.contains("uniform vec3  u_color_line"));
    }

    #[test]
    fn fragment_source_uses_smoothstep_for_lines() {
        assert!(FRAGMENT_SOURCE.contains("smoothstep"));
    }
}
