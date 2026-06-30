#![deny(unsafe_code)]
//! Mandelbrot escape-time fractal engine.
//!
//! Computes the classic Mandelbrot set: for each pixel `c = cx + i*cy`,
//! iterate `z_{n+1} = z_n^2 + c` starting from `z_0 = 0`. The escape time
//! (smoothed via continuous escape coloring) is normalized to [0, 1] and
//! exposed via the [`Engine::field`] output.
//!
//! Unlike the diffusion / agent engines, the Mandelbrot field is a pure
//! function of its parameters — `step()` is a no-op once the field has
//! been computed. Animation comes from re-initialising the engine each
//! frame with new `cx`, `cy`, and `zoom` values (see the CLI
//! `render-sequence` param-sweep mode).

use art_engine_core::error::EngineError;
use art_engine_core::field::Field;
use art_engine_core::params::{param_f64, param_usize};
use art_engine_core::Engine;
use serde_json::{json, Value};

/// Default real-axis center of the view.
const DEFAULT_CX: f64 = -0.75;
/// Default imaginary-axis center of the view.
const DEFAULT_CY: f64 = 0.0;
/// Default zoom factor (window half-width = 1.5 / zoom along the longer axis).
const DEFAULT_ZOOM: f64 = 1.0;
/// Default maximum escape-time iteration count.
const DEFAULT_MAX_ITER: usize = 256;
/// Default rotation angle (radians) about the view center.
const DEFAULT_ROTATION: f64 = 0.0;
/// Default gamma applied to the normalized escape value before output.
///
/// Most Mandelbrot escape times are small (< 50) even at high `max_iter`,
/// which crushes a linear `mu / max_iter` mapping into the dark end of
/// the palette. A sub-1 gamma brightens dim values; 0.4 is a typical
/// choice that keeps boundary detail readable without blowing out the
/// inner-set boundary.
const DEFAULT_COLOR_GAMMA: f64 = 0.4;
/// Bailout radius squared — points with |z|^2 > this have escaped.
const BAILOUT_RADIUS_SQ: f64 = 4.0;
/// Hard cap on `max_iter` to avoid pathological CPU costs.
const MAX_ITER_CAP: usize = 100_000;

/// Parameters controlling Mandelbrot view + iteration.
#[derive(Debug, Clone, Copy)]
pub struct MandelbrotParams {
    /// Real-axis center of the view.
    pub cx: f64,
    /// Imaginary-axis center of the view.
    pub cy: f64,
    /// Zoom factor (1.0 = classic full view, larger = closer).
    pub zoom: f64,
    /// Maximum escape-time iterations per pixel.
    pub max_iter: usize,
    /// Rotation angle (radians) about the view center.
    pub rotation: f64,
    /// Gamma applied to the normalized escape map (0 < gamma <= 5).
    /// Values < 1 brighten dim escape times; > 1 darken them.
    pub color_gamma: f64,
}

impl Default for MandelbrotParams {
    fn default() -> Self {
        Self {
            cx: DEFAULT_CX,
            cy: DEFAULT_CY,
            zoom: DEFAULT_ZOOM,
            max_iter: DEFAULT_MAX_ITER,
            rotation: DEFAULT_ROTATION,
            color_gamma: DEFAULT_COLOR_GAMMA,
        }
    }
}

impl MandelbrotParams {
    /// Extracts parameters from a JSON object, falling back to defaults.
    pub fn from_json(params: &Value) -> Self {
        Self {
            cx: param_f64(params, "cx", DEFAULT_CX),
            cy: param_f64(params, "cy", DEFAULT_CY),
            zoom: param_f64(params, "zoom", DEFAULT_ZOOM),
            max_iter: param_usize(params, "max_iter", DEFAULT_MAX_ITER).min(MAX_ITER_CAP),
            rotation: param_f64(params, "rotation", DEFAULT_ROTATION),
            color_gamma: param_f64(params, "color_gamma", DEFAULT_COLOR_GAMMA).clamp(0.05, 5.0),
        }
    }
}

/// Mandelbrot escape-time fractal engine.
pub struct Mandelbrot {
    field: Field,
    params: MandelbrotParams,
}

impl Mandelbrot {
    /// Creates a new Mandelbrot engine and computes the initial field.
    ///
    /// Returns `EngineError::InvalidDimensions` if width or height is zero.
    pub fn new(width: usize, height: usize, params: MandelbrotParams) -> Result<Self, EngineError> {
        let mut field = Field::new(width, height)?;
        compute_field(&mut field, &params, width, height);
        Ok(Self { field, params })
    }

    /// Creates a Mandelbrot engine from a JSON params object.
    ///
    /// `seed` is accepted for API uniformity but unused — Mandelbrot is
    /// deterministic in its parameters alone.
    pub fn from_json(
        width: usize,
        height: usize,
        _seed: u64,
        json_params: &Value,
    ) -> Result<Self, EngineError> {
        Self::new(width, height, MandelbrotParams::from_json(json_params))
    }
}

impl Engine for Mandelbrot {
    fn step(&mut self) -> Result<(), EngineError> {
        // Mandelbrot's field is a pure function of its parameters; no per-step
        // state to advance. Recompute is a no-op since params don't change
        // between steps without a re-init.
        Ok(())
    }

    fn field(&self) -> &Field {
        &self.field
    }

    fn params(&self) -> Value {
        json!({
            "cx": self.params.cx,
            "cy": self.params.cy,
            "zoom": self.params.zoom,
            "max_iter": self.params.max_iter,
            "rotation": self.params.rotation,
            "color_gamma": self.params.color_gamma,
        })
    }

    fn param_schema(&self) -> Value {
        json!({
            "cx": {
                "type": "number",
                "default": DEFAULT_CX,
                "min": -2.0,
                "max": 1.0,
                "description": "Real-axis center of the view"
            },
            "cy": {
                "type": "number",
                "default": DEFAULT_CY,
                "min": -1.5,
                "max": 1.5,
                "description": "Imaginary-axis center of the view"
            },
            "zoom": {
                "type": "number",
                "default": DEFAULT_ZOOM,
                "min": 0.1,
                "max": 1e12,
                "description": "Zoom factor (1.0 = classic full view, larger = closer)"
            },
            "max_iter": {
                "type": "integer",
                "default": DEFAULT_MAX_ITER,
                "min": 16,
                "max": MAX_ITER_CAP,
                "description": "Maximum escape-time iterations per pixel"
            },
            "rotation": {
                "type": "number",
                "default": DEFAULT_ROTATION,
                "min": -std::f64::consts::TAU,
                "max": std::f64::consts::TAU,
                "description": "Rotation angle (radians) about the view center"
            },
            "color_gamma": {
                "type": "number",
                "default": DEFAULT_COLOR_GAMMA,
                "min": 0.05,
                "max": 5.0,
                "description": "Gamma applied to escape map; <1 brightens, >1 darkens"
            }
        })
    }
}

/// Fills the field with smoothed escape-time values normalized to [0, 1].
///
/// Pixels in the set (no escape within `max_iter`) map to 0.0.
/// Escaped pixels use continuous (smooth) escape coloring:
/// `mu = n + 1 - log2(log2(|z|))`, then mapped to [0, 1] via `mu / max_iter`.
fn compute_field(field: &mut Field, params: &MandelbrotParams, width: usize, height: usize) {
    let half_extent = 1.5 / params.zoom.max(f64::MIN_POSITIVE);
    // Use the longer axis for the half-extent, preserve aspect on the other.
    let aspect = width as f64 / height as f64;
    let (half_w, half_h) = if aspect >= 1.0 {
        (half_extent * aspect, half_extent)
    } else {
        (half_extent, half_extent / aspect)
    };

    let cos_r = params.rotation.cos();
    let sin_r = params.rotation.sin();
    let max_iter = params.max_iter.max(1);
    let max_iter_f = max_iter as f64;
    let gamma = params.color_gamma.max(0.05);

    let data = field.data_mut();
    for py in 0..height {
        let v = (py as f64 + 0.5) / height as f64; // [0, 1]
        let dy = (v * 2.0 - 1.0) * half_h;
        for px in 0..width {
            let u = (px as f64 + 0.5) / width as f64; // [0, 1]
            let dx = (u * 2.0 - 1.0) * half_w;

            // Rotate offset about the view center, then translate.
            let rx = dx * cos_r - dy * sin_r;
            let ry = dx * sin_r + dy * cos_r;
            let cre = params.cx + rx;
            let cim = params.cy + ry;

            let t = escape_time_smooth(cre, cim, max_iter);
            // Linear normalize, then gamma-correct so dim escape times are
            // brightened (gamma < 1) before palette lookup. Inside-set
            // points (t == 0) stay at exactly 0.
            let normalized = (t / max_iter_f).clamp(0.0, 1.0);
            let shaped = if normalized > 0.0 {
                normalized.powf(gamma)
            } else {
                0.0
            };
            data[py * width + px] = shaped.clamp(0.0, 1.0);
        }
    }
}

/// Smoothed Mandelbrot escape time for `c = cre + i*cim`.
///
/// Returns 0.0 for points that don't escape within `max_iter` (i.e. inside
/// the set). Returns a smooth real value in (0, max_iter] for escaped points
/// using the continuous-coloring formula.
fn escape_time_smooth(cre: f64, cim: f64, max_iter: usize) -> f64 {
    let mut zr = 0.0_f64;
    let mut zi = 0.0_f64;
    let mut zr2 = 0.0_f64;
    let mut zi2 = 0.0_f64;

    for n in 0..max_iter {
        if zr2 + zi2 > BAILOUT_RADIUS_SQ {
            // Smooth escape: mu = n + 1 - log2(log2(|z|))
            let mag_sq = zr2 + zi2;
            // log2(log2(sqrt(mag_sq))) = log2(0.5 * log2(mag_sq))
            let half_log_mag_sq = 0.5_f64 * mag_sq.log2();
            // half_log_mag_sq > 0 since mag_sq > BAILOUT_RADIUS_SQ = 4 > 1
            let log_log = half_log_mag_sq.log2();
            let mu = (n as f64 + 1.0 - log_log).max(0.0);
            return mu;
        }
        zi = 2.0 * zr * zi + cim;
        zr = zr2 - zi2 + cre;
        zr2 = zr * zr;
        zi2 = zi * zi;
    }
    0.0 // Inside the set
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mb(w: usize, h: usize) -> Mandelbrot {
        Mandelbrot::new(w, h, MandelbrotParams::default()).unwrap()
    }

    #[test]
    fn new_creates_field_with_correct_dimensions() {
        let m = mb(64, 32);
        assert_eq!(m.field().width(), 64);
        assert_eq!(m.field().height(), 32);
    }

    #[test]
    fn new_with_zero_dimensions_returns_error() {
        let p = MandelbrotParams::default();
        assert!(Mandelbrot::new(0, 16, p).is_err());
        assert!(Mandelbrot::new(16, 0, p).is_err());
    }

    #[test]
    fn from_json_uses_defaults_for_empty_object() {
        let m = Mandelbrot::from_json(16, 16, 0, &json!({})).unwrap();
        assert!((m.params.cx - DEFAULT_CX).abs() < f64::EPSILON);
        assert!((m.params.cy - DEFAULT_CY).abs() < f64::EPSILON);
        assert_eq!(m.params.max_iter, DEFAULT_MAX_ITER);
    }

    #[test]
    fn from_json_caps_max_iter() {
        let m = Mandelbrot::from_json(8, 8, 0, &json!({"max_iter": MAX_ITER_CAP + 1000})).unwrap();
        assert_eq!(m.params.max_iter, MAX_ITER_CAP);
    }

    #[test]
    fn step_is_idempotent() {
        let mut m = mb(32, 32);
        let before: Vec<u64> = m.field().data().iter().map(|v| v.to_bits()).collect();
        m.step().unwrap();
        m.step().unwrap();
        let after: Vec<u64> = m.field().data().iter().map(|v| v.to_bits()).collect();
        assert_eq!(before, after, "step() should not mutate the field");
    }

    #[test]
    fn determinism_same_params() {
        let p = MandelbrotParams::default();
        let a = Mandelbrot::new(32, 32, p).unwrap();
        let b = Mandelbrot::new(32, 32, p).unwrap();
        assert!(a
            .field()
            .data()
            .iter()
            .zip(b.field().data().iter())
            .all(|(va, vb)| va.to_bits() == vb.to_bits()));
    }

    #[test]
    fn origin_is_inside_set() {
        // c = 0 is the origin of the cardioid — never escapes.
        let t = escape_time_smooth(0.0, 0.0, 256);
        assert_eq!(t, 0.0);
    }

    #[test]
    fn far_point_escapes_quickly() {
        // c = 10 + 0i is wildly outside any bound — escapes on iteration 1.
        let t = escape_time_smooth(10.0, 0.0, 256);
        assert!(t > 0.0 && t < 5.0, "expected fast escape, got {t}");
    }

    #[test]
    fn known_inside_point_minus_one() {
        // c = -1 is the period-2 bulb center — inside the set.
        let t = escape_time_smooth(-1.0, 0.0, 256);
        assert_eq!(t, 0.0);
    }

    #[test]
    fn field_values_in_unit_interval() {
        let m = mb(64, 64);
        for &v in m.field().data() {
            assert!(
                (0.0..=1.0).contains(&v) && !v.is_nan(),
                "out-of-range field value: {v}"
            );
        }
    }

    #[test]
    fn higher_zoom_changes_field() {
        let a = Mandelbrot::new(32, 32, MandelbrotParams::default()).unwrap();
        let zoomed = MandelbrotParams {
            zoom: 100.0,
            ..MandelbrotParams::default()
        };
        let b = Mandelbrot::new(32, 32, zoomed).unwrap();
        let differ = a
            .field()
            .data()
            .iter()
            .zip(b.field().data().iter())
            .any(|(va, vb)| va.to_bits() != vb.to_bits());
        assert!(differ, "zoom change should alter the field");
    }

    #[test]
    fn rotation_changes_field() {
        let a = Mandelbrot::new(32, 32, MandelbrotParams::default()).unwrap();
        let rotated = MandelbrotParams {
            rotation: 1.0,
            zoom: 5.0, // close enough that rotation is visible
            ..MandelbrotParams::default()
        };
        let unrotated = MandelbrotParams {
            zoom: 5.0,
            ..MandelbrotParams::default()
        };
        let b = Mandelbrot::new(32, 32, rotated).unwrap();
        let c = Mandelbrot::new(32, 32, unrotated).unwrap();
        // a vs c will differ (different zoom). b vs c is the meaningful check.
        let _ = a;
        let differ = b
            .field()
            .data()
            .iter()
            .zip(c.field().data().iter())
            .any(|(va, vb)| va.to_bits() != vb.to_bits());
        assert!(differ, "rotation should alter the field");
    }

    #[test]
    fn params_returns_current_values() {
        let p = MandelbrotParams {
            cx: -1.25,
            cy: 0.1,
            zoom: 50.0,
            max_iter: 128,
            rotation: 0.5,
            color_gamma: 0.4,
        };
        let m = Mandelbrot::new(8, 8, p).unwrap();
        let v = m.params();
        assert!((v["cx"].as_f64().unwrap() - p.cx).abs() < f64::EPSILON);
        assert!((v["cy"].as_f64().unwrap() - p.cy).abs() < f64::EPSILON);
        assert!((v["zoom"].as_f64().unwrap() - p.zoom).abs() < f64::EPSILON);
        assert_eq!(v["max_iter"].as_u64().unwrap(), p.max_iter as u64);
    }

    #[test]
    fn param_schema_has_all_keys() {
        let m = mb(8, 8);
        let s = m.param_schema();
        for key in ["cx", "cy", "zoom", "max_iter", "rotation"] {
            assert!(s.get(key).is_some(), "schema missing {key}");
        }
    }

    #[test]
    fn engine_is_object_safe() {
        let m = mb(8, 8);
        let _: Box<dyn Engine> = Box::new(m);
    }

    #[test]
    fn hue_field_is_none() {
        let m = mb(8, 8);
        assert!(m.hue_field().is_none());
    }

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn no_nans(
                w in 4_usize..=32,
                h in 4_usize..=32,
                cx in -2.0_f64..=1.0,
                cy in -1.5_f64..=1.5,
                zoom in 0.5_f64..=1000.0,
                max_iter in 16_usize..=512,
            ) {
                let p = MandelbrotParams { cx, cy, zoom, max_iter, rotation: 0.0, color_gamma: 0.4 };
                let m = Mandelbrot::new(w, h, p).unwrap();
                for &v in m.field().data() {
                    prop_assert!(!v.is_nan(), "NaN in field at params {:?}", p);
                    prop_assert!((0.0..=1.0).contains(&v));
                }
            }
        }
    }
}
