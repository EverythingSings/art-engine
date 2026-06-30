//! VectorField — streamlines + drifting dashes that reveal an unseen flow.
//!
//! Reads as invisible forces made legible. We draw the level curves of
//! a scalar streamfunction ψ(x,y) — by construction, those level
//! curves *are* the field lines of the velocity vector field ∇⊥ψ.
//! Dashes along the lines drift in the field direction so the eye reads
//! the flow's sign, not just its shape. For beats about gravity,
//! influence, momentum, the structure of forces shaping a visible
//! outcome, or "this is what's underneath what we see".
//!
//! ## Uniforms
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_time` | `float` | 0.0 | Drives dash motion and slow field drift. |
//! | `u_scale` | `float` | 2.5 | Spatial frequency of the streamfunction. |
//! | `u_freq` | `float` | 1.3 | Internal frequency multiplier for ψ. |
//! | `u_density` | `float` | 6.0 | Number of streamline levels visible. |
//! | `u_thickness` | `float` | 0.06 | Line thickness as fraction of one level. |
//! | `u_dash_speed` | `float` | 4.0 | How fast dashes travel along the field. |
//! | `u_intensity` | `float` | 1.0 | Overall brightness multiplier. |
//! | `u_color_lo` | `vec3` | (0.04, 0.05, 0.10) | Background. |
//! | `u_color_hi` | `vec3` | (0.96, 0.74, 0.36) | Streamline ink. |
//! | `u_resolution` | `vec2` | (1080, 1920) | Frame size for aspect correction. |

pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform float u_scale;
uniform float u_freq;
uniform float u_density;
uniform float u_thickness;
uniform float u_dash_speed;
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

    float t = u_time * 0.20;

    // Streamfunction ψ(x,y). Mixing two crossed sinusoids with a slow
    // time-varying tilt gives a non-trivial flow with closed cells AND
    // through-flowing channels — visually closer to a magnet's field
    // than to plain Topo terrain.
    float k = u_freq;
    float psi = sin(uv.x * k) * cos(uv.y * k)
              + 0.40 * sin(uv.y * 1.3 + t * 0.6)
              + 0.30 * cos(uv.x * 0.7 - t * 0.4);

    // Velocity = ∇⊥ψ = (∂ψ/∂y, -∂ψ/∂x). Computed analytically so the
    // dash phase matches the streamline tangent exactly.
    float dpsi_dy = -sin(uv.x * k) * sin(uv.y * k) * k
                   + 0.40 * cos(uv.y * 1.3 + t * 0.6) * 1.3;
    float dpsi_dx =  cos(uv.x * k) * cos(uv.y * k) * k
                   - 0.30 * sin(uv.x * 0.7 - t * 0.4) * 0.7;
    vec2 vel = vec2(dpsi_dy, -dpsi_dx);
    float vmag = length(vel) + 1e-3;
    vec2 along = vel / vmag;

    // Quantise ψ into level curves: distance to the nearest half-integer
    // of (ψ · density) yields a line mask. Same trick as Topo, but the
    // "terrain" here is a streamfunction — its contours are streamlines.
    float lvl = psi * u_density;
    float d = abs(fract(lvl) - 0.5);
    float contour = 1.0 - smoothstep(u_thickness, u_thickness + 0.18, d);

    // Dashes phase along the field direction. The dot(uv, along) gives
    // a coordinate that increases along the local streamline, so a sine
    // of it travels with the field rather than across it. On onset, the
    // dashes briefly surge faster — the invisible field momentarily
    // accelerates.
    float dash_speed_eff = u_dash_speed * (1.0 + 0.6 * u_onset);
    float along_coord = dot(uv, along);
    float dash = 0.45 + 0.55 * sin(along_coord * 8.0 - t * dash_speed_eff);

    float ink = contour * (0.35 + 0.65 * dash);

    vec2 e = v_uv - 0.5;
    float vig = 1.0 - smoothstep(0.45, 0.90, length(e));

    // rms brightens the whole field — the invisible force grows.
    ink = clamp(ink * vig * u_intensity * (1.0 + 0.35 * u_rms), 0.0, 1.0);
    vec3 col = mix(u_color_lo, u_color_hi, ink);
    fragColor = vec4(col, 1.0);
}
"#;

pub const NAME: &str = "vector_field";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_vector_field_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_density"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_dash_speed"));
    }

    #[test]
    fn fragment_source_uses_streamfunction_contours() {
        // VectorField identity: level curves of a streamfunction ψ ARE
        // the field lines of ∇⊥ψ — a contour test on ψ draws streamlines.
        assert!(FRAGMENT_SOURCE.contains("fract(lvl)"));
        assert!(FRAGMENT_SOURCE.contains("psi"));
    }

    #[test]
    fn fragment_source_declares_audio_uniforms() {
        assert!(FRAGMENT_SOURCE.contains("uniform float u_rms"));
        assert!(FRAGMENT_SOURCE.contains("uniform float u_onset"));
    }
}
