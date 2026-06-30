//! Sun — a single luminous disc with radial rays and outer glow.
//!
//! Reads as singular focal point — "the answer", "the source", "the
//! revelation". Best paired with moments where the spoken content
//! lands on a thesis or names a single underlying cause.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Drives slow ray rotation + pulse. |
//! | `u_radius` | `float` | 0.18 | Disc core radius (in normalised half-height units). |
//! | `u_rays` | `float` | 24.0 | Number of radial rays (sinusoidal). |
//! | `u_intensity` | `float` | 1.0 | Overall brightness multiplier. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Background. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Disc/ray color. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size for aspect correction. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_radius;
uniform float u_rays;
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

    float r = length(uv);
    float theta = atan(uv.y, uv.x);

    // Soft core: gaussian-ish falloff so the disc has smooth edges.
    // RMS gently expands the apparent disc — the sun "breathes" with
    // loudness — while onset gives an instantaneous flare.
    float core_r = max(u_radius, 0.01);
    float pulse = 0.92 + 0.08 * sin(u_time * 0.6) + 0.20 * u_rms;
    float disc = exp(-(r * r) / (core_r * core_r * pulse));

    // Radial rays sweep slowly. Onset brightens them sharply.
    float ray = 0.5 + 0.5 * sin(theta * u_rays + u_time * 0.3);
    float ray_mask = smoothstep(core_r * 0.7, core_r * 3.0, r) * exp(-r * 1.4);
    ray *= ray_mask * (1.0 + u_onset * 0.7);

    // Outer halo modulated by sustained loudness.
    float halo = exp(-r * 2.2) * (0.55 + u_rms * 0.4);

    float energy = clamp(disc + ray * 0.55 + halo, 0.0, 1.0) * u_intensity;
    vec3 c = mix(u_color_lo, u_color_hi, energy);
    fragColor = vec4(c, 1.0);
}
"#;

pub const NAME: &str = "sun";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_sun_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_radius"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rays"));
    }

    #[test]
    fn fragment_source_combines_disc_rays_halo() {
        assert!(FRAGMENT_SOURCE.contains("disc"));
        assert!(FRAGMENT_SOURCE.contains("ray"));
        assert!(FRAGMENT_SOURCE.contains("halo"));
    }
}
