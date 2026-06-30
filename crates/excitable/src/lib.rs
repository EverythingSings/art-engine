#![deny(unsafe_code)]
//! Barkley excitable-media engine.
//!
//! Simulates the Barkley model of an excitable medium: a fast activator `u`
//! (which diffuses) coupled to a slow recovery variable `v` (which does not).
//! Broken wavefronts seeded from the PRNG curl into rotating **spiral waves**
//! and concentric **target waves** — a canonical dissipative structure that
//! self-organizes far from equilibrium and never decays to a flat state.
//!
//! Governing equations, integrated with explicit Euler on a toroidal grid:
//!
//! ```text
//! du/dt = (1/epsilon) * u * (1 - u) * (u - (v + b) / a) + D * lap(u)
//! dv/dt = u - v
//! ```
//!
//! The `(1/epsilon)` reaction term is stiff; the default `dt` is chosen small
//! enough that the medium stays numerically finite over thousands of steps with
//! the 9-point Laplacian (see the stability tests in `engine.rs`).
//!
//! The primary output field is the activator `u`, clamped to [0, 1] so the
//! rendering pipeline can map it to pixels via a palette.

mod engine;

pub use engine::{Excitable, ExcitableParams};
