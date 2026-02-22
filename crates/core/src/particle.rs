//! CPU-side particle simulation for the art-engine rendering pipeline.
//!
//! Provides position/velocity/force integration, emission patterns, culling,
//! and density-field rasterization. Particle data uses `f32` and [`glam::Vec2`]
//! for direct GPU VBO upload. External forces come from [`FieldSource`] trait
//! objects — gravity, noise, vortices, attractors are all composable.
//!
//! `ParticleSystem` does **not** implement the `Engine` trait — engines produce fields
//! from simulation, while particles are a layer content type with individual
//! position data that the GPU renderer reads directly.

use glam::Vec2;
use serde_json::Value;

use crate::error::EngineError;
use crate::field::Field;
use crate::field_source::FieldSource;
use crate::params::{param_f64, param_usize};
use crate::prng::Xorshift64;

/// Hard upper bound on `max_particles` to prevent OOM from untrusted JSON input.
pub const MAX_PARTICLES_LIMIT: usize = 1_000_000;

/// Individual particle state.
///
/// All spatial data is in normalized `[0, 1]` coordinates so the system is
/// resolution-independent. GPU renderers read [`ParticleSystem::particles`]
/// directly for VBO upload.
#[derive(Debug, Clone)]
pub struct Particle {
    /// Position in normalized `[0, 1]` space.
    pub position: Vec2,
    /// Velocity per step.
    pub velocity: Vec2,
    /// Accumulated acceleration for the current step (reset each frame).
    pub acceleration: Vec2,
    /// Frames since emission.
    pub age: f32,
    /// Total lifetime in frames. Particle is culled when `age >= lifetime`.
    pub lifetime: f32,
    /// Visual radius (normalized).
    pub size: f32,
    /// Palette lookup index in `[0, 1]`.
    pub color_index: f32,
    /// Glow intensity in `[0, 1]`.
    pub glow: f32,
}

/// How particles are emitted each step.
#[derive(Debug, Clone)]
pub enum EmissionPattern {
    /// Emit `rate` particles every step.
    Continuous { rate: usize },
    /// Emit `count` particles on the first step only.
    Burst { count: usize },
    /// Each step has `probability` chance of emitting one particle.
    Sporadic { probability: f64 },
}

/// Configuration for particle spawning.
#[derive(Debug, Clone)]
pub struct EmissionConfig {
    /// Emission pattern.
    pub pattern: EmissionPattern,
    /// Minimum corner of the spawn area.
    pub position_min: Vec2,
    /// Maximum corner of the spawn area.
    pub position_max: Vec2,
    /// Minimum initial velocity.
    pub velocity_min: Vec2,
    /// Maximum initial velocity.
    pub velocity_max: Vec2,
    /// `(min, max)` lifetime range in frames.
    pub lifetime_range: (f32, f32),
    /// `(min, max)` visual size range.
    pub size_range: (f32, f32),
    /// `(min, max)` glow intensity range.
    pub glow_range: (f32, f32),
}

/// Full particle system configuration.
#[derive(Debug, Clone)]
pub struct ParticleSystemConfig {
    /// Hard cap on live particles.
    pub max_particles: usize,
    /// Emission configuration.
    pub emission: EmissionConfig,
    /// Velocity damping per step. `0.0` = no drag, `1.0` = full stop.
    pub drag: f32,
}

/// CPU-side particle simulation.
///
/// Owns a pool of [`Particle`]s, a deterministic PRNG, and a set of
/// [`FieldSource`] forces. Call [`step`](Self::step) each frame to advance
/// the simulation.
pub struct ParticleSystem {
    particles: Vec<Particle>,
    rng: Xorshift64,
    config: ParticleSystemConfig,
    forces: Vec<Box<dyn FieldSource>>,
    time: f64,
    burst_fired: bool,
}

impl ParticleSystem {
    /// Creates a new particle system with the given configuration and seed.
    pub fn new(config: ParticleSystemConfig, seed: u64) -> Self {
        Self {
            particles: Vec::with_capacity(config.max_particles),
            rng: Xorshift64::new(seed),
            config,
            forces: Vec::new(),
            time: 0.0,
            burst_fired: false,
        }
    }

    /// Adds a force source (builder pattern).
    pub fn with_force(mut self, force: Box<dyn FieldSource>) -> Self {
        self.forces.push(force);
        self
    }

    /// Advances the simulation by one frame.
    ///
    /// 1. Emit new particles
    /// 2. Accumulate forces, apply drag, integrate velocity and position
    /// 3. Cull expired particles
    /// 4. Enforce `max_particles` cap
    pub fn step(&mut self) {
        self.emit();
        self.integrate();
        self.cull();
        self.time += 1.0;
    }

    /// Read-only slice of live particles for GPU upload.
    pub fn particles(&self) -> &[Particle] {
        &self.particles
    }

    /// Number of live particles.
    pub fn count(&self) -> usize {
        self.particles.len()
    }

    /// Whether there are no live particles.
    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    /// Rasterizes particle positions onto a [`Field`].
    ///
    /// Each particle deposits `1.0` at its grid cell. The result is normalized
    /// by dividing by the maximum cell value so all values lie in `[0, 1]`.
    /// Returns a zero-filled field if there are no particles.
    pub fn to_density_field(&self, width: usize, height: usize) -> Result<Field, EngineError> {
        let mut field = Field::new(width, height)?;
        if self.particles.is_empty() {
            return Ok(field);
        }
        let w = width as f32;
        let h = height as f32;
        for p in &self.particles {
            let gx = (p.position.x * w).clamp(0.0, w - 1.0) as usize;
            let gy = (p.position.y * h).clamp(0.0, h - 1.0) as usize;
            let current = field.get(gx as isize, gy as isize);
            // Bypass clamping via data_mut for accumulation
            field.data_mut()[gy * width + gx] = current + 1.0;
        }
        // Normalize by max value
        let max_val = field.data().iter().copied().fold(0.0_f64, f64::max);
        if max_val > 0.0 {
            field.data_mut().iter_mut().for_each(|v| *v /= max_val);
        }
        Ok(field)
    }

    /// Constructs a `ParticleSystem` from a JSON params object.
    ///
    /// Recognized keys (all optional with sensible defaults):
    /// - `max_particles` (usize, default 1000)
    /// - `drag` (f64, default 0.02)
    /// - `emission_rate` (usize, default 10) — continuous emission
    /// - `position_min_x/y`, `position_max_x/y` (f64, default 0.0/1.0)
    /// - `velocity_min_x/y`, `velocity_max_x/y` (f64, default -0.001/0.001)
    /// - `lifetime_min/max` (f64, default 60.0/180.0)
    /// - `size_min/max` (f64, default 0.002/0.01)
    /// - `glow_min/max` (f64, default 0.0/0.5)
    pub fn from_json(params: &Value, seed: u64) -> Self {
        let max_particles = param_usize(params, "max_particles", 1000).min(MAX_PARTICLES_LIMIT);
        let drag = param_f64(params, "drag", 0.02) as f32;
        let emission_rate = param_usize(params, "emission_rate", 10);

        let position_min = Vec2::new(
            param_f64(params, "position_min_x", 0.0) as f32,
            param_f64(params, "position_min_y", 0.0) as f32,
        );
        let position_max = Vec2::new(
            param_f64(params, "position_max_x", 1.0) as f32,
            param_f64(params, "position_max_y", 1.0) as f32,
        );
        let velocity_min = Vec2::new(
            param_f64(params, "velocity_min_x", -0.001) as f32,
            param_f64(params, "velocity_min_y", -0.001) as f32,
        );
        let velocity_max = Vec2::new(
            param_f64(params, "velocity_max_x", 0.001) as f32,
            param_f64(params, "velocity_max_y", 0.001) as f32,
        );
        let lifetime_range = (
            param_f64(params, "lifetime_min", 60.0) as f32,
            param_f64(params, "lifetime_max", 180.0) as f32,
        );
        let size_range = (
            param_f64(params, "size_min", 0.002) as f32,
            param_f64(params, "size_max", 0.01) as f32,
        );
        let glow_range = (
            param_f64(params, "glow_min", 0.0) as f32,
            param_f64(params, "glow_max", 0.5) as f32,
        );

        let config = ParticleSystemConfig {
            max_particles,
            emission: EmissionConfig {
                pattern: EmissionPattern::Continuous {
                    rate: emission_rate,
                },
                position_min,
                position_max,
                velocity_min,
                velocity_max,
                lifetime_range,
                size_range,
                glow_range,
            },
            drag,
        };

        Self::new(config, seed)
    }

    // -- Private helpers --

    /// Emits new particles according to the configured pattern.
    fn emit(&mut self) {
        let count = match self.config.emission.pattern {
            EmissionPattern::Continuous { rate } => rate,
            EmissionPattern::Burst { count } => {
                if self.burst_fired {
                    0
                } else {
                    self.burst_fired = true;
                    count
                }
            }
            EmissionPattern::Sporadic { probability } => {
                if self.rng.next_f64() < probability {
                    1
                } else {
                    0
                }
            }
        };

        let remaining_capacity = self
            .config
            .max_particles
            .saturating_sub(self.particles.len());
        let to_emit = count.min(remaining_capacity);

        for _ in 0..to_emit {
            let p = self.spawn_particle();
            self.particles.push(p);
        }
    }

    /// Creates a single particle with randomized initial state.
    fn spawn_particle(&mut self) -> Particle {
        let ec = &self.config.emission;
        let px = self
            .rng
            .next_range(ec.position_min.x as f64, ec.position_max.x as f64) as f32;
        let py = self
            .rng
            .next_range(ec.position_min.y as f64, ec.position_max.y as f64) as f32;
        let vx = self
            .rng
            .next_range(ec.velocity_min.x as f64, ec.velocity_max.x as f64) as f32;
        let vy = self
            .rng
            .next_range(ec.velocity_min.y as f64, ec.velocity_max.y as f64) as f32;
        let lifetime = self
            .rng
            .next_range(ec.lifetime_range.0 as f64, ec.lifetime_range.1 as f64)
            as f32;
        let size = self
            .rng
            .next_range(ec.size_range.0 as f64, ec.size_range.1 as f64) as f32;
        let glow = self
            .rng
            .next_range(ec.glow_range.0 as f64, ec.glow_range.1 as f64) as f32;
        let color_index = self.rng.next_f64() as f32;

        Particle {
            position: Vec2::new(px, py),
            velocity: Vec2::new(vx, vy),
            acceleration: Vec2::ZERO,
            age: 0.0,
            lifetime,
            size,
            color_index,
            glow,
        }
    }

    /// Accumulates forces, applies drag, integrates velocity and position.
    fn integrate(&mut self) {
        let time = self.time;
        let drag = self.config.drag;

        for particle in &mut self.particles {
            particle.acceleration = Vec2::ZERO;

            // Sample each force at particle position, accumulate into acceleration
            let (ax, ay) = self
                .forces
                .iter()
                .fold((0.0_f64, 0.0_f64), |(ax, ay), force| {
                    let (fx, fy) =
                        force.sample(particle.position.x as f64, particle.position.y as f64, time);
                    (ax + fx, ay + fy)
                });
            particle.acceleration = Vec2::new(ax as f32, ay as f32);

            // Apply drag
            particle.velocity *= 1.0 - drag;

            // Euler integration
            particle.velocity += particle.acceleration;
            particle.position += particle.velocity;

            particle.age += 1.0;
        }
    }

    /// Removes expired particles and enforces the max cap.
    fn cull(&mut self) {
        self.particles.retain(|p| p.age < p.lifetime);

        if self.particles.len() > self.config.max_particles {
            self.particles.truncate(self.config.max_particles);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field_source::PointAttractor;
    use serde_json::json;

    /// Helper: default emission config for tests.
    fn test_emission() -> EmissionConfig {
        EmissionConfig {
            pattern: EmissionPattern::Continuous { rate: 5 },
            position_min: Vec2::new(0.0, 0.0),
            position_max: Vec2::new(1.0, 1.0),
            velocity_min: Vec2::new(-0.01, -0.01),
            velocity_max: Vec2::new(0.01, 0.01),
            lifetime_range: (10.0, 20.0),
            size_range: (0.01, 0.02),
            glow_range: (0.0, 0.5),
        }
    }

    /// Helper: default config for tests.
    fn test_config() -> ParticleSystemConfig {
        ParticleSystemConfig {
            max_particles: 100,
            emission: test_emission(),
            drag: 0.0,
        }
    }

    // =======================================================================
    // Construction
    // =======================================================================

    #[test]
    fn new_creates_empty_system() {
        let sys = ParticleSystem::new(test_config(), 42);
        assert!(sys.is_empty());
        assert_eq!(sys.count(), 0);
    }

    #[test]
    fn with_force_adds_force() {
        let sys = ParticleSystem::new(test_config(), 42).with_force(Box::new(PointAttractor {
            x: 0.5,
            y: 0.5,
            strength: 1.0,
            radius: 1.0,
        }));
        assert_eq!(sys.forces.len(), 1);
    }

    // =======================================================================
    // Emission: Continuous
    // =======================================================================

    #[test]
    fn continuous_emission_spawns_correct_count_per_step() {
        let mut sys = ParticleSystem::new(test_config(), 42);
        sys.step();
        assert_eq!(sys.count(), 5, "first step should emit 5 particles");
        sys.step();
        assert_eq!(sys.count(), 10, "second step should emit 5 more");
    }

    #[test]
    fn continuous_emission_respects_max_particles() {
        let mut config = test_config();
        config.max_particles = 8;
        config.emission.pattern = EmissionPattern::Continuous { rate: 5 };
        let mut sys = ParticleSystem::new(config, 42);
        sys.step();
        assert_eq!(sys.count(), 5);
        sys.step();
        // Only 3 more should be emitted (8 - 5 = 3)
        assert_eq!(sys.count(), 8);
    }

    // =======================================================================
    // Emission: Burst
    // =======================================================================

    #[test]
    fn burst_emission_fires_once() {
        let mut config = test_config();
        config.emission.pattern = EmissionPattern::Burst { count: 20 };
        let mut sys = ParticleSystem::new(config, 42);
        sys.step();
        assert_eq!(sys.count(), 20, "burst should emit 20 on first step");
        sys.step();
        // No new particles on second step (burst already fired)
        assert_eq!(sys.count(), 20);
    }

    #[test]
    fn burst_emission_respects_max_particles() {
        let mut config = test_config();
        config.max_particles = 10;
        config.emission.pattern = EmissionPattern::Burst { count: 20 };
        let mut sys = ParticleSystem::new(config, 42);
        sys.step();
        assert_eq!(sys.count(), 10, "burst capped at max_particles");
    }

    // =======================================================================
    // Emission: Sporadic
    // =======================================================================

    #[test]
    fn sporadic_emission_probability_one_always_emits() {
        let mut config = test_config();
        config.emission.pattern = EmissionPattern::Sporadic { probability: 1.0 };
        let mut sys = ParticleSystem::new(config, 42);
        for _ in 0..10 {
            sys.step();
        }
        // With probability 1.0, should emit 1 particle per step = 10 total
        assert_eq!(sys.count(), 10);
    }

    #[test]
    fn sporadic_emission_probability_zero_never_emits() {
        let mut config = test_config();
        config.emission.pattern = EmissionPattern::Sporadic { probability: 0.0 };
        let mut sys = ParticleSystem::new(config, 42);
        for _ in 0..100 {
            sys.step();
        }
        assert_eq!(sys.count(), 0);
    }

    // =======================================================================
    // Forces
    // =======================================================================

    #[test]
    fn point_attractor_affects_velocity() {
        let mut config = test_config();
        config.emission.pattern = EmissionPattern::Burst { count: 1 };
        config.emission.position_min = Vec2::new(0.1, 0.1);
        config.emission.position_max = Vec2::new(0.1, 0.1);
        config.emission.velocity_min = Vec2::ZERO;
        config.emission.velocity_max = Vec2::ZERO;

        let mut sys = ParticleSystem::new(config, 42).with_force(Box::new(PointAttractor {
            x: 0.9,
            y: 0.9,
            strength: 0.1,
            radius: 1.0,
        }));

        sys.step(); // emit + first integration
        let vel_after_1 = sys.particles()[0].velocity;

        // Velocity should point toward attractor (0.9, 0.9) from (0.1, 0.1)
        assert!(
            vel_after_1.x > 0.0,
            "velocity.x should be positive toward attractor, got {}",
            vel_after_1.x
        );
        assert!(
            vel_after_1.y > 0.0,
            "velocity.y should be positive toward attractor, got {}",
            vel_after_1.y
        );
    }

    #[test]
    fn drag_slows_particles() {
        let mut config = test_config();
        config.emission.pattern = EmissionPattern::Burst { count: 1 };
        config.emission.velocity_min = Vec2::new(0.1, 0.1);
        config.emission.velocity_max = Vec2::new(0.1, 0.1);
        config.drag = 0.5;

        let mut sys = ParticleSystem::new(config, 42);
        sys.step();

        let vel = sys.particles()[0].velocity;
        // After one step with drag=0.5, velocity should be 0.1 * 0.5 = 0.05
        assert!(
            (vel.x - 0.05).abs() < 1e-5,
            "expected velocity.x ~ 0.05, got {}",
            vel.x
        );
    }

    // =======================================================================
    // Culling
    // =======================================================================

    #[test]
    fn expired_particles_are_removed() {
        let mut config = test_config();
        config.emission.pattern = EmissionPattern::Burst { count: 5 };
        config.emission.lifetime_range = (3.0, 3.0); // all live exactly 3 steps
        let mut sys = ParticleSystem::new(config, 42);

        sys.step(); // emit 5 (age=1 after step)
        assert_eq!(sys.count(), 5);
        sys.step(); // age=2
        assert_eq!(sys.count(), 5);
        sys.step(); // age=3 => culled (age >= lifetime)
        assert_eq!(sys.count(), 0, "all particles should be culled at age=3");
    }

    // =======================================================================
    // Age increases
    // =======================================================================

    #[test]
    fn particle_age_increases_each_step() {
        let mut config = test_config();
        config.emission.pattern = EmissionPattern::Burst { count: 1 };
        config.emission.lifetime_range = (100.0, 100.0);
        let mut sys = ParticleSystem::new(config, 42);

        sys.step();
        assert!((sys.particles()[0].age - 1.0).abs() < f32::EPSILON);
        sys.step();
        assert!((sys.particles()[0].age - 2.0).abs() < f32::EPSILON);
        sys.step();
        assert!((sys.particles()[0].age - 3.0).abs() < f32::EPSILON);
    }

    // =======================================================================
    // Determinism
    // =======================================================================

    #[test]
    fn same_seed_produces_identical_state() {
        let config = test_config();

        let mut sys_a = ParticleSystem::new(config.clone(), 42);
        let mut sys_b = ParticleSystem::new(config, 42);

        for _ in 0..50 {
            sys_a.step();
            sys_b.step();
        }

        assert_eq!(sys_a.count(), sys_b.count(), "particle counts diverged");
        for (a, b) in sys_a.particles().iter().zip(sys_b.particles().iter()) {
            assert_eq!(a.position, b.position, "positions diverged");
            assert_eq!(a.velocity, b.velocity, "velocities diverged");
            assert_eq!(a.age, b.age, "ages diverged");
            assert_eq!(a.lifetime, b.lifetime, "lifetimes diverged");
            assert_eq!(a.size, b.size, "sizes diverged");
            assert_eq!(a.color_index, b.color_index, "color indices diverged");
            assert_eq!(a.glow, b.glow, "glow values diverged");
        }
    }

    #[test]
    fn different_seeds_produce_different_state() {
        let config = test_config();

        let mut sys_a = ParticleSystem::new(config.clone(), 42);
        let mut sys_b = ParticleSystem::new(config, 999);

        sys_a.step();
        sys_b.step();

        // With different seeds, at least some positions should differ
        let pos_a = sys_a.particles()[0].position;
        let pos_b = sys_b.particles()[0].position;
        assert_ne!(
            pos_a, pos_b,
            "different seeds should produce different positions"
        );
    }

    // =======================================================================
    // Determinism with forces
    // =======================================================================

    #[test]
    fn determinism_with_forces() {
        let make_system = || {
            let config = test_config();
            ParticleSystem::new(config, 42).with_force(Box::new(PointAttractor {
                x: 0.5,
                y: 0.5,
                strength: 0.05,
                radius: 1.0,
            }))
        };

        let mut sys_a = make_system();
        let mut sys_b = make_system();

        for _ in 0..30 {
            sys_a.step();
            sys_b.step();
        }

        assert_eq!(sys_a.count(), sys_b.count());
        for (a, b) in sys_a.particles().iter().zip(sys_b.particles().iter()) {
            assert_eq!(a.position, b.position, "positions diverged with forces");
            assert_eq!(a.velocity, b.velocity, "velocities diverged with forces");
        }
    }

    // =======================================================================
    // to_density_field
    // =======================================================================

    #[test]
    fn to_density_field_correct_dimensions() {
        let sys = ParticleSystem::new(test_config(), 42);
        let field = sys.to_density_field(64, 64).unwrap();
        assert_eq!(field.width(), 64);
        assert_eq!(field.height(), 64);
    }

    #[test]
    fn to_density_field_empty_system_returns_zero_field() {
        let sys = ParticleSystem::new(test_config(), 42);
        let field = sys.to_density_field(16, 16).unwrap();
        assert!(
            field.data().iter().all(|&v| v == 0.0),
            "empty system should produce zero field"
        );
    }

    #[test]
    fn to_density_field_values_in_unit_range() {
        let mut config = test_config();
        config.emission.pattern = EmissionPattern::Burst { count: 50 };
        config.emission.lifetime_range = (100.0, 100.0);
        let mut sys = ParticleSystem::new(config, 42);
        sys.step();

        let field = sys.to_density_field(32, 32).unwrap();
        for &v in field.data() {
            assert!(
                (0.0..=1.0).contains(&v),
                "density field value {v} out of [0, 1]"
            );
        }
    }

    #[test]
    fn to_density_field_has_nonzero_values_with_particles() {
        let mut config = test_config();
        config.emission.pattern = EmissionPattern::Burst { count: 50 };
        config.emission.lifetime_range = (100.0, 100.0);
        let mut sys = ParticleSystem::new(config, 42);
        sys.step();

        let field = sys.to_density_field(32, 32).unwrap();
        let max = field.data().iter().copied().fold(0.0_f64, f64::max);
        assert!(
            max > 0.0,
            "density field should have non-zero values with particles"
        );
    }

    #[test]
    fn to_density_field_invalid_dimensions_returns_error() {
        let sys = ParticleSystem::new(test_config(), 42);
        assert!(sys.to_density_field(0, 10).is_err());
        assert!(sys.to_density_field(10, 0).is_err());
    }

    // =======================================================================
    // from_json
    // =======================================================================

    #[test]
    fn from_json_default_params() {
        let sys = ParticleSystem::from_json(&json!({}), 42);
        assert_eq!(sys.config.max_particles, 1000);
        assert!((sys.config.drag - 0.02).abs() < 1e-5);
    }

    #[test]
    fn from_json_clamps_max_particles_to_limit() {
        let params = json!({"max_particles": 99_999_999});
        let sys = ParticleSystem::from_json(&params, 42);
        assert_eq!(
            sys.config.max_particles, MAX_PARTICLES_LIMIT,
            "max_particles should be clamped to MAX_PARTICLES_LIMIT"
        );
    }

    #[test]
    fn from_json_custom_params() {
        let params = json!({
            "max_particles": 500,
            "drag": 0.1,
            "emission_rate": 20,
            "position_min_x": 0.2,
            "position_max_x": 0.8,
            "lifetime_min": 30.0,
            "lifetime_max": 90.0,
        });
        let sys = ParticleSystem::from_json(&params, 42);
        assert_eq!(sys.config.max_particles, 500);
        assert!((sys.config.drag - 0.1).abs() < 1e-5);
        assert!((sys.config.emission.position_min.x - 0.2).abs() < 1e-5);
        assert!((sys.config.emission.position_max.x - 0.8).abs() < 1e-5);
        assert!((sys.config.emission.lifetime_range.0 - 30.0).abs() < 1e-3);
        assert!((sys.config.emission.lifetime_range.1 - 90.0).abs() < 1e-3);
    }

    #[test]
    fn from_json_produces_deterministic_system() {
        let params = json!({"max_particles": 100, "emission_rate": 5});
        let mut sys_a = ParticleSystem::from_json(&params, 42);
        let mut sys_b = ParticleSystem::from_json(&params, 42);
        for _ in 0..20 {
            sys_a.step();
            sys_b.step();
        }
        assert_eq!(sys_a.count(), sys_b.count());
        for (a, b) in sys_a.particles().iter().zip(sys_b.particles().iter()) {
            assert_eq!(a.position, b.position);
        }
    }

    // =======================================================================
    // Spawn bounds
    // =======================================================================

    #[test]
    fn particles_spawn_within_configured_bounds() {
        let mut config = test_config();
        config.emission.pattern = EmissionPattern::Burst { count: 100 };
        config.emission.position_min = Vec2::new(0.2, 0.3);
        config.emission.position_max = Vec2::new(0.8, 0.7);
        config.emission.lifetime_range = (1000.0, 1000.0);
        let mut sys = ParticleSystem::new(config, 42);
        sys.step();

        // Small epsilon for f64-to-f32 precision loss in spawn_particle
        let eps = 1e-3;
        for p in sys.particles() {
            assert!(
                p.position.x >= 0.2 - eps && p.position.x <= 0.8 + eps,
                "x={} out of [0.2, 0.8] (with eps)",
                p.position.x
            );
            assert!(
                p.position.y >= 0.3 - eps && p.position.y <= 0.7 + eps,
                "y={} out of [0.3, 0.7] (with eps)",
                p.position.y
            );
        }
    }

    // =======================================================================
    // Property-based tests
    // =======================================================================

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn determinism_for_any_seed(seed: u64) {
                let config = test_config();
                let mut sys_a = ParticleSystem::new(config.clone(), seed);
                let mut sys_b = ParticleSystem::new(config, seed);
                for _ in 0..20 {
                    sys_a.step();
                    sys_b.step();
                }
                prop_assert_eq!(sys_a.count(), sys_b.count());
                for (a, b) in sys_a.particles().iter().zip(sys_b.particles().iter()) {
                    prop_assert_eq!(a.position, b.position);
                    prop_assert_eq!(a.velocity, b.velocity);
                    prop_assert_eq!(a.age, b.age);
                }
            }

            #[test]
            fn age_always_increases(seed: u64) {
                let mut config = test_config();
                config.emission.pattern = EmissionPattern::Burst { count: 10 };
                config.emission.lifetime_range = (100.0, 100.0);
                let mut sys = ParticleSystem::new(config, seed);

                sys.step();
                for _ in 1..10 {
                    let ages_before: Vec<f32> = sys.particles().iter().map(|p| p.age).collect();
                    sys.step();
                    for (before, after) in ages_before.iter().zip(sys.particles().iter().map(|p| p.age)) {
                        prop_assert!(after > *before, "age should increase: {} -> {}", before, after);
                    }
                }
            }

            #[test]
            fn count_never_exceeds_max(
                seed: u64,
                max in 1_usize..200,
                rate in 1_usize..50,
            ) {
                let mut config = test_config();
                config.max_particles = max;
                config.emission.pattern = EmissionPattern::Continuous { rate };
                config.emission.lifetime_range = (1000.0, 1000.0);
                let mut sys = ParticleSystem::new(config, seed);
                for _ in 0..50 {
                    sys.step();
                    prop_assert!(
                        sys.count() <= max,
                        "count {} exceeds max {}", sys.count(), max
                    );
                }
            }

            #[test]
            fn density_field_values_in_unit_range(seed: u64) {
                let mut config = test_config();
                config.emission.pattern = EmissionPattern::Burst { count: 30 };
                config.emission.lifetime_range = (100.0, 100.0);
                let mut sys = ParticleSystem::new(config, seed);
                sys.step();

                let field = sys.to_density_field(16, 16).unwrap();
                for &v in field.data() {
                    prop_assert!(
                        (0.0..=1.0).contains(&v),
                        "density field value {} out of [0, 1]", v
                    );
                }
            }
        }
    }
}
