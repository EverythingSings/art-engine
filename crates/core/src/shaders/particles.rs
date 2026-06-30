//! Particles — N small bright dots orbiting at independent frequencies.
//!
//! Reads as "many discrete agents in motion" — a swarm, an idea
//! ecosystem, distributed cognition. Best paired with moments about
//! plurality, populations, networks of small actors.
//!
//! Implementation: each fragment sums an inverse-distance glow from
//! up to 32 particles whose positions are computed parametrically
//! from a per-particle phase. Pure shader, no buffers.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Drives orbital motion. |
//! | `u_count` | `float` | 16.0 | Active particle count (1..32, clamped). |
//! | `u_glow` | `float` | 0.025 | Per-particle glow magnitude. |
//! | `u_speed` | `float` | 1.0 | Orbital speed multiplier. |
//! | `u_intensity` | `float` | 1.0 | Overall brightness multiplier. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Background. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Particle color. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size for aspect correction. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_count;
uniform float u_glow;
uniform float u_speed;
uniform float u_intensity;
uniform float u_rms;
uniform float u_onset;
uniform vec3  u_color_lo;
uniform vec3  u_color_hi;
uniform vec2  u_resolution;

void main() {
    vec2 uv = v_uv * 2.0 - 1.0;
    float aspect = u_resolution.x / max(u_resolution.y, 1.0);
    uv.x *= aspect;

    float t = u_time * u_speed;
    int n = int(clamp(u_count, 1.0, 32.0));
    float energy = 0.0;

    for (int i = 0; i < 32; i++) {
        if (i >= n) break;
        float fi = float(i);
        // Per-particle orbital parameters — phase, radius, frequency
        // jitter — derived purely from index so the result is
        // deterministic. fract(fi*0.371) gives uniform-ish spread.
        float phase = fi * 6.2831853 / float(max(n, 1));
        float orbit_r = 0.18 + 0.55 * fract(fi * 0.371);
        float freq = 0.25 + fi * 0.045;
        float angle = t * freq + phase;
        float wobble = sin(t * 0.7 + fi) * 0.04;

        vec2 p = vec2(
            cos(angle) * (orbit_r + wobble),
            sin(angle * 1.17 + phase * 0.4) * (orbit_r + wobble)
        );

        float d = length(uv - p);
        // Inverse-distance glow, clamped via the +eps so we don't
        // divide by zero at the particle centre.
        energy += u_glow / (d + 0.006);
    }

    // RMS sustains overall swarm brightness; onset adds a sharp pop.
    float audio_gain = 1.0 + u_rms * 1.2 + u_onset * 0.8;
    energy *= u_intensity * 0.05 * audio_gain;
    energy = clamp(energy, 0.0, 1.0);

    vec3 c = mix(u_color_lo, u_color_hi, energy);
    fragColor = vec4(c, 1.0);
}
"#;

pub const NAME: &str = "particles";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_particle_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_count"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_glow"));
    }

    #[test]
    fn fragment_source_loops_over_particles_with_constant_bound() {
        // GLSL ES 3.0 prefers loops with constant bounds. The shader
        // iterates to 32 and breaks early once i >= n.
        assert!(FRAGMENT_SOURCE.contains("i < 32"));
        assert!(FRAGMENT_SOURCE.contains("if (i >= n) break;"));
    }
}
