#![deny(unsafe_code)]
//! Engine registry: maps engine names to implementations and provides CPU-side
//! snapshot rendering.
//!
//! This crate sits between `art-engine-core` (which defines the `Engine` trait)
//! and the individual engine crates (`art-engine-gray-scott`, etc.). Both the
//! CLI and WASM bindings depend on this crate to avoid duplicating dispatch logic.

pub mod pixel;

#[cfg(feature = "png")]
pub mod snapshot;

use art_engine_core::error::EngineError;
use art_engine_core::field::Field;
use art_engine_core::Engine;
use serde_json::Value;

/// All available engine names.
const ENGINE_NAMES: &[&str] = &["gray-scott"];

/// Enumeration of all available generative art engines.
///
/// Wraps each engine implementation and delegates `Engine` trait methods.
/// Use [`EngineKind::from_name`] for string-based construction (CLI, WASM).
pub enum EngineKind {
    /// Gray-Scott reaction-diffusion.
    GrayScott(art_engine_gray_scott::GrayScott),
}

impl EngineKind {
    /// Constructs an engine by name.
    ///
    /// Returns `EngineError::UnknownEngine` if the name is not recognized.
    pub fn from_name(
        name: &str,
        width: usize,
        height: usize,
        seed: u64,
        params: &Value,
    ) -> Result<Self, EngineError> {
        match name {
            "gray-scott" => Ok(EngineKind::GrayScott(
                art_engine_gray_scott::GrayScott::from_json(width, height, seed, params)?,
            )),
            _ => Err(EngineError::UnknownEngine(name.to_string())),
        }
    }

    /// Returns a slice of all recognized engine names.
    pub fn list_engines() -> &'static [&'static str] {
        ENGINE_NAMES
    }
}

impl Engine for EngineKind {
    fn step(&mut self) -> Result<(), EngineError> {
        match self {
            EngineKind::GrayScott(e) => e.step(),
        }
    }

    fn field(&self) -> &Field {
        match self {
            EngineKind::GrayScott(e) => e.field(),
        }
    }

    fn params(&self) -> Value {
        match self {
            EngineKind::GrayScott(e) => e.params(),
        }
    }

    fn param_schema(&self) -> Value {
        match self {
            EngineKind::GrayScott(e) => e.param_schema(),
        }
    }

    fn hue_field(&self) -> Option<&Field> {
        match self {
            EngineKind::GrayScott(e) => e.hue_field(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn from_name_gray_scott_succeeds() {
        let engine = EngineKind::from_name("gray-scott", 32, 32, 42, &json!({}));
        assert!(engine.is_ok());
    }

    #[test]
    fn from_name_unknown_returns_error() {
        let result = EngineKind::from_name("nonexistent", 32, 32, 42, &json!({}));
        assert!(matches!(result, Err(EngineError::UnknownEngine(_))));
    }

    #[test]
    fn list_engines_includes_gray_scott() {
        let names = EngineKind::list_engines();
        assert!(names.contains(&"gray-scott"));
    }

    #[test]
    fn trait_delegation_step_and_field() {
        let mut engine = EngineKind::from_name("gray-scott", 16, 16, 42, &json!({})).unwrap();
        assert_eq!(engine.field().width(), 16);
        assert_eq!(engine.field().height(), 16);
        engine.step().unwrap();
    }

    #[test]
    fn trait_delegation_params_and_schema() {
        let engine = EngineKind::from_name("gray-scott", 16, 16, 42, &json!({})).unwrap();
        let params = engine.params();
        assert!(params.get("feed_rate").is_some());
        let schema = engine.param_schema();
        assert!(schema.get("feed_rate").is_some());
    }

    #[test]
    fn trait_delegation_hue_field() {
        let engine = EngineKind::from_name("gray-scott", 16, 16, 42, &json!({})).unwrap();
        assert!(engine.hue_field().is_none());
    }

    #[test]
    fn determinism_same_seed() {
        let mut a = EngineKind::from_name("gray-scott", 32, 32, 99, &json!({})).unwrap();
        let mut b = EngineKind::from_name("gray-scott", 32, 32, 99, &json!({})).unwrap();
        for _ in 0..10 {
            a.step().unwrap();
            b.step().unwrap();
        }
        assert!(a
            .field()
            .data()
            .iter()
            .zip(b.field().data().iter())
            .all(|(va, vb)| va.to_bits() == vb.to_bits()));
    }

    #[test]
    fn object_safety() {
        let engine = EngineKind::from_name("gray-scott", 16, 16, 42, &json!({})).unwrap();
        let boxed: Box<dyn Engine> = Box::new(engine);
        assert_eq!(boxed.field().width(), 16);
    }
}
