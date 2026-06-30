//! Moire — interference between two near-identical line patterns.
//!
//! Reads as two systems colliding — beat frequencies, friction at a
//! seam, the visual signature of "almost-same-but-not." For beats
//! about institutional friction, two truths interfering, the gap
//! between a model and the thing it models, or any moment where the
//! speaker is naming a *near-miss* between frames. Distinct from
//! Lattice (one regular grid) and Wave (one sinusoid family) — the
//! whole point of Moire is the *product* of two slightly-offset
//! patterns, which reveals their disagreement as a slow beat.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Slowly rotates the angle delta. |
//! | `u_freq` | `float` | 36.0 | Stripe density of each pattern. |
//! | `u_angle_delta` | `float` | 0.06 | Rotation between the two patterns (radians). |
//! | `u_thickness` | `float` | 0.45 | Stripe thickness (0–1). |
//! | `u_intensity` | `float` | 1.0 | Overall brightness. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Background. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Stripe ink (where the product is bright). |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size for aspect correction. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_freq;
uniform float u_angle_delta;
uniform float u_thickness;
uniform float u_intensity;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;
// Audio reactivity (optional; default 0.0 leaves the shader unchanged).
uniform float u_rms;
uniform float u_onset;

float grating(vec2 p, float ang, float k) {
    // Continuous-amplitude grating (NOT a stripe mask). Returns the raw
    // sin() of the rotated coordinate — needed for the additive beat
    // formula below to work.
    float c = cos(ang); float s = sin(ang);
    float u = p.x * c - p.y * s;
    return sin(u * k);
}

void main() {
    float aspect = u_resolution.x / max(u_resolution.y, 1.0);
    vec2 p = (v_uv - 0.5);
    p.x *= aspect;

    float t = u_time;

    // Two near-identical line gratings. Base angle drifts slowly so the
    // beat fringes also drift — at u_time = 0 the pattern is static but
    // legible. On onset, push the angle delta up briefly so the fringes
    // visibly shift — the interference pattern "lurches" on the hit.
    float ang_base = 0.18 + 0.10 * sin(t * 0.10);
    float delta_eff = u_angle_delta + 0.05 * u_onset;
    float ang_a = ang_base;
    float ang_b = ang_base + delta_eff + 0.020 * sin(t * 0.15);

    float a = grating(p, ang_a, u_freq);
    float b = grating(p, ang_b, u_freq);

    // Classic moire-fringe formula. The *sum* of two near-identical
    // sinusoids carries an envelope at the difference frequency, which
    // *is* the moire fringe. Multiplication only gives intersection dots
    // — additive sum gives the smooth alternating-bright-and-dark bands
    // that read unmistakably as moire.
    //
    //   sin(α) + sin(β) = 2 · cos((α-β)/2) · sin((α+β)/2)
    //
    // cos((α-β)/2) is the slow envelope — visible as the alternating
    // fringe — and sin((α+β)/2) is the fast carrier.
    //
    // We render the sum directly (no threshold) so the smooth envelope
    // shape is what colours the frame. u_thickness here tightens the
    // gamma — higher values push the bright bands narrower.
    float raw = 0.5 + 0.25 * (a + b);                  // in [0, 1]
    float fringe = pow(clamp(raw, 0.0, 1.0), 1.0 + 2.0 * u_thickness);

    // Slight bias toward the centre.
    vec2 e = v_uv - 0.5;
    float vig = 1.0 - smoothstep(0.45, 0.92, length(e));

    // rms lifts the whole fringe pattern for sustained-loudness moments.
    float ink = clamp(fringe * vig * u_intensity * (1.0 + 0.35 * u_rms), 0.0, 1.0);
    vec3 col = mix(u_color_lo, u_color_hi, ink);
    fragColor = vec4(col, 1.0);
}
"#;

pub const NAME: &str = "moire";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_moire_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_angle_delta"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_thickness"));
    }

    #[test]
    fn fragment_source_adds_two_gratings() {
        // Moire identity: sin(α) + sin(β) carries an envelope at the
        // difference frequency — the fringe.
        assert!(FRAGMENT_SOURCE.contains("(a + b)"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
