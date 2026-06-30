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

#[cfg(feature = "gpu")]
pub mod gpu_snapshot;

use art_engine_core::error::EngineError;
use art_engine_core::field::Field;
use art_engine_core::Engine;
use serde_json::Value;

/// All available engine names.
const ENGINE_NAMES: &[&str] = &[
    "gray-scott",
    "physarum",
    "mandelbrot",
    "particles",
    "dla",
    "attractor",
    "ising",
    "differential",
    "quantum",
    "excitable",
];

/// Enumeration of all available generative art engines.
///
/// Wraps each engine implementation and delegates `Engine` trait methods.
/// Use [`EngineKind::from_name`] for string-based construction (CLI, WASM).
pub enum EngineKind {
    /// Gray-Scott reaction-diffusion.
    GrayScott(art_engine_gray_scott::GrayScott),
    /// Physarum polycephalum slime mold.
    Physarum(art_engine_physarum::Physarum),
    /// Mandelbrot escape-time fractal.
    Mandelbrot(art_engine_mandelbrot::Mandelbrot),
    /// CPU particle simulation with composable FieldSource forces.
    Particles(art_engine_particles::Particles),
    /// Diffusion-limited aggregation (random-walking sticking particles).
    Dla(art_engine_dla::Dla),
    /// Strange attractors (Lorenz / Rössler / Halvorsen / Pickover).
    Attractor(art_engine_attractor::Attractor),
    /// 2D Ising model (statistical mechanics).
    Ising(art_engine_ising::Ising),
    /// Differential growth (self-organizing polyline).
    Differential(art_engine_differential::Differential),
    /// 2D quantum walk (Hadamard / Grover / DFT coins).
    Quantum(art_engine_quantum::Quantum),
    /// Barkley excitable media (rotating spiral and target waves).
    Excitable(art_engine_excitable::Excitable),
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
            "physarum" => Ok(EngineKind::Physarum(
                art_engine_physarum::Physarum::from_json(width, height, seed, params)?,
            )),
            "mandelbrot" => Ok(EngineKind::Mandelbrot(
                art_engine_mandelbrot::Mandelbrot::from_json(width, height, seed, params)?,
            )),
            "particles" => Ok(EngineKind::Particles(
                art_engine_particles::Particles::from_json(width, height, seed, params)?,
            )),
            "dla" => Ok(EngineKind::Dla(art_engine_dla::Dla::from_json(
                width, height, seed, params,
            )?)),
            "attractor" => Ok(EngineKind::Attractor(
                art_engine_attractor::Attractor::from_json(width, height, seed, params)?,
            )),
            "ising" => Ok(EngineKind::Ising(art_engine_ising::Ising::from_json(
                width, height, seed, params,
            )?)),
            "differential" => Ok(EngineKind::Differential(
                art_engine_differential::Differential::from_json(width, height, seed, params)?,
            )),
            "quantum" => Ok(EngineKind::Quantum(art_engine_quantum::Quantum::from_json(
                width, height, seed, params,
            )?)),
            "excitable" => Ok(EngineKind::Excitable(
                art_engine_excitable::Excitable::from_json(width, height, seed, params)?,
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
            EngineKind::Physarum(e) => e.step(),
            EngineKind::Mandelbrot(e) => e.step(),
            EngineKind::Particles(e) => e.step(),
            EngineKind::Dla(e) => e.step(),
            EngineKind::Attractor(e) => e.step(),
            EngineKind::Ising(e) => e.step(),
            EngineKind::Differential(e) => e.step(),
            EngineKind::Quantum(e) => e.step(),
            EngineKind::Excitable(e) => e.step(),
        }
    }

    fn field(&self) -> &Field {
        match self {
            EngineKind::GrayScott(e) => e.field(),
            EngineKind::Physarum(e) => e.field(),
            EngineKind::Mandelbrot(e) => e.field(),
            EngineKind::Particles(e) => e.field(),
            EngineKind::Dla(e) => e.field(),
            EngineKind::Attractor(e) => e.field(),
            EngineKind::Ising(e) => e.field(),
            EngineKind::Differential(e) => e.field(),
            EngineKind::Quantum(e) => e.field(),
            EngineKind::Excitable(e) => e.field(),
        }
    }

    fn params(&self) -> Value {
        match self {
            EngineKind::GrayScott(e) => e.params(),
            EngineKind::Physarum(e) => e.params(),
            EngineKind::Mandelbrot(e) => e.params(),
            EngineKind::Particles(e) => e.params(),
            EngineKind::Dla(e) => e.params(),
            EngineKind::Attractor(e) => e.params(),
            EngineKind::Ising(e) => e.params(),
            EngineKind::Differential(e) => e.params(),
            EngineKind::Quantum(e) => e.params(),
            EngineKind::Excitable(e) => e.params(),
        }
    }

    fn param_schema(&self) -> Value {
        match self {
            EngineKind::GrayScott(e) => e.param_schema(),
            EngineKind::Physarum(e) => e.param_schema(),
            EngineKind::Mandelbrot(e) => e.param_schema(),
            EngineKind::Particles(e) => e.param_schema(),
            EngineKind::Dla(e) => e.param_schema(),
            EngineKind::Attractor(e) => e.param_schema(),
            EngineKind::Ising(e) => e.param_schema(),
            EngineKind::Differential(e) => e.param_schema(),
            EngineKind::Quantum(e) => e.param_schema(),
            EngineKind::Excitable(e) => e.param_schema(),
        }
    }

    fn hue_field(&self) -> Option<&Field> {
        match self {
            EngineKind::GrayScott(e) => e.hue_field(),
            EngineKind::Physarum(e) => e.hue_field(),
            EngineKind::Mandelbrot(e) => e.hue_field(),
            EngineKind::Particles(e) => e.hue_field(),
            EngineKind::Dla(e) => e.hue_field(),
            EngineKind::Attractor(e) => e.hue_field(),
            EngineKind::Ising(e) => e.hue_field(),
            EngineKind::Differential(e) => e.hue_field(),
            EngineKind::Quantum(e) => e.hue_field(),
            EngineKind::Excitable(e) => e.hue_field(),
        }
    }

    fn set_influence(&mut self, field: &Field) -> Result<(), EngineError> {
        match self {
            EngineKind::GrayScott(e) => e.set_influence(field),
            EngineKind::Physarum(e) => e.set_influence(field),
            EngineKind::Mandelbrot(e) => e.set_influence(field),
            EngineKind::Particles(e) => e.set_influence(field),
            EngineKind::Dla(e) => e.set_influence(field),
            EngineKind::Attractor(e) => e.set_influence(field),
            EngineKind::Ising(e) => e.set_influence(field),
            EngineKind::Differential(e) => e.set_influence(field),
            EngineKind::Quantum(e) => e.set_influence(field),
            EngineKind::Excitable(e) => e.set_influence(field),
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
    fn from_name_with_zero_dimensions_returns_error() {
        let result = EngineKind::from_name("gray-scott", 0, 32, 42, &json!({}));
        assert!(result.is_err(), "width=0 should fail");
        let result = EngineKind::from_name("gray-scott", 32, 0, 42, &json!({}));
        assert!(result.is_err(), "height=0 should fail");
    }

    #[test]
    fn list_engines_round_trip_all_names_succeed() {
        for name in EngineKind::list_engines() {
            let result = EngineKind::from_name(name, 16, 16, 42, &json!({}));
            assert!(result.is_ok(), "from_name failed for listed engine: {name}");
        }
    }

    #[test]
    fn object_safety() {
        let engine = EngineKind::from_name("gray-scott", 16, 16, 42, &json!({})).unwrap();
        let boxed: Box<dyn Engine> = Box::new(engine);
        assert_eq!(boxed.field().width(), 16);
    }
}
