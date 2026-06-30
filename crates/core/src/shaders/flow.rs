//! Flow field — a contemplative organic backdrop driven by audio.
//!
//! Generates a smooth interference field combining radial sinusoids and
//! angular cosines, modulated by a 3-octave value-noise stack and the
//! per-frame audio features (`u_rms`, `u_onset`, `u_centroid`). The
//! field value (0..1) is mapped through a 3-stop palette (`u_pal_low`
//! → `u_pal_mid` → `u_pal_high`) blended in linear RGB.
//!
//! Ports the Python `render_flow` from `examined-machine/scripts/render_viz.py`
//! to GLSL. Visually equivalent at the same `u_time + u_seed`.
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Elapsed time in seconds. Drives phase. |
//! | `u_rms` | `float` | 0.0 | Audio RMS (0..1). Modulates intensity. |
//! | `u_onset` | `float` | 0.0 | Onset strength (0..1). Triggers flashes. |
//! | `u_centroid` | `float` | 0.5 | Spectral centroid (0..1). Tints warm/cool. |
//! | `u_intensity` | `float` | 1.0 | Master gain on the field value. |
//! | `u_seed` | `float` | 11.0 | Decorrelates noise across scenes. |
//! | `u_pal_low` | `vec3` | (0.04, 0.05, 0.10) | Inkwell indigo. |
//! | `u_pal_mid` | `vec3` | (0.10, 0.32, 0.40) | Dusty teal. |
//! | `u_pal_high` | `vec3` | (0.96, 0.74, 0.36) | Warm amber. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size in pixels. |
//!
//! Sampler input (the field-derived texture) is intentionally ignored:
//! Flow is a generative backdrop, not a modulator.

/// GLSL ES 3.0 fragment shader source for the Flow backdrop.
pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_rms;
uniform float u_onset;
uniform float u_centroid;
uniform float u_intensity;
uniform float u_seed;
uniform vec3  u_pal_low;
uniform vec3  u_pal_mid;
uniform vec3  u_pal_high;
uniform vec2  u_resolution;

float hash21(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

// Bilinear value noise.
float vnoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    float a = hash21(i);
    float b = hash21(i + vec2(1.0, 0.0));
    float c = hash21(i + vec2(0.0, 1.0));
    float d = hash21(i + vec2(1.0, 1.0));
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// 3-stop palette mix in linear RGB. Smoothstep on input, then piecewise
// blend low→mid→high to avoid the green hue artefact of HSV interpolation.
vec3 palette3(float f, vec3 lo, vec3 mid, vec3 hi) {
    f = clamp(f, 0.0, 1.0);
    f = f * f * (3.0 - 2.0 * f);
    float t1 = clamp(f * 2.0, 0.0, 1.0);
    float t2 = clamp(f * 2.0 - 1.0, 0.0, 1.0);
    vec3 lm = mix(lo, mid, t1);
    return mix(lm, hi, t2);
}

void main() {
    // Center & normalise so the vertical short reads as [-1, 1] on the
    // long axis. Aspect handled in x.
    vec2 uv = v_uv * 2.0 - 1.0;
    float aspect = u_resolution.x / max(u_resolution.y, 1.0);
    uv.x *= aspect;

    float r = length(uv);
    float theta = atan(uv.y, uv.x);
    float phase = u_time * 0.30;

    // Three decorrelated noise samples (≠ Python pre-baked grids but
    // perceptually equivalent at this scale).
    float seed = u_seed;
    float n_a = vnoise(uv *  5.0 + vec2(seed));
    float n_b = vnoise(uv *  9.0 + vec2(seed * 2.0));
    float n_c = vnoise(uv * 17.0 + vec2(seed * 3.0));

    // Smoothstep fades out the angular term near the centre to hide the
    // theta singularity (the Python version had to do the same trick).
    float theta_w = smoothstep(0.10, 0.40, r);

    float field = sin(r * 5.0 - phase * 1.3 + n_a * 6.2831853) * 0.5
                + cos(theta * 3.0 + phase * 0.7 + n_b * 6.2831853) * 0.5 * theta_w;
    field = 0.5 + 0.5 * field;
    field = field * (0.55 + 0.50 * u_rms) + n_c * 0.10 + 0.12 * u_onset;
    field *= u_intensity;
    field = clamp(field, 0.0, 1.0);

    vec3 rgb = palette3(field, u_pal_low, u_pal_mid, u_pal_high);

    // Soft vignette so the vertical short doesn't read boxy.
    float vig = clamp(1.0 - 0.45 * pow(r, 1.5), 0.25, 1.0);
    rgb *= vig;

    // Centroid tint — slight warm/cool nudge based on spectral centroid.
    vec3 tint = vec3(1.0 + 0.05 * (u_centroid - 0.5),
                     1.0,
                     1.0 - 0.05 * (u_centroid - 0.5));
    rgb *= tint;

    // Per-frame deterministic grain.
    float g = (hash21(v_uv * u_resolution + fract(u_time * 1000.0)) - 0.5) * 0.025;
    rgb += vec3(g);

    fragColor = vec4(clamp(rgb, 0.0, 1.0), 1.0);
}
"#;

/// Name used in the shader registry (case-insensitive lookups).
pub const NAME: &str = "flow";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_centroid"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_time"));
    }

    #[test]
    fn fragment_source_declares_palette_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform vec3  u_pal_low"));
        assert!(FRAGMENT_SOURCE.contains("uniform vec3  u_pal_mid"));
        assert!(FRAGMENT_SOURCE.contains("uniform vec3  u_pal_high"));
    }

    #[test]
    fn fragment_source_declares_resolution() {
        assert!(FRAGMENT_SOURCE.contains("uniform vec2  u_resolution"));
    }

    #[test]
    fn fragment_source_outputs_frag_color() {
        assert!(FRAGMENT_SOURCE.contains("out vec4 fragColor"));
        assert!(FRAGMENT_SOURCE.contains("fragColor ="));
    }

    #[test]
    fn fragment_source_uses_three_stop_palette_mix() {
        // The piecewise palette3 helper should be present.
        assert!(FRAGMENT_SOURCE.contains("vec3 palette3("));
    }

    #[test]
    fn fragment_source_handles_theta_singularity() {
        // The angular term must be faded near r=0.
        assert!(FRAGMENT_SOURCE.contains("theta_w"));
        assert!(FRAGMENT_SOURCE.contains("smoothstep(0.10, 0.40, r)"));
    }
}
