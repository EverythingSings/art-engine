#![deny(unsafe_code)]
//! Core types and traits for the art-engine generative art system.
//!
//! Provides the `Engine` trait, `Field` type, `Canvas`/`Layer`/`BlendMode`/`ContentType`
//! data model, color types (`Srgb`, `OkLab`, `OkLch`), `Palette` (OKLab/OKLCh),
//! `ParticleSystem` (CPU-side particle simulation), `FieldSource` (composable 2D forces),
//! `Xorshift64` PRNG, `Seed`, and parameter helpers.

pub mod canvas;
pub mod color;
pub mod engine;
pub mod error;
pub mod field;
pub mod field_source;
pub mod palette;
pub mod params;
pub mod particle;
pub mod prng;
pub mod seed;

#[cfg(feature = "render")]
pub mod render;

pub use canvas::{BlendMode, Canvas, ContentType, Layer};
pub use color::{LinearRgb, OkLab, OkLch, Srgb};
pub use engine::Engine;
pub use error::EngineError;
pub use field::Field;
pub use palette::Palette;
pub use particle::{
    EmissionConfig, EmissionPattern, Particle, ParticleSystem, ParticleSystemConfig,
};
pub use prng::Xorshift64;
pub use seed::Seed;
