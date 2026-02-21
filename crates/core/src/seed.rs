//! Reproducible specification for a generative art piece.
//!
//! A [`Seed`] captures everything needed to recreate an artwork:
//! engine name, canvas dimensions, parameters, PRNG seed, and step count.

use crate::error::EngineError;
use serde::{Deserialize, Serialize};

/// Reproducible specification for a generative art piece.
///
/// Contains the engine name, canvas dimensions, parameter overrides,
/// PRNG seed, and simulation step count. Two identical `Seed` values
/// fed to the same engine binary produce bit-identical output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Seed {
    pub engine: String,
    pub width: usize,
    pub height: usize,
    pub params: serde_json::Value,
    pub seed: u64,
    pub steps: usize,
}

impl Seed {
    /// Creates a new Seed with default params (`{}`) and steps (`0`).
    pub fn new(engine: &str, width: usize, height: usize, seed: u64) -> Self {
        Self {
            engine: engine.to_string(),
            width,
            height,
            params: serde_json::Value::Object(serde_json::Map::new()),
            seed,
            steps: 0,
        }
    }

    /// Validates that the seed has non-zero dimensions and that
    /// `width * height` does not overflow.
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.width == 0 || self.height == 0 {
            return Err(EngineError::InvalidDimensions);
        }
        self.width
            .checked_mul(self.height)
            .ok_or(EngineError::InvalidDimensions)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_seed_with_default_params_and_steps() {
        let s = Seed::new("gray-scott", 512, 512, 42);
        assert_eq!(s.engine, "gray-scott");
        assert_eq!(s.width, 512);
        assert_eq!(s.height, 512);
        assert_eq!(s.seed, 42);
        assert_eq!(s.steps, 0);
        assert_eq!(s.params, serde_json::json!({}));
    }

    #[test]
    fn json_round_trip_with_defaults() {
        let original = Seed::new("physarum", 1024, 1024, 8675309);
        let json = serde_json::to_string(&original).unwrap();
        let restored: Seed = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn json_round_trip_with_custom_params() {
        let mut s = Seed::new("ising", 256, 256, 99);
        s.params = serde_json::json!({
            "temperature": 2.269,
            "coupling": 1.0,
            "lattice": "square"
        });
        s.steps = 5000;

        let json = serde_json::to_string_pretty(&s).unwrap();
        let restored: Seed = serde_json::from_str(&json).unwrap();
        assert_eq!(s, restored);
    }

    #[test]
    fn json_contains_expected_keys() {
        let s = Seed::new("dla", 128, 128, 1);
        let v: serde_json::Value = serde_json::to_value(&s).unwrap();
        assert!(v.get("engine").is_some());
        assert!(v.get("width").is_some());
        assert!(v.get("height").is_some());
        assert!(v.get("params").is_some());
        assert!(v.get("seed").is_some());
        assert!(v.get("steps").is_some());
    }

    #[test]
    fn clone_produces_equal_value() {
        let s = Seed::new("rose", 800, 600, 777);
        let cloned = s.clone();
        assert_eq!(s, cloned);
    }

    #[test]
    fn validate_succeeds_for_valid_seed() {
        let s = Seed::new("gray-scott", 512, 512, 42);
        assert!(s.validate().is_ok());
    }

    #[test]
    fn validate_fails_for_zero_width() {
        let s = Seed::new("gray-scott", 0, 512, 42);
        assert!(s.validate().is_err());
    }

    #[test]
    fn validate_fails_for_zero_height() {
        let s = Seed::new("gray-scott", 512, 0, 42);
        assert!(s.validate().is_err());
    }

    #[test]
    fn validate_fails_for_overflow() {
        let s = Seed::new("gray-scott", usize::MAX, 2, 42);
        assert!(s.validate().is_err());
    }
}
