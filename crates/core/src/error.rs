//! Error types for the art-engine core.

use thiserror::Error;

/// Errors produced by engine operations.
#[derive(Debug, Error)]
pub enum EngineError {
    /// Width or height was zero when creating a Field or canvas.
    #[error("invalid dimensions: width and height must be non-zero")]
    InvalidDimensions,

    /// A requested parameter name was not found in the params object.
    #[error("parameter not found: {0}")]
    ParamNotFound(String),

    /// A parameter existed but had the wrong JSON type.
    #[error("parameter type mismatch for '{name}': expected {expected}, got {got}")]
    ParamTypeMismatch {
        name: String,
        expected: String,
        got: String,
    },

    /// An (x, y) coordinate was outside the field bounds (used for non-toroidal access).
    #[error("index ({x}, {y}) out of bounds for field of size ({width}, {height})")]
    OutOfBounds {
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    },

    /// Two fields had incompatible dimensions for an element-wise operation.
    #[error("dimension mismatch: ({lhs_w}, {lhs_h}) vs ({rhs_w}, {rhs_h})")]
    DimensionMismatch {
        lhs_w: usize,
        lhs_h: usize,
        rhs_w: usize,
        rhs_h: usize,
    },

    /// A color string could not be parsed.
    #[error("invalid color: {0}")]
    InvalidColor(String),

    /// A palette could not be constructed from the given colors.
    #[error("invalid palette: {0}")]
    InvalidPalette(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_dimensions_displays_readable_message() {
        let err = EngineError::InvalidDimensions;
        let msg = format!("{err}");
        assert!(
            msg.contains("width") && msg.contains("height"),
            "expected message mentioning width and height, got: {msg}"
        );
    }

    #[test]
    fn param_not_found_includes_name() {
        let err = EngineError::ParamNotFound("speed".into());
        let msg = format!("{err}");
        assert!(
            msg.contains("speed"),
            "expected message containing 'speed', got: {msg}"
        );
    }

    #[test]
    fn param_type_mismatch_includes_all_fields() {
        let err = EngineError::ParamTypeMismatch {
            name: "radius".into(),
            expected: "f64".into(),
            got: "string".into(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("radius"), "missing param name in: {msg}");
        assert!(msg.contains("f64"), "missing expected type in: {msg}");
        assert!(msg.contains("string"), "missing got type in: {msg}");
    }

    #[test]
    fn out_of_bounds_includes_coordinates_and_dimensions() {
        let err = EngineError::OutOfBounds {
            x: 10,
            y: 20,
            width: 8,
            height: 8,
        };
        let msg = format!("{err}");
        assert!(msg.contains("10"), "missing x in: {msg}");
        assert!(msg.contains("20"), "missing y in: {msg}");
        assert!(msg.contains("8"), "missing dimension in: {msg}");
    }

    #[test]
    fn dimension_mismatch_includes_all_dimensions() {
        let err = EngineError::DimensionMismatch {
            lhs_w: 10,
            lhs_h: 20,
            rhs_w: 30,
            rhs_h: 40,
        };
        let msg = format!("{err}");
        assert!(msg.contains("10"), "missing lhs_w in: {msg}");
        assert!(msg.contains("20"), "missing lhs_h in: {msg}");
        assert!(msg.contains("30"), "missing rhs_w in: {msg}");
        assert!(msg.contains("40"), "missing rhs_h in: {msg}");
    }

    #[test]
    fn invalid_color_includes_message() {
        let err = EngineError::InvalidColor("bad hex".into());
        let msg = format!("{err}");
        assert!(msg.contains("bad hex"), "missing message in: {msg}");
    }

    #[test]
    fn invalid_palette_includes_message() {
        let err = EngineError::InvalidPalette("empty".into());
        let msg = format!("{err}");
        assert!(msg.contains("empty"), "missing message in: {msg}");
    }

    #[test]
    fn engine_error_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<EngineError>();
    }

    #[test]
    fn engine_error_implements_std_error() {
        fn assert_std_error<T: std::error::Error>() {}
        assert_std_error::<EngineError>();
    }
}
