//! Bokeh — soft out-of-focus circles of light at varied depths.
//!
//! Reads as attention / focal depth / "what's nearby and what's far."
//! For beats where the speaker is naming the *foreground* of their
//! thinking (the in-focus thing) and the *background* (what they're
//! deliberately blurring out). Distinct from Particles (sharp orbiting
//! points), Sun (single focal disc), and Constellation (graph of
//! connected nodes) — Bokeh's circles are *soft, at different sizes
//! and brightnesses*, reading immediately as depth-of-field.
//!
//! Audio-reactive (see [`super`] doc): u_rms multiplies overall
//! brightness; u_onset triggers a per-circle radius pulse (each circle
//! gets a different fraction of the onset, so the whole field
//! shimmers asymmetrically rather than uniformly).
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Drives circle drift + slow brightness flicker. |
//! | `u_count` | `float` | 9.0 | Number of bokeh circles (clamped 3..=16). |
//! | `u_radius` | `float` | 0.18 | Base radius — actual radii hash around this. |
//! | `u_intensity` | `float` | 1.0 | Overall brightness. |
//! | `u_rms` | `float` | 0.0 | Audio loudness (optional). |
//! | `u_onset` | `float` | 0.0 | Audio onset (optional). |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Background. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Circle highlight. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size for aspect correction. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_count;
uniform float u_radius;
uniform float u_intensity;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;
// Audio reactivity (optional; default 0.0 leaves the shader unchanged).
uniform float u_rms;
uniform float u_onset;

float hash11(float x) {
    return fract(sin(x * 12.9898 + 78.233) * 43758.5453);
}

void main() {
    float aspect = u_resolution.x / max(u_resolution.y, 1.0);
    vec2 p = vec2((v_uv.x - 0.5) * aspect, v_uv.y - 0.5);

    float t = u_time * 0.18;

    int n = int(clamp(u_count, 3.0, 16.0));
    float field = 0.0;

    // Each circle has a hashed position, radius, and brightness; on
    // onset, each gets a slightly different radius bump so the field
    // shimmers asymmetrically rather than pulsing in unison.
    for (int i = 0; i < 16; i++) {
        if (i >= n) break;
        float fi = float(i);
        float h_x = hash11(fi);
        float h_y = hash11(fi + 11.3);
        float h_r = hash11(fi + 23.7);
        float h_b = hash11(fi + 37.1);

        // Slow drift around the hashed position.
        vec2 c = vec2(
            (h_x - 0.5) * 0.95 * aspect + 0.05 * sin(t + fi * 1.7),
            (h_y - 0.5) * 0.95          + 0.05 * cos(t * 0.9 + fi * 2.1)
        );
        // Per-circle radius varies 0.4×–1.6× the base, so the field
        // reads as foreground/background depth.
        float radius_base = u_radius * (0.4 + 1.2 * h_r);
        float pulse = 1.0 + 0.45 * u_onset * h_r;
        float radius = radius_base * pulse;

        // Soft gaussian disc — bokeh is *only* soft falloff, no edge.
        float d = length(p - c);
        float disc = exp(-(d * d) / max(radius * radius, 1e-6));

        // Per-circle brightness varies; some hashes give dim circles
        // that read as far-away, some give bright near ones.
        float brightness = 0.4 + 0.6 * h_b;
        // Subtle per-circle brightness flicker so the field is alive.
        brightness *= 0.85 + 0.15 * sin(t * 1.3 + fi * 2.3);

        field += brightness * disc;
    }

    // rms brightens the whole field.
    field = clamp(field * u_intensity * (1.0 + 0.35 * u_rms), 0.0, 1.0);
    vec3 col = mix(u_color_lo, u_color_hi, field);
    fragColor = vec4(col, 1.0);
}
"#;

pub const NAME: &str = "bokeh";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_bokeh_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_count"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_radius"));
    }

    #[test]
    fn fragment_source_uses_per_circle_hash_varying_size() {
        // Bokeh identity: each circle has a hashed radius so the field
        // reads as varying depths. A constant-radius set of dots would
        // read as Particles, not Bokeh.
        assert!(FRAGMENT_SOURCE.contains("radius_base"));
        assert!(FRAGMENT_SOURCE.contains("h_r"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
