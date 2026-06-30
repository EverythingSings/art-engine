//! Smoke — soft drifting volumetric haze.
//!
//! Reads as obscurity / what hides between you and the thing. For beats
//! about ambiguity, the held question before a name lands, what's behind
//! the curtain, fog as a medium that *softens* legibility rather than
//! blocks it entirely. Distinct from NoiseStatic (sharp glitch) and
//! Caustics (crisp light filaments) — Smoke is slow, soft, sublethal to
//! detail.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Drives the drift. |
//! | `u_scale` | `float` | 2.2 | Spatial frequency of the haze body. |
//! | `u_warp` | `float` | 0.7 | Strength of domain warping; higher = more lobed. |
//! | `u_speed` | `float` | 1.0 | Drift speed multiplier. |
//! | `u_intensity` | `float` | 1.0 | Overall brightness. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Dark, dense fog. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Backlit edge of haze. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size for aspect correction. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_scale;
uniform float u_warp;
uniform float u_speed;
uniform float u_intensity;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;
// Audio reactivity (optional; default 0.0 leaves the shader unchanged).
uniform float u_rms;
uniform float u_onset;

void main() {
    float aspect = u_resolution.x / max(u_resolution.y, 1.0);
    vec2 p = (v_uv - 0.5) * u_scale;
    p.x *= aspect;

    float t = u_time * u_speed * 0.18;

    // On onset, briefly amplify the warp — the haze "swirls" as if
    // disturbed by the transient.
    float warp_eff = u_warp * (1.0 + 0.4 * u_onset);

    // Three nested layers of domain warping. Each layer perturbs the
    // sampling position of the next, building up the soft-lobed shape
    // of real smoke without any noise textures. The result is a slow,
    // breathing haze rather than the crisp filaments of Caustics or
    // the directional flow lines of Flow.
    vec2 q = p;
    q += warp_eff * vec2(
        sin(p.y * 1.7 + t * 1.10),
        cos(p.x * 1.3 - t * 0.95)
    );
    q += warp_eff * 0.55 * vec2(
        sin(q.y * 2.7 - t * 0.80),
        cos(q.x * 2.3 + t * 0.70)
    );

    // Multi-octave sum: low frequencies give broad lobes, higher ones
    // add wisps + tendrils. Five terms is the sweet spot — fewer reads
    // as one blob, more starts looking like static.
    float v = 0.0;
    v += sin(q.x * 1.1 + t * 0.40) * 0.55;
    v += sin(q.y * 0.9 - t * 0.30) * 0.45;
    v += sin((q.x + q.y * 0.7) * 1.3 + t * 0.20) * 0.35;
    v += sin((q.x * 1.7 - q.y * 1.1) * 0.8 - t * 0.27) * 0.30;
    v += sin((q.x * 0.5 + q.y * 1.9) * 1.1 + t * 0.33) * 0.25;
    v = 0.5 + 0.5 * (v / 1.90);

    // Wide-band response keeps the whole frame in play; clamping with a
    // gentle smoothstep avoids pure black/white extremes.
    float haze = smoothstep(0.08, 0.92, v);

    // Very soft vignette — Smoke should fill the frame, not concentrate
    // to a single bright lobe. Pushed outer radius to 1.05 so the falloff
    // barely touches the safe area.
    vec2 e = v_uv - 0.5;
    float vig = 1.0 - smoothstep(0.55, 1.05, length(e));

    // rms lifts the haze brightness for sustained-loudness sections.
    float ink = clamp(haze * vig * u_intensity * (1.0 + 0.30 * u_rms), 0.0, 1.0);
    vec3 col = mix(u_color_lo, u_color_hi, ink);
    fragColor = vec4(col, 1.0);
}
"#;

pub const NAME: &str = "smoke";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_smoke_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_warp"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_speed"));
    }

    #[test]
    fn fragment_source_uses_layered_warping() {
        // Smoke identity: nested domain warps produce the soft-lobed shape.
        assert!(FRAGMENT_SOURCE.contains("q += warp_eff"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
