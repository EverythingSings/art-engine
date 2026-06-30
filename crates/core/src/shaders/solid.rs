//! Solid-color "backdrop" — paints the frame with `u_color`.
//!
//! Trivial helper used by `art-engine-storyboard`'s `Backdrop::Solid`
//! variant. Useful as a transition pad, an inert dark frame to let a
//! title card breathe, or a base for foreground overlays.

/// GLSL ES 3.0 fragment shader: outputs `vec4(u_color, 1.0)` for every
/// pixel. No input sampling.
pub const FRAGMENT_SOURCE: &str = r#"#version 300 es
precision highp float;
in vec2 v_uv;
out vec4 fragColor;
uniform vec3 u_color;
void main() {
    fragColor = vec4(u_color, 1.0);
}
"#;

/// Name used in the shader registry (case-insensitive lookups).
pub const NAME: &str = "solid";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_source_is_glsl_es_300() {
        assert!(FRAGMENT_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn fragment_source_declares_color_uniform() {
        assert!(FRAGMENT_SOURCE.contains("uniform vec3 u_color"));
    }

    #[test]
    fn fragment_source_outputs_frag_color() {
        assert!(FRAGMENT_SOURCE.contains("out vec4 fragColor"));
        assert!(FRAGMENT_SOURCE.contains("fragColor ="));
    }
}
