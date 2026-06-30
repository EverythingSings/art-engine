#![deny(unsafe_code)]
//! Diffusion-limited aggregation engine.
//!
//! Random-walking particles ("walkers") drift through the canvas. When a
//! walker becomes adjacent to an already-stuck particle, it sticks too —
//! producing the classic dendritic / fractal-tree growth seen in mineral
//! deposition, electrostatic discharges, and bacterial colonies. The
//! field output is the "age map" of stuck particles, normalized to
//! `[0, 1]`: oldest sticks read 1.0, newest sticks read low values, empty
//! cells read 0.0. Mapping age through a palette gives a natural growth
//! gradient (early sticks dim, recent growth bright).
//!
//! # Determinism
//!
//! Same seed + same params + same step count = bit-identical field. Walkers
//! are spawned, moved, and sticked using a single seeded [`Xorshift64`].
//!
//! # JSON parameters
//!
//! ```json
//! {
//!   "walker_count": 800,
//!   "stick_probability": 1.0,
//!   "seed_pattern": "point",
//!   "max_walker_age": 5000
//! }
//! ```
//!
//! `seed_pattern` recognized values: `"point"` (single cell at canvas
//! center), `"line"` (horizontal line on bottom edge), `"edges"` (all four
//! edges seeded — classic ferrofluid-style growth), `"ring"` (small ring at
//! the center).

use art_engine_core::error::EngineError;
use art_engine_core::field::Field;
use art_engine_core::params::{param_f64, param_string, param_usize};
use art_engine_core::prng::Xorshift64;
use art_engine_core::Engine;
use serde_json::{json, Value};

/// Default number of walkers maintained at any time.
const DEFAULT_WALKER_COUNT: usize = 800;
/// Default per-contact stick probability (1.0 = always stick on first touch).
const DEFAULT_STICK_PROBABILITY: f64 = 1.0;
/// Default initial seed pattern.
const DEFAULT_SEED_PATTERN: &str = "point";
/// Default maximum walker age in steps before it respawns at a new location.
const DEFAULT_MAX_WALKER_AGE: usize = 5000;
/// Hard upper bound on `walker_count` to prevent OOM from untrusted input.
const WALKER_LIMIT: usize = 200_000;

/// One frame's growth step. Each walker may take a single neighbor jump.
///
/// Sticking happens when a walker has at least one occupied 4-neighbor.
/// On stick, the walker's current cell becomes occupied and the walker
/// respawns at a fresh random edge position.
#[derive(Debug, Clone, Copy)]
struct Walker {
    x: i32,
    y: i32,
    age: usize,
}

/// Tunable parameters for the DLA simulation.
#[derive(Debug, Clone)]
pub struct DlaParams {
    /// Number of walkers maintained simultaneously.
    pub walker_count: usize,
    /// Probability that a walker sticks when adjacent to the cluster.
    pub stick_probability: f64,
    /// Initial seed pattern: "point" / "line" / "edges" / "ring".
    pub seed_pattern: String,
    /// Maximum age (in steps) a walker can live before being respawned.
    pub max_walker_age: usize,
}

impl Default for DlaParams {
    fn default() -> Self {
        Self {
            walker_count: DEFAULT_WALKER_COUNT,
            stick_probability: DEFAULT_STICK_PROBABILITY,
            seed_pattern: DEFAULT_SEED_PATTERN.to_string(),
            max_walker_age: DEFAULT_MAX_WALKER_AGE,
        }
    }
}

impl DlaParams {
    /// Extracts parameters from a JSON object, falling back to defaults.
    pub fn from_json(params: &Value) -> Self {
        Self {
            walker_count: param_usize(params, "walker_count", DEFAULT_WALKER_COUNT)
                .clamp(1, WALKER_LIMIT),
            stick_probability: param_f64(params, "stick_probability", DEFAULT_STICK_PROBABILITY)
                .clamp(0.0, 1.0),
            seed_pattern: param_string(params, "seed_pattern", DEFAULT_SEED_PATTERN),
            max_walker_age: param_usize(params, "max_walker_age", DEFAULT_MAX_WALKER_AGE).max(1),
        }
    }
}

/// DLA engine.
///
/// Holds an occupancy bitmap, an age map, the live walker pool, and a
/// deterministic PRNG. The age map is normalized at field-read time so
/// the highest-age cell maps to `t=1.0`.
pub struct Dla {
    params: DlaParams,
    width: usize,
    height: usize,
    /// Whether each cell is part of the cluster.
    occupied: Vec<bool>,
    /// Stick age (step number when this cell stuck), 0 for unoccupied.
    age_map: Vec<usize>,
    /// Total stick count, used as the current "age" stamp on new sticks.
    stick_counter: usize,
    /// Active walkers.
    walkers: Vec<Walker>,
    rng: Xorshift64,
    /// Output field (rebuilt from age_map each `step()`).
    field: Field,
}

impl Dla {
    /// Constructs a new DLA engine, seeding the cluster according to params.
    pub fn new(
        width: usize,
        height: usize,
        seed: u64,
        params: DlaParams,
    ) -> Result<Self, EngineError> {
        let mut field = Field::new(width, height)?;
        let len = width
            .checked_mul(height)
            .ok_or(EngineError::InvalidDimensions)?;
        let mut occupied = vec![false; len];
        let mut age_map = vec![0_usize; len];
        let mut rng = Xorshift64::new(seed);

        seed_cluster(
            &mut occupied,
            &mut age_map,
            width,
            height,
            &params.seed_pattern,
        );

        // Spawn initial walker pool at random positions, biased toward
        // canvas edges (away from the central seed) for faster growth.
        let walkers = (0..params.walker_count)
            .map(|_| spawn_walker(&mut rng, width, height))
            .collect();

        // Initial field reflects only the seed: most cells 0, seed cells 1.
        rebuild_field(&mut field, &occupied);

        Ok(Self {
            params,
            width,
            height,
            occupied,
            age_map,
            stick_counter: 0,
            walkers,
            rng,
            field,
        })
    }

    /// Constructs from a JSON params object.
    pub fn from_json(
        width: usize,
        height: usize,
        seed: u64,
        params: &Value,
    ) -> Result<Self, EngineError> {
        Self::new(width, height, seed, DlaParams::from_json(params))
    }

    /// Total cells currently part of the cluster.
    pub fn cluster_size(&self) -> usize {
        self.occupied.iter().filter(|&&v| v).count()
    }
}

impl Engine for Dla {
    fn step(&mut self) -> Result<(), EngineError> {
        let w = self.width as i32;
        let h = self.height as i32;
        let stick_prob = self.params.stick_probability;
        let max_age = self.params.max_walker_age;

        for walker in self.walkers.iter_mut() {
            // Random 8-direction jump.
            let dir = self.rng.next_usize(8);
            let (dx, dy) = direction_offset(dir);
            walker.x += dx;
            walker.y += dy;
            walker.age += 1;

            // Wrap into canvas (toroidal walk for the diffusion phase).
            walker.x = walker.x.rem_euclid(w);
            walker.y = walker.y.rem_euclid(h);

            let idx = (walker.y as usize) * self.width + (walker.x as usize);

            // If this cell is already occupied, immediately respawn (we don't
            // overwrite existing crystal).
            if self.occupied[idx] {
                *walker = spawn_walker(&mut self.rng, self.width, self.height);
                continue;
            }

            // Stick if any 4-neighbor is occupied (and the stick lottery passes).
            if has_occupied_neighbor(&self.occupied, self.width, self.height, walker.x, walker.y)
                && (stick_prob >= 1.0 || self.rng.next_f64() < stick_prob)
            {
                self.stick_counter += 1;
                self.occupied[idx] = true;
                self.age_map[idx] = self.stick_counter;
                *walker = spawn_walker(&mut self.rng, self.width, self.height);
                continue;
            }

            // Old walkers respawn even without sticking, to prevent drifters
            // dominating the PRNG stream.
            if walker.age >= max_age {
                *walker = spawn_walker(&mut self.rng, self.width, self.height);
            }
        }

        rebuild_field_with_ages(
            &mut self.field,
            &self.occupied,
            &self.age_map,
            self.stick_counter,
        );
        Ok(())
    }

    fn field(&self) -> &Field {
        &self.field
    }

    fn params(&self) -> Value {
        json!({
            "walker_count": self.params.walker_count,
            "stick_probability": self.params.stick_probability,
            "seed_pattern": self.params.seed_pattern,
            "max_walker_age": self.params.max_walker_age,
        })
    }

    fn param_schema(&self) -> Value {
        json!({
            "walker_count": {
                "type": "integer",
                "default": DEFAULT_WALKER_COUNT,
                "min": 1,
                "max": WALKER_LIMIT,
                "description": "Number of walkers maintained at any time"
            },
            "stick_probability": {
                "type": "number",
                "default": DEFAULT_STICK_PROBABILITY,
                "min": 0.0,
                "max": 1.0,
                "description": "Per-contact stick probability (1 = stick on first touch)"
            },
            "seed_pattern": {
                "type": "string",
                "default": DEFAULT_SEED_PATTERN,
                "enum": ["point", "line", "edges", "ring"],
                "description": "Initial cluster shape"
            },
            "max_walker_age": {
                "type": "integer",
                "default": DEFAULT_MAX_WALKER_AGE,
                "min": 1,
                "description": "Steps before a non-sticking walker is respawned"
            }
        })
    }
}

/// Spawns a walker at a uniformly-random canvas position. We deliberately
/// don't bias toward edges — pure-uniform spawn is simpler and still produces
/// classic DLA fractals because center-spawned walkers wander before sticking.
fn spawn_walker(rng: &mut Xorshift64, width: usize, height: usize) -> Walker {
    Walker {
        x: rng.next_usize(width) as i32,
        y: rng.next_usize(height) as i32,
        age: 0,
    }
}

/// Returns the (dx, dy) offset for one of 8 directions.
fn direction_offset(dir: usize) -> (i32, i32) {
    match dir & 7 {
        0 => (1, 0),
        1 => (-1, 0),
        2 => (0, 1),
        3 => (0, -1),
        4 => (1, 1),
        5 => (-1, 1),
        6 => (1, -1),
        _ => (-1, -1),
    }
}

/// Checks whether the cell at `(x, y)` has any occupied 4-neighbor.
fn has_occupied_neighbor(occupied: &[bool], width: usize, height: usize, x: i32, y: i32) -> bool {
    let w = width as i32;
    let h = height as i32;
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let nx = (x + dx).rem_euclid(w);
        let ny = (y + dy).rem_euclid(h);
        let idx = (ny as usize) * width + (nx as usize);
        if occupied[idx] {
            return true;
        }
    }
    false
}

/// Stamps the cluster seed into `occupied` and `age_map` according to the
/// requested pattern. Each seed cell is given age 1 so it lights up at the
/// dim end of the gradient.
fn seed_cluster(
    occupied: &mut [bool],
    age_map: &mut [usize],
    width: usize,
    height: usize,
    pattern: &str,
) {
    let mut set = |x: usize, y: usize| {
        if x < width && y < height {
            let idx = y * width + x;
            occupied[idx] = true;
            age_map[idx] = 1;
        }
    };

    match pattern {
        "line" => {
            // Horizontal line at the bottom row.
            let y = height.saturating_sub(1);
            for x in 0..width {
                set(x, y);
            }
        }
        "edges" => {
            // All four edges (one-pixel-thick frame).
            for x in 0..width {
                set(x, 0);
                set(x, height.saturating_sub(1));
            }
            for y in 0..height {
                set(0, y);
                set(width.saturating_sub(1), y);
            }
        }
        "ring" => {
            // Small ring of radius ~6% of the shorter axis.
            let cx = (width / 2) as f64;
            let cy = (height / 2) as f64;
            let r = (width.min(height) as f64) * 0.06;
            let r_inner_sq = (r - 1.0).max(0.0).powi(2);
            let r_outer_sq = (r + 1.0).powi(2);
            for y in 0..height {
                for x in 0..width {
                    let dx = x as f64 - cx;
                    let dy = y as f64 - cy;
                    let d2 = dx * dx + dy * dy;
                    if d2 >= r_inner_sq && d2 <= r_outer_sq {
                        set(x, y);
                    }
                }
            }
        }
        // "point" or unrecognized: single cell at canvas center.
        _ => {
            set(width / 2, height / 2);
        }
    }
}

/// Rebuilds the field with seed-only data: 1.0 at occupied cells, 0.0
/// elsewhere. Used during construction before any step has accumulated ages.
fn rebuild_field(field: &mut Field, occupied: &[bool]) {
    for (dst, &occ) in field.data_mut().iter_mut().zip(occupied.iter()) {
        *dst = if occ { 1.0 } else { 0.0 };
    }
}

/// Rebuilds the field with age-based gradients.
///
/// Cells stuck most recently get the highest values (palette top), giving
/// growing dendrites a luminous edge while older cluster cores fade. We
/// invert the age via `age / max_age` for "newer = brighter", which is
/// the opposite of stick order but matches how the eye reads growth.
fn rebuild_field_with_ages(
    field: &mut Field,
    occupied: &[bool],
    age_map: &[usize],
    max_age: usize,
) {
    if max_age == 0 {
        rebuild_field(field, occupied);
        return;
    }
    let max_age_f = max_age as f64;
    for (i, dst) in field.data_mut().iter_mut().enumerate() {
        if !occupied[i] {
            *dst = 0.0;
            continue;
        }
        // age is in [1, stick_counter]. Map to (0, 1].
        let age = age_map[i] as f64;
        // Use a soft compression so very old cells aren't completely dark:
        // t = 0.25 + 0.75 * (age / max_age). The 0.25 floor keeps the seed
        // visible at dim amber.
        *dst = (0.25 + 0.75 * (age / max_age_f)).clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(w: usize, h: usize, seed: u64) -> Dla {
        Dla::new(w, h, seed, DlaParams::default()).unwrap()
    }

    // ---- Construction ----

    #[test]
    fn new_creates_field_with_correct_dims() {
        let d = make(64, 32, 42);
        assert_eq!(d.field().width(), 64);
        assert_eq!(d.field().height(), 32);
    }

    #[test]
    fn new_with_zero_dimensions_returns_error() {
        let p = DlaParams::default();
        assert!(Dla::new(0, 16, 42, p.clone()).is_err());
        assert!(Dla::new(16, 0, 42, p).is_err());
    }

    #[test]
    fn point_seed_creates_single_cell() {
        let d = make(33, 33, 42);
        let nonzero: Vec<_> = d.occupied.iter().enumerate().filter(|(_, &v)| v).collect();
        assert_eq!(nonzero.len(), 1, "point seed should occupy exactly 1 cell");
    }

    #[test]
    fn line_seed_creates_full_row() {
        let p = DlaParams {
            seed_pattern: "line".into(),
            ..Default::default()
        };
        let d = Dla::new(40, 20, 42, p).unwrap();
        assert_eq!(d.cluster_size(), 40);
    }

    #[test]
    fn edges_seed_creates_frame() {
        let p = DlaParams {
            seed_pattern: "edges".into(),
            ..Default::default()
        };
        let d = Dla::new(10, 10, 42, p).unwrap();
        // 10*4 - 4 corners = 36
        assert_eq!(d.cluster_size(), 36);
    }

    #[test]
    fn ring_seed_creates_some_cells() {
        let p = DlaParams {
            seed_pattern: "ring".into(),
            ..Default::default()
        };
        let d = Dla::new(80, 80, 42, p).unwrap();
        let n = d.cluster_size();
        assert!(
            n > 5 && n < 300,
            "ring should be a thin annulus, got {n} cells"
        );
    }

    #[test]
    fn unknown_seed_pattern_falls_back_to_point() {
        let p = DlaParams {
            seed_pattern: "warp_drive".into(),
            ..Default::default()
        };
        let d = Dla::new(11, 11, 42, p).unwrap();
        assert_eq!(d.cluster_size(), 1);
    }

    // ---- Growth ----

    #[test]
    fn cluster_grows_over_steps() {
        let mut d = make(64, 64, 42);
        let initial = d.cluster_size();
        for _ in 0..200 {
            d.step().unwrap();
        }
        let after = d.cluster_size();
        assert!(
            after > initial,
            "cluster should grow: initial={initial}, after={after}"
        );
    }

    #[test]
    fn no_sticks_when_probability_zero() {
        let p = DlaParams {
            stick_probability: 0.0,
            ..Default::default()
        };
        let mut d = Dla::new(32, 32, 42, p).unwrap();
        let initial = d.cluster_size();
        for _ in 0..100 {
            d.step().unwrap();
        }
        assert_eq!(d.cluster_size(), initial, "no sticks expected at p=0");
    }

    #[test]
    fn field_values_in_unit_interval() {
        let mut d = make(40, 40, 42);
        for _ in 0..150 {
            d.step().unwrap();
        }
        for &v in d.field().data() {
            assert!((0.0..=1.0).contains(&v) && !v.is_nan(), "out of range: {v}");
        }
    }

    // ---- Determinism ----

    #[test]
    fn determinism_same_seed_and_params() {
        let mut a = make(48, 48, 12345);
        let mut b = make(48, 48, 12345);
        for _ in 0..80 {
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
    fn different_seeds_produce_different_state() {
        let mut a = make(32, 32, 1);
        let mut b = make(32, 32, 2);
        for _ in 0..100 {
            a.step().unwrap();
            b.step().unwrap();
        }
        assert!(a
            .field()
            .data()
            .iter()
            .zip(b.field().data().iter())
            .any(|(va, vb)| va.to_bits() != vb.to_bits()));
    }

    // ---- JSON ----

    #[test]
    fn from_json_uses_defaults() {
        let d = Dla::from_json(16, 16, 42, &json!({})).unwrap();
        assert_eq!(d.params.walker_count, DEFAULT_WALKER_COUNT);
        assert_eq!(d.params.seed_pattern, DEFAULT_SEED_PATTERN);
    }

    #[test]
    fn from_json_extracts_custom_values() {
        let d = Dla::from_json(
            16,
            16,
            42,
            &json!({
                "walker_count": 50,
                "stick_probability": 0.5,
                "seed_pattern": "line",
                "max_walker_age": 100
            }),
        )
        .unwrap();
        assert_eq!(d.params.walker_count, 50);
        assert!((d.params.stick_probability - 0.5).abs() < 1e-12);
        assert_eq!(d.params.seed_pattern, "line");
        assert_eq!(d.params.max_walker_age, 100);
    }

    #[test]
    fn from_json_caps_walker_count() {
        let d = Dla::from_json(8, 8, 42, &json!({"walker_count": 9_999_999})).unwrap();
        assert_eq!(d.params.walker_count, WALKER_LIMIT);
    }

    #[test]
    fn from_json_clamps_stick_probability() {
        let high = Dla::from_json(8, 8, 42, &json!({"stick_probability": 5.0})).unwrap();
        assert_eq!(high.params.stick_probability, 1.0);
        let low = Dla::from_json(8, 8, 42, &json!({"stick_probability": -1.0})).unwrap();
        assert_eq!(low.params.stick_probability, 0.0);
    }

    // ---- Engine trait ----

    #[test]
    fn params_returns_current_values() {
        let d = make(8, 8, 42);
        let v = d.params();
        assert_eq!(
            v["walker_count"].as_u64().unwrap() as usize,
            DEFAULT_WALKER_COUNT
        );
    }

    #[test]
    fn param_schema_has_all_keys() {
        let d = make(8, 8, 42);
        let s = d.param_schema();
        for k in [
            "walker_count",
            "stick_probability",
            "seed_pattern",
            "max_walker_age",
        ] {
            assert!(s.get(k).is_some(), "schema missing {k}");
        }
    }

    #[test]
    fn engine_is_object_safe() {
        let d = make(8, 8, 42);
        let _: Box<dyn Engine> = Box::new(d);
    }

    #[test]
    fn hue_field_is_none() {
        let d = make(8, 8, 42);
        assert!(d.hue_field().is_none());
    }

    // ---- Property-based ----

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn deterministic_for_any_seed(seed: u64) {
                let mut a = make(24, 24, seed);
                let mut b = make(24, 24, seed);
                for _ in 0..30 {
                    a.step().unwrap();
                    b.step().unwrap();
                }
                for (va, vb) in a.field().data().iter().zip(b.field().data().iter()) {
                    prop_assert_eq!(va.to_bits(), vb.to_bits());
                }
            }

            #[test]
            fn no_nans_for_any_seed(seed: u64) {
                let mut d = make(20, 20, seed);
                for _ in 0..50 {
                    d.step().unwrap();
                }
                for &v in d.field().data() {
                    prop_assert!(!v.is_nan());
                    prop_assert!((0.0..=1.0).contains(&v));
                }
            }
        }
    }
}
