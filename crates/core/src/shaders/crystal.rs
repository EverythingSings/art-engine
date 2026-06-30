//! Crystal — hard-faceted polygonal cells with quantised values.
//!
//! Reads as clarity crystallising — the moment an idea snaps from
//! "soft cloud of related thoughts" into "discrete polished facets I
//! can name." For beats where the speaker is *resolving* something
//! into legibility: definition, distinction, the snap-to-grid of a
//! good frame. Distinct from Voronoi (soft gradients, organic cells)
//! and Lattice (regular rectilinear grid) — Crystal has sharp polygonal
//! edges *and* quantised per-cell tone, like a polished gemstone.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Slowly rotates each cell's value. |
//! | `u_scale` | `float` | 7.0 | Cell density. |
//! | `u_levels` | `float` | 5.0 | Number of discrete tone levels (3–8 reads cleanest). |
//! | `u_edge_width` | `float` | 0.03 | Crack width between facets (smaller = sharper). |
//! | `u_intensity` | `float` | 1.0 | Overall brightness. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Darkest facet / edge crack. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Brightest facet. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size for aspect correction. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_scale;
uniform float u_levels;
uniform float u_edge_width;
uniform float u_intensity;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;
// Audio reactivity (optional; default 0.0 leaves the shader unchanged).
uniform float u_rms;
uniform float u_onset;

vec2 hash2(vec2 p) {
    p = vec2(dot(p, vec2(127.1, 311.7)), dot(p, vec2(269.5, 183.3)));
    return fract(sin(p) * 43758.5453);
}

float hash1(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

void main() {
    float aspect = u_resolution.x / max(u_resolution.y, 1.0);
    vec2 uv = v_uv;
    uv.x *= aspect;
    vec2 p = uv * u_scale;

    vec2 cell = floor(p);
    vec2 frac = fract(p);

    // Find the two nearest seed distances in the 3x3 neighbourhood.
    // Distance to first → which cell we're in. Distance to first minus
    // second → edge proximity (used to ink the cracks).
    float d1 = 9.0;
    float d2 = 9.0;
    vec2  nearest = cell;
    for (int j = -1; j <= 1; j++) {
        for (int i = -1; i <= 1; i++) {
            vec2 nb = vec2(float(i), float(j));
            vec2 seed = hash2(cell + nb);
            vec2 diff = nb + seed - frac;
            float d = dot(diff, diff);
            if (d < d1) {
                d2 = d1;
                d1 = d;
                nearest = cell + nb;
            } else if (d < d2) {
                d2 = d;
            }
        }
    }
    d1 = sqrt(d1);
    d2 = sqrt(d2);

    // Per-cell tone, quantised to u_levels discrete steps and slowly
    // animated so the gemstone breathes. floor(v * L) / (L-1) snaps a
    // continuous [0,1] value into L bands.
    float h = hash1(nearest);
    float raw = 0.5 + 0.5 * sin(h * 6.28318 + u_time * 0.25);
    float L = max(u_levels, 2.0);
    float facet = floor(raw * L) / (L - 1.0);

    // Hard edge mask: smoothstep only across u_edge_width, so cracks
    // are crisp rather than soft. On onset, briefly widen the edge band
    // so the cracks momentarily flare — the gemstone "shifts."
    float edge_w_eff = u_edge_width * (1.0 + 0.5 * u_onset);
    float edge = d2 - d1;
    float facet_mask = smoothstep(0.0, edge_w_eff, edge);

    // rms lifts the whole facet brightness.
    float ink = clamp(facet * facet_mask * u_intensity * (1.0 + 0.35 * u_rms), 0.0, 1.0);
    vec3 col = mix(u_color_lo, u_color_hi, ink);
    fragColor = vec4(col, 1.0);
}
"#;

pub const NAME: &str = "crystal";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_crystal_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_levels"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_edge_width"));
    }

    #[test]
    fn fragment_source_quantises_per_cell_value() {
        // Crystal identity: discrete tone levels per facet via floor().
        assert!(FRAGMENT_SOURCE.contains("floor(raw * L)"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
