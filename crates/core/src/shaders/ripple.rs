//! Ripple — a single disturbance propagating outward from an origin,
//! with amplitude decaying over distance.
//!
//! Reads as cause-then-consequence: a stone dropped, an event leaving
//! its signature in everything downstream. For beats about influence,
//! a single decision propagating, an originating moment whose effects
//! you can still trace. Distinct from Concentric (steady symmetric
//! rings, no origin event) — Ripple has a localised source and a
//! clear amplitude falloff, so the eye reads it as *something
//! happened here*.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Drives ring expansion + re-trigger timing. |
//! | `u_freq` | `float` | 18.0 | Ring spatial frequency. |
//! | `u_speed` | `float` | 1.2 | Outward propagation speed. |
//! | `u_decay` | `float` | 2.0 | Amplitude decay with distance (higher = shorter reach). |
//! | `u_sharpness` | `float` | 3.0 | Crest sharpness exponent. |
//! | `u_intensity` | `float` | 1.0 | Overall brightness. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Calm water. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Crest ink. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size for aspect correction. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_freq;
uniform float u_speed;
uniform float u_decay;
uniform float u_sharpness;
uniform float u_intensity;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;
// Audio reactivity (optional; default 0.0 leaves the shader unchanged).
// Ripple's symbolic register is event-driven, so onset has the most
// natural mapping: each onset adds a fresh amplitude burst on top of
// any currently-active ripples, like a new stone being dropped.
uniform float u_rms;
uniform float u_onset;

void main() {
    float aspect = u_resolution.x / max(u_resolution.y, 1.0);
    vec2 p = (v_uv - 0.5);
    p.x *= aspect;

    float t = u_time;

    // Three stones, dropped at staggered times and slightly different
    // positions. Each one triggers a fresh ripple ring every ~6s. The
    // overlay reads as event-driven rather than as a steady oscillator.
    float v = 0.0;
    for (int i = 0; i < 3; i++) {
        float fi = float(i);
        vec2 origin = vec2(
            (fract(sin(fi * 1.7) * 43.0) - 0.5) * 0.50 * aspect,
            (fract(sin(fi * 2.3) * 43.0) - 0.5) * 0.50
        );
        // Per-stone fire schedule: t_fire repeats every 6s but
        // staggered so the three sources don't ring in unison.
        float period = 6.0;
        float phase = t - fi * 2.0;
        float local_t = mod(phase, period);

        float d = length(p - origin);
        // Outward-travelling crest at radius local_t * speed. The
        // sin() inside (d * freq - local_t * speed * freq) produces a
        // travelling wave; we sharpen positive crests and decay them
        // with distance so each ring fades as it expands.
        float crest = sin(d * u_freq - local_t * u_speed * u_freq);
        crest = pow(max(crest, 0.0), max(u_sharpness, 1.0));
        // Spatial decay: amplitude shrinks with distance from origin.
        // Temporal envelope: fade in over the first 0.3s, then linearly
        // fade out over the rest of the period so each ripple has a
        // clear "event" feel rather than reading as a steady oscillator.
        float spatial = exp(-d * u_decay);
        float env = smoothstep(0.0, 0.3, local_t)
                  * (1.0 - smoothstep(period * 0.4, period, local_t));
        v += crest * spatial * env;
    }

    // rms lifts the steady ring brightness; onset visibly *adds* a fresh
    // burst across all active ripples, reading as a new stone landing.
    float ink = v * u_intensity * (1.0 + 0.35 * u_rms);
    ink += 0.45 * u_onset * v;
    ink = clamp(ink, 0.0, 1.0);
    vec3 col = mix(u_color_lo, u_color_hi, ink);
    fragColor = vec4(col, 1.0);
}
"#;

pub const NAME: &str = "ripple";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_ripple_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_decay"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_sharpness"));
    }

    #[test]
    fn fragment_source_uses_travelling_wave() {
        // Ripple identity: sin(d * freq - t * speed * freq) is the
        // travelling-wave term that makes crests radiate outward.
        assert!(FRAGMENT_SOURCE.contains("d * u_freq - local_t"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
