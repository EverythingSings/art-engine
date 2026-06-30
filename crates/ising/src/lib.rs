#![deny(unsafe_code)]
//! 2D Ising model engine.
//!
//! A toroidal grid of ±1 spins evolves under Metropolis-Hastings dynamics:
//! at each attempt, a random site is chosen and its proposed spin flip is
//! accepted with probability `min(1, exp(-ΔE / T))`, where ΔE is the energy
//! change of flipping. Energy is `E = -J Σ_<ij> s_i s_j - h Σ_i s_i`, with
//! coupling `J` (default 1.0) favoring aligned neighbors and external field
//! `h` (default 0.0) biasing one spin direction.
//!
//! Visually, the system has three distinct regimes set by `temperature`:
//! - **Below T_c ≈ 2.27**: large aligned domains form, reading as smooth
//!   amber/black blocks.
//! - **Near T_c**: fractal-edged, scale-invariant clusters at every size —
//!   the eye-catching critical regime.
//! - **Above T_c**: high-entropy noise, reading as snow.
//!
//! Field output is `(spin + 1) / 2`, mapping ±1 → 0/1 so palettes see a
//! straightforward [0, 1] field.

use art_engine_core::error::EngineError;
use art_engine_core::field::Field;
use art_engine_core::params::{param_f64, param_usize};
use art_engine_core::prng::Xorshift64;
use art_engine_core::Engine;
use serde_json::{json, Value};

/// Default temperature (just below the 2D-square-lattice critical point ≈ 2.27).
const DEFAULT_TEMPERATURE: f64 = 2.269;
/// Default coupling J.
const DEFAULT_COUPLING: f64 = 1.0;
/// Default external field h.
const DEFAULT_EXTERNAL_FIELD: f64 = 0.0;
/// Default Metropolis sweeps (one sweep = N site attempts) per `step()`.
const DEFAULT_SWEEPS_PER_STEP: usize = 1;
/// Default per-step gain on the optional influence field (added to local h).
const DEFAULT_INFLUENCE_STRENGTH: f64 = 0.0;
/// Hard cap on sweeps_per_step so untrusted JSON can't DoS the CPU.
const MAX_SWEEPS_PER_STEP: usize = 200;
/// Lowest accepted temperature; values below this would cause exp underflow.
const MIN_TEMPERATURE: f64 = 1e-3;

/// Tunable parameters.
#[derive(Debug, Clone, Copy)]
pub struct IsingParams {
    pub temperature: f64,
    pub coupling: f64,
    pub external_field: f64,
    pub sweeps_per_step: usize,
    /// Per-step gain on an external influence field. The influence value at
    /// each cell is added to that cell's local field h, biasing flips.
    /// Has no effect unless `set_influence` has been called.
    pub influence_strength: f64,
}

impl Default for IsingParams {
    fn default() -> Self {
        Self {
            temperature: DEFAULT_TEMPERATURE,
            coupling: DEFAULT_COUPLING,
            external_field: DEFAULT_EXTERNAL_FIELD,
            sweeps_per_step: DEFAULT_SWEEPS_PER_STEP,
            influence_strength: DEFAULT_INFLUENCE_STRENGTH,
        }
    }
}

impl IsingParams {
    pub fn from_json(params: &Value) -> Self {
        Self {
            temperature: param_f64(params, "temperature", DEFAULT_TEMPERATURE).max(MIN_TEMPERATURE),
            coupling: param_f64(params, "coupling", DEFAULT_COUPLING),
            external_field: param_f64(params, "external_field", DEFAULT_EXTERNAL_FIELD),
            sweeps_per_step: param_usize(params, "sweeps_per_step", DEFAULT_SWEEPS_PER_STEP)
                .clamp(1, MAX_SWEEPS_PER_STEP),
            influence_strength: param_f64(params, "influence_strength", DEFAULT_INFLUENCE_STRENGTH),
        }
    }
}

/// Ising engine.
pub struct Ising {
    params: IsingParams,
    width: usize,
    height: usize,
    /// Spins encoded as i8 (`+1` or `-1`). i8 saves 7/8 of the memory vs f64
    /// and is hot-cached; the field output is materialized lazily into `field`.
    spins: Vec<i8>,
    rng: Xorshift64,
    field: Field,
    influence: Option<Vec<f64>>,
}

impl Ising {
    pub fn new(
        width: usize,
        height: usize,
        seed: u64,
        params: IsingParams,
    ) -> Result<Self, EngineError> {
        let len = width
            .checked_mul(height)
            .ok_or(EngineError::InvalidDimensions)?;
        if width == 0 || height == 0 {
            return Err(EngineError::InvalidDimensions);
        }
        let mut rng = Xorshift64::new(seed);
        // Random initial state: each spin ±1 with equal probability.
        let spins: Vec<i8> = (0..len)
            .map(|_| if rng.next_f64() < 0.5 { 1 } else { -1 })
            .collect();
        let mut field = Field::new(width, height)?;
        rebuild_field(&mut field, &spins);
        Ok(Self {
            params,
            width,
            height,
            spins,
            rng,
            field,
            influence: None,
        })
    }

    pub fn from_json(
        width: usize,
        height: usize,
        seed: u64,
        params: &Value,
    ) -> Result<Self, EngineError> {
        Self::new(width, height, seed, IsingParams::from_json(params))
    }

    /// Net magnetization in `[-1, 1]`: average of all spins.
    pub fn magnetization(&self) -> f64 {
        if self.spins.is_empty() {
            return 0.0;
        }
        let sum: i64 = self.spins.iter().map(|&s| s as i64).sum();
        sum as f64 / self.spins.len() as f64
    }
}

impl Engine for Ising {
    fn step(&mut self) -> Result<(), EngineError> {
        let n = self.width * self.height;
        let total_attempts = n.saturating_mul(self.params.sweeps_per_step);
        let temperature = self.params.temperature.max(MIN_TEMPERATURE);
        let inv_t = 1.0 / temperature;
        let j = self.params.coupling;
        let h = self.params.external_field;
        let s = self.params.influence_strength;
        let influence_active = s != 0.0 && self.influence.is_some();

        for _ in 0..total_attempts {
            let idx = self.rng.next_usize(n);
            let x = idx % self.width;
            let y = idx / self.width;

            let neighbor_sum = neighbor_spin_sum(&self.spins, self.width, self.height, x, y);
            let s_i = self.spins[idx] as f64;

            // Local field at this site = global h + (optional) influence
            let h_local = if influence_active {
                // influence is Some when influence_active; unwrap is safe
                h + s * self.influence.as_ref().unwrap()[idx]
            } else {
                h
            };

            // Energy change for flipping: ΔE = 2*s_i*(J*Σneighbors + h)
            let de = 2.0 * s_i * (j * neighbor_sum as f64 + h_local);

            // Metropolis acceptance: always accept ΔE ≤ 0; otherwise flip
            // with probability exp(-ΔE / T).
            let accept = if de <= 0.0 {
                true
            } else {
                self.rng.next_f64() < (-de * inv_t).exp()
            };

            if accept {
                self.spins[idx] = -self.spins[idx];
            }
        }

        rebuild_field(&mut self.field, &self.spins);
        Ok(())
    }

    fn field(&self) -> &Field {
        &self.field
    }

    fn params(&self) -> Value {
        json!({
            "temperature": self.params.temperature,
            "coupling": self.params.coupling,
            "external_field": self.params.external_field,
            "sweeps_per_step": self.params.sweeps_per_step,
            "influence_strength": self.params.influence_strength,
        })
    }

    fn param_schema(&self) -> Value {
        json!({
            "temperature": {
                "type": "number",
                "default": DEFAULT_TEMPERATURE,
                "min": MIN_TEMPERATURE,
                "description": "Bath temperature T (in units of J/k_B); critical point ≈ 2.269"
            },
            "coupling": {
                "type": "number",
                "default": DEFAULT_COUPLING,
                "description": "Spin-spin coupling J (positive = ferromagnetic)"
            },
            "external_field": {
                "type": "number",
                "default": DEFAULT_EXTERNAL_FIELD,
                "description": "Uniform external field h biasing one spin direction"
            },
            "sweeps_per_step": {
                "type": "integer",
                "default": DEFAULT_SWEEPS_PER_STEP,
                "min": 1,
                "max": MAX_SWEEPS_PER_STEP,
                "description": "Metropolis sweeps per Engine step (1 sweep = N site attempts)"
            },
            "influence_strength": {
                "type": "number",
                "default": DEFAULT_INFLUENCE_STRENGTH,
                "description": "Per-step gain on external influence field added to local h"
            }
        })
    }

    fn set_influence(&mut self, field: &Field) -> Result<(), EngineError> {
        if field.width() != self.width || field.height() != self.height {
            return Err(EngineError::InvalidDimensions);
        }
        self.influence = Some(field.data().to_vec());
        Ok(())
    }
}

/// Sum of the 4 toroidal neighbors' spins as i32 (range [-4, 4]).
fn neighbor_spin_sum(spins: &[i8], w: usize, h: usize, x: usize, y: usize) -> i32 {
    let xm = if x == 0 { w - 1 } else { x - 1 };
    let xp = if x + 1 == w { 0 } else { x + 1 };
    let ym = if y == 0 { h - 1 } else { y - 1 };
    let yp = if y + 1 == h { 0 } else { y + 1 };

    spins[y * w + xm] as i32
        + spins[y * w + xp] as i32
        + spins[ym * w + x] as i32
        + spins[yp * w + x] as i32
}

/// Maps spins (`±1`) into the field via `(s + 1) / 2`.
fn rebuild_field(field: &mut Field, spins: &[i8]) {
    for (dst, &s) in field.data_mut().iter_mut().zip(spins.iter()) {
        *dst = (s as f64 + 1.0) * 0.5;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ising(w: usize, h: usize, seed: u64) -> Ising {
        Ising::new(w, h, seed, IsingParams::default()).unwrap()
    }

    // ---- Construction ----

    #[test]
    fn new_creates_field_with_correct_dims() {
        let i = ising(64, 32, 42);
        assert_eq!(i.field().width(), 64);
        assert_eq!(i.field().height(), 32);
    }

    #[test]
    fn new_with_zero_dims_returns_error() {
        let p = IsingParams::default();
        assert!(Ising::new(0, 16, 42, p).is_err());
        assert!(Ising::new(16, 0, 42, p).is_err());
    }

    #[test]
    fn initial_spins_are_only_plus_or_minus_one() {
        let i = ising(64, 64, 42);
        for &s in &i.spins {
            assert!(s == 1 || s == -1, "non-binary spin: {s}");
        }
    }

    #[test]
    fn initial_field_values_are_zero_or_one() {
        let i = ising(32, 32, 42);
        for &v in i.field().data() {
            assert!(v == 0.0 || v == 1.0, "spin field not binary: {v}");
        }
    }

    // ---- Step ----

    #[test]
    fn step_keeps_spins_binary() {
        let mut i = ising(32, 32, 42);
        for _ in 0..10 {
            i.step().unwrap();
        }
        for &s in &i.spins {
            assert!(s == 1 || s == -1, "step produced non-binary spin: {s}");
        }
    }

    #[test]
    fn step_keeps_field_in_unit_interval() {
        let mut i = ising(32, 32, 42);
        for _ in 0..10 {
            i.step().unwrap();
        }
        for &v in i.field().data() {
            assert!(
                (0.0..=1.0).contains(&v) && !v.is_nan(),
                "field value out of range: {v}"
            );
        }
    }

    // ---- Physics ----

    #[test]
    fn at_zero_field_initial_magnetization_near_zero() {
        // Random ±1 init: the law of large numbers gives |m| ~ 1/sqrt(N) for
        // an N=4096 lattice → about ±0.016. Use a wide tolerance.
        let i = ising(64, 64, 42);
        let m = i.magnetization();
        assert!(m.abs() < 0.1, "initial magnetization {m} too far from 0");
    }

    #[test]
    fn cold_aligned_state_stays_aligned() {
        // Force all-up spins, set T very low. After many sweeps virtually
        // no spin should flip (energy cost is huge, exp(-ΔE/T) → 0).
        let p = IsingParams {
            temperature: 0.05,
            sweeps_per_step: 5,
            ..IsingParams::default()
        };
        let mut i = Ising::new(24, 24, 42, p).unwrap();
        for s in i.spins.iter_mut() {
            *s = 1;
        }
        rebuild_field(&mut i.field, &i.spins);
        for _ in 0..3 {
            i.step().unwrap();
        }
        // Allow a small handful of spurious flips (some near-zero ΔE accepts
        // can still happen at the boundary of the Boltzmann cutoff).
        let down = i.spins.iter().filter(|&&s| s == -1).count();
        assert!(
            down < 5,
            "cold aligned state should stay aligned, got {down} flipped"
        );
    }

    #[test]
    fn hot_state_explores_full_phase_space() {
        // At very high T (≫ T_c) acceptance probability ≈ 1 for most flips;
        // the magnetization should random-walk near zero rather than locking.
        let p = IsingParams {
            temperature: 100.0,
            sweeps_per_step: 5,
            ..IsingParams::default()
        };
        let mut i = Ising::new(32, 32, 42, p).unwrap();
        for _ in 0..20 {
            i.step().unwrap();
        }
        let m = i.magnetization();
        assert!(m.abs() < 0.3, "hot state magnetization {m} too aligned");
    }

    #[test]
    fn strong_external_field_drives_alignment() {
        // With a strong positive h and moderate T, the system should align
        // strongly with h (positive magnetization).
        let p = IsingParams {
            temperature: 1.5,
            external_field: 5.0,
            sweeps_per_step: 8,
            ..IsingParams::default()
        };
        let mut i = Ising::new(32, 32, 42, p).unwrap();
        for _ in 0..5 {
            i.step().unwrap();
        }
        let m = i.magnetization();
        assert!(m > 0.5, "strong h>0 should drive m positive, got {m}");
    }

    // ---- Determinism ----

    #[test]
    fn same_seed_identical_state() {
        let mut a = ising(32, 32, 12345);
        let mut b = ising(32, 32, 12345);
        for _ in 0..10 {
            a.step().unwrap();
            b.step().unwrap();
        }
        assert_eq!(a.spins, b.spins);
        assert!(a
            .field()
            .data()
            .iter()
            .zip(b.field().data().iter())
            .all(|(va, vb)| va.to_bits() == vb.to_bits()));
    }

    #[test]
    fn different_seeds_different_state() {
        let mut a = ising(32, 32, 1);
        let mut b = ising(32, 32, 2);
        for _ in 0..5 {
            a.step().unwrap();
            b.step().unwrap();
        }
        // At least one cell must differ.
        assert!(a.spins.iter().zip(b.spins.iter()).any(|(x, y)| x != y));
    }

    // ---- Influence coupling ----

    #[test]
    fn set_influence_with_wrong_dims_returns_error() {
        let mut i = ising(16, 16, 42);
        let bad = Field::new(8, 8).unwrap();
        assert!(i.set_influence(&bad).is_err());
    }

    #[test]
    fn set_influence_with_zero_strength_no_effect() {
        let p = IsingParams {
            influence_strength: 0.0,
            ..IsingParams::default()
        };
        let mut a = Ising::new(24, 24, 42, p).unwrap();
        let mut b = Ising::new(24, 24, 42, p).unwrap();
        let inf = Field::filled(24, 24, 1.0).unwrap();
        b.set_influence(&inf).unwrap();
        for _ in 0..5 {
            a.step().unwrap();
            b.step().unwrap();
        }
        assert_eq!(a.spins, b.spins);
    }

    #[test]
    fn positive_influence_drives_alignment() {
        // High influence_strength * uniform 1.0 field should bias spins
        // strongly positive even at temperature near critical.
        let p = IsingParams {
            temperature: 2.0,
            influence_strength: 5.0,
            sweeps_per_step: 6,
            ..IsingParams::default()
        };
        let mut i = Ising::new(32, 32, 42, p).unwrap();
        let inf = Field::filled(32, 32, 1.0).unwrap();
        i.set_influence(&inf).unwrap();
        for _ in 0..5 {
            i.step().unwrap();
        }
        let m = i.magnetization();
        assert!(
            m > 0.4,
            "positive influence should align m positive, got {m}"
        );
    }

    // ---- JSON ----

    #[test]
    fn from_json_uses_defaults() {
        let i = Ising::from_json(8, 8, 42, &json!({})).unwrap();
        assert_eq!(i.params.temperature, DEFAULT_TEMPERATURE);
        assert_eq!(i.params.coupling, DEFAULT_COUPLING);
        assert_eq!(i.params.sweeps_per_step, DEFAULT_SWEEPS_PER_STEP);
    }

    #[test]
    fn from_json_clamps_temperature_above_min() {
        let i = Ising::from_json(8, 8, 42, &json!({"temperature": -1.0})).unwrap();
        assert!(i.params.temperature >= MIN_TEMPERATURE);
    }

    #[test]
    fn from_json_clamps_sweeps() {
        let lo = Ising::from_json(8, 8, 42, &json!({"sweeps_per_step": 0})).unwrap();
        assert_eq!(lo.params.sweeps_per_step, 1);
        let hi = Ising::from_json(8, 8, 42, &json!({"sweeps_per_step": 9999})).unwrap();
        assert_eq!(hi.params.sweeps_per_step, MAX_SWEEPS_PER_STEP);
    }

    // ---- Engine trait ----

    #[test]
    fn params_returns_current_values() {
        let i = ising(8, 8, 42);
        let v = i.params();
        assert!((v["temperature"].as_f64().unwrap() - DEFAULT_TEMPERATURE).abs() < 1e-12);
    }

    #[test]
    fn param_schema_has_all_keys() {
        let i = ising(8, 8, 42);
        let s = i.param_schema();
        for k in [
            "temperature",
            "coupling",
            "external_field",
            "sweeps_per_step",
            "influence_strength",
        ] {
            assert!(s.get(k).is_some(), "schema missing {k}");
        }
    }

    #[test]
    fn engine_is_object_safe() {
        let i = ising(8, 8, 42);
        let _: Box<dyn Engine> = Box::new(i);
    }

    #[test]
    fn hue_field_is_none() {
        let i = ising(8, 8, 42);
        assert!(i.hue_field().is_none());
    }

    // ---- Property-based ----

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn no_nans_for_any_seed_and_temperature(
                seed: u64,
                t in 0.1_f64..10.0,
            ) {
                let p = IsingParams { temperature: t, ..IsingParams::default() };
                let mut i = Ising::new(20, 20, seed, p).unwrap();
                for _ in 0..10 {
                    i.step().unwrap();
                }
                for &v in i.field().data() {
                    prop_assert!(!v.is_nan());
                    prop_assert!((0.0..=1.0).contains(&v));
                }
                for &s in &i.spins {
                    prop_assert!(s == 1 || s == -1);
                }
            }

            #[test]
            fn deterministic_for_any_seed(seed: u64) {
                let mut a = ising(20, 20, seed);
                let mut b = ising(20, 20, seed);
                for _ in 0..10 {
                    a.step().unwrap();
                    b.step().unwrap();
                }
                prop_assert_eq!(a.spins.clone(), b.spins.clone());
            }
        }
    }
}
