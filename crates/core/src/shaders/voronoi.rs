//! Voronoi cell pattern fragment shader.
//!
//! Generates a Voronoi tessellation as a standalone generative pattern.
//! Cell points are hashed from grid coordinates and optionally animated.
//! Edge detection uses the distance to the second-nearest cell point.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_scale` | `float` | 5.0 | Cell density (higher = more cells) |
//! | `u_edge_width` | `float` | 0.04 | Edge line width (0 = no edges) |
//! | `u_time` | `float` | 0.0 | Animation time (cells drift) |
//! | `u_jitter` | `float` | 1.0 | Cell point randomness (0 = grid, 1 = full) |
//! | `u_edge_color` | `vec3` | (1,1,1) | Edge line color |
//! | `u_color_a` | `vec3` | (0.1, 0.05, 0.2) | Cell gradient start |
//! | `u_color_b` | `vec3` | (0.0, 0.5, 0.8) | Cell gradient end |

/// GLSL ES 3.0 fragment shader for Voronoi cell patterns.
///
/// Each pixel finds its nearest cell point via a 3x3 neighborhood search.
/// Cell colors interpolate between two gradient endpoints based on a
/// hash of the cell point. Edges are detected by the gap between the
/// nearest and second-nearest distances.
pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_scale;
uniform float u_edge_width;
uniform float u_time;
uniform float u_jitter;
uniform vec3 u_edge_color;
uniform vec3 u_color_a;
uniform vec3 u_color_b;
// Audio reactivity (optional; default 0.0 leaves the shader unchanged).
uniform float u_rms;
uniform float u_onset;

// Deterministic hash — two input floats to two pseudo-random outputs.
// The constants are chosen to avoid visible patterns at typical scales.
vec2 hash(vec2 p) {
    p = vec2(dot(p, vec2(127.1, 311.7)),
             dot(p, vec2(269.5, 183.3)));
    return fract(sin(p) * 43758.5453);
}

void main() {
    vec2 st = v_uv * u_scale;
    vec2 i_st = floor(st);
    vec2 f_st = fract(st);

    float min_dist = 10.0;
    float second_dist = 10.0;
    vec2 min_point = vec2(0.0);

    // Search the 3x3 neighborhood around the current cell.
    for (int y = -1; y <= 1; y++) {
        for (int x = -1; x <= 1; x++) {
            vec2 neighbor = vec2(float(x), float(y));
            vec2 point = hash(i_st + neighbor);

            // Animate cell points with sinusoidal drift.
            point = 0.5 + u_jitter * 0.5 * sin(u_time * 0.4 + 6.2831853 * point);

            vec2 diff = neighbor + point - f_st;
            float dist = length(diff);

            if (dist < min_dist) {
                second_dist = min_dist;
                min_dist = dist;
                min_point = point;
            } else if (dist < second_dist) {
                second_dist = dist;
            }
        }
    }

    // Edge factor: 0 at edge, 1 inside cell.
    // On onset, sharpen the transition so cells crisp up on the hit —
    // pow(edge, p) with p>1 steepens the smoothstep response.
    float edge = smoothstep(0.0, u_edge_width, second_dist - min_dist);
    edge = pow(edge, 1.0 + 0.6 * u_onset);

    // Cell color from hash of the nearest point.
    float cell_id = fract(dot(min_point, vec2(7.13, 13.71)));
    vec3 cell_color = mix(u_color_a, u_color_b, cell_id);

    // Blend edge and cell colors. rms gives a sustained brightness lift.
    vec3 color = mix(u_edge_color, cell_color, edge);
    color = clamp(color * (1.0 + 0.35 * u_rms), 0.0, 1.0);
    fragColor = vec4(color, 1.0);
}
"#;

/// Name used in the shader registry.
pub const NAME: &str = "voronoi";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_required_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_scale"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_edge_width"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_time"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_jitter"));
        assert!(FRAGMENT_SOURCE.contains("uniform vec3 u_edge_color"));
        assert!(FRAGMENT_SOURCE.contains("uniform vec3 u_color_a"));
        assert!(FRAGMENT_SOURCE.contains("uniform vec3 u_color_b"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        // Convention: every backdrop accepts u_rms + u_onset as optional
        // inputs, defaulting to 0.0 in the schema so behaviour is unchanged
        // unless the renderer drives them.
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }

    #[test]
    fn fragment_source_has_3x3_neighborhood_search() {
        // The nested loop should search -1..1 in both axes.
        assert!(FRAGMENT_SOURCE.contains("int y = -1; y <= 1"));
        assert!(FRAGMENT_SOURCE.contains("int x = -1; x <= 1"));
    }

    #[test]
    fn fragment_source_tracks_two_nearest_distances() {
        assert!(FRAGMENT_SOURCE.contains("min_dist"));
        assert!(FRAGMENT_SOURCE.contains("second_dist"));
    }

    #[test]
    fn fragment_source_uses_smoothstep_for_edges() {
        assert!(
            FRAGMENT_SOURCE.contains("smoothstep"),
            "expected smoothstep for anti-aliased edge detection"
        );
    }

    #[test]
    fn fragment_source_outputs_frag_color() {
        assert!(FRAGMENT_SOURCE.contains("out vec4 fragColor"));
        assert!(FRAGMENT_SOURCE.contains("fragColor ="));
    }

    #[test]
    fn hash_function_is_deterministic_by_design() {
        // The hash uses sin() with large multipliers — standard GLSL hash.
        // Verify the constants are present (they're chosen to avoid patterns).
        assert!(FRAGMENT_SOURCE.contains("127.1"));
        assert!(FRAGMENT_SOURCE.contains("43758.5453"));
    }
}
