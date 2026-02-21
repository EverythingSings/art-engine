#![deny(unsafe_code)]
//! Core types and traits for the art-engine generative art system.
//!
//! Provides the `Engine` trait, `Field` type, `Palette` (OKLab/OKLCh),
//! `Xorshift64` PRNG, `Seed`, and parameter helpers.

pub mod color;
pub mod engine;
pub mod error;
pub mod field;
pub mod field_source;
pub mod palette;
pub mod params;
pub mod prng;
pub mod seed;

#[cfg(feature = "render")]
pub mod render;

pub use color::{LinearRgb, OkLab, OkLch, Srgb};
pub use engine::Engine;
pub use error::EngineError;
pub use field::Field;
pub use palette::Palette;
pub use prng::Xorshift64;
pub use seed::Seed;
