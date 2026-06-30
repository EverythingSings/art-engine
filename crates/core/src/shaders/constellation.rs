//! Constellation — bright nodes connected by faint edges.
//!
//! Reads as relations between specifics: a small set of points wired
//! into a graph. For beats about connection, mapping, dependency,
//! "this leads to that," the structure-of-explanation, or the moment
//! the speaker is naming a set of things and how they relate.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Drives node drift + edge twinkle. |
//! | `u_node_glow` | `float` | 240.0 | Node falloff exponent (higher = sharper dots). |
//! | `u_edge_glow` | `float` | 620.0 | Edge falloff exponent (higher = thinner lines). |
//! | `u_edge_strength` | `float` | 0.55 | Edge brightness relative to nodes. |
//! | `u_intensity` | `float` | 1.0 | Overall brightness multiplier. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Background. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Node + edge ink. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size for aspect correction. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_node_glow;
uniform float u_edge_glow;
uniform float u_edge_strength;
uniform float u_intensity;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;
// Audio reactivity (optional; default 0.0 leaves the shader unchanged).
uniform float u_rms;
uniform float u_onset;

const int N_NODES = 14;

// Deterministic node position with mild time-driven drift. We hash by
// index into the unit square, biased away from edges, then jitter on a
// slow per-node sin so the constellation breathes.
vec2 node_pos(int i, float t) {
    float fi = float(i);
    vec2 base = vec2(
        0.5 + 0.40 * sin(fi * 1.73 + 0.13),
        0.5 + 0.40 * cos(fi * 2.31 + 0.07)
    );
    base += 0.025 * vec2(sin(t * 0.4 + fi), cos(t * 0.55 + fi * 1.3));
    return base;
}

// Distance from point p to line segment ab.
float seg_dist(vec2 p, vec2 a, vec2 b) {
    vec2 ab = b - a;
    float h = clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-6), 0.0, 1.0);
    return length(p - a - h * ab);
}

void main() {
    float aspect = u_resolution.x / max(u_resolution.y, 1.0);
    vec2 p = vec2((v_uv.x - 0.5) * aspect, v_uv.y - 0.5);
    float t = u_time * 0.3;

    // Nodes: per-pixel sum of exp(-d * glow) gives a clean dot per node.
    // Twinkle the brightness with a per-node phase so the field has life.
    float nodes = 0.0;
    for (int i = 0; i < N_NODES; i++) {
        vec2 ni = node_pos(i, t);
        vec2 q = vec2((ni.x - 0.5) * aspect, ni.y - 0.5);
        float d = length(p - q);
        float twinkle = 0.65 + 0.35 * sin(t * 1.2 + float(i) * 1.7);
        nodes += twinkle * exp(-d * u_node_glow);
    }

    // Edges: connect i to (i+1)%N and to (i+5)%N. The 5-skip gives a
    // graph that isn't just a ring — it has chords that make the
    // structure read as a network rather than a polygon.
    float edges = 0.0;
    for (int i = 0; i < N_NODES; i++) {
        vec2 a = node_pos(i, t);
        vec2 b = node_pos((i + 1) - (N_NODES * ((i + 1) / N_NODES)), t);
        vec2 c = node_pos((i + 5) - (N_NODES * ((i + 5) / N_NODES)), t);
        vec2 pa = vec2((a.x - 0.5) * aspect, a.y - 0.5);
        vec2 pb = vec2((b.x - 0.5) * aspect, b.y - 0.5);
        vec2 pc = vec2((c.x - 0.5) * aspect, c.y - 0.5);
        edges += exp(-seg_dist(p, pa, pb) * u_edge_glow);
        edges += exp(-seg_dist(p, pa, pc) * u_edge_glow);
    }

    // On onset, edges illuminate harder — the network "fires" along
    // its connections on the transient. rms lifts overall brightness.
    float edge_eff = u_edge_strength * (1.0 + 0.6 * u_onset);
    float ink = clamp((nodes + edges * edge_eff) * u_intensity * (1.0 + 0.35 * u_rms), 0.0, 1.0);
    vec3 col = mix(u_color_lo, u_color_hi, ink);
    fragColor = vec4(col, 1.0);
}
"#;

pub const NAME: &str = "constellation";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_constellation_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_node_glow"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_edge_glow"));
    }

    #[test]
    fn fragment_source_uses_segment_distance() {
        // Constellation identity: edges drawn via point-to-segment distance.
        assert!(FRAGMENT_SOURCE.contains("seg_dist"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
