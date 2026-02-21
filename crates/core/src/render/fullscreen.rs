//! Fullscreen triangle constants for post-processing and compositing passes.
//!
//! A single fullscreen triangle is more efficient than a fullscreen quad
//! because it avoids the diagonal seam where two triangles meet, which
//! can cause redundant fragment shader invocations along that edge.
//! The vertex shader generates positions from `gl_VertexID` with no VBO.

/// GLSL ES 3.0 vertex shader that renders a fullscreen triangle.
///
/// Generates clip-space positions and UV coordinates from `gl_VertexID`
/// alone -- no vertex buffer is needed. Draw with:
///
/// ```text
/// gl.draw_arrays(TRIANGLES, 0, 3)
/// ```
///
/// with an empty VAO bound. The three vertices cover the entire screen
/// (actually a 2x-oversized triangle, but the GPU clips it for free).
pub const FULLSCREEN_VERTEX_SHADER: &str = r#"#version 300 es
out vec2 v_uv;
void main() {
    v_uv = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
    gl_Position = vec4(v_uv * 2.0 - 1.0, 0.0, 1.0);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullscreen_vertex_shader_contains_version_directive() {
        assert!(
            FULLSCREEN_VERTEX_SHADER.contains("#version 300 es"),
            "expected GLSL ES 3.0 version directive in:\n{FULLSCREEN_VERTEX_SHADER}"
        );
    }

    #[test]
    fn fullscreen_vertex_shader_uses_gl_vertex_id() {
        assert!(
            FULLSCREEN_VERTEX_SHADER.contains("gl_VertexID"),
            "expected gl_VertexID usage in:\n{FULLSCREEN_VERTEX_SHADER}"
        );
    }

    #[test]
    fn fullscreen_vertex_shader_outputs_uv_varying() {
        assert!(
            FULLSCREEN_VERTEX_SHADER.contains("v_uv"),
            "expected v_uv output varying in:\n{FULLSCREEN_VERTEX_SHADER}"
        );
    }

    #[test]
    fn fullscreen_vertex_shader_sets_gl_position() {
        assert!(
            FULLSCREEN_VERTEX_SHADER.contains("gl_Position"),
            "expected gl_Position assignment in:\n{FULLSCREEN_VERTEX_SHADER}"
        );
    }

    #[test]
    fn fullscreen_vertex_shader_is_valid_glsl_structure() {
        // Basic structural checks: has main function, out keyword
        assert!(
            FULLSCREEN_VERTEX_SHADER.contains("void main()"),
            "expected main function in:\n{FULLSCREEN_VERTEX_SHADER}"
        );
        assert!(
            FULLSCREEN_VERTEX_SHADER.contains("out vec2 v_uv"),
            "expected 'out vec2 v_uv' declaration in:\n{FULLSCREEN_VERTEX_SHADER}"
        );
    }
}
