//! Phyllotaxis — sunflower-seed packing via the golden angle.
//!
//! Reads as natural mathematical order: a pattern that looks designed
//! but is the inevitable consequence of one local rule (each new seed
//! placed at radius √n and angle n·φ, where φ is the golden angle
//! ≈ 137.5°). For beats about emergence, order-without-designer,
//! "looks like a system but it's just a packing constraint", or the
//! way deep structure surfaces from a one-line rule.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Slowly rotates the whole spiral. |
//! | `u_count` | `float` | 140.0 | Number of seeds rendered (clamped to ≤256). |
//! | `u_radius_scale` | `float` | 0.030 | Per-seed radial step; larger spreads the head. |
//! | `u_seed_radius` | `float` | 90.0 | Per-seed glow falloff exponent (higher = sharper dots). |
//! | `u_intensity` | `float` | 1.0 | Overall brightness multiplier. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Background. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Seed ink. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size for aspect correction. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_count;
uniform float u_radius_scale;
uniform float u_seed_radius;
uniform float u_intensity;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;
// Audio reactivity (optional; default 0.0 leaves the shader unchanged).
uniform float u_rms;
uniform float u_onset;

void main() {
    // Centered, aspect-corrected position. The seed head sits at (0,0)
    // in this coordinate space.
    float aspect = u_resolution.x / max(u_resolution.y, 1.0);
    vec2 p = vec2((v_uv.x - 0.5) * aspect, v_uv.y - 0.5);

    // Golden angle in radians: π · (3 - √5).
    const float GOLD = 2.39996323;
    float t = u_time * 0.07;

    // Maximum loop count is a compile-time bound; the runtime `u_count`
    // gates the actual work via early break.
    float ink = 0.0;
    int max_n = int(min(u_count, 256.0));
    for (int i = 0; i < 256; i++) {
        if (i >= max_n) break;
        float fi = float(i);
        float r = sqrt(fi + 0.5) * u_radius_scale;
        float a = fi * GOLD + t;
        vec2 seed = vec2(r * cos(a), r * sin(a));
        float d = length(p - seed);
        // Each seed contributes a tight gaussian-ish glow. Outer seeds
        // (larger fi) fade slightly so the rim isn't punchier than the
        // densely packed centre.
        float fade = 1.0 - 0.4 * (fi / max(u_count, 1.0));
        ink += fade * exp(-d * u_seed_radius);
    }

    // Soft vignette so the spiral sits in a held space rather than
    // touching the safe-area margins.
    vec2 e = v_uv - 0.5;
    float vig = 1.0 - smoothstep(0.50, 0.95, length(e));

    // rms lifts the seed glow; onset gives a brief overall flare as if
    // each seed pops a little brighter on the transient.
    ink = clamp(ink * vig * u_intensity * (1.0 + 0.35 * u_rms) + 0.25 * u_onset * vig, 0.0, 1.0);
    vec3 col = mix(u_color_lo, u_color_hi, ink);
    fragColor = vec4(col, 1.0);
}
"#;

pub const NAME: &str = "phyllotaxis";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_phyllotaxis_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_count"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_radius_scale"));
    }

    #[test]
    fn fragment_source_uses_golden_angle() {
        // Phyllotaxis identity: seed N placed at angle N · φ, where
        // φ = π(3 − √5) ≈ 2.39996.
        assert!(FRAGMENT_SOURCE.contains("2.39996"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
