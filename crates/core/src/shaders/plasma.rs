//! Plasma — fluid blobs that merge and separate via smooth-min metaballs.
//!
//! Reads as energy state / transformation / alchemy — two or more
//! distinct flows touching and becoming one. For beats about merging
//! frames, ideas combining, the moment two separate threads of an
//! argument fuse into a single new one. Distinct from Flow (directional
//! contemplative current), Smoke (layered haze without focal points),
//! and Caustics (crisp filaments) — Plasma has *named blob centres*
//! that visibly merge and separate as they drift.
//!
//! Audio-reactive (see [`super`] doc): u_rms multiplies overall
//! brightness; u_onset surges blob radius briefly, so each transient
//! reads as the blobs swelling and re-touching.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Drives blob drift. |
//! | `u_count` | `float` | 6.0 | Number of blobs (clamped 2..=8). |
//! | `u_radius` | `float` | 0.20 | Blob influence radius (gaussian sigma). |
//! | `u_intensity` | `float` | 1.0 | Overall brightness. |
//! | `u_rms` | `float` | 0.0 | Audio loudness (optional). |
//! | `u_onset` | `float` | 0.0 | Audio onset (optional). |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Empty space. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Blob core. |
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

void main() {
    float aspect = u_resolution.x / max(u_resolution.y, 1.0);
    vec2 p = vec2((v_uv.x - 0.5) * aspect, v_uv.y - 0.5);

    float t = u_time * 0.30;

    // On onset, briefly swell the blob radius — the field surges as if
    // a pulse of heat passed through it.
    float r_eff = u_radius * (1.0 + 0.30 * u_onset);

    // Each blob drifts on its own slow sinusoid. Sum gaussian falloff
    // per blob — exp(-d²/r²) keeps each blob's influence local while
    // still allowing overlapping blobs to visibly merge.
    int n = int(clamp(u_count, 2.0, 8.0));
    float field = 0.0;
    for (int i = 0; i < 8; i++) {
        if (i >= n) break;
        float fi = float(i);
        vec2 c = vec2(
            0.42 * sin(t + fi * 1.71) * aspect,
            0.42 * cos(t * 0.78 + fi * 2.31)
        );
        // Per-blob slow size variation so the field doesn't lock into
        // a static rhythm.
        float scale = 0.85 + 0.25 * sin(t * 0.42 + fi * 1.13);
        float r = max(r_eff * scale, 1e-3);
        vec2 dv = p - c;
        float d2 = dot(dv, dv);
        field += exp(-d2 / (r * r));
    }

    // Soft threshold so the field reads as discrete blob bodies rather
    // than a continuous gradient — like real plasma cells. The
    // smoothstep range is tuned so isolated blobs are visible and
    // merged blobs read as one connected body.
    float ink = smoothstep(0.20, 1.2, field);

    // rms lifts overall brightness.
    ink = clamp(ink * u_intensity * (1.0 + 0.35 * u_rms), 0.0, 1.0);
    vec3 col = mix(u_color_lo, u_color_hi, ink);
    fragColor = vec4(col, 1.0);
}
"#;

pub const NAME: &str = "plasma";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_plasma_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_count"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_radius"));
    }

    #[test]
    fn fragment_source_uses_metaball_sum() {
        // Plasma identity: sum of gaussian falloffs over multiple drifting
        // blobs produces the smooth-min'd metaball appearance. Switched
        // from exp(-d/r) to exp(-d²/r²) so each blob's influence is
        // localised — the un-thresholded sum used to bleed across most
        // of the frame.
        assert!(FRAGMENT_SOURCE.contains("field += exp(-d2"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
