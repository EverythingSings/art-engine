//! Shader compilation and linking helpers for WebGL2 / OpenGL.
//!
//! Provides error types, source formatting for debugging, and functions
//! to compile individual shader stages and link them into programs.
//! The compilation/linking functions require a `glow::Context` and are
//! only usable with a live GPU context; the formatting utilities are
//! pure string processing.

use thiserror::Error;

/// Errors that can occur during shader compilation or program linking.
#[derive(Debug, Clone, Error)]
pub enum ShaderError {
    /// A shader stage failed to compile.
    #[error("shader compile error ({stage}):\n{log}")]
    CompileError {
        /// The shader stage that failed (e.g. "vertex", "fragment").
        stage: String,
        /// The driver's info log describing the error.
        log: String,
    },
    /// A program failed to link.
    #[error("shader link error:\n{0}")]
    LinkError(String),
}

/// Formats a shader compilation error for human-readable debugging.
///
/// Prepends right-aligned line numbers to each line of `source`, then
/// appends the driver's error `log`. This makes it easy to correlate
/// error messages (which reference line numbers) with the actual GLSL.
///
/// Both `source` and `log` may be empty; the function handles all
/// combinations gracefully.
pub fn format_shader_error(source: &str, log: &str) -> String {
    let source_lines: Vec<&str> = if source.is_empty() {
        Vec::new()
    } else {
        source.lines().collect()
    };

    let line_count = source_lines.len();
    let width = if line_count == 0 {
        1
    } else {
        line_count.to_string().len()
    };

    let numbered: String = source_lines
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:>width$}: {line}", i + 1, width = width))
        .collect::<Vec<_>>()
        .join("\n");

    match (numbered.is_empty(), log.is_empty()) {
        (true, true) => String::new(),
        (true, false) => log.to_string(),
        (false, true) => numbered,
        (false, false) => format!("{numbered}\n\n{log}"),
    }
}

/// Compiles a single shader stage.
///
/// Requires a live `glow::Context`. Returns the compiled shader handle
/// or a `ShaderError::CompileError` with the driver's info log.
///
/// # Errors
///
/// Returns `ShaderError::CompileError` if the GLSL source fails to compile.
#[allow(unsafe_code)]
pub fn compile_shader(
    gl: &glow::Context,
    shader_type: u32,
    source: &str,
) -> Result<glow::Shader, ShaderError> {
    use glow::HasContext;

    let stage_name = match shader_type {
        glow::VERTEX_SHADER => "vertex",
        glow::FRAGMENT_SHADER => "fragment",
        _ => "unknown",
    };

    // SAFETY: glow wraps raw GL calls as unsafe. We pass valid shader_type
    // constants and valid source strings. Resource cleanup is handled on
    // all error paths.
    let shader = unsafe {
        gl.create_shader(shader_type)
            .map_err(|e| ShaderError::CompileError {
                stage: stage_name.to_string(),
                log: e,
            })?
    };

    unsafe {
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
    }

    let compiled = unsafe { gl.get_shader_compile_status(shader) };

    if compiled {
        Ok(shader)
    } else {
        let info_log = unsafe { gl.get_shader_info_log(shader) };
        unsafe { gl.delete_shader(shader) };
        Err(ShaderError::CompileError {
            stage: stage_name.to_string(),
            log: format_shader_error(source, &info_log),
        })
    }
}

/// Links a vertex and fragment shader into a program.
///
/// Requires a live `glow::Context`. Attaches both shaders, links, and
/// detaches them afterward (the program retains its own copies).
///
/// # Errors
///
/// Returns `ShaderError::LinkError` if linking fails.
#[allow(unsafe_code)]
pub fn link_program(
    gl: &glow::Context,
    vertex: glow::Shader,
    fragment: glow::Shader,
) -> Result<glow::Program, ShaderError> {
    use glow::HasContext;

    // SAFETY: glow wraps raw GL calls as unsafe. We pass valid shader/program
    // handles obtained from prior glow calls. Resources are cleaned up on error.
    let program = unsafe { gl.create_program().map_err(ShaderError::LinkError)? };

    unsafe {
        gl.attach_shader(program, vertex);
        gl.attach_shader(program, fragment);
        gl.link_program(program);

        // Detach shaders regardless of link success -- the program owns copies.
        gl.detach_shader(program, vertex);
        gl.detach_shader(program, fragment);
    }

    let linked = unsafe { gl.get_program_link_status(program) };

    if linked {
        Ok(program)
    } else {
        let info_log = unsafe { gl.get_program_info_log(program) };
        unsafe { gl.delete_program(program) };
        Err(ShaderError::LinkError(info_log))
    }
}

/// Compiles vertex and fragment sources and links them into a program.
///
/// This is a convenience wrapper around [`compile_shader`] and [`link_program`].
/// Shader handles are cleaned up after linking regardless of success or failure.
///
/// # Errors
///
/// Returns `ShaderError::CompileError` if either shader fails to compile,
/// or `ShaderError::LinkError` if linking fails.
#[allow(unsafe_code)]
pub fn compile_program(
    gl: &glow::Context,
    vertex_src: &str,
    fragment_src: &str,
) -> Result<glow::Program, ShaderError> {
    use glow::HasContext;

    let vert = compile_shader(gl, glow::VERTEX_SHADER, vertex_src)?;
    let frag = match compile_shader(gl, glow::FRAGMENT_SHADER, fragment_src) {
        Ok(f) => f,
        Err(e) => {
            // SAFETY: vert is a valid shader handle from a successful compile_shader call.
            unsafe { gl.delete_shader(vert) };
            return Err(e);
        }
    };

    let result = link_program(gl, vert, frag);

    // SAFETY: vert and frag are valid shader handles. The linked program
    // retains its own copies, so deleting these is correct.
    unsafe {
        gl.delete_shader(vert);
        gl.delete_shader(frag);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- format_shader_error tests ---

    #[test]
    fn format_shader_error_prepends_line_numbers() {
        let source = "#version 300 es\nvoid main() {\n}\n";
        let log = "ERROR: 0:2: syntax error";
        let formatted = format_shader_error(source, log);

        assert!(
            formatted.contains("1: #version 300 es"),
            "expected line 1 with content, got:\n{formatted}"
        );
        assert!(
            formatted.contains("2: void main() {"),
            "expected line 2 with content, got:\n{formatted}"
        );
        assert!(
            formatted.contains("3: }"),
            "expected line 3 with content, got:\n{formatted}"
        );
        assert!(
            formatted.contains(log),
            "expected original log in output, got:\n{formatted}"
        );
    }

    #[test]
    fn format_shader_error_handles_empty_source() {
        let formatted = format_shader_error("", "some error");
        assert!(
            formatted.contains("some error"),
            "expected log in output, got:\n{formatted}"
        );
    }

    #[test]
    fn format_shader_error_handles_empty_log() {
        let formatted = format_shader_error("void main() {}", "");
        assert!(
            formatted.contains("1: void main() {}"),
            "expected numbered source line, got:\n{formatted}"
        );
    }

    #[test]
    fn format_shader_error_handles_both_empty() {
        let formatted = format_shader_error("", "");
        assert!(
            formatted.is_empty(),
            "expected empty output, got: {formatted}"
        );
    }

    #[test]
    fn format_shader_error_preserves_multiline_source_order() {
        let source = "line_a\nline_b\nline_c\nline_d\nline_e";
        let formatted = format_shader_error(source, "err");
        let lines: Vec<&str> = formatted.lines().collect();

        // First 5 lines should be the numbered source
        assert!(lines[0].starts_with("1: "), "got: {}", lines[0]);
        assert!(lines[1].starts_with("2: "), "got: {}", lines[1]);
        assert!(lines[2].starts_with("3: "), "got: {}", lines[2]);
        assert!(lines[3].starts_with("4: "), "got: {}", lines[3]);
        assert!(lines[4].starts_with("5: "), "got: {}", lines[4]);
    }

    #[test]
    fn format_shader_error_right_aligns_line_numbers() {
        // With 10+ lines, single-digit numbers should be right-aligned
        let source = (1..=12)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let formatted = format_shader_error(&source, "err");
        let lines: Vec<&str> = formatted.lines().collect();

        // Line 1 should be padded: " 1: line 1"
        assert!(
            lines[0].starts_with(" 1: "),
            "expected right-aligned single digit, got: '{}'",
            lines[0]
        );
        // Line 10 should not be padded: "10: line 10"
        assert!(
            lines[9].starts_with("10: "),
            "expected no padding for double digit, got: '{}'",
            lines[9]
        );
    }

    // --- ShaderError Display tests ---

    #[test]
    fn shader_compile_error_display_includes_stage_and_log() {
        let err = ShaderError::CompileError {
            stage: "fragment".into(),
            log: "undeclared identifier".into(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("fragment"), "missing stage in: {msg}");
        assert!(
            msg.contains("undeclared identifier"),
            "missing log in: {msg}"
        );
    }

    #[test]
    fn shader_link_error_display_includes_log() {
        let err = ShaderError::LinkError("varying mismatch".into());
        let msg = format!("{err}");
        assert!(msg.contains("varying mismatch"), "missing log in: {msg}");
    }

    #[test]
    fn shader_error_implements_std_error() {
        fn assert_error<T: std::error::Error>() {}
        assert_error::<ShaderError>();
    }
}
