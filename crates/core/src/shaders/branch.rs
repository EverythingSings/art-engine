//! Branch — fractal tree silhouette via line-segment SDFs.
//!
//! Trunk + N main branches in an upper fan + 2 sub-branches per main.
//! Reads as dendritic / growth / branching-decision. Best paired with
//! talk about trees of possibility, neural dendrites, family trees,
//! L-systems, organic complexity.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Drives slow sway in the wind. |
//! | `u_branches` | `float` | 4.0 | Number of main branches (2..6, clamped). |
//! | `u_thickness` | `float` | 0.012 | Branch line thickness. |
//! | `u_intensity` | `float` | 1.0 | Overall brightness multiplier. |
//! | `u_rms` | `float` | 0.0 | Audio RMS — thickens branches with loudness. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Background / sky. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Branch / wood color. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size for aspect correction. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_branches;
uniform float u_thickness;
uniform float u_intensity;
uniform float u_rms;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;

const float PI = 3.14159265;

// Distance from point p to the segment [a, b].
float seg_dist(vec2 p, vec2 a, vec2 b) {
    vec2 pa = p - a;
    vec2 ba = b - a;
    float t = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-6), 0.0, 1.0);
    return length(pa - ba * t);
}

void main() {
    // Centre, aspect-correct so the tree reads upright on a vertical frame.
    vec2 uv = v_uv * 2.0 - 1.0;
    float aspect = u_resolution.x / max(u_resolution.y, 1.0);
    uv.x *= aspect;

    // Trunk — bottom centre to just above middle, with a tiny lean from sway.
    float sway = sin(u_time * 0.4) * 0.04;
    vec2 trunk_base = vec2(0.0, -1.0);
    vec2 trunk_top  = vec2(sway, 0.05);
    float d = seg_dist(uv, trunk_base, trunk_top);

    // Main branches fan out in the upper hemisphere from the trunk top.
    int n_main = int(clamp(u_branches, 2.0, 6.0));
    for (int i = 0; i < 6; i++) {
        if (i >= n_main) break;
        float fi = float(i);
        float frac = (fi + 0.5) / float(n_main);
        // Spread from upper-left (~0.85π) to upper-right (~0.15π).
        float angle = mix(PI * 0.85, PI * 0.15, frac);
        float main_len = 0.75;
        vec2 main_end = trunk_top + vec2(cos(angle), sin(angle)) * main_len;
        // Per-branch wind sway (small horizontal jitter).
        main_end.x += sin(u_time * 0.6 + fi) * 0.045;
        d = min(d, seg_dist(uv, trunk_top, main_end));

        // Two sub-branches splitting from the upper portion of the
        // main branch.
        vec2 sub_start = mix(trunk_top, main_end, 0.65);
        for (int j = 0; j < 2; j++) {
            float fj = float(j);
            float sub_angle = angle + (fj - 0.5) * 0.8;
            float sub_len = 0.32;
            vec2 sub_end = sub_start + vec2(cos(sub_angle), sin(sub_angle)) * sub_len;
            sub_end.x += sin(u_time * 0.8 + fi * 1.7 + fj * 2.3) * 0.03;
            d = min(d, seg_dist(uv, sub_start, sub_end));
        }
    }

    // Thickness grows with sustained loudness — the tree breathes.
    float t = u_thickness * (1.0 + u_rms * 0.5);
    float branch = 1.0 - smoothstep(t * 0.7, t * 1.3, d);

    // Soft halo around the branches gives the silhouette some weight.
    float glow = exp(-d * 18.0) * 0.42;

    // Quiet vignette so the bottom-anchored tree feels grounded.
    float vig = clamp(1.0 - length(v_uv - 0.5) * 0.75, 0.2, 1.0);

    float energy = clamp((branch + glow) * vig * u_intensity, 0.0, 1.0);
    vec3 c = mix(u_color_lo, u_color_hi, energy);
    fragColor = vec4(c, 1.0);
}
"#;

pub const NAME: &str = "branch";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_branch_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_branches"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_thickness"));
    }

    #[test]
    fn fragment_source_uses_segment_distance() {
        assert!(FRAGMENT_SOURCE.contains("seg_dist"));
    }

    #[test]
    fn fragment_source_has_constant_bound_loops() {
        // GLSL ES 3.0 prefers constant loop bounds; we iterate up to 6
        // main branches and break early.
        assert!(FRAGMENT_SOURCE.contains("i < 6"));
        assert!(FRAGMENT_SOURCE.contains("if (i >= n_main) break;"));
    }
}
