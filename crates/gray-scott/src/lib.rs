#![deny(unsafe_code)]
//! Gray-Scott reaction-diffusion engine.
//!
//! Simulates the Gray-Scott model: two chemicals (U substrate, V activator)
//! react and diffuse on a 2D toroidal grid. The interplay of feed rate (F),
//! kill rate (k), and diffusion constants produces a rich variety of patterns
//! — spots, stripes, coral, mitosis, and more.
//!
//! The primary output field is the V (activator) concentration, which the
//! rendering pipeline maps to pixels via a palette.

use art_engine_core::error::EngineError;
use art_engine_core::field::Field;
use art_engine_core::params::param_f64;
use art_engine_core::prng::Xorshift64;
use art_engine_core::Engine;
use serde_json::{json, Value};

/// Default feed rate — controls how fast U is replenished.
const DEFAULT_FEED_RATE: f64 = 0.055;
/// Default kill rate — controls how fast V is removed.
const DEFAULT_KILL_RATE: f64 = 0.062;
/// Default diffusion rate for U (substrate).
const DEFAULT_DIFFUSION_A: f64 = 1.0;
/// Default diffusion rate for V (activator).
const DEFAULT_DIFFUSION_B: f64 = 0.5;
/// Default time step per `step()` call.
const DEFAULT_DT: f64 = 1.0;
/// Spot radius in cells for initial V seeding.
const SPOT_RADIUS: isize = 3;
/// Fraction of total area used to determine spot count.
const SPOT_DENSITY: f64 = 0.0005;

/// Simulation parameters for the Gray-Scott model.
///
/// Bundles the five tunable constants that control pattern formation.
/// Use [`Default`] for the classic coral parameters (F=0.055, k=0.062).
#[derive(Debug, Clone, Copy)]
pub struct GrayScottParams {
    /// Feed rate (F): how fast substrate U is replenished.
    pub feed_rate: f64,
    /// Kill rate (k): how fast activator V is removed.
    pub kill_rate: f64,
    /// Diffusion rate for U (substrate).
    pub diffusion_a: f64,
    /// Diffusion rate for V (activator).
    pub diffusion_b: f64,
    /// Time step per `step()` call.
    pub dt: f64,
}

impl Default for GrayScottParams {
    fn default() -> Self {
        Self {
            feed_rate: DEFAULT_FEED_RATE,
            kill_rate: DEFAULT_KILL_RATE,
            diffusion_a: DEFAULT_DIFFUSION_A,
            diffusion_b: DEFAULT_DIFFUSION_B,
            dt: DEFAULT_DT,
        }
    }
}

impl GrayScottParams {
    /// Extracts parameters from a JSON object, falling back to defaults.
    pub fn from_json(params: &Value) -> Self {
        Self {
            feed_rate: param_f64(params, "feed_rate", DEFAULT_FEED_RATE),
            kill_rate: param_f64(params, "kill_rate", DEFAULT_KILL_RATE),
            diffusion_a: param_f64(params, "diffusion_a", DEFAULT_DIFFUSION_A),
            diffusion_b: param_f64(params, "diffusion_b", DEFAULT_DIFFUSION_B),
            dt: param_f64(params, "dt", DEFAULT_DT),
        }
    }
}

/// Gray-Scott reaction-diffusion engine.
///
/// Two chemical species U (substrate) and V (activator) react and diffuse:
/// - U is fed at rate F and consumed by the reaction U + 2V → 3V
/// - V is produced by the reaction and removed at rate (F + k)
/// - Both diffuse with independent rates Du, Dv
///
/// Uses a 9-point Laplacian stencil for isotropic diffusion and explicit
/// Euler integration.
pub struct GrayScott {
    u: Field,
    v: Field,
    params: GrayScottParams,
}

impl GrayScott {
    /// Creates a new Gray-Scott engine.
    ///
    /// U is initialized to 1.0 everywhere. V is initialized to 0.0 with
    /// circular spots of V=1.0 seeded at random positions (determined by `seed`).
    /// Spot count scales with grid area.
    ///
    /// Returns `EngineError::InvalidDimensions` if width or height is zero.
    pub fn new(
        width: usize,
        height: usize,
        seed: u64,
        params: GrayScottParams,
    ) -> Result<Self, EngineError> {
        let u = Field::filled(width, height, 1.0)?;
        let mut v = Field::new(width, height)?;
        let mut rng = Xorshift64::new(seed);
        seed_initial_spots(&mut v, &mut rng, width, height);
        Ok(Self { u, v, params })
    }

    /// Creates a Gray-Scott engine from a JSON params object.
    ///
    /// Extracts `feed_rate`, `kill_rate`, `diffusion_a`, `diffusion_b`, and `dt`
    /// from the JSON, falling back to defaults for missing keys.
    pub fn from_json(
        width: usize,
        height: usize,
        seed: u64,
        json_params: &Value,
    ) -> Result<Self, EngineError> {
        Self::new(width, height, seed, GrayScottParams::from_json(json_params))
    }

    /// Read-only access to the U (substrate) field.
    pub fn u_field(&self) -> &Field {
        &self.u
    }

    /// Read-only access to the V (activator) field.
    pub fn v_field(&self) -> &Field {
        &self.v
    }

    /// Current feed rate (F).
    pub fn feed_rate(&self) -> f64 {
        self.params.feed_rate
    }

    /// Current kill rate (k).
    pub fn kill_rate(&self) -> f64 {
        self.params.kill_rate
    }
}

impl Engine for GrayScott {
    fn step(&mut self) -> Result<(), EngineError> {
        let w = self.u.width();
        let h = self.u.height();
        let u_data = self.u.data();
        let v_data = self.v.data();

        let len = w * h;
        let mut u_next = vec![0.0_f64; len];
        let mut v_next = vec![0.0_f64; len];

        let f = self.params.feed_rate;
        let k = self.params.kill_rate;
        let du = self.params.diffusion_a;
        let dv = self.params.diffusion_b;
        let dt = self.params.dt;

        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let u = u_data[idx];
                let v = v_data[idx];

                let lap_u = laplacian_9pt(u_data, x, y, w, h);
                let lap_v = laplacian_9pt(v_data, x, y, w, h);

                let reaction = u * v * v;

                u_next[idx] = (u + dt * (du * lap_u - reaction + f * (1.0 - u))).clamp(0.0, 1.0);
                v_next[idx] = (v + dt * (dv * lap_v + reaction - (f + k) * v)).clamp(0.0, 1.0);
            }
        }

        self.u.data_mut().copy_from_slice(&u_next);
        self.v.data_mut().copy_from_slice(&v_next);

        Ok(())
    }

    fn field(&self) -> &Field {
        &self.v
    }

    fn params(&self) -> Value {
        json!({
            "feed_rate": self.params.feed_rate,
            "kill_rate": self.params.kill_rate,
            "diffusion_a": self.params.diffusion_a,
            "diffusion_b": self.params.diffusion_b,
            "dt": self.params.dt,
        })
    }

    fn param_schema(&self) -> Value {
        json!({
            "feed_rate": {
                "type": "number",
                "default": DEFAULT_FEED_RATE,
                "min": 0.0,
                "max": 0.1,
                "description": "Feed rate (F): how fast substrate U is replenished"
            },
            "kill_rate": {
                "type": "number",
                "default": DEFAULT_KILL_RATE,
                "min": 0.0,
                "max": 0.1,
                "description": "Kill rate (k): how fast activator V is removed"
            },
            "diffusion_a": {
                "type": "number",
                "default": DEFAULT_DIFFUSION_A,
                "min": 0.0,
                "max": 2.0,
                "description": "Diffusion rate for U (substrate)"
            },
            "diffusion_b": {
                "type": "number",
                "default": DEFAULT_DIFFUSION_B,
                "min": 0.0,
                "max": 2.0,
                "description": "Diffusion rate for V (activator)"
            },
            "dt": {
                "type": "number",
                "default": DEFAULT_DT,
                "min": 0.0,
                "max": 2.0,
                "description": "Time step per step() call"
            }
        })
    }
}

/// Seeds circular spots of V=1.0 at random positions.
///
/// Spot count scales with grid area: `(w * h) as f64 * SPOT_DENSITY`, minimum 1.
/// Each spot is a filled circle of radius [`SPOT_RADIUS`]. Uses `Field::set()`
/// which handles toroidal wrapping for spots near edges.
fn seed_initial_spots(v: &mut Field, rng: &mut Xorshift64, width: usize, height: usize) {
    let spot_count = ((width * height) as f64 * SPOT_DENSITY).ceil().max(1.0) as usize;
    let r = SPOT_RADIUS;

    for _ in 0..spot_count {
        let cx = rng.next_usize(width) as isize;
        let cy = rng.next_usize(height) as isize;
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    v.set(cx + dx, cy + dy, 1.0);
                }
            }
        }
    }
}

/// 9-point Laplacian stencil for isotropic diffusion.
///
/// Kernel weights:
/// ```text
///   0.05  0.2  0.05
///   0.2  -1.0  0.2
///   0.05  0.2  0.05
/// ```
///
/// Operates on raw data slice with explicit toroidal coordinate wrapping
/// for performance (avoids `Field::get()` per-access overhead in hot loop).
fn laplacian_9pt(data: &[f64], x: usize, y: usize, w: usize, h: usize) -> f64 {
    let xm = wrap(x, -1, w);
    let xp = wrap(x, 1, w);
    let ym = wrap(y, -1, h);
    let yp = wrap(y, 1, h);

    let center = data[y * w + x];

    // Cardinals (weight 0.2 each)
    let n = data[ym * w + x];
    let s = data[yp * w + x];
    let we = data[y * w + xm];
    let e = data[y * w + xp];

    // Diagonals (weight 0.05 each)
    let nw = data[ym * w + xm];
    let ne = data[ym * w + xp];
    let sw = data[yp * w + xm];
    let se = data[yp * w + xp];

    0.2 * (n + s + we + e) + 0.05 * (nw + ne + sw + se) - center
}

/// Toroidal coordinate wrap: `(coord + offset) mod size`.
fn wrap(coord: usize, offset: isize, size: usize) -> usize {
    ((coord as isize + offset).rem_euclid(size as isize)) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: default params for concise test construction.
    fn default_params() -> GrayScottParams {
        GrayScottParams::default()
    }

    /// Helper: construct with default params.
    fn gs(width: usize, height: usize, seed: u64) -> GrayScott {
        GrayScott::new(width, height, seed, default_params()).unwrap()
    }

    // ---- Construction tests ----

    #[test]
    fn new_creates_engine_with_correct_dimensions() {
        let gs = gs(64, 32, 42);
        assert_eq!(gs.u_field().width(), 64);
        assert_eq!(gs.u_field().height(), 32);
        assert_eq!(gs.v_field().width(), 64);
        assert_eq!(gs.v_field().height(), 32);
    }

    #[test]
    fn new_with_zero_dimensions_returns_error() {
        assert!(GrayScott::new(0, 10, 42, default_params()).is_err());
        assert!(GrayScott::new(10, 0, 42, default_params()).is_err());
    }

    #[test]
    fn new_initializes_u_to_ones() {
        let engine = gs(16, 16, 42);
        assert!(engine
            .u_field()
            .data()
            .iter()
            .all(|&v| (v - 1.0).abs() < f64::EPSILON));
    }

    #[test]
    fn new_v_has_seed_spots() {
        let engine = gs(128, 128, 42);
        let v_data = engine.v_field().data();
        let nonzero_count = v_data.iter().filter(|&&v| v > 0.0).count();
        assert!(nonzero_count > 0, "V field should have seeded spots");
        assert!(
            nonzero_count < v_data.len() / 2,
            "V field should be mostly zero"
        );
    }

    #[test]
    fn from_json_uses_defaults_for_empty_json() {
        let engine = GrayScott::from_json(32, 32, 42, &json!({})).unwrap();
        assert!((engine.feed_rate() - DEFAULT_FEED_RATE).abs() < f64::EPSILON);
        assert!((engine.kill_rate() - DEFAULT_KILL_RATE).abs() < f64::EPSILON);
    }

    #[test]
    fn from_json_extracts_custom_values() {
        let params = json!({
            "feed_rate": 0.04,
            "kill_rate": 0.06,
            "diffusion_a": 0.8,
            "diffusion_b": 0.4,
            "dt": 0.5,
        });
        let engine = GrayScott::from_json(32, 32, 42, &params).unwrap();
        assert!((engine.feed_rate() - 0.04).abs() < f64::EPSILON);
        assert!((engine.kill_rate() - 0.06).abs() < f64::EPSILON);
        let p = engine.params();
        assert!((p["diffusion_a"].as_f64().unwrap() - 0.8).abs() < f64::EPSILON);
        assert!((p["diffusion_b"].as_f64().unwrap() - 0.4).abs() < f64::EPSILON);
        assert!((p["dt"].as_f64().unwrap() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn params_returns_current_values() {
        let params = GrayScottParams {
            feed_rate: 0.03,
            kill_rate: 0.05,
            diffusion_a: 0.9,
            diffusion_b: 0.4,
            dt: 0.7,
        };
        let engine = GrayScott::new(16, 16, 42, params).unwrap();
        let p = engine.params();
        assert!((p["feed_rate"].as_f64().unwrap() - 0.03).abs() < f64::EPSILON);
        assert!((p["kill_rate"].as_f64().unwrap() - 0.05).abs() < f64::EPSILON);
        assert!((p["diffusion_a"].as_f64().unwrap() - 0.9).abs() < f64::EPSILON);
        assert!((p["diffusion_b"].as_f64().unwrap() - 0.4).abs() < f64::EPSILON);
        assert!((p["dt"].as_f64().unwrap() - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn param_schema_has_all_five_parameters() {
        let engine = gs(16, 16, 42);
        let schema = engine.param_schema();
        for key in &["feed_rate", "kill_rate", "diffusion_a", "diffusion_b", "dt"] {
            assert!(schema.get(key).is_some(), "schema missing parameter: {key}");
            assert!(schema[key].get("type").is_some(), "{key} missing 'type'");
            assert!(
                schema[key].get("default").is_some(),
                "{key} missing 'default'"
            );
            assert!(
                schema[key].get("description").is_some(),
                "{key} missing 'description'"
            );
        }
    }

    // ---- Determinism tests ----

    #[test]
    fn same_seed_identical_initial_state() {
        let a = gs(64, 64, 12345);
        let b = gs(64, 64, 12345);
        assert!(a
            .v_field()
            .data()
            .iter()
            .zip(b.v_field().data().iter())
            .all(|(va, vb)| va.to_bits() == vb.to_bits()));
    }

    #[test]
    fn same_seed_identical_after_100_steps() {
        let mut a = gs(32, 32, 42);
        let mut b = gs(32, 32, 42);
        for _ in 0..100 {
            a.step().unwrap();
            b.step().unwrap();
        }
        assert!(a
            .v_field()
            .data()
            .iter()
            .zip(b.v_field().data().iter())
            .all(|(va, vb)| va.to_bits() == vb.to_bits()));
        assert!(a
            .u_field()
            .data()
            .iter()
            .zip(b.u_field().data().iter())
            .all(|(ua, ub)| ua.to_bits() == ub.to_bits()));
    }

    #[test]
    fn different_seed_different_state() {
        let a = gs(64, 64, 1);
        let b = gs(64, 64, 2);
        assert!(a
            .v_field()
            .data()
            .iter()
            .zip(b.v_field().data().iter())
            .any(|(va, vb)| va.to_bits() != vb.to_bits()));
    }

    // ---- Step correctness tests ----

    #[test]
    fn step_returns_ok() {
        let mut engine = gs(16, 16, 42);
        assert!(engine.step().is_ok());
    }

    #[test]
    fn uniform_u_no_v_is_steady_state() {
        let mut engine = gs(16, 16, 42);
        // Zero out V to remove seeded spots
        engine.v.data_mut().fill(0.0);
        for _ in 0..10 {
            engine.step().unwrap();
        }
        assert!(
            engine
                .u_field()
                .data()
                .iter()
                .all(|&u| (u - 1.0).abs() < 1e-10),
            "U should stay at 1.0 with no V present"
        );
        assert!(
            engine.v_field().data().iter().all(|&v| v.abs() < 1e-10),
            "V should stay at 0.0 with no initial V"
        );
    }

    #[test]
    fn values_remain_in_unit_interval() {
        let mut engine = gs(32, 32, 42);
        for _ in 0..500 {
            engine.step().unwrap();
        }
        assert!(engine
            .u_field()
            .data()
            .iter()
            .all(|&u| (0.0..=1.0).contains(&u)));
        assert!(engine
            .v_field()
            .data()
            .iter()
            .all(|&v| (0.0..=1.0).contains(&v)));
    }

    #[test]
    fn laplacian_of_uniform_field_is_zero() {
        let data = vec![0.5; 16 * 16];
        for y in 0..16 {
            for x in 0..16 {
                let lap = laplacian_9pt(&data, x, y, 16, 16);
                assert!(
                    lap.abs() < 1e-12,
                    "Laplacian of uniform field should be 0, got {lap} at ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn laplacian_of_single_spike_is_negative_at_center() {
        let w = 16;
        let h = 16;
        let mut data = vec![0.0; w * h];
        data[8 * w + 8] = 1.0;
        let lap = laplacian_9pt(&data, 8, 8, w, h);
        assert!(
            lap < 0.0,
            "Laplacian at spike center should be negative, got {lap}"
        );
    }

    #[test]
    fn laplacian_wraps_toroidally() {
        let w = 8;
        let h = 8;
        let mut data = vec![0.0; w * h];
        data[0] = 1.0; // spike at (0, 0)
        let lap = laplacian_9pt(&data, 0, 0, w, h);
        assert!(
            lap < 0.0,
            "Laplacian at corner spike should be negative (wrapping works), got {lap}"
        );
        let lap_right = laplacian_9pt(&data, 1, 0, w, h);
        assert!(
            lap_right > 0.0,
            "Neighbor of spike should have positive Laplacian, got {lap_right}"
        );
    }

    #[test]
    fn zero_dt_produces_no_change() {
        let params = GrayScottParams {
            dt: 0.0,
            ..default_params()
        };
        let mut engine = GrayScott::new(32, 32, 42, params).unwrap();
        let u_before: Vec<u64> = engine
            .u_field()
            .data()
            .iter()
            .map(|v| v.to_bits())
            .collect();
        let v_before: Vec<u64> = engine
            .v_field()
            .data()
            .iter()
            .map(|v| v.to_bits())
            .collect();
        engine.step().unwrap();
        let u_after: Vec<u64> = engine
            .u_field()
            .data()
            .iter()
            .map(|v| v.to_bits())
            .collect();
        let v_after: Vec<u64> = engine
            .v_field()
            .data()
            .iter()
            .map(|v| v.to_bits())
            .collect();
        assert_eq!(u_before, u_after, "U should not change with dt=0");
        assert_eq!(v_before, v_after, "V should not change with dt=0");
    }

    // ---- Known pattern tests (aggregate properties) ----

    #[test]
    fn coral_pattern_f0_055_k0_062() {
        let mut engine = gs(64, 64, 42);
        for _ in 0..1000 {
            engine.step().unwrap();
        }
        let v_nonzero = engine
            .v_field()
            .data()
            .iter()
            .filter(|&&v| v > 0.01)
            .count();
        assert!(
            v_nonzero > 0,
            "Coral pattern should have non-trivial V area after 1000 steps"
        );
    }

    #[test]
    fn decay_pattern_high_kill_rate() {
        let params = GrayScottParams {
            feed_rate: 0.01,
            kill_rate: 0.09,
            ..default_params()
        };
        let mut engine = GrayScott::new(32, 32, 42, params).unwrap();
        for _ in 0..500 {
            engine.step().unwrap();
        }
        let v_mean: f64 =
            engine.v_field().data().iter().sum::<f64>() / engine.v_field().data().len() as f64;
        assert!(
            v_mean < 0.01,
            "High kill rate should decay V to near-zero, got mean {v_mean}"
        );
    }

    // ---- Trait compliance tests ----

    #[test]
    fn field_returns_v_not_u() {
        let engine = gs(16, 16, 42);
        let field = engine.field();
        let has_nonzero = field.data().iter().any(|&v| v > 0.0);
        let has_zero = field.data().iter().any(|&v| v == 0.0);
        assert!(
            has_nonzero && has_zero,
            "field() should return V (mix of 0s and spots)"
        );
    }

    #[test]
    fn hue_field_returns_none() {
        let engine = gs(16, 16, 42);
        assert!(engine.hue_field().is_none());
    }

    #[test]
    fn engine_is_object_safe() {
        let engine = gs(16, 16, 42);
        let boxed: Box<dyn Engine> = Box::new(engine);
        assert_eq!(boxed.field().width(), 16);
    }

    // ---- Property-based tests ----

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        fn dimension() -> impl Strategy<Value = usize> {
            4_usize..=32
        }

        fn sim_params() -> impl Strategy<Value = GrayScottParams> {
            (
                0.01_f64..=0.08,
                0.03_f64..=0.07,
                0.1_f64..=1.5,
                0.1_f64..=1.5,
                0.1_f64..=1.0,
            )
                .prop_map(|(f, k, da, db, dt)| GrayScottParams {
                    feed_rate: f,
                    kill_rate: k,
                    diffusion_a: da,
                    diffusion_b: db,
                    dt,
                })
        }

        proptest! {
            #[test]
            fn values_always_in_unit_interval(
                w in dimension(),
                h in dimension(),
                seed: u64,
                p in sim_params(),
            ) {
                let mut engine = GrayScott::new(w, h, seed, p).unwrap();
                for _ in 0..10 {
                    engine.step().unwrap();
                }
                for &u in engine.u_field().data() {
                    prop_assert!((0.0..=1.0).contains(&u), "U out of range: {u}");
                }
                for &v in engine.v_field().data() {
                    prop_assert!((0.0..=1.0).contains(&v), "V out of range: {v}");
                }
            }

            #[test]
            fn deterministic_across_instances(
                w in dimension(),
                h in dimension(),
                seed: u64,
            ) {
                let p = GrayScottParams::default();
                let mut a = GrayScott::new(w, h, seed, p).unwrap();
                let mut b = GrayScott::new(w, h, seed, p).unwrap();
                for _ in 0..10 {
                    a.step().unwrap();
                    b.step().unwrap();
                }
                for (va, vb) in a.v_field().data().iter().zip(b.v_field().data().iter()) {
                    prop_assert_eq!(va.to_bits(), vb.to_bits());
                }
            }

            #[test]
            fn no_nans_produced(
                w in dimension(),
                h in dimension(),
                seed: u64,
                p in sim_params(),
            ) {
                let mut engine = GrayScott::new(w, h, seed, p).unwrap();
                for _ in 0..10 {
                    engine.step().unwrap();
                }
                for &u in engine.u_field().data() {
                    prop_assert!(!u.is_nan(), "NaN in U field");
                }
                for &v in engine.v_field().data() {
                    prop_assert!(!v.is_nan(), "NaN in V field");
                }
            }

            #[test]
            fn no_v_means_steady_state(
                w in dimension(),
                h in dimension(),
                seed: u64,
            ) {
                let p = GrayScottParams::default();
                let mut engine = GrayScott::new(w, h, seed, p).unwrap();
                engine.v.data_mut().fill(0.0);
                for _ in 0..10 {
                    engine.step().unwrap();
                }
                for &u in engine.u_field().data() {
                    prop_assert!(
                        (u - 1.0).abs() < 1e-8,
                        "U should stay near 1.0 with no V, got {u}"
                    );
                }
                for &v in engine.v_field().data() {
                    prop_assert!(v.abs() < 1e-8, "V should stay near 0.0, got {v}");
                }
            }
        }
    }
}
