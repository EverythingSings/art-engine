//! The core `Engine` trait that every generative art engine must implement.
//!
//! The trait is object-safe so engines can be used as `dyn Engine` for runtime
//! switching between different generative algorithms.

use crate::error::EngineError;
use crate::field::Field;
use serde_json::Value;

/// Core trait for generative art engines.
///
/// Each engine implements a step-based simulation that produces a scalar
/// [`Field`] (and optionally a hue field) which the rendering pipeline
/// maps to pixels via a [`Palette`](crate).
///
/// This trait is **object-safe**: you can use `Box<dyn Engine>` or `&dyn Engine`
/// for runtime polymorphism.
pub trait Engine {
    /// Advance the simulation by one step.
    ///
    /// Returns `Ok(())` on success, or an `EngineError` if the step fails
    /// (e.g. due to a field dimension mismatch or invalid state).
    fn step(&mut self) -> Result<(), EngineError>;

    /// The primary scalar field output of the engine.
    fn field(&self) -> &Field;

    /// Current parameter values as a JSON object.
    fn params(&self) -> Value;

    /// Schema describing all available parameters, their types, ranges, and defaults.
    fn param_schema(&self) -> Value;

    /// Optional secondary field encoding per-cell hue offset.
    ///
    /// Returns `None` by default. Engines that modulate color spatially
    /// override this to return a hue field in [0, 1] mapped to a full hue rotation.
    fn hue_field(&self) -> Option<&Field> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Minimal engine implementation used to verify trait object safety.
    struct MockEngine {
        field: Field,
        step_count: usize,
    }

    impl MockEngine {
        fn new() -> Self {
            Self {
                field: Field::new(4, 4).unwrap(),
                step_count: 0,
            }
        }
    }

    impl Engine for MockEngine {
        fn step(&mut self) -> Result<(), EngineError> {
            self.step_count += 1;
            Ok(())
        }

        fn field(&self) -> &Field {
            &self.field
        }

        fn params(&self) -> Value {
            json!({"step_count": self.step_count})
        }

        fn param_schema(&self) -> Value {
            json!({
                "step_count": {
                    "type": "integer",
                    "default": 0,
                    "description": "Number of steps executed"
                }
            })
        }
    }

    #[test]
    fn engine_trait_is_object_safe() {
        // This test verifies that Engine can be used as a trait object.
        // If the trait were not object-safe, this would fail to compile.
        let engine: Box<dyn Engine> = Box::new(MockEngine::new());
        assert_eq!(engine.field().width(), 4);
        assert_eq!(engine.field().height(), 4);
    }

    #[test]
    fn mock_engine_step_advances_state() {
        let mut engine = MockEngine::new();
        assert_eq!(engine.step_count, 0);
        engine.step().unwrap();
        engine.step().unwrap();
        assert_eq!(engine.step_count, 2);
    }

    #[test]
    fn mock_engine_params_reflects_state() {
        let mut engine = MockEngine::new();
        engine.step().unwrap();
        let params = engine.params();
        assert_eq!(params["step_count"], 1);
    }

    #[test]
    fn mock_engine_param_schema_has_expected_structure() {
        let engine = MockEngine::new();
        let schema = engine.param_schema();
        assert!(schema.get("step_count").is_some());
        assert_eq!(schema["step_count"]["type"], "integer");
    }

    #[test]
    fn default_hue_field_is_none() {
        let engine = MockEngine::new();
        assert!(engine.hue_field().is_none());
    }

    #[test]
    fn dyn_engine_reference_works() {
        let engine = MockEngine::new();
        let engine_ref: &dyn Engine = &engine;
        assert_eq!(engine_ref.field().width(), 4);
        assert!(engine_ref.hue_field().is_none());
    }

    #[test]
    fn dyn_engine_mut_reference_works() {
        let mut engine = MockEngine::new();
        let engine_ref: &mut dyn Engine = &mut engine;
        engine_ref.step().unwrap();
        assert_eq!(engine_ref.params()["step_count"], 1);
    }
}
