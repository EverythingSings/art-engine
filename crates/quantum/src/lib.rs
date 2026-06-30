#![deny(unsafe_code)]
//! 2D quantum walk engine.
//!
//! Maintains a wavefunction `ψ(x, y, c)` over a toroidal lattice with a
//! 4-dimensional coin space (one component per neighbor direction). Each
//! step applies a *coin operator* (Hadamard or Grover) to the coin index
//! at every cell, then a *shift operator* that moves each coin component
//! to the corresponding neighbor.
//!
//! The field output is the marginal probability `Σ_c |ψ(x, y, c)|²` per
//! cell, normalized to `[0, 1]`. The classic single-point initial
//! condition produces visually striking diamond-shaped interference
//! fronts that have no classical analogue.
//!
//! # Determinism
//!
//! Same params + same step count = bit-identical output. The walk is
//! purely deterministic (no PRNG used outside testing); any randomness
//! the visual shows comes from quantum interference.
//!
//! # JSON parameters
//!
//! ```json
//! {
//!   "coin": "grover",
//!   "init": "single_point",
//!   "field_gamma": 0.4
//! }
//! ```
//!
//! `coin` recognized values: `"grover"` (the default — strong corner-bound
//! interference), `"hadamard"` (separable rectangular pattern), `"dft"`
//! (discrete Fourier transform — uniform spreading).

use art_engine_core::error::EngineError;
use art_engine_core::field::Field;
use art_engine_core::params::{param_f64, param_string};
use art_engine_core::Engine;
use serde_json::{json, Value};

/// Number of coin states in the 2D walk (one per neighbor direction).
const COIN_DIM: usize = 4;
/// Coin direction indices.
const NORTH: usize = 0;
const SOUTH: usize = 1;
const EAST: usize = 2;
const WEST: usize = 3;

const DEFAULT_COIN: &str = "grover";
const DEFAULT_INIT: &str = "single_point";
const DEFAULT_FIELD_GAMMA: f64 = 0.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coin {
    /// Grover coin — most visually distinctive (strong central interference).
    Grover,
    /// Tensor product of two 1D Hadamard coins. Separable, produces
    /// rectangular wavefronts.
    Hadamard,
    /// Discrete Fourier transform coin — most uniform spreading.
    Dft,
}

impl Coin {
    fn from_str_or_default(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "grover" => Self::Grover,
            "hadamard" => Self::Hadamard,
            "dft" => Self::Dft,
            _ => Self::Grover,
        }
    }
    fn name(&self) -> &'static str {
        match self {
            Self::Grover => "grover",
            Self::Hadamard => "hadamard",
            Self::Dft => "dft",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitState {
    /// All amplitude concentrated at the canvas-center cell.
    SinglePoint,
    /// Equal amplitude in a 5x5 square around center.
    SmallSquare,
}

impl InitState {
    fn from_str_or_default(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "single_point" | "point" => Self::SinglePoint,
            "small_square" | "square" => Self::SmallSquare,
            _ => Self::SinglePoint,
        }
    }
    fn name(&self) -> &'static str {
        match self {
            Self::SinglePoint => "single_point",
            Self::SmallSquare => "small_square",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct QuantumParams {
    pub coin: Coin,
    pub init: InitState,
    pub field_gamma: f64,
}

impl Default for QuantumParams {
    fn default() -> Self {
        Self {
            coin: Coin::Grover,
            init: InitState::SinglePoint,
            field_gamma: DEFAULT_FIELD_GAMMA,
        }
    }
}

impl QuantumParams {
    pub fn from_json(params: &Value) -> Self {
        Self {
            coin: Coin::from_str_or_default(&param_string(params, "coin", DEFAULT_COIN)),
            init: InitState::from_str_or_default(&param_string(params, "init", DEFAULT_INIT)),
            field_gamma: param_f64(params, "field_gamma", DEFAULT_FIELD_GAMMA).clamp(0.05, 5.0),
        }
    }
}

/// 2D quantum walk engine.
pub struct Quantum {
    params: QuantumParams,
    width: usize,
    height: usize,
    /// Wavefunction stored as parallel real/imag slices: idx = (y * w + x) * COIN_DIM + c.
    psi_re: Vec<f64>,
    psi_im: Vec<f64>,
    /// Scratch buffers used during shift to avoid in-place hazards.
    next_re: Vec<f64>,
    next_im: Vec<f64>,
    field: Field,
}

impl Quantum {
    pub fn new(width: usize, height: usize, params: QuantumParams) -> Result<Self, EngineError> {
        if width == 0 || height == 0 {
            return Err(EngineError::InvalidDimensions);
        }
        let len = width
            .checked_mul(height)
            .and_then(|n| n.checked_mul(COIN_DIM))
            .ok_or(EngineError::InvalidDimensions)?;

        let mut psi_re = vec![0.0_f64; len];
        let mut psi_im = vec![0.0_f64; len];
        seed_initial_state(&mut psi_re, &mut psi_im, width, height, &params.init);

        Ok(Self {
            params,
            width,
            height,
            next_re: vec![0.0_f64; len],
            next_im: vec![0.0_f64; len],
            psi_re,
            psi_im,
            field: Field::new(width, height)?,
        })
    }

    pub fn from_json(
        width: usize,
        height: usize,
        _seed: u64,
        params: &Value,
    ) -> Result<Self, EngineError> {
        Self::new(width, height, QuantumParams::from_json(params))
    }

    /// Total norm `Σ_x,y,c |ψ|²`. Should remain ≈ 1.0 throughout the walk
    /// (a unit-norm initial state evolves under unitary operators).
    pub fn norm(&self) -> f64 {
        self.psi_re
            .iter()
            .zip(self.psi_im.iter())
            .map(|(r, i)| r * r + i * i)
            .sum()
    }
}

impl Engine for Quantum {
    fn step(&mut self) -> Result<(), EngineError> {
        // 1. Coin: apply 4x4 unitary to each cell's coin vector in place.
        match self.params.coin {
            Coin::Grover => apply_grover(&mut self.psi_re, &mut self.psi_im),
            Coin::Hadamard => apply_hadamard2d(&mut self.psi_re, &mut self.psi_im),
            Coin::Dft => apply_dft4(&mut self.psi_re, &mut self.psi_im),
        }

        // 2. Shift: move coin components into neighbor cells. Write into
        // next_re / next_im to avoid read-write hazards, then swap.
        for v in self.next_re.iter_mut() {
            *v = 0.0;
        }
        for v in self.next_im.iter_mut() {
            *v = 0.0;
        }
        let w = self.width;
        let h = self.height;
        for y in 0..h {
            for x in 0..w {
                let dst_base = (y * w + x) * COIN_DIM;
                let xm = if x == 0 { w - 1 } else { x - 1 };
                let xp = if x + 1 == w { 0 } else { x + 1 };
                let ym = if y == 0 { h - 1 } else { y - 1 };
                let yp = if y + 1 == h { 0 } else { y + 1 };
                // Receiver perspective: cell at (x, y) inherits coin component
                // c from the neighbor that is "behind" direction c.
                // NORTH component travels north, so we receive from (x, y+1).
                let src_n = (yp * w + x) * COIN_DIM + NORTH;
                let src_s = (ym * w + x) * COIN_DIM + SOUTH;
                let src_e = (y * w + xm) * COIN_DIM + EAST;
                let src_w = (y * w + xp) * COIN_DIM + WEST;
                self.next_re[dst_base + NORTH] = self.psi_re[src_n];
                self.next_im[dst_base + NORTH] = self.psi_im[src_n];
                self.next_re[dst_base + SOUTH] = self.psi_re[src_s];
                self.next_im[dst_base + SOUTH] = self.psi_im[src_s];
                self.next_re[dst_base + EAST] = self.psi_re[src_e];
                self.next_im[dst_base + EAST] = self.psi_im[src_e];
                self.next_re[dst_base + WEST] = self.psi_re[src_w];
                self.next_im[dst_base + WEST] = self.psi_im[src_w];
            }
        }
        std::mem::swap(&mut self.psi_re, &mut self.next_re);
        std::mem::swap(&mut self.psi_im, &mut self.next_im);

        // 3. Build output field: marginal probability per cell, gamma-shaped
        // and normalized so the brightest cell maps to 1.0.
        let cells = self.width * self.height;
        let gamma = self.params.field_gamma;
        let mut max_p = 0.0_f64;
        let mut probs = vec![0.0_f64; cells];
        for (i, prob) in probs.iter_mut().enumerate().take(cells) {
            let base = i * COIN_DIM;
            let mut p = 0.0;
            for c in 0..COIN_DIM {
                let r = self.psi_re[base + c];
                let im = self.psi_im[base + c];
                p += r * r + im * im;
            }
            *prob = p;
            if p > max_p {
                max_p = p;
            }
        }
        let inv_max = if max_p > 0.0 { 1.0 / max_p } else { 1.0 };
        for (dst, &p) in self.field.data_mut().iter_mut().zip(probs.iter()) {
            let n = (p * inv_max).clamp(0.0, 1.0);
            *dst = if n > 0.0 { n.powf(gamma) } else { 0.0 };
        }
        Ok(())
    }

    fn field(&self) -> &Field {
        &self.field
    }

    fn params(&self) -> Value {
        json!({
            "coin": self.params.coin.name(),
            "init": self.params.init.name(),
            "field_gamma": self.params.field_gamma,
        })
    }

    fn param_schema(&self) -> Value {
        json!({
            "coin": {
                "type": "string",
                "default": DEFAULT_COIN,
                "enum": ["grover", "hadamard", "dft"],
                "description": "4x4 coin operator applied to each cell's coin vector"
            },
            "init": {
                "type": "string",
                "default": DEFAULT_INIT,
                "enum": ["single_point", "small_square"],
                "description": "Initial wavefunction shape"
            },
            "field_gamma": {
                "type": "number",
                "default": DEFAULT_FIELD_GAMMA,
                "min": 0.05,
                "max": 5.0,
                "description": "Gamma applied to per-cell probability before palette lookup"
            }
        })
    }
}

/// Seeds `psi` with the requested initial state, normalized to unit total norm.
fn seed_initial_state(
    psi_re: &mut [f64],
    psi_im: &mut [f64],
    width: usize,
    height: usize,
    init: &InitState,
) {
    // Coin amplitudes (1, i, -1, -i) / 2 — magnitude 1/2 each, total
    // norm 1. Crucially this state is orthogonal to (1,1,1,1) so it lies
    // outside the +1 eigenspace of the Grover coin and will spread
    // dynamically. Hadamard and DFT also produce non-trivial spreading
    // from this state.
    let phased: [(f64, f64); 4] = [(0.5, 0.0), (0.0, 0.5), (-0.5, 0.0), (0.0, -0.5)];

    match init {
        InitState::SinglePoint => {
            let cx = width / 2;
            let cy = height / 2;
            let base = (cy * width + cx) * COIN_DIM;
            for (c, (re, im)) in phased.iter().enumerate() {
                psi_re[base + c] = *re;
                psi_im[base + c] = *im;
            }
        }
        InitState::SmallSquare => {
            // 5x5 region centered. Each cell's coin component scaled so
            // total norm = 1.
            let cx = width / 2;
            let cy = height / 2;
            let scale = 1.0 / 5.0; // sqrt(1/25) — 25 cells, each contributes 1/25 of total
            for dy in -2_i32..=2 {
                for dx in -2_i32..=2 {
                    let x = (cx as i32 + dx).rem_euclid(width as i32) as usize;
                    let y = (cy as i32 + dy).rem_euclid(height as i32) as usize;
                    let base = (y * width + x) * COIN_DIM;
                    for (c, (re, im)) in phased.iter().enumerate() {
                        psi_re[base + c] = re * scale;
                        psi_im[base + c] = im * scale;
                    }
                }
            }
        }
    }
}

/// In-place Grover coin: G_ij = 2/N - delta_ij. For N=4: G_ii = -1/2,
/// G_ij = 1/2 (i ≠ j). Equivalent to 2*<a> - x_i where <a> = mean of vector.
fn apply_grover(psi_re: &mut [f64], psi_im: &mut [f64]) {
    let cells = psi_re.len() / COIN_DIM;
    for i in 0..cells {
        let base = i * COIN_DIM;
        let sum_re: f64 = (0..COIN_DIM).map(|c| psi_re[base + c]).sum();
        let sum_im: f64 = (0..COIN_DIM).map(|c| psi_im[base + c]).sum();
        // 2 * mean = sum / 2 (since N=4)
        let avg_re = sum_re * 0.5;
        let avg_im = sum_im * 0.5;
        for c in 0..COIN_DIM {
            psi_re[base + c] = avg_re - psi_re[base + c];
            psi_im[base + c] = avg_im - psi_im[base + c];
        }
    }
}

/// 2D Hadamard coin: tensor product of two 1D Hadamards. Equivalent to
/// the 4x4 matrix (1/2) * [[1,1,1,1],[1,-1,1,-1],[1,1,-1,-1],[1,-1,-1,1]].
fn apply_hadamard2d(psi_re: &mut [f64], psi_im: &mut [f64]) {
    let cells = psi_re.len() / COIN_DIM;
    for i in 0..cells {
        let base = i * COIN_DIM;
        let a = (psi_re[base], psi_im[base]);
        let b = (psi_re[base + 1], psi_im[base + 1]);
        let c = (psi_re[base + 2], psi_im[base + 2]);
        let d = (psi_re[base + 3], psi_im[base + 3]);
        psi_re[base] = 0.5 * (a.0 + b.0 + c.0 + d.0);
        psi_im[base] = 0.5 * (a.1 + b.1 + c.1 + d.1);
        psi_re[base + 1] = 0.5 * (a.0 - b.0 + c.0 - d.0);
        psi_im[base + 1] = 0.5 * (a.1 - b.1 + c.1 - d.1);
        psi_re[base + 2] = 0.5 * (a.0 + b.0 - c.0 - d.0);
        psi_im[base + 2] = 0.5 * (a.1 + b.1 - c.1 - d.1);
        psi_re[base + 3] = 0.5 * (a.0 - b.0 - c.0 + d.0);
        psi_im[base + 3] = 0.5 * (a.1 - b.1 - c.1 + d.1);
    }
}

/// 4x4 discrete Fourier transform coin. F_jk = (1/2) * exp(-2πi jk/4).
/// The phases for jk mod 4 = 0, 1, 2, 3 are 1, -i, -1, i respectively.
fn apply_dft4(psi_re: &mut [f64], psi_im: &mut [f64]) {
    let cells = psi_re.len() / COIN_DIM;
    for i in 0..cells {
        let base = i * COIN_DIM;
        let a = (psi_re[base], psi_im[base]);
        let b = (psi_re[base + 1], psi_im[base + 1]);
        let c = (psi_re[base + 2], psi_im[base + 2]);
        let d = (psi_re[base + 3], psi_im[base + 3]);
        // Row 0: 1, 1, 1, 1
        psi_re[base] = 0.5 * (a.0 + b.0 + c.0 + d.0);
        psi_im[base] = 0.5 * (a.1 + b.1 + c.1 + d.1);
        // Row 1: 1, -i, -1, i
        // multiply b by -i: (br, bi) -> (bi, -br)
        // multiply c by -1: (-cr, -ci)
        // multiply d by i: (-di, dr)
        psi_re[base + 1] = 0.5 * (a.0 + b.1 - c.0 - d.1);
        psi_im[base + 1] = 0.5 * (a.1 - b.0 - c.1 + d.0);
        // Row 2: 1, -1, 1, -1
        psi_re[base + 2] = 0.5 * (a.0 - b.0 + c.0 - d.0);
        psi_im[base + 2] = 0.5 * (a.1 - b.1 + c.1 - d.1);
        // Row 3: 1, i, -1, -i
        // b * i: (-bi, br); c * -1: (-cr, -ci); d * -i: (di, -dr)
        psi_re[base + 3] = 0.5 * (a.0 - b.1 - c.0 + d.1);
        psi_im[base + 3] = 0.5 * (a.1 + b.0 - c.1 - d.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(w: usize, h: usize) -> Quantum {
        Quantum::new(w, h, QuantumParams::default()).unwrap()
    }

    // ---- Construction ----

    #[test]
    fn new_creates_field_with_correct_dims() {
        let e = q(64, 32);
        assert_eq!(e.field().width(), 64);
        assert_eq!(e.field().height(), 32);
    }

    #[test]
    fn new_with_zero_dims_returns_error() {
        assert!(Quantum::new(0, 16, QuantumParams::default()).is_err());
        assert!(Quantum::new(16, 0, QuantumParams::default()).is_err());
    }

    #[test]
    fn initial_norm_is_unity() {
        let e = q(32, 32);
        let n = e.norm();
        assert!((n - 1.0).abs() < 1e-12, "initial norm not 1: {n}");
    }

    #[test]
    fn small_square_init_normalized() {
        let e = Quantum::new(
            32,
            32,
            QuantumParams {
                init: InitState::SmallSquare,
                ..QuantumParams::default()
            },
        )
        .unwrap();
        let n = e.norm();
        assert!((n - 1.0).abs() < 1e-12, "small_square init norm not 1: {n}");
    }

    // ---- Unitarity ----

    #[test]
    fn norm_preserved_after_many_steps_grover() {
        let mut e = q(32, 32);
        for _ in 0..50 {
            e.step().unwrap();
        }
        let n = e.norm();
        assert!(
            (n - 1.0).abs() < 1e-9,
            "Grover norm drifted: {n} (expected 1)"
        );
    }

    #[test]
    fn norm_preserved_after_many_steps_hadamard() {
        let mut e = Quantum::new(
            32,
            32,
            QuantumParams {
                coin: Coin::Hadamard,
                ..QuantumParams::default()
            },
        )
        .unwrap();
        for _ in 0..50 {
            e.step().unwrap();
        }
        let n = e.norm();
        assert!(
            (n - 1.0).abs() < 1e-9,
            "Hadamard norm drifted: {n} (expected 1)"
        );
    }

    #[test]
    fn norm_preserved_after_many_steps_dft() {
        let mut e = Quantum::new(
            32,
            32,
            QuantumParams {
                coin: Coin::Dft,
                ..QuantumParams::default()
            },
        )
        .unwrap();
        for _ in 0..50 {
            e.step().unwrap();
        }
        let n = e.norm();
        assert!((n - 1.0).abs() < 1e-9, "DFT norm drifted: {n} (expected 1)");
    }

    // ---- Spreading ----

    #[test]
    fn wavefunction_spreads_from_initial_point() {
        let mut e = q(64, 64);
        let cx = 64 / 2;
        let cy = 64 / 2;
        let base = (cy * 64 + cx) * COIN_DIM;
        let initial_p_at_center: f64 = (0..COIN_DIM)
            .map(|c| e.psi_re[base + c].powi(2) + e.psi_im[base + c].powi(2))
            .sum();
        assert!(initial_p_at_center > 0.99);

        for _ in 0..15 {
            e.step().unwrap();
        }
        let final_p_at_center: f64 = (0..COIN_DIM)
            .map(|c| e.psi_re[base + c].powi(2) + e.psi_im[base + c].powi(2))
            .sum();
        assert!(
            final_p_at_center < initial_p_at_center,
            "wavefunction did not spread: center p = {final_p_at_center}"
        );
    }

    // ---- Field output ----

    #[test]
    fn field_values_in_unit_interval() {
        let mut e = q(32, 32);
        for _ in 0..20 {
            e.step().unwrap();
        }
        for &v in e.field().data() {
            assert!((0.0..=1.0).contains(&v) && !v.is_nan(), "out: {v}");
        }
    }

    // ---- Determinism ----

    #[test]
    fn determinism_same_params() {
        let mut a = q(40, 40);
        let mut b = q(40, 40);
        for _ in 0..30 {
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
    fn different_coins_produce_different_state() {
        let mut g = q(32, 32);
        let mut h = Quantum::new(
            32,
            32,
            QuantumParams {
                coin: Coin::Hadamard,
                ..QuantumParams::default()
            },
        )
        .unwrap();
        for _ in 0..15 {
            g.step().unwrap();
            h.step().unwrap();
        }
        assert!(g
            .field()
            .data()
            .iter()
            .zip(h.field().data().iter())
            .any(|(va, vb)| va.to_bits() != vb.to_bits()));
    }

    // ---- JSON ----

    #[test]
    fn from_json_default_coin_is_grover() {
        let e = Quantum::from_json(8, 8, 0, &json!({})).unwrap();
        assert_eq!(e.params.coin, Coin::Grover);
    }

    #[test]
    fn from_json_recognizes_each_coin() {
        for (k, expected) in [
            ("grover", Coin::Grover),
            ("hadamard", Coin::Hadamard),
            ("dft", Coin::Dft),
        ] {
            let e = Quantum::from_json(8, 8, 0, &json!({"coin": k})).unwrap();
            assert_eq!(e.params.coin, expected, "coin {k}");
        }
    }

    #[test]
    fn from_json_unknown_coin_falls_back_to_grover() {
        let e = Quantum::from_json(8, 8, 0, &json!({"coin": "warp"})).unwrap();
        assert_eq!(e.params.coin, Coin::Grover);
    }

    // ---- Engine trait ----

    #[test]
    fn params_returns_current_values() {
        let e = q(8, 8);
        let v = e.params();
        assert_eq!(v["coin"].as_str().unwrap(), "grover");
    }

    #[test]
    fn param_schema_has_all_keys() {
        let e = q(8, 8);
        let s = e.param_schema();
        for k in ["coin", "init", "field_gamma"] {
            assert!(s.get(k).is_some(), "schema missing {k}");
        }
    }

    #[test]
    fn engine_is_object_safe() {
        let e = q(8, 8);
        let _: Box<dyn Engine> = Box::new(e);
    }

    #[test]
    fn hue_field_is_none() {
        let e = q(8, 8);
        assert!(e.hue_field().is_none());
    }

    // ---- Property-based ----

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn no_nans_for_each_coin(coin in 0_usize..3) {
                let coin = match coin {
                    0 => Coin::Grover,
                    1 => Coin::Hadamard,
                    _ => Coin::Dft,
                };
                let mut e = Quantum::new(20, 20, QuantumParams { coin, ..QuantumParams::default() }).unwrap();
                for _ in 0..15 {
                    e.step().unwrap();
                }
                for &v in e.field().data() {
                    prop_assert!(!v.is_nan());
                    prop_assert!((0.0..=1.0).contains(&v));
                }
            }
        }
    }
}
