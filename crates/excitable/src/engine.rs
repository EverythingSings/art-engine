//! Barkley excitable-media engine implementation.
//!
//! Two coupled fields on a toroidal grid:
//! - `u`: fast activator. Diffuses via a 9-point Laplacian.
//! - `v`: slow recovery variable. Does not diffuse.
//!
//! Spiral waves require a *broken* wavefront — an excited region adjacent to a
//! refractory region. We nucleate them deterministically from `seed` (see
//! [`seed_broken_wavefronts`]) so the same seed yields bit-identical art while
//! different seeds produce different spiral arrangements.

use art_engine_core::error::EngineError;
use art_engine_core::field::Field;
use art_engine_core::params::param_f64;
use art_engine_core::prng::Xorshift64;
use art_engine_core::Engine;
use serde_json::{json, Value};

/// Default excitability threshold parameter `a`.
const DEFAULT_A: f64 = 0.75;
/// Default threshold offset `b`.
const DEFAULT_B: f64 = 0.02;
/// Default timescale separation `epsilon` (smaller = sharper, stiffer waves).
const DEFAULT_EPSILON: f64 = 0.02;
/// Default diffusion coefficient `D` for the activator `u`.
const DEFAULT_DIFFUSION: f64 = 1.0;
/// Default time step per `step()` call.
///
/// Chosen for explicit-Euler stability: with `epsilon = 0.02`, `D = 1.0`, and
/// the 9-point Laplacian the stiff reaction term stays finite over thousands of
/// steps at `dt = 0.02` (verified by [`tests::values_finite_over_long_run`]).
const DEFAULT_DT: f64 = 0.02;

impl Default for ExcitableParams {
    fn default() -> Self {
        Self {
            a: DEFAULT_A,
            b: DEFAULT_B,
            epsilon: DEFAULT_EPSILON,
            diffusion: DEFAULT_DIFFUSION,
            dt: DEFAULT_DT,
        }
    }
}

/// Simulation parameters for the Barkley excitable-media model.
///
/// Bundles the five tunable constants that control wave behavior. Use
/// [`Default`] for the excitable regime that reliably forms spiral waves
/// (a = 0.75, b = 0.02, epsilon = 0.02).
///
/// # Interesting combinations
///
/// - Default (`a = 0.75`, `b = 0.02`, `epsilon = 0.02`): crisp rotating spirals.
/// - Lower `epsilon` (e.g. 0.01): thinner, faster wavefronts (reduce `dt` if it
///   destabilizes).
/// - Higher `b` (e.g. 0.05): raises the excitation threshold, sparser activity.
#[derive(Debug, Clone, Copy)]
pub struct ExcitableParams {
    /// Excitability parameter `a`: scales the cubic nullcline; larger values
    /// widen the excited plateau.
    pub a: f64,
    /// Threshold offset `b`: raises the excitation threshold `(v + b) / a`.
    pub b: f64,
    /// Timescale separation `epsilon`: ratio of recovery to activation speed.
    /// Smaller values give sharper, stiffer wavefronts.
    pub epsilon: f64,
    /// Diffusion coefficient `D` applied to the activator `u` only.
    pub diffusion: f64,
    /// Time step per `step()` call (kept small for explicit-Euler stability).
    pub dt: f64,
}

impl ExcitableParams {
    /// Extracts parameters from a JSON object, falling back to defaults.
    ///
    /// `epsilon` is floored at a tiny positive value so the `1/epsilon`
    /// reaction term can never divide by zero.
    pub fn from_json(params: &Value) -> Self {
        Self {
            a: param_f64(params, "a", DEFAULT_A),
            b: param_f64(params, "b", DEFAULT_B),
            epsilon: param_f64(params, "epsilon", DEFAULT_EPSILON).max(1e-6),
            diffusion: param_f64(params, "diffusion", DEFAULT_DIFFUSION),
            dt: param_f64(params, "dt", DEFAULT_DT),
        }
    }
}

/// Barkley excitable-media engine.
///
/// A fast activator `u` and slow recovery variable `v` are coupled so that a
/// broken wavefront curls into a self-sustaining rotating spiral. Only `u`
/// diffuses (via a 9-point Laplacian); `v` is purely local. Both fields are
/// clamped to [0, 1] each step to keep the explicit-Euler integration bounded
/// and the exposed [`field`](Engine::field) renderable.
pub struct Excitable {
    u: Field,
    v: Field,
    params: ExcitableParams,
}

impl Excitable {
    /// Creates a new excitable-media engine.
    ///
    /// Both fields start at rest (`u = v = 0`), then `seed`-driven broken
    /// wavefronts are stamped in (see [`seed_broken_wavefronts`]) to nucleate
    /// spirals. The number of nuclei scales with grid area.
    ///
    /// Returns `EngineError::InvalidDimensions` if width or height is zero.
    pub fn new(
        width: usize,
        height: usize,
        seed: u64,
        params: ExcitableParams,
    ) -> Result<Self, EngineError> {
        let mut u = Field::new(width, height)?;
        let mut v = Field::new(width, height)?;
        let mut rng = Xorshift64::new(seed);
        seed_broken_wavefronts(&mut u, &mut v, &mut rng, width, height, params.a);
        Ok(Self { u, v, params })
    }

    /// Creates an excitable-media engine from a JSON params object.
    ///
    /// Extracts `a`, `b`, `epsilon`, `diffusion`, and `dt` from the JSON,
    /// falling back to defaults for missing keys.
    pub fn from_json(
        width: usize,
        height: usize,
        seed: u64,
        json_params: &Value,
    ) -> Result<Self, EngineError> {
        Self::new(width, height, seed, ExcitableParams::from_json(json_params))
    }

    /// Read-only access to the activator field `u`.
    pub fn u_field(&self) -> &Field {
        &self.u
    }

    /// Read-only access to the recovery field `v`.
    pub fn v_field(&self) -> &Field {
        &self.v
    }

    /// Current excitability parameter `a`.
    pub fn a(&self) -> f64 {
        self.params.a
    }

    /// Current threshold offset `b`.
    pub fn b(&self) -> f64 {
        self.params.b
    }
}

impl Engine for Excitable {
    fn step(&mut self) -> Result<(), EngineError> {
        let w = self.u.width();
        let h = self.u.height();
        let u_data = self.u.data();
        let v_data = self.v.data();

        let len = w * h;
        let mut u_next = vec![0.0_f64; len];
        let mut v_next = vec![0.0_f64; len];

        let a = self.params.a;
        let b = self.params.b;
        let inv_eps = 1.0 / self.params.epsilon;
        let d = self.params.diffusion;
        let dt = self.params.dt;

        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let u = u_data[idx];
                let v = v_data[idx];

                let lap_u = laplacian_9pt(u_data, x, y, w, h);

                // Barkley reaction kinetics. The (1/epsilon) factor makes the
                // activator dynamics fast and stiff; the threshold (v+b)/a sets
                // the firing point relative to the recovery state.
                let reaction = inv_eps * u * (1.0 - u) * (u - (v + b) / a);
                let nu = (u + dt * (reaction + d * lap_u)).clamp(0.0, 1.0);
                let nv = v + dt * (u - v);

                // Guard against any non-finite excursion: keep the previous
                // value so a single stiff blow-up cannot poison the whole grid.
                u_next[idx] = if nu.is_finite() { nu } else { u };
                v_next[idx] = if nv.is_finite() { nv.clamp(0.0, 1.0) } else { v };
            }
        }

        self.u.data_mut().copy_from_slice(&u_next);
        self.v.data_mut().copy_from_slice(&v_next);

        Ok(())
    }

    fn field(&self) -> &Field {
        // `u` is clamped to [0, 1] in-place each step, so it is render-ready.
        &self.u
    }

    fn params(&self) -> Value {
        json!({
            "a": self.params.a,
            "b": self.params.b,
            "epsilon": self.params.epsilon,
            "diffusion": self.params.diffusion,
            "dt": self.params.dt,
        })
    }

    fn param_schema(&self) -> Value {
        json!({
            "a": {
                "type": "number",
                "default": DEFAULT_A,
                "min": 0.1,
                "max": 1.5,
                "description": "Excitability parameter a: scales the cubic nullcline (wider excited plateau for larger a)"
            },
            "b": {
                "type": "number",
                "default": DEFAULT_B,
                "min": 0.0,
                "max": 0.3,
                "description": "Threshold offset b: raises the excitation threshold (v + b) / a"
            },
            "epsilon": {
                "type": "number",
                "default": DEFAULT_EPSILON,
                "min": 0.005,
                "max": 0.2,
                "description": "Timescale separation: smaller values give sharper, stiffer wavefronts"
            },
            "diffusion": {
                "type": "number",
                "default": DEFAULT_DIFFUSION,
                "min": 0.0,
                "max": 2.0,
                "description": "Diffusion coefficient D for the activator u"
            },
            "dt": {
                "type": "number",
                "default": DEFAULT_DT,
                "min": 0.0,
                "max": 0.05,
                "description": "Time step per step() call (small for explicit-Euler stability)"
            }
        })
    }
}

/// Seeds broken wavefronts that nucleate self-sustaining rotating spirals.
///
/// The trap to avoid: exciting a *half-plane* of `u = 1` floods most of the
/// torus, and since the Barkley reaction `u(1-u)(…)` vanishes at `u = 1`, the
/// whole excited mass simply recovers in sync and decays to rest — a single
/// flash, not a spiral. Instead we excite a **thin vertical strip** (a
/// localized wavefront) and lay a **refractory wake** behind only its lower
/// half. The upper half is free to propagate while the lower half is blocked,
/// so the wavefront has a free end that curls into a spiral tip and rotates
/// indefinitely. Centers come from the PRNG, so the layout is seed-dependent
/// and bit-reproducible.
///
/// Nucleus count is kept small (1-2) so spirals fill the domain rather than
/// colliding and pair-annihilating.
fn seed_broken_wavefronts(
    u: &mut Field,
    v: &mut Field,
    rng: &mut Xorshift64,
    width: usize,
    height: usize,
    a: f64,
) {
    let nuclei = ((width * height) as f64 / 65536.0).ceil().clamp(1.0, 2.0) as usize;
    let refractory = a * 0.5;
    // A thin strip (~3% of the smaller dimension, min 3 cells) is a localized
    // wavefront rather than a flood.
    let strip = ((width.min(height) as f64) * 0.03).round().max(3.0) as isize;

    (0..nuclei).for_each(|_| {
        let cx = rng.next_usize(width) as isize;
        let cy = rng.next_usize(height) as isize;

        (0..height as isize).for_each(|gy| {
            (0..width as isize).for_each(|gx| {
                let dx = toroidal_delta(gx, cx, width);
                let dy = toroidal_delta(gy, cy, height);

                // Excited wavefront: a thin strip just ahead of the center.
                if (0..strip).contains(&dx) {
                    u.set(gx, gy, 1.0);
                }
                // Refractory wake behind the strip, lower half only — this
                // breaks the front so its free end (at the y = cy line) curls.
                if dx < 0 && dy < 0 {
                    v.set(gx, gy, refractory);
                }
            });
        });
    });
}

/// Signed toroidal offset of `coord` relative to `center` in `[-size/2, size/2)`.
fn toroidal_delta(coord: isize, center: isize, size: usize) -> isize {
    let s = size as isize;
    let raw = (coord - center).rem_euclid(s);
    if raw > s / 2 {
        raw - s
    } else {
        raw
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
/// Operates on the raw data slice with explicit toroidal coordinate wrapping
/// for performance (avoids `Field::get()` overhead in the hot loop).
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
    fn default_params() -> ExcitableParams {
        ExcitableParams::default()
    }

    /// Helper: construct with default params.
    fn ex(width: usize, height: usize, seed: u64) -> Excitable {
        Excitable::new(width, height, seed, default_params()).unwrap()
    }

    /// Standard-deviation of a slice (population), used for non-uniformity checks.
    fn std_dev(data: &[f64]) -> f64 {
        let n = data.len() as f64;
        let mean = data.iter().sum::<f64>() / n;
        let var = data.iter().map(|&v| (v - mean).powi(2)).sum::<f64>() / n;
        var.sqrt()
    }

    // ---- Construction tests ----

    #[test]
    fn new_creates_engine_with_correct_dimensions() {
        let e = ex(64, 32, 42);
        assert_eq!(e.u_field().width(), 64);
        assert_eq!(e.u_field().height(), 32);
        assert_eq!(e.v_field().width(), 64);
        assert_eq!(e.v_field().height(), 32);
    }

    #[test]
    fn field_has_correct_element_count() {
        let e = ex(48, 24, 7);
        assert_eq!(e.field().data().len(), 48 * 24);
    }

    #[test]
    fn new_with_zero_dimensions_returns_error() {
        assert!(Excitable::new(0, 10, 42, default_params()).is_err());
        assert!(Excitable::new(10, 0, 42, default_params()).is_err());
    }

    #[test]
    fn new_seeds_nontrivial_initial_state() {
        let e = ex(128, 128, 42);
        let u = e.u_field().data();
        let v = e.v_field().data();
        let u_excited = u.iter().filter(|&&x| x > 0.5).count();
        let v_refractory = v.iter().filter(|&&x| x > 0.0).count();
        assert!(u_excited > 0, "u should have an excited region");
        assert!(u_excited < u.len(), "u should not be uniformly excited");
        assert!(v_refractory > 0, "v should have a refractory region");
    }

    #[test]
    fn from_json_uses_defaults_for_empty_json() {
        let e = Excitable::from_json(32, 32, 42, &json!({})).unwrap();
        assert!((e.a() - DEFAULT_A).abs() < f64::EPSILON);
        assert!((e.b() - DEFAULT_B).abs() < f64::EPSILON);
        let p = e.params();
        assert!((p["epsilon"].as_f64().unwrap() - DEFAULT_EPSILON).abs() < f64::EPSILON);
        assert!((p["diffusion"].as_f64().unwrap() - DEFAULT_DIFFUSION).abs() < f64::EPSILON);
        assert!((p["dt"].as_f64().unwrap() - DEFAULT_DT).abs() < f64::EPSILON);
    }

    #[test]
    fn from_json_extracts_custom_values() {
        let params = json!({
            "a": 0.8,
            "b": 0.05,
            "epsilon": 0.03,
            "diffusion": 0.7,
            "dt": 0.01,
        });
        let e = Excitable::from_json(32, 32, 42, &params).unwrap();
        assert!((e.a() - 0.8).abs() < f64::EPSILON);
        assert!((e.b() - 0.05).abs() < f64::EPSILON);
        let p = e.params();
        assert!((p["epsilon"].as_f64().unwrap() - 0.03).abs() < f64::EPSILON);
        assert!((p["diffusion"].as_f64().unwrap() - 0.7).abs() < f64::EPSILON);
        assert!((p["dt"].as_f64().unwrap() - 0.01).abs() < f64::EPSILON);
    }

    #[test]
    fn from_json_floors_epsilon_to_avoid_division_by_zero() {
        let e = Excitable::from_json(16, 16, 42, &json!({ "epsilon": 0.0 })).unwrap();
        let eps = e.params()["epsilon"].as_f64().unwrap();
        assert!(eps > 0.0, "epsilon must be floored above zero, got {eps}");
    }

    #[test]
    fn params_returns_current_values() {
        let params = ExcitableParams {
            a: 0.7,
            b: 0.03,
            epsilon: 0.025,
            diffusion: 0.9,
            dt: 0.015,
        };
        let e = Excitable::new(16, 16, 42, params).unwrap();
        let p = e.params();
        assert!((p["a"].as_f64().unwrap() - 0.7).abs() < f64::EPSILON);
        assert!((p["b"].as_f64().unwrap() - 0.03).abs() < f64::EPSILON);
        assert!((p["epsilon"].as_f64().unwrap() - 0.025).abs() < f64::EPSILON);
        assert!((p["diffusion"].as_f64().unwrap() - 0.9).abs() < f64::EPSILON);
        assert!((p["dt"].as_f64().unwrap() - 0.015).abs() < f64::EPSILON);
    }

    #[test]
    fn param_schema_has_all_five_parameters() {
        let e = ex(16, 16, 42);
        let schema = e.param_schema();
        for key in &["a", "b", "epsilon", "diffusion", "dt"] {
            assert!(schema.get(key).is_some(), "schema missing parameter: {key}");
            assert!(schema[key].get("type").is_some(), "{key} missing 'type'");
            assert!(
                schema[key].get("default").is_some(),
                "{key} missing 'default'"
            );
            assert!(
                schema[key].get("min").is_some(),
                "{key} missing 'min'"
            );
            assert!(
                schema[key].get("max").is_some(),
                "{key} missing 'max'"
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
        let a = ex(64, 64, 12345);
        let b = ex(64, 64, 12345);
        assert!(a
            .u_field()
            .data()
            .iter()
            .zip(b.u_field().data().iter())
            .all(|(ua, ub)| ua.to_bits() == ub.to_bits()));
        assert!(a
            .v_field()
            .data()
            .iter()
            .zip(b.v_field().data().iter())
            .all(|(va, vb)| va.to_bits() == vb.to_bits()));
    }

    #[test]
    fn same_seed_identical_after_100_steps() {
        let mut a = ex(32, 32, 42);
        let mut b = ex(32, 32, 42);
        for _ in 0..100 {
            a.step().unwrap();
            b.step().unwrap();
        }
        assert!(a
            .u_field()
            .data()
            .iter()
            .zip(b.u_field().data().iter())
            .all(|(ua, ub)| ua.to_bits() == ub.to_bits()));
        assert!(a
            .v_field()
            .data()
            .iter()
            .zip(b.v_field().data().iter())
            .all(|(va, vb)| va.to_bits() == vb.to_bits()));
    }

    #[test]
    fn different_seed_different_state() {
        let a = ex(64, 64, 1);
        let b = ex(64, 64, 2);
        assert!(a
            .u_field()
            .data()
            .iter()
            .zip(b.u_field().data().iter())
            .any(|(ua, ub)| ua.to_bits() != ub.to_bits()));
    }

    // ---- Step correctness tests ----

    #[test]
    fn step_returns_ok() {
        let mut e = ex(16, 16, 42);
        assert!(e.step().is_ok());
    }

    #[test]
    fn rest_state_stays_at_rest() {
        // With u = v = 0 everywhere, the medium is at the quiescent fixed point
        // and must not spontaneously fire.
        let mut e = ex(16, 16, 42);
        e.u.data_mut().fill(0.0);
        e.v.data_mut().fill(0.0);
        for _ in 0..50 {
            e.step().unwrap();
        }
        assert!(
            e.u_field().data().iter().all(|&u| u.abs() < 1e-12),
            "u should stay at rest"
        );
        assert!(
            e.v_field().data().iter().all(|&v| v.abs() < 1e-12),
            "v should stay at rest"
        );
    }

    #[test]
    fn field_values_in_unit_interval_after_stepping() {
        let mut e = ex(48, 48, 42);
        for _ in 0..400 {
            e.step().unwrap();
        }
        assert!(e
            .field()
            .data()
            .iter()
            .all(|&u| (0.0..=1.0).contains(&u)));
    }

    #[test]
    fn values_finite_over_long_run() {
        // The stiff (1/epsilon) reaction term must not blow up with the shipped
        // default dt. Run well past 1000 steps and assert nothing escapes to
        // NaN/inf and u stays bounded in [0, 1].
        let mut e = ex(48, 48, 42);
        for _ in 0..2000 {
            e.step().unwrap();
        }
        for &u in e.u_field().data() {
            assert!(u.is_finite(), "u became non-finite: {u}");
            assert!((0.0..=1.0).contains(&u), "u left [0,1]: {u}");
        }
        for &v in e.v_field().data() {
            assert!(v.is_finite(), "v became non-finite: {v}");
        }
    }

    #[test]
    fn field_changes_after_stepping() {
        let mut e = ex(64, 64, 42);
        let before: Vec<u64> = e.field().data().iter().map(|x| x.to_bits()).collect();
        for _ in 0..50 {
            e.step().unwrap();
        }
        let after: Vec<u64> = e.field().data().iter().map(|x| x.to_bits()).collect();
        assert_ne!(before, after, "field should evolve after stepping");
    }

    #[test]
    fn field_is_nonuniform_after_evolution() {
        // The point of the model: after a few hundred steps the medium shows
        // rotating wave structure, not a flat decayed state.
        let mut e = ex(96, 96, 42);
        for _ in 0..500 {
            e.step().unwrap();
        }
        let sd = std_dev(e.field().data());
        assert!(
            sd > 0.05,
            "field should be non-uniform (spiral structure), std-dev was {sd}"
        );
    }

    // ---- Laplacian tests ----

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
        // Toroidal: cell at the opposite (wrapped) edge also sees the spike.
        let lap_wrap = laplacian_9pt(&data, w - 1, 0, w, h);
        assert!(
            lap_wrap > 0.0,
            "Cell across the toroidal seam should feel the spike, got {lap_wrap}"
        );
    }

    #[test]
    fn zero_dt_produces_no_change() {
        let params = ExcitableParams {
            dt: 0.0,
            ..default_params()
        };
        let mut e = Excitable::new(32, 32, 42, params).unwrap();
        let u_before: Vec<u64> = e.u_field().data().iter().map(|v| v.to_bits()).collect();
        let v_before: Vec<u64> = e.v_field().data().iter().map(|v| v.to_bits()).collect();
        e.step().unwrap();
        let u_after: Vec<u64> = e.u_field().data().iter().map(|v| v.to_bits()).collect();
        let v_after: Vec<u64> = e.v_field().data().iter().map(|v| v.to_bits()).collect();
        assert_eq!(u_before, u_after, "u should not change with dt=0");
        assert_eq!(v_before, v_after, "v should not change with dt=0");
    }

    // ---- Trait compliance tests ----

    #[test]
    fn field_returns_u() {
        let e = ex(16, 16, 42);
        let has_excited = e.field().data().iter().any(|&u| u > 0.5);
        let has_rest = e.field().data().contains(&0.0);
        assert!(
            has_excited && has_rest,
            "field() should return u (mix of excited and rest cells)"
        );
    }

    #[test]
    fn hue_field_returns_none() {
        let e = ex(16, 16, 42);
        assert!(e.hue_field().is_none());
    }

    #[test]
    fn engine_is_object_safe() {
        let e = ex(16, 16, 42);
        let boxed: Box<dyn Engine> = Box::new(e);
        assert_eq!(boxed.field().width(), 16);
    }

    // ---- Property-based tests ----

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        fn dimension() -> impl Strategy<Value = usize> {
            4_usize..=32
        }

        fn sim_params() -> impl Strategy<Value = ExcitableParams> {
            (
                0.3_f64..=1.2,   // a
                0.0_f64..=0.1,   // b
                0.01_f64..=0.1,  // epsilon
                0.1_f64..=1.5,   // diffusion
                0.005_f64..=0.02, // dt
            )
                .prop_map(|(a, b, epsilon, diffusion, dt)| ExcitableParams {
                    a,
                    b,
                    epsilon,
                    diffusion,
                    dt,
                })
        }

        proptest! {
            #[test]
            fn field_always_in_unit_interval(
                w in dimension(),
                h in dimension(),
                seed: u64,
                p in sim_params(),
            ) {
                let mut e = Excitable::new(w, h, seed, p).unwrap();
                for _ in 0..20 {
                    e.step().unwrap();
                }
                for &u in e.u_field().data() {
                    prop_assert!((0.0..=1.0).contains(&u), "u out of range: {u}");
                }
            }

            #[test]
            fn deterministic_across_instances(
                w in dimension(),
                h in dimension(),
                seed: u64,
            ) {
                let p = ExcitableParams::default();
                let mut a = Excitable::new(w, h, seed, p).unwrap();
                let mut b = Excitable::new(w, h, seed, p).unwrap();
                for _ in 0..20 {
                    a.step().unwrap();
                    b.step().unwrap();
                }
                for (ua, ub) in a.u_field().data().iter().zip(b.u_field().data().iter()) {
                    prop_assert_eq!(ua.to_bits(), ub.to_bits());
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
                let mut e = Excitable::new(w, h, seed, p).unwrap();
                for _ in 0..20 {
                    e.step().unwrap();
                }
                for &u in e.u_field().data() {
                    prop_assert!(u.is_finite(), "non-finite u: {u}");
                }
                for &v in e.v_field().data() {
                    prop_assert!(v.is_finite(), "non-finite v: {v}");
                }
            }

            #[test]
            fn rest_state_stays_at_rest_for_any_params(
                w in dimension(),
                h in dimension(),
                seed: u64,
                p in sim_params(),
            ) {
                let mut e = Excitable::new(w, h, seed, p).unwrap();
                e.u.data_mut().fill(0.0);
                e.v.data_mut().fill(0.0);
                for _ in 0..20 {
                    e.step().unwrap();
                }
                for &u in e.u_field().data() {
                    prop_assert!(u.abs() < 1e-9, "u should stay at rest, got {u}");
                }
                for &v in e.v_field().data() {
                    prop_assert!(v.abs() < 1e-9, "v should stay at rest, got {v}");
                }
            }
        }
    }
}
