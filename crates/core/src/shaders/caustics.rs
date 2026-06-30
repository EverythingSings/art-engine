//! Caustics — underwater light dapples / light-through-water shimmer.
//!
//! Reads as perception filtered by a medium: what reaches the bottom is
//! bright, crisp filaments warped by the surface above — not the source
//! itself. For beats where the speaker is talking about what's seen vs
//! what causes the seeing, surface vs depth, observation through an
//! intervening layer, or the limits of one's vantage.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Drives the dappling motion. |
//! | `u_scale` | `float` | 3.0 | Spatial frequency of the caustic field. |
//! | `u_sharpness` | `float` | 16.0 | Filament crispness exponent (higher = thinner). |
//! | `u_intensity` | `float` | 1.0 | Overall brightness multiplier. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Depth / shadow. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Surface light. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size for aspect correction. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_scale;
uniform float u_sharpness;
uniform float u_intensity;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;
// Audio reactivity (optional; default 0.0 leaves the shader unchanged).
uniform float u_rms;
uniform float u_onset;

void main() {
    vec2 uv = (v_uv - 0.5) * u_scale;
    float aspect = u_resolution.x / max(u_resolution.y, 1.0);
    uv.x *= aspect;

    float t = u_time * 0.42;

    // Domain warping — the sampling position itself is perturbed by a
    // slowly drifting field. Without this the pattern collapses to a
    // regular crisscross; with it the dapples gain the irregular cell
    // network you actually see on a swimming-pool floor.
    vec2 q = uv;
    q += 0.5 * vec2(
        sin(uv.y * 1.7 + t * 1.10),
        cos(uv.x * 1.3 - t * 0.95)
    );
    q += 0.25 * vec2(
        sin(q.y * 3.1 - t * 0.7),
        cos(q.x * 2.7 + t * 0.6)
    );

    // On onset, sharpen the filaments further — the dapples crisp up
    // on the transient.
    float sharp_eff = max(u_sharpness, 1.0) * (1.0 + 0.4 * u_onset);

    // Caustic filaments live where the warped field's gradient is small,
    // i.e. where a sum-of-sines crosses zero. abs() folds those into
    // bright lines; pow() narrows them to thin filaments.
    float w = sin(q.x * 3.6 + t * 0.5)
            + sin(q.y * 3.2 - t * 0.4)
            + sin((q.x - q.y) * 2.6 + t * 0.3)
            + sin((q.x + q.y) * 2.1 - t * 0.25);
    float lit = 1.0 - abs(w) / 4.0;
    lit = pow(clamp(lit, 0.0, 1.0), sharp_eff);

    // Soft vignette so the dapples concentrate toward centre, as if
    // the medium thins toward the edges.
    vec2 e = v_uv - 0.5;
    float vig = 1.0 - smoothstep(0.45, 0.90, length(e));

    // rms lifts the whole field for sustained-loudness sections.
    float ink = clamp(lit * vig * u_intensity * (1.0 + 0.35 * u_rms), 0.0, 1.0);
    vec3 col = mix(u_color_lo, u_color_hi, ink);
    fragColor = vec4(col, 1.0);
}
"#;

pub const NAME: &str = "caustics";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_caustics_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_sharpness"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_scale"));
    }

    #[test]
    fn fragment_source_uses_domain_warping() {
        // Caustics identity: the sampling position is itself perturbed
        // by a drifting field. Without it the pattern reads as a
        // regular crisscross instead of an organic cell network.
        assert!(FRAGMENT_SOURCE.contains("q += "));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
