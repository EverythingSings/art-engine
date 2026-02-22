//! Structured CLI errors with meaningful exit codes.
//!
//! Exit code scheme:
//! - 0:  success
//! - 2:  clap arg parse error (automatic, before our code runs)
//! - 10: engine error (unknown engine, step failure, bad dimensions)
//! - 11: I/O error (file write, snapshot)
//! - 12: input error (bad palette, bad JSON params)
//! - 13: serialization error

use art_engine_core::EngineError;
use std::fmt;

/// Errors produced by CLI operations, each mapped to a distinct exit code.
pub enum CliError {
    /// An engine-level error (unknown engine, step failure, bad dimensions).
    Engine(EngineError),
    /// An I/O error (file write, snapshot rendering).
    Io(String),
    /// A user input error (bad palette name, bad JSON params).
    Input(String),
    /// A serialization error (JSON output failure).
    Serialization(String),
}

impl CliError {
    /// Returns the process exit code for this error.
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::Engine(_) => 10,
            CliError::Io(_) => 11,
            CliError::Input(_) => 12,
            CliError::Serialization(_) => 13,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Engine(e) => write!(f, "{e}"),
            CliError::Io(msg) => write!(f, "{msg}"),
            CliError::Input(msg) => write!(f, "{msg}"),
            CliError::Serialization(msg) => write!(f, "{msg}"),
        }
    }
}

impl From<EngineError> for CliError {
    fn from(e: EngineError) -> Self {
        match e {
            EngineError::Io(msg) => CliError::Io(msg),
            other => CliError::Engine(other),
        }
    }
}

impl From<serde_json::Error> for CliError {
    fn from(e: serde_json::Error) -> Self {
        CliError::Serialization(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_error_exit_code_is_10() {
        let err = CliError::Engine(EngineError::UnknownEngine("foo".into()));
        assert_eq!(err.exit_code(), 10);
    }

    #[test]
    fn io_error_exit_code_is_11() {
        let err = CliError::Io("write failed".into());
        assert_eq!(err.exit_code(), 11);
    }

    #[test]
    fn input_error_exit_code_is_12() {
        let err = CliError::Input("bad palette".into());
        assert_eq!(err.exit_code(), 12);
    }

    #[test]
    fn serialization_error_exit_code_is_13() {
        let err = CliError::Serialization("json fail".into());
        assert_eq!(err.exit_code(), 13);
    }

    #[test]
    fn from_engine_error_io_routes_to_cli_io() {
        let engine_err = EngineError::Io("disk full".into());
        let cli_err = CliError::from(engine_err);
        assert_eq!(cli_err.exit_code(), 11);
        assert!(cli_err.to_string().contains("disk full"));
    }

    #[test]
    fn from_engine_error_non_io_routes_to_cli_engine() {
        let engine_err = EngineError::UnknownEngine("xyz".into());
        let cli_err = CliError::from(engine_err);
        assert_eq!(cli_err.exit_code(), 10);
        assert!(cli_err.to_string().contains("xyz"));
    }

    #[test]
    fn from_serde_json_error_routes_to_serialization() {
        let bad_json = serde_json::from_str::<serde_json::Value>("{invalid");
        let cli_err = CliError::from(bad_json.unwrap_err());
        assert_eq!(cli_err.exit_code(), 13);
    }
}
