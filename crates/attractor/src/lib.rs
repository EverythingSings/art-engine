#![deny(unsafe_code)]
//! Strange-attractor engine.
//!
//! Plots a single trajectory of a chosen 3D ODE (Lorenz / Rössler / Halvorsen)
//! or 2D iterated map (Pickover) onto the canvas. Each `step()` advances the
//! trajectory by `iterations_per_step` micro-iterations, depositing each
//! visited 2D projection cell onto an internal density field. With non-zero
//! `trail_decay` the field accumulates over time, so the attractor's
//! signature shape emerges as more of the orbit is sampled.
//!
//! Projection: for ODE systems the (x, y) plane is shown by default;
//! Pickover is already 2D. Use `projection` parameter (`"xy" | "xz" | "yz"`)
//! to pick a different planar slice for ODEs.
//!
//! # JSON parameters
//!
//! ```json
//! {
//!   "kind": "lorenz",
//!   "iterations_per_step": 600,
//!   "dt": 0.005,
//!   "scale": 0.018,
//!   "center_x": 0.5,
//!   "center_y": 0.5,
//!   "trail_decay": 0.985,
//!   "field_gamma": 0.5,
//!   "splat_radius": 1.0,
//!   "projection": "xz"
//! }
//! ```

use art_engine_core::error::EngineError;
use art_engine_core::field::Field;
use art_engine_core::params::{param_f64, param_string, param_usize};
use art_engine_core::Engine;
use serde_json::{json, Value};

/// Default attractor type (case-insensitive).
const DEFAULT_KIND: &str = "lorenz";
/// Default iterations per `step()` call.
const DEFAULT_ITERATIONS_PER_STEP: usize = 500;
/// Default integration step size for ODE attractors.
const DEFAULT_DT: f64 = 0.005;
/// Default canvas-units-per-attractor-unit scale factor.
const DEFAULT_SCALE: f64 = 0.018;
/// Default canvas-coord center (where attractor origin lands).
const DEFAULT_CENTER_X: f64 = 0.5;
const DEFAULT_CENTER_Y: f64 = 0.5;
/// Default trail decay (0 = fresh per step, near-1 = long persistence).
const DEFAULT_TRAIL_DECAY: f64 = 0.985;
/// Default gamma for field shaping.
const DEFAULT_FIELD_GAMMA: f64 = 0.5;
/// Default splat radius (in pixels) per trajectory sample.
const DEFAULT_SPLAT_RADIUS: f64 = 1.0;
/// Default projection plane (which two of x, y, z to show).
const DEFAULT_PROJECTION: &str = "xz";
/// Hard cap on iterations per step (prevents runaway CPU).
const MAX_ITERATIONS_PER_STEP: usize = 100_000;

/// The recognized attractor variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttractorKind {
    /// Lorenz system (1963). Classic butterfly.
    Lorenz,
    /// Rössler system (1976). Single-scroll spiral.
    Rossler,
    /// Halvorsen system (cyclic-symmetric). Three-fold rotational symmetry.
    Halvorsen,
    /// Pickover 2D iterated map. Quasi-periodic, high-detail.
    Pickover,
}

impl AttractorKind {
    fn from_str_or_default(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "lorenz" => Self::Lorenz,
            "rossler" | "rössler" => Self::Rossler,
            "halvorsen" => Self::Halvorsen,
            "pickover" | "de_jong" | "dejong" => Self::Pickover,
            _ => Self::Lorenz,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Lorenz => "lorenz",
            Self::Rossler => "rossler",
            Self::Halvorsen => "halvorsen",
            Self::Pickover => "pickover",
        }
    }

    /// Sensible starting point for a fresh trajectory.
    fn initial_state(&self) -> [f64; 3] {
        match self {
            Self::Lorenz => [0.1, 0.0, 0.0],
            Self::Rossler => [1.0, 0.0, 0.0],
            Self::Halvorsen => [0.5, 0.0, 0.0],
            Self::Pickover => [0.1, 0.1, 0.0],
        }
    }
}

/// Tunable parameters.
#[derive(Debug, Clone)]
pub struct AttractorParams {
    pub kind: AttractorKind,
    pub iterations_per_step: usize,
    pub dt: f64,
    pub scale: f64,
    pub center_x: f64,
    pub center_y: f64,
    pub trail_decay: f64,
    pub field_gamma: f64,
    pub splat_radius: f64,
    pub projection: Projection,
}

impl Default for AttractorParams {
    fn default() -> Self {
        Self {
            kind: AttractorKind::Lorenz,
            iterations_per_step: DEFAULT_ITERATIONS_PER_STEP,
            dt: DEFAULT_DT,
            scale: DEFAULT_SCALE,
            center_x: DEFAULT_CENTER_X,
            center_y: DEFAULT_CENTER_Y,
            trail_decay: DEFAULT_TRAIL_DECAY,
            field_gamma: DEFAULT_FIELD_GAMMA,
            splat_radius: DEFAULT_SPLAT_RADIUS,
            projection: Projection::Xz,
        }
    }
}

impl AttractorParams {
    pub fn from_json(params: &Value) -> Self {
        let kind = AttractorKind::from_str_or_default(&param_string(params, "kind", DEFAULT_KIND));
        let iterations_per_step =
            param_usize(params, "iterations_per_step", DEFAULT_ITERATIONS_PER_STEP)
                .clamp(1, MAX_ITERATIONS_PER_STEP);
        let dt = param_f64(params, "dt", DEFAULT_DT).max(1e-6);
        let scale = param_f64(params, "scale", DEFAULT_SCALE).max(1e-6);
        let center_x = param_f64(params, "center_x", DEFAULT_CENTER_X);
        let center_y = param_f64(params, "center_y", DEFAULT_CENTER_Y);
        let trail_decay = param_f64(params, "trail_decay", DEFAULT_TRAIL_DECAY).clamp(0.0, 0.999);
        let field_gamma = param_f64(params, "field_gamma", DEFAULT_FIELD_GAMMA).clamp(0.05, 5.0);
        let splat_radius = param_f64(params, "splat_radius", DEFAULT_SPLAT_RADIUS).clamp(0.0, 16.0);
        let projection = Projection::from_str_or_default(&param_string(
            params,
            "projection",
            DEFAULT_PROJECTION,
        ));

        Self {
            kind,
            iterations_per_step,
            dt,
            scale,
            center_x,
            center_y,
            trail_decay,
            field_gamma,
            splat_radius,
            projection,
        }
    }
}

/// Which two of the three (x, y, z) coordinates form the projected plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Projection {
    Xy,
    Xz,
    Yz,
}

impl Projection {
    fn from_str_or_default(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "xy" => Self::Xy,
            "xz" => Self::Xz,
            "yz" => Self::Yz,
            _ => Self::Xz,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Xy => "xy",
            Self::Xz => "xz",
            Self::Yz => "yz",
        }
    }

    fn project(&self, state: [f64; 3]) -> (f64, f64) {
        match self {
            Self::Xy => (state[0], state[1]),
            Self::Xz => (state[0], state[2]),
            Self::Yz => (state[1], state[2]),
        }
    }
}

/// The attractor engine.
pub struct Attractor {
    params: AttractorParams,
    width: usize,
    height: usize,
    state: [f64; 3],
    field: Field,
    /// Density buffer reused each step (avoids per-step allocation).
    scratch: Vec<f64>,
}

impl Attractor {
    pub fn new(width: usize, height: usize, params: AttractorParams) -> Result<Self, EngineError> {
        let field = Field::new(width, height)?;
        let len = width
            .checked_mul(height)
            .ok_or(EngineError::InvalidDimensions)?;
        let scratch = vec![0.0_f64; len];
        let state = params.kind.initial_state();
        Ok(Self {
            params,
            width,
            height,
            state,
            field,
            scratch,
        })
    }

    /// Constructs from JSON. `seed` is unused (attractors are deterministic
    /// by their initial state and parameters), but accepted for API uniformity
    /// with the rest of the engine registry.
    pub fn from_json(
        width: usize,
        height: usize,
        _seed: u64,
        params: &Value,
    ) -> Result<Self, EngineError> {
        Self::new(width, height, AttractorParams::from_json(params))
    }
}

impl Engine for Attractor {
    fn step(&mut self) -> Result<(), EngineError> {
        // Reset scratch density buffer.
        for v in self.scratch.iter_mut() {
            *v = 0.0;
        }

        // Advance trajectory and accumulate density into scratch.
        let mut max_density = 0.0_f64;
        let dt = self.params.dt;
        for _ in 0..self.params.iterations_per_step {
            self.state = advance(&self.params.kind, self.state, dt);
            // Bail out if the integrator went unstable.
            if !self.state.iter().all(|v| v.is_finite()) {
                self.state = self.params.kind.initial_state();
                continue;
            }

            let (px, py) = self.params.projection.project(self.state);
            let cx = self.params.center_x + px * self.params.scale;
            let cy = self.params.center_y + py * self.params.scale;

            // Skip points that fall outside the canvas.
            if !(0.0..=1.0).contains(&cx) || !(0.0..=1.0).contains(&cy) {
                continue;
            }

            splat(
                &mut self.scratch,
                self.width,
                self.height,
                cx,
                cy,
                self.params.splat_radius,
                &mut max_density,
            );
        }

        // Normalize by the per-step max so any visited cell maxes out.
        if max_density > 0.0 {
            for v in self.scratch.iter_mut() {
                *v /= max_density;
            }
        }

        // Decay-blend into the persistent field, with gamma applied to the
        // *new* contribution so accumulated trails follow the same brightness
        // curve regardless of decay setting.
        let gamma = self.params.field_gamma;
        let decay = self.params.trail_decay;
        for (dst, &src) in self.field.data_mut().iter_mut().zip(self.scratch.iter()) {
            let shaped = if src > 0.0 { src.powf(gamma) } else { 0.0 };
            let v = *dst * decay + shaped;
            *dst = if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
        Ok(())
    }

    fn field(&self) -> &Field {
        &self.field
    }

    fn params(&self) -> Value {
        json!({
            "kind": self.params.kind.name(),
            "iterations_per_step": self.params.iterations_per_step,
            "dt": self.params.dt,
            "scale": self.params.scale,
            "center_x": self.params.center_x,
            "center_y": self.params.center_y,
            "trail_decay": self.params.trail_decay,
            "field_gamma": self.params.field_gamma,
            "splat_radius": self.params.splat_radius,
            "projection": self.params.projection.name(),
        })
    }

    fn param_schema(&self) -> Value {
        json!({
            "kind": {
                "type": "string",
                "default": DEFAULT_KIND,
                "enum": ["lorenz", "rossler", "halvorsen", "pickover"],
                "description": "Which attractor system to integrate"
            },
            "iterations_per_step": {
                "type": "integer",
                "default": DEFAULT_ITERATIONS_PER_STEP,
                "min": 1,
                "max": MAX_ITERATIONS_PER_STEP,
                "description": "Number of micro-iterations per Engine step"
            },
            "dt": {
                "type": "number",
                "default": DEFAULT_DT,
                "min": 1e-6,
                "description": "Integration step size (ODE attractors only)"
            },
            "scale": {
                "type": "number",
                "default": DEFAULT_SCALE,
                "min": 1e-6,
                "description": "Canvas-units per attractor-unit"
            },
            "center_x": {
                "type": "number",
                "default": DEFAULT_CENTER_X,
                "description": "Canvas X for attractor origin"
            },
            "center_y": {
                "type": "number",
                "default": DEFAULT_CENTER_Y,
                "description": "Canvas Y for attractor origin"
            },
            "trail_decay": {
                "type": "number",
                "default": DEFAULT_TRAIL_DECAY,
                "min": 0.0,
                "max": 0.999,
                "description": "Field persistence per step"
            },
            "field_gamma": {
                "type": "number",
                "default": DEFAULT_FIELD_GAMMA,
                "min": 0.05,
                "max": 5.0,
                "description": "Gamma applied to per-step density before accumulation"
            },
            "splat_radius": {
                "type": "number",
                "default": DEFAULT_SPLAT_RADIUS,
                "min": 0.0,
                "max": 16.0,
                "description": "Per-sample deposit radius (px)"
            },
            "projection": {
                "type": "string",
                "default": DEFAULT_PROJECTION,
                "enum": ["xy", "xz", "yz"],
                "description": "Which planar slice of the 3D state to show"
            }
        })
    }
}

/// Advances the chosen attractor by one micro-iteration.
///
/// ODE systems use a 4th-order Runge-Kutta integrator for stability at
/// reasonable `dt` values; Pickover is an iterated map and ignores `dt`.
fn advance(kind: &AttractorKind, state: [f64; 3], dt: f64) -> [f64; 3] {
    match kind {
        AttractorKind::Lorenz => rk4(state, dt, lorenz_deriv),
        AttractorKind::Rossler => rk4(state, dt, rossler_deriv),
        AttractorKind::Halvorsen => rk4(state, dt, halvorsen_deriv),
        AttractorKind::Pickover => pickover_step(state),
    }
}

/// Standard 4th-order Runge-Kutta integrator for a 3D autonomous ODE.
fn rk4(s: [f64; 3], dt: f64, f: fn([f64; 3]) -> [f64; 3]) -> [f64; 3] {
    let k1 = f(s);
    let s2 = vec_add(s, vec_scale(k1, dt * 0.5));
    let k2 = f(s2);
    let s3 = vec_add(s, vec_scale(k2, dt * 0.5));
    let k3 = f(s3);
    let s4 = vec_add(s, vec_scale(k3, dt));
    let k4 = f(s4);
    let combined = [
        (k1[0] + 2.0 * k2[0] + 2.0 * k3[0] + k4[0]) / 6.0,
        (k1[1] + 2.0 * k2[1] + 2.0 * k3[1] + k4[1]) / 6.0,
        (k1[2] + 2.0 * k2[2] + 2.0 * k3[2] + k4[2]) / 6.0,
    ];
    vec_add(s, vec_scale(combined, dt))
}

fn vec_add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn vec_scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

// -- Vector fields --

/// Lorenz: dx = sigma*(y-x), dy = x*(rho-z) - y, dz = x*y - beta*z.
fn lorenz_deriv(s: [f64; 3]) -> [f64; 3] {
    const SIGMA: f64 = 10.0;
    const RHO: f64 = 28.0;
    const BETA: f64 = 8.0 / 3.0;
    let (x, y, z) = (s[0], s[1], s[2]);
    [SIGMA * (y - x), x * (RHO - z) - y, x * y - BETA * z]
}

/// Rössler: dx = -y - z, dy = x + a*y, dz = b + z*(x - c).
fn rossler_deriv(s: [f64; 3]) -> [f64; 3] {
    const A: f64 = 0.2;
    const B: f64 = 0.2;
    const C: f64 = 5.7;
    let (x, y, z) = (s[0], s[1], s[2]);
    [-y - z, x + A * y, B + z * (x - C)]
}

/// Halvorsen cyclic-symmetric: dx = -a*x - 4*y - 4*z - y^2 (and cyclic).
fn halvorsen_deriv(s: [f64; 3]) -> [f64; 3] {
    const A: f64 = 1.4;
    let (x, y, z) = (s[0], s[1], s[2]);
    [
        -A * x - 4.0 * y - 4.0 * z - y * y,
        -A * y - 4.0 * z - 4.0 * x - z * z,
        -A * z - 4.0 * x - 4.0 * y - x * x,
    ]
}

/// Pickover 2D iterated map: x' = sin(a*y) - cos(b*x), y' = sin(c*x) - cos(d*y).
/// z is unused (kept at 0 so the 3-vector state shape is uniform).
fn pickover_step(s: [f64; 3]) -> [f64; 3] {
    const A: f64 = 2.01;
    const B: f64 = -2.53;
    const C: f64 = 1.61;
    const D: f64 = -0.33;
    let (x, y) = (s[0], s[1]);
    let nx = (A * y).sin() - (B * x).cos();
    let ny = (C * x).sin() - (D * y).cos();
    [nx, ny, 0.0]
}

/// Soft circular splat onto the density buffer at normalized canvas
/// coordinates `(cx, cy)`. Updates `max_val` if any cell exceeds it.
fn splat(
    buffer: &mut [f64],
    width: usize,
    height: usize,
    cx: f64,
    cy: f64,
    radius: f64,
    max_val: &mut f64,
) {
    let r = radius.max(0.0);
    let r_int = r.ceil() as i32;
    let r_sq = (r * r).max(1.0);
    let w = width as f64;
    let h = height as f64;
    let wi = width as i32;
    let hi = height as i32;
    let cx_pix = cx * w;
    let cy_pix = cy * h;
    let cx_int = cx_pix as i32;
    let cy_int = cy_pix as i32;

    if r_int == 0 {
        if cx_int >= 0 && cx_int < wi && cy_int >= 0 && cy_int < hi {
            let idx = (cy_int as usize) * width + (cx_int as usize);
            buffer[idx] += 1.0;
            if buffer[idx] > *max_val {
                *max_val = buffer[idx];
            }
        }
        return;
    }

    for dy in -r_int..=r_int {
        let py = cy_int + dy;
        if py < 0 || py >= hi {
            continue;
        }
        for dx in -r_int..=r_int {
            let px = cx_int + dx;
            if px < 0 || px >= wi {
                continue;
            }
            let fx = px as f64 + 0.5 - cx_pix;
            let fy = py as f64 + 0.5 - cy_pix;
            let d2 = fx * fx + fy * fy;
            if d2 > r_sq {
                continue;
            }
            let weight = 1.0 - d2 / r_sq;
            let idx = (py as usize) * width + (px as usize);
            buffer[idx] += weight;
            if buffer[idx] > *max_val {
                *max_val = buffer[idx];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(w: usize, h: usize) -> Attractor {
        Attractor::new(w, h, AttractorParams::default()).unwrap()
    }

    // ---- Construction ----

    #[test]
    fn new_creates_field_with_correct_dims() {
        let e = a(64, 32);
        assert_eq!(e.field().width(), 64);
        assert_eq!(e.field().height(), 32);
    }

    #[test]
    fn new_with_zero_dimensions_returns_error() {
        assert!(Attractor::new(0, 16, AttractorParams::default()).is_err());
        assert!(Attractor::new(16, 0, AttractorParams::default()).is_err());
    }

    // ---- Step + field output ----

    #[test]
    fn lorenz_lights_up_field_within_a_few_steps() {
        let mut e = a(64, 64);
        for _ in 0..10 {
            e.step().unwrap();
        }
        let max = e.field().data().iter().copied().fold(0.0_f64, f64::max);
        assert!(
            max > 0.05,
            "Lorenz should produce visible density after 10 steps, got max={max}"
        );
    }

    #[test]
    fn field_values_in_unit_interval_for_each_kind() {
        for kind in ["lorenz", "rossler", "halvorsen", "pickover"] {
            // Pickover lives in roughly [-2, 2], so it needs a different scale.
            let scale = if kind == "pickover" { 0.2 } else { 0.018 };
            let params = json!({"kind": kind, "scale": scale, "iterations_per_step": 200});
            let mut e = Attractor::from_json(48, 48, 0, &params).unwrap();
            for _ in 0..15 {
                e.step().unwrap();
            }
            for &v in e.field().data() {
                assert!(
                    (0.0..=1.0).contains(&v) && !v.is_nan(),
                    "{kind}: out-of-range field value {v}"
                );
            }
        }
    }

    // ---- Determinism ----

    #[test]
    fn determinism_same_params() {
        let mut a1 = a(40, 40);
        let mut a2 = a(40, 40);
        for _ in 0..30 {
            a1.step().unwrap();
            a2.step().unwrap();
        }
        assert!(a1
            .field()
            .data()
            .iter()
            .zip(a2.field().data().iter())
            .all(|(va, vb)| va.to_bits() == vb.to_bits()));
    }

    #[test]
    fn different_kinds_produce_different_state() {
        let mut lor = Attractor::from_json(40, 40, 0, &json!({"kind": "lorenz"})).unwrap();
        let mut hal =
            Attractor::from_json(40, 40, 0, &json!({"kind": "halvorsen", "scale": 0.05})).unwrap();
        for _ in 0..30 {
            lor.step().unwrap();
            hal.step().unwrap();
        }
        assert!(lor
            .field()
            .data()
            .iter()
            .zip(hal.field().data().iter())
            .any(|(va, vb)| va.to_bits() != vb.to_bits()));
    }

    // ---- JSON ----

    #[test]
    fn from_json_default_kind_is_lorenz() {
        let e = Attractor::from_json(8, 8, 0, &json!({})).unwrap();
        assert_eq!(e.params.kind, AttractorKind::Lorenz);
    }

    #[test]
    fn from_json_unknown_kind_falls_back_to_lorenz() {
        let e = Attractor::from_json(8, 8, 0, &json!({"kind": "warp_drive"})).unwrap();
        assert_eq!(e.params.kind, AttractorKind::Lorenz);
    }

    #[test]
    fn from_json_recognizes_each_kind() {
        for (k, expected) in [
            ("lorenz", AttractorKind::Lorenz),
            ("rossler", AttractorKind::Rossler),
            ("halvorsen", AttractorKind::Halvorsen),
            ("pickover", AttractorKind::Pickover),
        ] {
            let e = Attractor::from_json(8, 8, 0, &json!({"kind": k})).unwrap();
            assert_eq!(e.params.kind, expected, "kind {k}");
        }
    }

    #[test]
    fn from_json_caps_iterations() {
        let e = Attractor::from_json(8, 8, 0, &json!({"iterations_per_step": 9_999_999})).unwrap();
        assert_eq!(e.params.iterations_per_step, MAX_ITERATIONS_PER_STEP);
    }

    #[test]
    fn from_json_clamps_trail_decay() {
        let high = Attractor::from_json(8, 8, 0, &json!({"trail_decay": 5.0})).unwrap();
        assert!(high.params.trail_decay <= 0.999);
        let low = Attractor::from_json(8, 8, 0, &json!({"trail_decay": -1.0})).unwrap();
        assert_eq!(low.params.trail_decay, 0.0);
    }

    // ---- Projection ----

    #[test]
    fn projection_xy_xz_yz_recognized() {
        for proj in ["xy", "xz", "yz"] {
            let e = Attractor::from_json(8, 8, 0, &json!({"projection": proj})).unwrap();
            assert_eq!(e.params.projection.name(), proj);
        }
    }

    #[test]
    fn unknown_projection_falls_back_to_xz() {
        let e = Attractor::from_json(8, 8, 0, &json!({"projection": "abc"})).unwrap();
        assert_eq!(e.params.projection, Projection::Xz);
    }

    // ---- Engine trait ----

    #[test]
    fn params_returns_current_values() {
        let e = a(8, 8);
        let v = e.params();
        assert_eq!(v["kind"].as_str().unwrap(), "lorenz");
        assert_eq!(v["projection"].as_str().unwrap(), "xz");
    }

    #[test]
    fn param_schema_has_all_keys() {
        let e = a(8, 8);
        let s = e.param_schema();
        for k in [
            "kind",
            "iterations_per_step",
            "dt",
            "scale",
            "center_x",
            "center_y",
            "trail_decay",
            "field_gamma",
            "splat_radius",
            "projection",
        ] {
            assert!(s.get(k).is_some(), "schema missing {k}");
        }
    }

    #[test]
    fn engine_is_object_safe() {
        let e = a(8, 8);
        let _: Box<dyn Engine> = Box::new(e);
    }

    #[test]
    fn hue_field_is_none() {
        let e = a(8, 8);
        assert!(e.hue_field().is_none());
    }

    // ---- Numerics ----

    #[test]
    fn lorenz_trajectory_stays_finite_after_many_steps() {
        let mut e = a(32, 32);
        for _ in 0..200 {
            e.step().unwrap();
        }
        for &v in &e.state {
            assert!(v.is_finite(), "Lorenz state diverged: {v}");
        }
    }

    #[test]
    fn pickover_trajectory_stays_bounded() {
        let mut e =
            Attractor::from_json(32, 32, 0, &json!({"kind": "pickover", "scale": 0.2})).unwrap();
        for _ in 0..100 {
            e.step().unwrap();
        }
        // Pickover map outputs are bounded by [-2, 2] in x and y.
        assert!(e.state[0].abs() <= 2.5);
        assert!(e.state[1].abs() <= 2.5);
    }

    // ---- Property-based ----

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn no_nans_in_field(seed: u64) {
                // Seed is unused but we vary it to ensure the engine is robust
                // even when callers pass arbitrary seeds.
                let mut e = Attractor::from_json(24, 24, seed, &json!({})).unwrap();
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
