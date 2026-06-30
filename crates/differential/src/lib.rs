#![deny(unsafe_code)]
//! Differential-growth engine.
//!
//! A closed polyline of nodes evolves under three competing rules:
//! 1. **Repulsion** — every node pushes away from neighbors within
//!    `repulsion_radius`. A uniform-grid spatial hash keeps the per-step
//!    cost near-linear in node count.
//! 2. **Attraction** — every node is pulled toward its two polyline
//!    neighbors, which keeps the curve smooth and topologically connected.
//! 3. **Subdivision** — when a polyline segment exceeds `max_segment_length`,
//!    a new node is inserted at its midpoint. The polyline grows.
//!
//! Net effect: a self-organizing curve that wrinkles, doubles back on
//! itself, and tiles space with sinuous folds — the visual signature of
//! Sage Jenson / Andy Lomas / Zach Lieberman generative work, and of
//! biological growth in cortical folding and coral.
//!
//! # Determinism
//!
//! Same seed + same params + same step count = bit-identical field. The PRNG
//! is consumed only for the initial node positions and for break-tie jitter
//! during subdivision.
//!
//! # JSON parameters
//!
//! ```json
//! {
//!   "max_nodes": 4000,
//!   "initial_nodes": 60,
//!   "repulsion_radius": 0.012,
//!   "repulsion_strength": 0.0008,
//!   "attraction_strength": 0.04,
//!   "max_segment_length": 0.012,
//!   "growth_rate": 4,
//!   "trail_decay": 0.96,
//!   "splat_radius": 1.5,
//!   "field_gamma": 0.5
//! }
//! ```
//!
//! `growth_rate` controls how often subdivision is checked: every Nth step.
//! Higher values make the curve grow more slowly (and the simulation cheaper).

use art_engine_core::error::EngineError;
use art_engine_core::field::Field;
use art_engine_core::params::{param_f64, param_usize};
use art_engine_core::prng::Xorshift64;
use art_engine_core::Engine;
use glam::Vec2;
use serde_json::{json, Value};
use std::f32::consts::TAU;

const DEFAULT_MAX_NODES: usize = 4000;
const DEFAULT_INITIAL_NODES: usize = 60;
const DEFAULT_REPULSION_RADIUS: f64 = 0.012;
const DEFAULT_REPULSION_STRENGTH: f64 = 0.0008;
const DEFAULT_ATTRACTION_STRENGTH: f64 = 0.04;
const DEFAULT_MAX_SEGMENT_LENGTH: f64 = 0.012;
const DEFAULT_GROWTH_RATE: usize = 4;
const DEFAULT_TRAIL_DECAY: f64 = 0.96;
const DEFAULT_SPLAT_RADIUS: f64 = 1.5;
const DEFAULT_FIELD_GAMMA: f64 = 0.5;
const DEFAULT_INFLUENCE_STRENGTH: f64 = 0.0;
/// Hard cap to prevent runaway growth from blowing memory.
const MAX_NODES_LIMIT: usize = 200_000;

#[derive(Debug, Clone, Copy)]
pub struct DifferentialParams {
    pub max_nodes: usize,
    pub initial_nodes: usize,
    pub repulsion_radius: f64,
    pub repulsion_strength: f64,
    pub attraction_strength: f64,
    pub max_segment_length: f64,
    pub growth_rate: usize,
    pub trail_decay: f64,
    pub splat_radius: f64,
    pub field_gamma: f64,
    pub influence_strength: f64,
}

impl Default for DifferentialParams {
    fn default() -> Self {
        Self {
            max_nodes: DEFAULT_MAX_NODES,
            initial_nodes: DEFAULT_INITIAL_NODES,
            repulsion_radius: DEFAULT_REPULSION_RADIUS,
            repulsion_strength: DEFAULT_REPULSION_STRENGTH,
            attraction_strength: DEFAULT_ATTRACTION_STRENGTH,
            max_segment_length: DEFAULT_MAX_SEGMENT_LENGTH,
            growth_rate: DEFAULT_GROWTH_RATE,
            trail_decay: DEFAULT_TRAIL_DECAY,
            splat_radius: DEFAULT_SPLAT_RADIUS,
            field_gamma: DEFAULT_FIELD_GAMMA,
            influence_strength: DEFAULT_INFLUENCE_STRENGTH,
        }
    }
}

impl DifferentialParams {
    pub fn from_json(params: &Value) -> Self {
        Self {
            max_nodes: param_usize(params, "max_nodes", DEFAULT_MAX_NODES)
                .clamp(3, MAX_NODES_LIMIT),
            initial_nodes: param_usize(params, "initial_nodes", DEFAULT_INITIAL_NODES)
                .clamp(3, MAX_NODES_LIMIT),
            repulsion_radius: param_f64(params, "repulsion_radius", DEFAULT_REPULSION_RADIUS)
                .max(1e-5),
            repulsion_strength: param_f64(params, "repulsion_strength", DEFAULT_REPULSION_STRENGTH)
                .max(0.0),
            attraction_strength: param_f64(
                params,
                "attraction_strength",
                DEFAULT_ATTRACTION_STRENGTH,
            )
            .max(0.0),
            max_segment_length: param_f64(params, "max_segment_length", DEFAULT_MAX_SEGMENT_LENGTH)
                .max(1e-5),
            growth_rate: param_usize(params, "growth_rate", DEFAULT_GROWTH_RATE).max(1),
            trail_decay: param_f64(params, "trail_decay", DEFAULT_TRAIL_DECAY).clamp(0.0, 0.999),
            splat_radius: param_f64(params, "splat_radius", DEFAULT_SPLAT_RADIUS).clamp(0.0, 16.0),
            field_gamma: param_f64(params, "field_gamma", DEFAULT_FIELD_GAMMA).clamp(0.05, 5.0),
            influence_strength: param_f64(params, "influence_strength", DEFAULT_INFLUENCE_STRENGTH)
                .max(0.0),
        }
    }
}

/// Differential-growth engine.
pub struct Differential {
    params: DifferentialParams,
    width: usize,
    height: usize,
    /// Closed polyline. Index N+1 wraps to index 0.
    nodes: Vec<Vec2>,
    /// Accumulated step counter, used to schedule subdivision passes.
    tick: usize,
    field: Field,
    /// Reusable density buffer for splatting.
    scratch: Vec<f64>,
    influence: Option<Vec<f64>>,
    influence_w: usize,
    influence_h: usize,
}

impl Differential {
    pub fn new(
        width: usize,
        height: usize,
        seed: u64,
        params: DifferentialParams,
    ) -> Result<Self, EngineError> {
        if width == 0 || height == 0 {
            return Err(EngineError::InvalidDimensions);
        }
        let len = width
            .checked_mul(height)
            .ok_or(EngineError::InvalidDimensions)?;
        let field = Field::new(width, height)?;
        let scratch = vec![0.0_f64; len];
        let mut rng = Xorshift64::new(seed);

        // Seed: a small circle of `initial_nodes` nodes around canvas center.
        // Radius is 6% of the shorter axis — enough to leave growth room.
        let n_init = params.initial_nodes.min(params.max_nodes);
        let r = 0.06_f32;
        // Tiny per-node jitter so the initial circle isn't perfectly symmetric;
        // symmetry would lock the system into trivial breathing modes.
        let nodes: Vec<Vec2> = (0..n_init)
            .map(|i| {
                let theta = TAU * (i as f32) / (n_init as f32);
                let jitter = ((rng.next_f64() - 0.5) * 0.001) as f32;
                Vec2::new(
                    0.5 + (r + jitter) * theta.cos(),
                    0.5 + (r + jitter) * theta.sin(),
                )
            })
            .collect();

        let _ = rng; // PRNG was consumed during initial circle setup
        Ok(Self {
            params,
            width,
            height,
            nodes,
            tick: 0,
            field,
            scratch,
            influence: None,
            influence_w: 0,
            influence_h: 0,
        })
    }

    pub fn from_json(
        width: usize,
        height: usize,
        seed: u64,
        params: &Value,
    ) -> Result<Self, EngineError> {
        Self::new(width, height, seed, DifferentialParams::from_json(params))
    }

    /// Current node count (the polyline's length).
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

impl Engine for Differential {
    fn step(&mut self) -> Result<(), EngineError> {
        let n = self.nodes.len();
        if n < 3 {
            // Degenerate; rebuild output and return.
            rasterize_into(
                &mut self.scratch,
                self.width,
                self.height,
                &self.nodes,
                self.params.splat_radius,
            );
            blend_into_field(
                &mut self.field,
                &self.scratch,
                self.params.trail_decay,
                self.params.field_gamma,
            );
            return Ok(());
        }

        let r_rep = self.params.repulsion_radius as f32;
        let s_rep = self.params.repulsion_strength as f32;
        let s_att = self.params.attraction_strength as f32;
        let r_rep_sq = r_rep * r_rep;

        // Build spatial hash with cell_size == repulsion_radius. Each node
        // only needs to check the 9 surrounding cells.
        let grid_w = ((1.0 / r_rep as f64).ceil() as usize).max(1);
        let grid_h = grid_w;
        let mut grid: Vec<Vec<u32>> = vec![Vec::new(); grid_w * grid_h];
        for (i, p) in self.nodes.iter().enumerate() {
            let gx = ((p.x.clamp(0.0, 0.9999) * grid_w as f32) as usize).min(grid_w - 1);
            let gy = ((p.y.clamp(0.0, 0.9999) * grid_h as f32) as usize).min(grid_h - 1);
            grid[gy * grid_w + gx].push(i as u32);
        }

        // Compute displacements per node (output buffer reused across passes).
        let mut delta: Vec<Vec2> = vec![Vec2::ZERO; n];

        // -- Repulsion pass --
        for (i, delta_i) in delta.iter_mut().enumerate().take(n) {
            let p_i = self.nodes[i];
            let gx = ((p_i.x.clamp(0.0, 0.9999) * grid_w as f32) as i32).max(0);
            let gy = ((p_i.y.clamp(0.0, 0.9999) * grid_h as f32) as i32).max(0);
            for dy in -1..=1 {
                let cy = gy + dy;
                if cy < 0 || cy >= grid_h as i32 {
                    continue;
                }
                for dx in -1..=1 {
                    let cx = gx + dx;
                    if cx < 0 || cx >= grid_w as i32 {
                        continue;
                    }
                    let cell = &grid[(cy as usize) * grid_w + (cx as usize)];
                    for &j_u32 in cell {
                        let j = j_u32 as usize;
                        if j == i {
                            continue;
                        }
                        let p_j = self.nodes[j];
                        let diff = p_i - p_j;
                        let d2 = diff.length_squared();
                        if d2 == 0.0 || d2 > r_rep_sq {
                            continue;
                        }
                        let d = d2.sqrt();
                        // Linear falloff: full strength at d=0, zero at d=r.
                        let weight = (1.0 - d / r_rep).max(0.0);
                        // Avoid singularity by normalizing by d (not d^2) and
                        // capping the unit vector via .normalize_or_zero().
                        let dir = diff.normalize_or_zero();
                        *delta_i += dir * (s_rep * weight);
                    }
                }
            }
        }

        // -- Attraction pass (toward polyline neighbors) --
        for (i, delta_i) in delta.iter_mut().enumerate().take(n) {
            let p_i = self.nodes[i];
            let prev_idx = if i == 0 { n - 1 } else { i - 1 };
            let next_idx = if i + 1 == n { 0 } else { i + 1 };
            let mid = (self.nodes[prev_idx] + self.nodes[next_idx]) * 0.5;
            // Pull toward the midpoint of neighbors — this is exactly the
            // discrete Laplacian of the polyline, which smooths curvature.
            *delta_i += (mid - p_i) * s_att;
        }

        // -- Influence pass (optional gradient force) --
        if let Some(inf) = self.influence.as_ref() {
            let s = self.params.influence_strength as f32;
            if s > 0.0 && self.influence_w > 0 && self.influence_h > 0 {
                let iw = self.influence_w;
                let ih = self.influence_h;
                let ifw = iw as f32;
                let ifh = ih as f32;
                for (i, p) in self.nodes.iter().enumerate() {
                    let cx = (p.x * ifw) as i32;
                    let cy = (p.y * ifh) as i32;
                    let cx = cx.clamp(1, iw as i32 - 2);
                    let cy = cy.clamp(1, ih as i32 - 2);
                    let idx_l = (cy as usize) * iw + (cx as usize - 1);
                    let idx_r = (cy as usize) * iw + (cx as usize + 1);
                    let idx_u = (cy as usize - 1) * iw + (cx as usize);
                    let idx_d = (cy as usize + 1) * iw + (cx as usize);
                    let dx = (inf[idx_r] - inf[idx_l]) as f32 * 0.5;
                    let dy = (inf[idx_d] - inf[idx_u]) as f32 * 0.5;
                    let dvx = s * dx;
                    let dvy = s * dy;
                    if dvx.is_finite() && dvy.is_finite() {
                        delta[i].x += dvx;
                        delta[i].y += dvy;
                    }
                }
            }
        }

        // -- Apply displacement, clamp to canvas --
        for (p, d) in self.nodes.iter_mut().zip(delta.iter()) {
            let np = *p + *d;
            // Drop NaN/inf to keep the simulation alive on bad params.
            if np.x.is_finite() && np.y.is_finite() {
                p.x = np.x.clamp(0.0, 1.0);
                p.y = np.y.clamp(0.0, 1.0);
            }
        }

        // -- Subdivision pass (every growth_rate steps) --
        self.tick += 1;
        if self.tick.is_multiple_of(self.params.growth_rate)
            && self.nodes.len() < self.params.max_nodes
        {
            let max_seg = self.params.max_segment_length as f32;
            let max_seg_sq = max_seg * max_seg;
            // Walk segments from end to start so insertion indices stay valid.
            let cap = self.params.max_nodes;
            let mut i = self.nodes.len();
            while i > 0 {
                i -= 1;
                if self.nodes.len() >= cap {
                    break;
                }
                let curr = self.nodes[i];
                let next_idx = if i + 1 == self.nodes.len() { 0 } else { i + 1 };
                let next = self.nodes[next_idx];
                let seg = next - curr;
                if seg.length_squared() > max_seg_sq {
                    let mid = curr + seg * 0.5;
                    // Insert after i (or at end if i is last).
                    if i + 1 == self.nodes.len() {
                        self.nodes.push(mid);
                    } else {
                        self.nodes.insert(i + 1, mid);
                    }
                }
            }
        }

        // -- Rasterize and blend into field --
        rasterize_into(
            &mut self.scratch,
            self.width,
            self.height,
            &self.nodes,
            self.params.splat_radius,
        );
        blend_into_field(
            &mut self.field,
            &self.scratch,
            self.params.trail_decay,
            self.params.field_gamma,
        );

        Ok(())
    }

    fn field(&self) -> &Field {
        &self.field
    }

    fn params(&self) -> Value {
        json!({
            "max_nodes": self.params.max_nodes,
            "initial_nodes": self.params.initial_nodes,
            "repulsion_radius": self.params.repulsion_radius,
            "repulsion_strength": self.params.repulsion_strength,
            "attraction_strength": self.params.attraction_strength,
            "max_segment_length": self.params.max_segment_length,
            "growth_rate": self.params.growth_rate,
            "trail_decay": self.params.trail_decay,
            "splat_radius": self.params.splat_radius,
            "field_gamma": self.params.field_gamma,
            "influence_strength": self.params.influence_strength,
        })
    }

    fn param_schema(&self) -> Value {
        json!({
            "max_nodes": {
                "type": "integer",
                "default": DEFAULT_MAX_NODES,
                "min": 3,
                "max": MAX_NODES_LIMIT,
                "description": "Hard cap on polyline node count"
            },
            "initial_nodes": {
                "type": "integer",
                "default": DEFAULT_INITIAL_NODES,
                "min": 3,
                "description": "Starting node count (initialized as a circle)"
            },
            "repulsion_radius": {
                "type": "number",
                "default": DEFAULT_REPULSION_RADIUS,
                "min": 1e-5,
                "description": "Distance below which nodes push each other apart (normalized canvas units)"
            },
            "repulsion_strength": {
                "type": "number",
                "default": DEFAULT_REPULSION_STRENGTH,
                "min": 0.0,
                "description": "Per-step displacement gain from repulsion"
            },
            "attraction_strength": {
                "type": "number",
                "default": DEFAULT_ATTRACTION_STRENGTH,
                "min": 0.0,
                "description": "Per-step pull toward midpoint of polyline neighbors"
            },
            "max_segment_length": {
                "type": "number",
                "default": DEFAULT_MAX_SEGMENT_LENGTH,
                "min": 1e-5,
                "description": "Segments longer than this are subdivided"
            },
            "growth_rate": {
                "type": "integer",
                "default": DEFAULT_GROWTH_RATE,
                "min": 1,
                "description": "Subdivision happens every Nth step"
            },
            "trail_decay": {
                "type": "number",
                "default": DEFAULT_TRAIL_DECAY,
                "min": 0.0,
                "max": 0.999,
                "description": "Per-step trail decay; 0 = fresh per frame, larger = longer trails"
            },
            "splat_radius": {
                "type": "number",
                "default": DEFAULT_SPLAT_RADIUS,
                "min": 0.0,
                "max": 16.0,
                "description": "Per-sample deposit radius (px)"
            },
            "field_gamma": {
                "type": "number",
                "default": DEFAULT_FIELD_GAMMA,
                "min": 0.05,
                "max": 5.0,
                "description": "Gamma applied to per-step density before accumulation"
            },
            "influence_strength": {
                "type": "number",
                "default": DEFAULT_INFLUENCE_STRENGTH,
                "min": 0.0,
                "description": "Per-step gain on the gradient force from external influence field"
            }
        })
    }

    fn set_influence(&mut self, field: &Field) -> Result<(), EngineError> {
        if field.width() != self.width || field.height() != self.height {
            return Err(EngineError::InvalidDimensions);
        }
        self.influence = Some(field.data().to_vec());
        self.influence_w = field.width();
        self.influence_h = field.height();
        Ok(())
    }
}

/// Rasterizes the polyline into `buffer` by densely sampling each segment
/// and splatting a soft circle at every sample. Buffer is cleared first.
fn rasterize_into(buffer: &mut [f64], width: usize, height: usize, nodes: &[Vec2], radius: f64) {
    for v in buffer.iter_mut() {
        *v = 0.0;
    }
    if nodes.len() < 2 {
        return;
    }

    let w = width as f64;
    let h = height as f64;
    let mut max_val = 0.0_f64;
    let n = nodes.len();
    for i in 0..n {
        let a = nodes[i];
        let b = nodes[(i + 1) % n];
        // Sample one point per ~half-pixel along the segment.
        let dx = (b.x - a.x) as f64 * w;
        let dy = (b.y - a.y) as f64 * h;
        let seg_len_pix = (dx * dx + dy * dy).sqrt().max(1.0);
        let samples = (seg_len_pix * 2.0).ceil().min(2048.0) as usize;
        for k in 0..=samples {
            let t = k as f64 / samples as f64;
            let cx = a.x as f64 + (b.x - a.x) as f64 * t;
            let cy = a.y as f64 + (b.y - a.y) as f64 * t;
            if !(0.0..=1.0).contains(&cx) || !(0.0..=1.0).contains(&cy) {
                continue;
            }
            splat(buffer, width, height, cx, cy, radius, &mut max_val);
        }
    }
    if max_val > 0.0 {
        for v in buffer.iter_mut() {
            *v /= max_val;
        }
    }
}

/// Decay-blend a normalized scratch buffer into the engine field with gamma.
fn blend_into_field(field: &mut Field, scratch: &[f64], decay: f64, gamma: f64) {
    for (dst, &src) in field.data_mut().iter_mut().zip(scratch.iter()) {
        let shaped = if src > 0.0 { src.powf(gamma) } else { 0.0 };
        let v = *dst * decay + shaped;
        *dst = if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }
}

/// Soft-disc splat (matches the pattern from particles/attractor engines).
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

    fn d(w: usize, h: usize, seed: u64) -> Differential {
        Differential::new(w, h, seed, DifferentialParams::default()).unwrap()
    }

    // ---- Construction ----

    #[test]
    fn new_creates_field_with_correct_dims() {
        let e = d(64, 32, 42);
        assert_eq!(e.field().width(), 64);
        assert_eq!(e.field().height(), 32);
    }

    #[test]
    fn new_with_zero_dims_returns_error() {
        assert!(Differential::new(0, 16, 42, DifferentialParams::default()).is_err());
        assert!(Differential::new(16, 0, 42, DifferentialParams::default()).is_err());
    }

    #[test]
    fn initial_circle_has_expected_node_count() {
        let e = d(32, 32, 42);
        assert_eq!(e.node_count(), DEFAULT_INITIAL_NODES);
    }

    #[test]
    fn initial_nodes_lie_on_circle_around_center() {
        let e = d(32, 32, 42);
        for p in &e.nodes {
            let dx = p.x - 0.5;
            let dy = p.y - 0.5;
            let r = (dx * dx + dy * dy).sqrt();
            // Nominal radius 0.06, jitter ±0.0005.
            assert!(
                r > 0.05 && r < 0.07,
                "initial node off the seed circle: r={r}"
            );
        }
    }

    // ---- Step + growth ----

    #[test]
    fn nodes_grow_over_time() {
        // Use a tighter subdivision threshold than the default so the
        // initial 60-node circle (segments ~0.0063 long) immediately has
        // segments above threshold and grows on the first subdivision pass.
        let p = DifferentialParams {
            max_segment_length: 0.005,
            growth_rate: 1,
            ..DifferentialParams::default()
        };
        let mut e = Differential::new(64, 64, 42, p).unwrap();
        let initial = e.node_count();
        for _ in 0..10 {
            e.step().unwrap();
        }
        let after = e.node_count();
        assert!(
            after > initial,
            "polyline should grow: initial={initial}, after={after}"
        );
    }

    #[test]
    fn node_count_never_exceeds_max() {
        let p = DifferentialParams {
            max_nodes: 80,
            initial_nodes: 60,
            growth_rate: 1,
            ..DifferentialParams::default()
        };
        let mut e = Differential::new(48, 48, 42, p).unwrap();
        for _ in 0..200 {
            e.step().unwrap();
            assert!(e.node_count() <= 80, "node_count {} > max", e.node_count());
        }
    }

    #[test]
    fn no_growth_when_growth_rate_huge() {
        let p = DifferentialParams {
            growth_rate: 10_000_000,
            ..DifferentialParams::default()
        };
        let mut e = Differential::new(48, 48, 42, p).unwrap();
        let initial = e.node_count();
        for _ in 0..30 {
            e.step().unwrap();
        }
        // With growth_rate that high, no subdivision should occur in 30 steps.
        assert_eq!(e.node_count(), initial);
    }

    #[test]
    fn field_values_in_unit_interval() {
        let mut e = d(40, 40, 42);
        for _ in 0..30 {
            e.step().unwrap();
        }
        for &v in e.field().data() {
            assert!((0.0..=1.0).contains(&v) && !v.is_nan(), "out: {v}");
        }
    }

    #[test]
    fn nodes_stay_in_canvas() {
        let mut e = d(48, 48, 42);
        for _ in 0..100 {
            e.step().unwrap();
        }
        for p in &e.nodes {
            assert!(
                (0.0..=1.0).contains(&p.x) && (0.0..=1.0).contains(&p.y),
                "node escaped canvas: {p:?}"
            );
        }
    }

    #[test]
    fn no_nans_in_node_positions() {
        let mut e = d(40, 40, 42);
        for _ in 0..50 {
            e.step().unwrap();
        }
        for p in &e.nodes {
            assert!(p.x.is_finite() && p.y.is_finite(), "NaN node {p:?}");
        }
    }

    // ---- Determinism ----

    #[test]
    fn determinism_same_seed() {
        let mut a = d(40, 40, 12345);
        let mut b = d(40, 40, 12345);
        for _ in 0..30 {
            a.step().unwrap();
            b.step().unwrap();
        }
        assert_eq!(a.node_count(), b.node_count());
        for (pa, pb) in a.nodes.iter().zip(b.nodes.iter()) {
            assert_eq!(pa.x.to_bits(), pb.x.to_bits());
            assert_eq!(pa.y.to_bits(), pb.y.to_bits());
        }
    }

    #[test]
    fn different_seeds_different_state() {
        let mut a = d(40, 40, 1);
        let mut b = d(40, 40, 2);
        for _ in 0..15 {
            a.step().unwrap();
            b.step().unwrap();
        }
        assert!(a
            .nodes
            .iter()
            .zip(b.nodes.iter())
            .any(|(pa, pb)| pa.x.to_bits() != pb.x.to_bits() || pa.y.to_bits() != pb.y.to_bits()));
    }

    // ---- JSON ----

    #[test]
    fn from_json_uses_defaults() {
        let e = Differential::from_json(8, 8, 42, &json!({})).unwrap();
        assert_eq!(e.params.max_nodes, DEFAULT_MAX_NODES);
        assert_eq!(e.params.initial_nodes, DEFAULT_INITIAL_NODES);
    }

    #[test]
    fn from_json_caps_max_nodes() {
        let e = Differential::from_json(8, 8, 42, &json!({"max_nodes": 9_999_999})).unwrap();
        assert_eq!(e.params.max_nodes, MAX_NODES_LIMIT);
    }

    #[test]
    fn from_json_clamps_trail_decay() {
        let high = Differential::from_json(8, 8, 42, &json!({"trail_decay": 5.0})).unwrap();
        assert!(high.params.trail_decay <= 0.999);
        let low = Differential::from_json(8, 8, 42, &json!({"trail_decay": -1.0})).unwrap();
        assert_eq!(low.params.trail_decay, 0.0);
    }

    // ---- Influence coupling ----

    #[test]
    fn set_influence_with_wrong_dims_returns_error() {
        let mut e = d(16, 16, 42);
        let bad = Field::new(8, 8).unwrap();
        assert!(e.set_influence(&bad).is_err());
    }

    #[test]
    fn set_influence_with_zero_strength_no_effect() {
        let mut a = d(24, 24, 42);
        let mut b = d(24, 24, 42);
        let inf = Field::filled(24, 24, 1.0).unwrap();
        b.set_influence(&inf).unwrap();
        for _ in 0..15 {
            a.step().unwrap();
            b.step().unwrap();
        }
        assert_eq!(a.node_count(), b.node_count());
        for (pa, pb) in a.nodes.iter().zip(b.nodes.iter()) {
            assert_eq!(pa.x.to_bits(), pb.x.to_bits());
            assert_eq!(pa.y.to_bits(), pb.y.to_bits());
        }
    }

    // ---- Engine trait ----

    #[test]
    fn params_returns_current_values() {
        let e = d(8, 8, 42);
        let v = e.params();
        assert_eq!(
            v["initial_nodes"].as_u64().unwrap() as usize,
            DEFAULT_INITIAL_NODES
        );
    }

    #[test]
    fn param_schema_has_all_keys() {
        let e = d(8, 8, 42);
        let s = e.param_schema();
        for k in [
            "max_nodes",
            "initial_nodes",
            "repulsion_radius",
            "repulsion_strength",
            "attraction_strength",
            "max_segment_length",
            "growth_rate",
            "trail_decay",
            "splat_radius",
            "field_gamma",
            "influence_strength",
        ] {
            assert!(s.get(k).is_some(), "schema missing {k}");
        }
    }

    #[test]
    fn engine_is_object_safe() {
        let e = d(8, 8, 42);
        let _: Box<dyn Engine> = Box::new(e);
    }

    #[test]
    fn hue_field_is_none() {
        let e = d(8, 8, 42);
        assert!(e.hue_field().is_none());
    }

    // ---- Property-based ----

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn no_nans_for_any_seed(seed: u64) {
                let mut e = d(20, 20, seed);
                for _ in 0..15 {
                    e.step().unwrap();
                }
                for &v in e.field().data() {
                    prop_assert!(!v.is_nan());
                    prop_assert!((0.0..=1.0).contains(&v));
                }
                for p in &e.nodes {
                    prop_assert!(p.x.is_finite() && p.y.is_finite());
                }
            }

            #[test]
            fn deterministic_for_any_seed(seed: u64) {
                let mut a = d(16, 16, seed);
                let mut b = d(16, 16, seed);
                for _ in 0..10 {
                    a.step().unwrap();
                    b.step().unwrap();
                }
                prop_assert_eq!(a.node_count(), b.node_count());
                for (pa, pb) in a.nodes.iter().zip(b.nodes.iter()) {
                    prop_assert_eq!(pa.x.to_bits(), pb.x.to_bits());
                    prop_assert_eq!(pa.y.to_bits(), pb.y.to_bits());
                }
            }
        }
    }
}
