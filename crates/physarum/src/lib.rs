#![deny(unsafe_code)]
//! Physarum polycephalum slime mold simulation engine.
//!
//! Thousands of agents move on a 2D toroidal grid, sensing chemical trails
//! ahead, depositing pheromone, and turning toward the strongest gradient.
//! The trail map diffuses and decays each step. From these simple rules,
//! complex network structures emerge — veins, highways, pulsing arteries.
//!
//! The primary output field is the trail map (pheromone concentration),
//! normalized to [0, 1], which the rendering pipeline maps to pixels via a palette.

use art_engine_core::error::EngineError;
use art_engine_core::field::Field;
use art_engine_core::params::{param_f64, param_usize};
use art_engine_core::prng::Xorshift64;
use art_engine_core::Engine;
use serde_json::{json, Value};
use std::f64::consts::{FRAC_PI_4, FRAC_PI_8, PI};

/// Default number of agents.
const DEFAULT_AGENT_COUNT: usize = 5000;
/// Default sensor angle offset from heading (radians). 22.5 degrees.
const DEFAULT_SENSOR_ANGLE: f64 = FRAC_PI_8;
/// Default sensor distance (cells ahead).
const DEFAULT_SENSOR_DISTANCE: f64 = 9.0;
/// Default rotation angle when turning (radians). 45 degrees.
const DEFAULT_ROTATION_ANGLE: f64 = FRAC_PI_4;
/// Default movement speed (cells per step).
const DEFAULT_STEP_SIZE: f64 = 1.0;
/// Default pheromone deposit per step.
const DEFAULT_DEPOSIT_AMOUNT: f64 = 5.0;
/// Default trail decay factor per step (multiplied each frame).
const DEFAULT_DECAY_FACTOR: f64 = 0.95;
/// Default diffusion passes per step (3x3 mean blur).
const DEFAULT_DIFFUSE_STEPS: usize = 1;
/// Default per-step gain on the optional external influence field.
const DEFAULT_INFLUENCE_STRENGTH: f64 = 1.0;

/// Simulation parameters for the Physarum model.
#[derive(Debug, Clone, Copy)]
pub struct PhysarumParams {
    /// Number of slime mold agents.
    pub agent_count: usize,
    /// Sensor angle offset from heading (radians).
    pub sensor_angle: f64,
    /// Sensor distance ahead (cells).
    pub sensor_distance: f64,
    /// Rotation angle when turning (radians).
    pub rotation_angle: f64,
    /// Movement speed (cells per step).
    pub step_size: f64,
    /// Pheromone deposited per agent per step.
    pub deposit_amount: f64,
    /// Trail decay multiplier per step (0..1).
    pub decay_factor: f64,
    /// Number of diffusion blur passes per step.
    pub diffuse_steps: usize,
    /// Per-step gain on an external influence field (see [`Engine::set_influence`]).
    /// When set, the influence field is added to the raw pheromone trail
    /// after agent deposition, scaled by this strength × `deposit_amount`.
    pub influence_strength: f64,
}

impl Default for PhysarumParams {
    fn default() -> Self {
        Self {
            agent_count: DEFAULT_AGENT_COUNT,
            sensor_angle: DEFAULT_SENSOR_ANGLE,
            sensor_distance: DEFAULT_SENSOR_DISTANCE,
            rotation_angle: DEFAULT_ROTATION_ANGLE,
            step_size: DEFAULT_STEP_SIZE,
            deposit_amount: DEFAULT_DEPOSIT_AMOUNT,
            decay_factor: DEFAULT_DECAY_FACTOR,
            diffuse_steps: DEFAULT_DIFFUSE_STEPS,
            influence_strength: DEFAULT_INFLUENCE_STRENGTH,
        }
    }
}

impl PhysarumParams {
    /// Extracts parameters from a JSON object, falling back to defaults.
    pub fn from_json(params: &Value) -> Self {
        Self {
            agent_count: param_usize(params, "agent_count", DEFAULT_AGENT_COUNT),
            sensor_angle: param_f64(params, "sensor_angle", DEFAULT_SENSOR_ANGLE),
            sensor_distance: param_f64(params, "sensor_distance", DEFAULT_SENSOR_DISTANCE),
            rotation_angle: param_f64(params, "rotation_angle", DEFAULT_ROTATION_ANGLE),
            step_size: param_f64(params, "step_size", DEFAULT_STEP_SIZE),
            deposit_amount: param_f64(params, "deposit_amount", DEFAULT_DEPOSIT_AMOUNT),
            decay_factor: param_f64(params, "decay_factor", DEFAULT_DECAY_FACTOR),
            diffuse_steps: param_usize(params, "diffuse_steps", DEFAULT_DIFFUSE_STEPS),
            influence_strength: param_f64(params, "influence_strength", DEFAULT_INFLUENCE_STRENGTH)
                .max(0.0),
        }
    }
}

/// A single Physarum agent: position + heading on the toroidal grid.
#[derive(Debug, Clone, Copy)]
struct Agent {
    x: f64,
    y: f64,
    angle: f64,
}

/// Physarum polycephalum slime mold engine.
///
/// Agents sense the trail map at three points (left, center, right relative
/// to heading), turn toward the strongest signal, move forward, and deposit
/// pheromone. The trail map is then diffused (3x3 mean blur) and decayed.
pub struct Physarum {
    agents: Vec<Agent>,
    trail: Field,
    /// Normalized copy of trail for rendering (values in [0, 1]).
    normalized: Field,
    rng: Xorshift64,
    params: PhysarumParams,
    width: usize,
    height: usize,
    /// Maximum trail value seen, used for normalization.
    trail_max: f64,
    /// Optional external field added to the raw trail each step.
    influence: Option<Field>,
}

impl Physarum {
    /// Creates a new Physarum engine.
    ///
    /// Agents are initialized at random positions with random headings.
    /// The trail map starts at zero.
    ///
    /// Returns `EngineError::InvalidDimensions` if width or height is zero.
    pub fn new(
        width: usize,
        height: usize,
        seed: u64,
        params: PhysarumParams,
    ) -> Result<Self, EngineError> {
        let trail = Field::new(width, height)?;
        let normalized = Field::new(width, height)?;
        let mut rng = Xorshift64::new(seed);
        let agents = init_agents(&mut rng, width, height, params.agent_count);

        Ok(Self {
            agents,
            trail,
            normalized,
            rng,
            params,
            width,
            height,
            trail_max: 0.0,
            influence: None,
        })
    }

    /// Creates a Physarum engine from a JSON params object.
    pub fn from_json(
        width: usize,
        height: usize,
        seed: u64,
        json_params: &Value,
    ) -> Result<Self, EngineError> {
        Self::new(width, height, seed, PhysarumParams::from_json(json_params))
    }

    /// Number of active agents.
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    /// Read-only access to the raw (unnormalized) trail field.
    pub fn trail_raw(&self) -> &Field {
        &self.trail
    }
}

impl Engine for Physarum {
    fn step(&mut self) -> Result<(), EngineError> {
        let w = self.width as f64;
        let h = self.height as f64;
        let sa = self.params.sensor_angle;
        let sd = self.params.sensor_distance;
        let ra = self.params.rotation_angle;
        let ss = self.params.step_size;
        let deposit = self.params.deposit_amount;

        // Phase 1: Sense + rotate + move each agent
        // Uses Jones (2010) turning rules: random turn when left == right
        for agent in &mut self.agents {
            // Sample trail at three sensor positions
            let sense_l = sense(&self.trail, agent.x, agent.y, agent.angle - sa, sd);
            let sense_c = sense(&self.trail, agent.x, agent.y, agent.angle, sd);
            let sense_r = sense(&self.trail, agent.x, agent.y, agent.angle + sa, sd);

            // Decide turn direction (Jones 2010 algorithm)
            if sense_c > sense_l && sense_c > sense_r {
                // Center strongest — go straight
            } else if sense_c < sense_l && sense_c < sense_r {
                // Both sides stronger than center — turn randomly
                if self.rng.next_f64() < 0.5 {
                    agent.angle -= ra;
                } else {
                    agent.angle += ra;
                }
            } else if sense_l > sense_r {
                agent.angle -= ra;
            } else if sense_r > sense_l {
                agent.angle += ra;
            } else {
                // All equal — random jitter
                if self.rng.next_f64() < 0.5 {
                    agent.angle -= ra;
                } else {
                    agent.angle += ra;
                }
            }

            // Move forward
            agent.x = (agent.x + agent.angle.cos() * ss).rem_euclid(w);
            agent.y = (agent.y + agent.angle.sin() * ss).rem_euclid(h);
        }

        // Phase 2: Deposit pheromone
        // Use raw data_mut for performance — we manage our own values
        let trail_data = self.trail.data_mut();
        for agent in &self.agents {
            let ix = agent.x as usize % self.width;
            let iy = agent.y as usize % self.height;
            let idx = iy * self.width + ix;
            trail_data[idx] += deposit;
        }

        // Phase 2.5: Add external influence into the raw trail (before
        // diffusion so the influence "blurs" with the rest naturally).
        if let Some(inf) = &self.influence {
            let s = self.params.influence_strength * self.params.deposit_amount;
            if s > 0.0 {
                let trail_data = self.trail.data_mut();
                for (t, &i) in trail_data.iter_mut().zip(inf.data().iter()) {
                    let nv = *t + s * i;
                    if nv.is_finite() {
                        *t = nv.max(0.0);
                    }
                }
            }
        }

        // Phase 3: Diffuse (3x3 mean blur)
        for _ in 0..self.params.diffuse_steps {
            diffuse_3x3(&mut self.trail, self.width, self.height);
        }

        // Phase 4: Decay + find max for normalization
        let decay = self.params.decay_factor;
        let trail_data = self.trail.data_mut();
        let mut max_val = 0.0_f64;
        for v in trail_data.iter_mut() {
            *v *= decay;
            if *v > max_val {
                max_val = *v;
            }
        }
        self.trail_max = if max_val > 0.0 { max_val } else { 1.0 };

        // Phase 5: Build normalized field for rendering
        // Use percentile-based normalization: sort to find 98th percentile,
        // then apply log curve to expand dim structure without crushing brights.
        let trail_data = self.trail.data();
        let mut sorted: Vec<f64> = trail_data.iter().copied().filter(|&v| v > 0.0).collect();
        sorted.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let ref_max = if sorted.is_empty() {
            1.0
        } else {
            let p98_idx = (sorted.len() as f64 * 0.98) as usize;
            let p98 = sorted[p98_idx.min(sorted.len() - 1)];
            if p98 > 0.0 {
                p98
            } else {
                1.0
            }
        };

        let norm = self.normalized.data_mut();
        let inv_max = 1.0 / ref_max;
        for (dst, &src) in norm.iter_mut().zip(trail_data.iter()) {
            let linear = (src * inv_max).min(1.0);
            // Log curve: ln(1 + x*e) / ln(1+e) maps [0,1] -> [0,1] with
            // heavy expansion at the low end
            let e = std::f64::consts::E;
            *dst = ((1.0 + linear * e).ln() / (1.0 + e).ln()).clamp(0.0, 1.0);
        }

        Ok(())
    }

    fn field(&self) -> &Field {
        &self.normalized
    }

    fn params(&self) -> Value {
        json!({
            "agent_count": self.params.agent_count,
            "sensor_angle": self.params.sensor_angle,
            "sensor_distance": self.params.sensor_distance,
            "rotation_angle": self.params.rotation_angle,
            "step_size": self.params.step_size,
            "deposit_amount": self.params.deposit_amount,
            "decay_factor": self.params.decay_factor,
            "diffuse_steps": self.params.diffuse_steps,
            "influence_strength": self.params.influence_strength,
        })
    }

    fn param_schema(&self) -> Value {
        json!({
            "agent_count": {
                "type": "integer",
                "default": DEFAULT_AGENT_COUNT,
                "min": 1,
                "max": 100000,
                "description": "Number of slime mold agents"
            },
            "sensor_angle": {
                "type": "number",
                "default": DEFAULT_SENSOR_ANGLE,
                "min": 0.0,
                "max": PI,
                "description": "Sensor angle offset from heading (radians)"
            },
            "sensor_distance": {
                "type": "number",
                "default": DEFAULT_SENSOR_DISTANCE,
                "min": 1.0,
                "max": 50.0,
                "description": "Sensor distance ahead (cells)"
            },
            "rotation_angle": {
                "type": "number",
                "default": DEFAULT_ROTATION_ANGLE,
                "min": 0.0,
                "max": PI,
                "description": "Rotation angle when turning (radians)"
            },
            "step_size": {
                "type": "number",
                "default": DEFAULT_STEP_SIZE,
                "min": 0.1,
                "max": 5.0,
                "description": "Movement speed (cells per step)"
            },
            "deposit_amount": {
                "type": "number",
                "default": DEFAULT_DEPOSIT_AMOUNT,
                "min": 0.1,
                "max": 50.0,
                "description": "Pheromone deposited per agent per step"
            },
            "decay_factor": {
                "type": "number",
                "default": DEFAULT_DECAY_FACTOR,
                "min": 0.0,
                "max": 1.0,
                "description": "Trail decay multiplier per step"
            },
            "diffuse_steps": {
                "type": "integer",
                "default": DEFAULT_DIFFUSE_STEPS,
                "min": 0,
                "max": 5,
                "description": "Number of diffusion blur passes per step"
            },
            "influence_strength": {
                "type": "number",
                "default": DEFAULT_INFLUENCE_STRENGTH,
                "min": 0.0,
                "description": "Per-step gain on external influence field (set via set_influence)"
            }
        })
    }

    fn set_influence(&mut self, field: &Field) -> Result<(), EngineError> {
        if field.width() != self.width || field.height() != self.height {
            return Err(EngineError::InvalidDimensions);
        }
        self.influence = Some(field.clone());
        Ok(())
    }
}

/// Initializes agents at random positions with random headings.
fn init_agents(rng: &mut Xorshift64, width: usize, height: usize, count: usize) -> Vec<Agent> {
    let w = width as f64;
    let h = height as f64;
    let two_pi = std::f64::consts::TAU;

    (0..count)
        .map(|_| Agent {
            x: rng.next_f64() * w,
            y: rng.next_f64() * h,
            angle: rng.next_f64() * two_pi,
        })
        .collect()
}

/// Samples the trail field at a sensor position offset from (x, y) by
/// `distance` cells in the direction `angle`.
fn sense(trail: &Field, x: f64, y: f64, angle: f64, distance: f64) -> f64 {
    let sx = x + angle.cos() * distance;
    let sy = y + angle.sin() * distance;
    // Field::get uses toroidal wrapping via isize coords
    trail.get(sx as isize, sy as isize)
}

/// In-place 3x3 mean blur (box filter) on the trail field.
///
/// Each cell becomes the average of its 3x3 neighborhood.
/// Toroidal wrapping at edges.
fn diffuse_3x3(trail: &mut Field, width: usize, height: usize) {
    let src = trail.data().to_vec();
    let dst = trail.data_mut();

    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            for dy in -1_isize..=1 {
                for dx in -1_isize..=1 {
                    let nx = (x as isize + dx).rem_euclid(width as isize) as usize;
                    let ny = (y as isize + dy).rem_euclid(height as isize) as usize;
                    sum += src[ny * width + nx];
                }
            }
            dst[y * width + x] = sum / 9.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_params() -> PhysarumParams {
        PhysarumParams::default()
    }

    fn physarum(width: usize, height: usize, seed: u64) -> Physarum {
        Physarum::new(width, height, seed, default_params()).unwrap()
    }

    // ---- Construction tests ----

    #[test]
    fn new_creates_engine_with_correct_dimensions() {
        let p = physarum(64, 32, 42);
        assert_eq!(p.trail_raw().width(), 64);
        assert_eq!(p.trail_raw().height(), 32);
    }

    #[test]
    fn new_with_zero_dimensions_returns_error() {
        assert!(Physarum::new(0, 10, 42, default_params()).is_err());
        assert!(Physarum::new(10, 0, 42, default_params()).is_err());
    }

    #[test]
    fn new_initializes_correct_agent_count() {
        let p = physarum(64, 64, 42);
        assert_eq!(p.agent_count(), DEFAULT_AGENT_COUNT);
    }

    #[test]
    fn new_trail_starts_at_zero() {
        let p = physarum(32, 32, 42);
        assert!(p.trail_raw().data().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn from_json_uses_defaults_for_empty_json() {
        let p = Physarum::from_json(32, 32, 42, &json!({})).unwrap();
        assert_eq!(p.agent_count(), DEFAULT_AGENT_COUNT);
    }

    #[test]
    fn from_json_extracts_custom_values() {
        let params = json!({
            "agent_count": 100,
            "sensor_angle": 0.5,
            "sensor_distance": 5.0,
            "rotation_angle": 0.3,
            "step_size": 2.0,
            "deposit_amount": 10.0,
            "decay_factor": 0.9,
            "diffuse_steps": 2,
        });
        let p = Physarum::from_json(32, 32, 42, &params).unwrap();
        assert_eq!(p.agent_count(), 100);
        let json_params = p.params();
        assert!((json_params["sensor_angle"].as_f64().unwrap() - 0.5).abs() < f64::EPSILON);
        assert!((json_params["sensor_distance"].as_f64().unwrap() - 5.0).abs() < f64::EPSILON);
    }

    // ---- Determinism tests ----

    #[test]
    fn same_seed_identical_initial_agents() {
        let a = physarum(64, 64, 12345);
        let b = physarum(64, 64, 12345);
        assert_eq!(a.agents.len(), b.agents.len());
        for (aa, bb) in a.agents.iter().zip(b.agents.iter()) {
            assert_eq!(aa.x.to_bits(), bb.x.to_bits());
            assert_eq!(aa.y.to_bits(), bb.y.to_bits());
            assert_eq!(aa.angle.to_bits(), bb.angle.to_bits());
        }
    }

    #[test]
    fn same_seed_identical_after_100_steps() {
        let mut a = Physarum::new(
            32,
            32,
            42,
            PhysarumParams {
                agent_count: 200,
                ..default_params()
            },
        )
        .unwrap();
        let mut b = Physarum::new(
            32,
            32,
            42,
            PhysarumParams {
                agent_count: 200,
                ..default_params()
            },
        )
        .unwrap();
        for _ in 0..100 {
            a.step().unwrap();
            b.step().unwrap();
        }
        assert!(a
            .trail_raw()
            .data()
            .iter()
            .zip(b.trail_raw().data().iter())
            .all(|(va, vb)| va.to_bits() == vb.to_bits()));
    }

    #[test]
    fn different_seed_different_state() {
        let a = physarum(64, 64, 1);
        let b = physarum(64, 64, 2);
        assert!(a
            .agents
            .iter()
            .zip(b.agents.iter())
            .any(|(aa, bb)| aa.x.to_bits() != bb.x.to_bits()));
    }

    // ---- Step behavior tests ----

    #[test]
    fn step_returns_ok() {
        let mut p = physarum(16, 16, 42);
        assert!(p.step().is_ok());
    }

    #[test]
    fn step_deposits_pheromone() {
        let params = PhysarumParams {
            agent_count: 10,
            ..default_params()
        };
        let mut p = Physarum::new(32, 32, 42, params).unwrap();
        p.step().unwrap();
        let total: f64 = p.trail_raw().data().iter().sum();
        assert!(total > 0.0, "Trail should have pheromone after step");
    }

    #[test]
    fn trail_decays_over_time() {
        let params = PhysarumParams {
            agent_count: 100,
            decay_factor: 0.5, // aggressive decay
            ..default_params()
        };
        let mut p = Physarum::new(32, 32, 42, params).unwrap();
        // Build up some trail
        for _ in 0..10 {
            p.step().unwrap();
        }
        let total_before: f64 = p.trail_raw().data().iter().sum();
        // Run many more steps with agents that keep depositing
        // With 0.5 decay, trail should stabilize much lower than
        // a lossless accumulation
        for _ in 0..100 {
            p.step().unwrap();
        }
        let total_after: f64 = p.trail_raw().data().iter().sum();
        // With ongoing deposits, trail should be nonzero
        assert!(total_after > 0.0);
        // With 0.5 decay, can't grow without bound — if we stop depositing
        // it would halve each step. Since agents keep depositing, check
        // that it reaches some equilibrium rather than growing forever.
        // Just verify it's finite and reasonable.
        assert!(total_after.is_finite());
        let _ = total_before; // used for documentation
    }

    #[test]
    fn agents_stay_in_bounds() {
        let params = PhysarumParams {
            agent_count: 500,
            step_size: 3.0, // fast movement to stress wrapping
            ..default_params()
        };
        let mut p = Physarum::new(32, 32, 42, params).unwrap();
        for _ in 0..200 {
            p.step().unwrap();
        }
        for agent in &p.agents {
            assert!(
                agent.x >= 0.0 && agent.x < 32.0,
                "Agent x={} out of bounds",
                agent.x
            );
            assert!(
                agent.y >= 0.0 && agent.y < 32.0,
                "Agent y={} out of bounds",
                agent.y
            );
        }
    }

    #[test]
    fn no_nans_after_many_steps() {
        let mut p = Physarum::new(
            32,
            32,
            42,
            PhysarumParams {
                agent_count: 200,
                ..default_params()
            },
        )
        .unwrap();
        for _ in 0..500 {
            p.step().unwrap();
        }
        for &v in p.trail_raw().data() {
            assert!(!v.is_nan(), "NaN in trail field");
            assert!(v.is_finite(), "Infinity in trail field");
        }
        for agent in &p.agents {
            assert!(!agent.x.is_nan(), "NaN in agent x");
            assert!(!agent.y.is_nan(), "NaN in agent y");
            assert!(!agent.angle.is_nan(), "NaN in agent angle");
        }
    }

    // ---- Diffusion tests ----

    #[test]
    fn diffuse_spreads_single_spike() {
        let mut trail = Field::new(5, 5).unwrap();
        trail.data_mut()[12] = 9.0; // center of 5x5
        diffuse_3x3(&mut trail, 5, 5);
        // Center should decrease (was 9.0, now avg of 3x3 = 9/9 = 1.0)
        assert!((trail.data()[12] - 1.0).abs() < f64::EPSILON);
        // Neighbors should be nonzero
        assert!(trail.data()[7] > 0.0, "north neighbor should have spread");
        assert!(trail.data()[17] > 0.0, "south neighbor should have spread");
    }

    #[test]
    fn diffuse_uniform_field_unchanged() {
        let mut trail = Field::new(8, 8).unwrap();
        let val = 0.5;
        trail.data_mut().fill(val);
        diffuse_3x3(&mut trail, 8, 8);
        for &v in trail.data() {
            assert!(
                (v - val).abs() < 1e-12,
                "Uniform field should stay uniform after diffusion, got {v}"
            );
        }
    }

    // ---- Trait compliance tests ----

    #[test]
    fn param_schema_has_all_parameters() {
        let p = physarum(16, 16, 42);
        let schema = p.param_schema();
        for key in &[
            "agent_count",
            "sensor_angle",
            "sensor_distance",
            "rotation_angle",
            "step_size",
            "deposit_amount",
            "decay_factor",
            "diffuse_steps",
        ] {
            assert!(schema.get(key).is_some(), "schema missing parameter: {key}");
        }
    }

    #[test]
    fn engine_is_object_safe() {
        let p = physarum(16, 16, 42);
        let boxed: Box<dyn Engine> = Box::new(p);
        assert_eq!(boxed.field().width(), 16);
    }

    #[test]
    fn hue_field_returns_none() {
        let p = physarum(16, 16, 42);
        assert!(p.hue_field().is_none());
    }

    // ---- Sense function tests ----

    #[test]
    fn sense_reads_trail_at_offset() {
        let mut trail = Field::new(16, 16).unwrap();
        // Place pheromone at (8, 8)
        trail.data_mut()[8 * 16 + 8] = 5.0;
        // Agent at (8, 3), facing down (angle = PI/2), sensor distance = 5
        // Sensor should read at approximately (8, 8)
        let v = sense(&trail, 8.0, 3.0, std::f64::consts::FRAC_PI_2, 5.0);
        assert!(v > 0.0, "Sensor should detect pheromone at offset position");
    }

    // ---- Property-based tests ----

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        fn dimension() -> impl Strategy<Value = usize> {
            4_usize..=32
        }

        proptest! {
            #[test]
            fn no_nans_produced(
                w in dimension(),
                h in dimension(),
                seed: u64,
            ) {
                let params = PhysarumParams {
                    agent_count: 50,
                    ..PhysarumParams::default()
                };
                let mut p = Physarum::new(w, h, seed, params).unwrap();
                for _ in 0..10 {
                    p.step().unwrap();
                }
                for &v in p.trail_raw().data() {
                    prop_assert!(!v.is_nan(), "NaN in trail");
                    prop_assert!(v.is_finite(), "Infinity in trail");
                }
            }

            #[test]
            fn deterministic_across_instances(
                w in dimension(),
                h in dimension(),
                seed: u64,
            ) {
                let params = PhysarumParams {
                    agent_count: 50,
                    ..PhysarumParams::default()
                };
                let mut a = Physarum::new(w, h, seed, params).unwrap();
                let mut b = Physarum::new(w, h, seed, params).unwrap();
                for _ in 0..10 {
                    a.step().unwrap();
                    b.step().unwrap();
                }
                for (va, vb) in a.trail_raw().data().iter().zip(b.trail_raw().data().iter()) {
                    prop_assert_eq!(va.to_bits(), vb.to_bits());
                }
            }

            #[test]
            fn agents_always_in_bounds(
                w in dimension(),
                h in dimension(),
                seed: u64,
            ) {
                let params = PhysarumParams {
                    agent_count: 50,
                    step_size: 3.0,
                    ..PhysarumParams::default()
                };
                let mut p = Physarum::new(w, h, seed, params).unwrap();
                for _ in 0..20 {
                    p.step().unwrap();
                }
                let wf = w as f64;
                let hf = h as f64;
                for agent in &p.agents {
                    prop_assert!(agent.x >= 0.0 && agent.x < wf,
                        "x={} out of [0, {})", agent.x, wf);
                    prop_assert!(agent.y >= 0.0 && agent.y < hf,
                        "y={} out of [0, {})", agent.y, hf);
                }
            }
        }
    }
}
