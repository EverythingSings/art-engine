//! Field sources: composable 2D vector field generators.
//!
//! A [`FieldSource`] produces (dx, dy) displacement vectors at any point in
//! space and time. Sources include noise generators (Perlin, Simplex, Curl,
//! Worley, Turbulence), geometric attractors (point, line, orbital, gravity
//! well), vortices, and composites that sum multiple sources.
//!
//! All implementations are deterministic: same inputs produce the same output.

use noise::{NoiseFn, OpenSimplex, Perlin};

/// A source of 2D vector values for field-based simulation.
///
/// Returns a (dx, dy) displacement at any point in space and time.
/// All implementations must be deterministic: same inputs = same output.
pub trait FieldSource: Send + Sync {
    /// Sample the field at position (x, y) at the given time.
    /// Returns (dx, dy) displacement vector.
    fn sample(&self, x: f64, y: f64, time: f64) -> (f64, f64);
}

// ---------------------------------------------------------------------------
// Noise-based sources
// ---------------------------------------------------------------------------

/// Perlin noise field producing displacement vectors from two offset noise
/// samples.
pub struct PerlinField {
    noise: Perlin,
    scale: f64,
    strength: f64,
}

/// Simplex (OpenSimplex) noise field, same pattern as [`PerlinField`].
pub struct SimplexField {
    noise: OpenSimplex,
    scale: f64,
    strength: f64,
}

/// Curl noise field: the curl of a scalar Perlin noise, producing
/// approximately divergence-free flow.
pub struct CurlField {
    noise: Perlin,
    scale: f64,
    strength: f64,
    eps: f64,
}

/// Worley (cellular/Voronoi) noise field producing gradient-like displacement.
///
/// Uses two Perlin noise generators at different seeds to approximate
/// cellular noise gradients while remaining `Send + Sync` safe. The
/// `noise::Worley` type uses `Rc` internally and cannot satisfy the
/// thread-safety bounds required by [`FieldSource`].
pub struct WorleyField {
    noise_x: Perlin,
    noise_y: Perlin,
    scale: f64,
    strength: f64,
}

/// Multi-octave turbulence noise: sum of scaled noise at increasing
/// frequencies.
pub struct TurbulenceField {
    noise: Perlin,
    scale: f64,
    strength: f64,
    octaves: u32,
    persistence: f64,
    lacunarity: f64,
}

// ---------------------------------------------------------------------------
// Attractor-based sources
// ---------------------------------------------------------------------------

/// Point attractor: pulls toward a single point with distance-based falloff.
pub struct PointAttractor {
    pub x: f64,
    pub y: f64,
    pub strength: f64,
    pub radius: f64,
}

/// Point repulsor: pushes away from a single point (negated attractor).
pub struct PointRepulsor {
    pub x: f64,
    pub y: f64,
    pub strength: f64,
    pub radius: f64,
}

/// Line attractor: pulls toward the nearest point on a line segment.
pub struct LineAttractor {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub strength: f64,
    pub radius: f64,
}

/// Orbital attractor: creates circular orbits around a center point.
pub struct OrbitalAttractor {
    pub x: f64,
    pub y: f64,
    pub strength: f64,
    pub radius: f64,
}

/// Gravity well: inverse-square attraction toward a point, clamped to avoid
/// singularity.
pub struct GravityWell {
    pub x: f64,
    pub y: f64,
    pub mass: f64,
}

// ---------------------------------------------------------------------------
// Vortex
// ---------------------------------------------------------------------------

/// Rotational vortex field with Gaussian distance falloff.
pub struct Vortex {
    pub x: f64,
    pub y: f64,
    pub strength: f64,
    pub radius: f64,
}

// ---------------------------------------------------------------------------
// Composite
// ---------------------------------------------------------------------------

/// Sums the displacements from multiple [`FieldSource`] objects.
pub struct CompositeField {
    sources: Vec<Box<dyn FieldSource>>,
}

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

impl PerlinField {
    /// Creates a new Perlin noise field source.
    pub fn new(scale: f64, strength: f64, seed: u32) -> Self {
        Self {
            noise: Perlin::new(seed),
            scale,
            strength,
        }
    }
}

impl SimplexField {
    /// Creates a new OpenSimplex noise field source.
    pub fn new(scale: f64, strength: f64, seed: u32) -> Self {
        Self {
            noise: OpenSimplex::new(seed),
            scale,
            strength,
        }
    }
}

impl CurlField {
    /// Creates a new curl noise field source with default epsilon of 0.001.
    pub fn new(scale: f64, strength: f64, seed: u32) -> Self {
        Self {
            noise: Perlin::new(seed),
            scale,
            strength,
            eps: 0.001,
        }
    }
}

impl WorleyField {
    /// Creates a new Worley-like noise field source using two Perlin generators
    /// at distinct seeds to approximate cellular noise gradients.
    pub fn new(scale: f64, strength: f64, seed: u32) -> Self {
        Self {
            noise_x: Perlin::new(seed),
            noise_y: Perlin::new(seed.wrapping_add(7919)),
            scale,
            strength,
        }
    }
}

impl TurbulenceField {
    /// Creates a new multi-octave turbulence noise field source.
    pub fn new(
        scale: f64,
        strength: f64,
        seed: u32,
        octaves: u32,
        persistence: f64,
        lacunarity: f64,
    ) -> Self {
        Self {
            noise: Perlin::new(seed),
            scale,
            strength,
            octaves,
            persistence,
            lacunarity,
        }
    }
}

impl CompositeField {
    /// Creates an empty composite field.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    /// Adds a source to the composite (builder pattern).
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, source: Box<dyn FieldSource>) -> Self {
        self.sources.push(source);
        self
    }
}

impl Default for CompositeField {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helper: singularity guard for attractor-type sources
// ---------------------------------------------------------------------------

/// Singularity threshold. Distances below this are treated as zero.
const SINGULARITY_EPS: f64 = 1e-10;

/// Maximum force magnitude for gravity wells to avoid singularity blowup.
const MAX_GRAVITY_FORCE: f64 = 1000.0;

/// Computes the displacement vector toward a target point with distance-based
/// falloff. Returns (0, 0) at singularity.
fn attract_toward(
    target_x: f64,
    target_y: f64,
    px: f64,
    py: f64,
    strength: f64,
    radius: f64,
) -> (f64, f64) {
    let dx = target_x - px;
    let dy = target_y - py;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist < SINGULARITY_EPS {
        return (0.0, 0.0);
    }
    if radius.abs() < SINGULARITY_EPS {
        return (0.0, 0.0);
    }
    let magnitude = strength / (1.0 + dist / radius);
    let nx = dx / dist;
    let ny = dy / dist;
    (nx * magnitude, ny * magnitude)
}

/// Projects point (px, py) onto the line segment from (x0, y0) to (x1, y1),
/// returning the nearest point on the segment.
fn nearest_point_on_segment(x0: f64, y0: f64, x1: f64, y1: f64, px: f64, py: f64) -> (f64, f64) {
    let seg_dx = x1 - x0;
    let seg_dy = y1 - y0;
    let seg_len_sq = seg_dx * seg_dx + seg_dy * seg_dy;
    if seg_len_sq < SINGULARITY_EPS * SINGULARITY_EPS {
        // Degenerate segment (point)
        return (x0, y0);
    }
    let t = ((px - x0) * seg_dx + (py - y0) * seg_dy) / seg_len_sq;
    let t_clamped = t.clamp(0.0, 1.0);
    (x0 + t_clamped * seg_dx, y0 + t_clamped * seg_dy)
}

// ---------------------------------------------------------------------------
// FieldSource implementations
// ---------------------------------------------------------------------------

impl FieldSource for PerlinField {
    fn sample(&self, x: f64, y: f64, time: f64) -> (f64, f64) {
        let sx = x * self.scale;
        let sy = y * self.scale;
        let dx = self.noise.get([sx, sy, time]) * self.strength;
        let dy = self.noise.get([sx + 100.0, sy + 100.0, time]) * self.strength;
        (dx, dy)
    }
}

impl FieldSource for SimplexField {
    fn sample(&self, x: f64, y: f64, time: f64) -> (f64, f64) {
        let sx = x * self.scale;
        let sy = y * self.scale;
        let dx = self.noise.get([sx, sy, time]) * self.strength;
        let dy = self.noise.get([sx + 100.0, sy + 100.0, time]) * self.strength;
        (dx, dy)
    }
}

impl FieldSource for CurlField {
    fn sample(&self, x: f64, y: f64, time: f64) -> (f64, f64) {
        let sx = x * self.scale;
        let sy = y * self.scale;
        let eps = self.eps * self.scale;
        if eps.abs() < SINGULARITY_EPS {
            return (0.0, 0.0);
        }
        // Curl of a 2D scalar field F:
        //   dx = dF/dy, dy = -dF/dx
        let df_dy = (self.noise.get([sx, sy + eps, time]) - self.noise.get([sx, sy - eps, time]))
            / (2.0 * eps);
        let df_dx = (self.noise.get([sx + eps, sy, time]) - self.noise.get([sx - eps, sy, time]))
            / (2.0 * eps);
        (df_dy * self.strength, -df_dx * self.strength)
    }
}

impl FieldSource for WorleyField {
    fn sample(&self, x: f64, y: f64, time: f64) -> (f64, f64) {
        let sx = x * self.scale;
        let sy = y * self.scale;
        let dx = self.noise_x.get([sx, sy, time]) * self.strength;
        let dy = self.noise_y.get([sx, sy, time]) * self.strength;
        (dx, dy)
    }
}

impl FieldSource for TurbulenceField {
    fn sample(&self, x: f64, y: f64, time: f64) -> (f64, f64) {
        let (dx_total, dy_total, _, _) =
            (0..self.octaves).fold((0.0, 0.0, 1.0, 1.0), |(dx, dy, amp, freq), _| {
                let sx = x * self.scale * freq;
                let sy = y * self.scale * freq;
                (
                    dx + self.noise.get([sx, sy, time]) * amp,
                    dy + self.noise.get([sx + 100.0, sy + 100.0, time]) * amp,
                    amp * self.persistence,
                    freq * self.lacunarity,
                )
            });
        (dx_total * self.strength, dy_total * self.strength)
    }
}

impl FieldSource for PointAttractor {
    fn sample(&self, x: f64, y: f64, _time: f64) -> (f64, f64) {
        attract_toward(self.x, self.y, x, y, self.strength, self.radius)
    }
}

impl FieldSource for PointRepulsor {
    fn sample(&self, x: f64, y: f64, _time: f64) -> (f64, f64) {
        let (dx, dy) = attract_toward(self.x, self.y, x, y, self.strength, self.radius);
        (-dx, -dy)
    }
}

impl FieldSource for LineAttractor {
    fn sample(&self, x: f64, y: f64, _time: f64) -> (f64, f64) {
        let (nx, ny) = nearest_point_on_segment(self.x0, self.y0, self.x1, self.y1, x, y);
        attract_toward(nx, ny, x, y, self.strength, self.radius)
    }
}

impl FieldSource for OrbitalAttractor {
    fn sample(&self, x: f64, y: f64, _time: f64) -> (f64, f64) {
        let dx_toward = self.x - x;
        let dy_toward = self.y - y;
        let dist = (dx_toward * dx_toward + dy_toward * dy_toward).sqrt();
        if dist < SINGULARITY_EPS {
            return (0.0, 0.0);
        }
        if self.radius.abs() < SINGULARITY_EPS {
            return (0.0, 0.0);
        }
        let magnitude = self.strength / (1.0 + dist / self.radius);
        // Perpendicular to the toward-center vector (counter-clockwise)
        let perp_x = -dy_toward / dist;
        let perp_y = dx_toward / dist;
        (perp_x * magnitude, perp_y * magnitude)
    }
}

impl FieldSource for GravityWell {
    fn sample(&self, x: f64, y: f64, _time: f64) -> (f64, f64) {
        let dx = self.x - x;
        let dy = self.y - y;
        let dist_sq = dx * dx + dy * dy;
        let dist = dist_sq.sqrt();
        if dist < SINGULARITY_EPS {
            return (0.0, 0.0);
        }
        let force = (self.mass / dist_sq).clamp(-MAX_GRAVITY_FORCE, MAX_GRAVITY_FORCE);
        let nx = dx / dist;
        let ny = dy / dist;
        (nx * force, ny * force)
    }
}

impl FieldSource for Vortex {
    fn sample(&self, x: f64, y: f64, _time: f64) -> (f64, f64) {
        let rx = x - self.x;
        let ry = y - self.y;
        let dist_sq = rx * rx + ry * ry;
        let dist = dist_sq.sqrt();
        if dist < SINGULARITY_EPS {
            return (0.0, 0.0);
        }
        if self.radius.abs() < SINGULARITY_EPS {
            return (0.0, 0.0);
        }
        // Gaussian falloff
        let falloff = (-dist_sq / (2.0 * self.radius * self.radius)).exp();
        // Perpendicular direction (counter-clockwise)
        let perp_x = -ry / dist;
        let perp_y = rx / dist;
        (
            perp_x * self.strength * falloff,
            perp_y * self.strength * falloff,
        )
    }
}

impl FieldSource for CompositeField {
    fn sample(&self, x: f64, y: f64, time: f64) -> (f64, f64) {
        self.sources.iter().fold((0.0, 0.0), |(ax, ay), source| {
            let (sx, sy) = source.sample(x, y, time);
            (ax + sx, ay + sy)
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // =======================================================================
    // Attractor tests
    // =======================================================================

    #[test]
    fn point_attractor_vector_points_toward_target() {
        let attr = PointAttractor {
            x: 5.0,
            y: 5.0,
            strength: 1.0,
            radius: 1.0,
        };
        // Sample from (0, 0) -- should pull toward (5, 5), both dx and dy positive
        let (dx, dy) = attr.sample(0.0, 0.0, 0.0);
        assert!(dx > 0.0, "dx should be positive toward target, got {dx}");
        assert!(dy > 0.0, "dy should be positive toward target, got {dy}");
    }

    #[test]
    fn point_repulsor_vector_points_away_from_target() {
        let rep = PointRepulsor {
            x: 5.0,
            y: 5.0,
            strength: 1.0,
            radius: 1.0,
        };
        // Sample from (0, 0) -- should push away from (5, 5), both dx and dy negative
        let (dx, dy) = rep.sample(0.0, 0.0, 0.0);
        assert!(dx < 0.0, "dx should be negative away from target, got {dx}");
        assert!(dy < 0.0, "dy should be negative away from target, got {dy}");
    }

    #[test]
    fn attractor_at_singularity_returns_zero() {
        let attr = PointAttractor {
            x: 3.0,
            y: 3.0,
            strength: 1.0,
            radius: 1.0,
        };
        let (dx, dy) = attr.sample(3.0, 3.0, 0.0);
        assert!(
            dx.abs() < 1e-9 && dy.abs() < 1e-9,
            "expected (0,0) at singularity, got ({dx}, {dy})"
        );
    }

    #[test]
    fn attractor_strength_scales_output() {
        let weak = PointAttractor {
            x: 5.0,
            y: 0.0,
            strength: 1.0,
            radius: 1.0,
        };
        let strong = PointAttractor {
            x: 5.0,
            y: 0.0,
            strength: 3.0,
            radius: 1.0,
        };
        let (dx_weak, _) = weak.sample(0.0, 0.0, 0.0);
        let (dx_strong, _) = strong.sample(0.0, 0.0, 0.0);
        let ratio = dx_strong / dx_weak;
        assert!(
            (ratio - 3.0).abs() < 1e-9,
            "expected 3x scaling, got ratio {ratio}"
        );
    }

    #[test]
    fn gravity_well_inverse_square_falloff() {
        let well = GravityWell {
            x: 0.0,
            y: 0.0,
            mass: 1.0,
        };
        // Sample at distance 1 and distance 2 along x-axis
        let (dx1, _) = well.sample(-1.0, 0.0, 0.0);
        let (dx2, _) = well.sample(-2.0, 0.0, 0.0);
        // Inverse-square: at distance 2, force should be 1/4 of distance 1
        assert!(dx1 > 0.0, "dx1 should be positive, got {dx1}");
        assert!(dx2 > 0.0, "dx2 should be positive, got {dx2}");
        let ratio = dx1.abs() / dx2.abs();
        assert!(
            (ratio - 4.0).abs() < 0.1,
            "expected 4x ratio for inverse-square at 2x distance, got {ratio}"
        );
    }

    #[test]
    fn orbital_attractor_perpendicular_to_radial() {
        let orbital = OrbitalAttractor {
            x: 0.0,
            y: 0.0,
            strength: 1.0,
            radius: 1.0,
        };
        // Sample at (3, 0). Radial direction is (-3, 0).
        // Orbital force should be perpendicular: dot product with radial ~ 0
        let (dx, dy) = orbital.sample(3.0, 0.0, 0.0);
        let radial_x = 0.0 - 3.0;
        let radial_y = 0.0;
        let dot = dx * radial_x + dy * radial_y;
        assert!(
            dot.abs() < 1e-9,
            "orbital force should be perpendicular to radial, dot product = {dot}"
        );
        let magnitude = (dx * dx + dy * dy).sqrt();
        assert!(
            magnitude > 1e-9,
            "orbital force should be non-zero, got magnitude {magnitude}"
        );
    }

    #[test]
    fn line_attractor_attracts_toward_nearest_point() {
        // Horizontal line segment from (0, 0) to (10, 0)
        let line = LineAttractor {
            x0: 0.0,
            y0: 0.0,
            x1: 10.0,
            y1: 0.0,
            strength: 1.0,
            radius: 1.0,
        };
        // Point above the midpoint: (5, 3). Nearest point on segment is (5, 0).
        // Should pull downward (dy negative).
        let (dx, dy) = line.sample(5.0, 3.0, 0.0);
        assert!(
            dy < 0.0,
            "should attract downward toward segment, got dy={dy}"
        );
        // dx should be ~0 since nearest point is directly below
        assert!(
            dx.abs() < 1e-9,
            "dx should be ~0 for point directly above segment midpoint, got {dx}"
        );
    }

    // =======================================================================
    // Noise field tests
    // =======================================================================

    #[test]
    fn perlin_field_returns_finite_values() {
        let field = PerlinField::new(1.0, 1.0, 42);
        for i in 0..100 {
            let x = i as f64 * 0.1;
            let y = i as f64 * 0.07;
            let (dx, dy) = field.sample(x, y, 0.0);
            assert!(dx.is_finite(), "dx not finite at ({x}, {y}): {dx}");
            assert!(dy.is_finite(), "dy not finite at ({x}, {y}): {dy}");
        }
    }

    #[test]
    fn curl_field_approximately_divergence_free() {
        let field = CurlField::new(1.0, 1.0, 42);
        // Numerical divergence: div = d(dx)/dx + d(dy)/dy
        let h = 0.001;
        let test_points = [(1.0, 1.0), (2.5, 3.7), (0.1, 0.9), (5.0, 5.0)];
        for (px, py) in test_points {
            let (dx_right, _) = field.sample(px + h, py, 0.0);
            let (dx_left, _) = field.sample(px - h, py, 0.0);
            let (_, dy_up) = field.sample(px, py + h, 0.0);
            let (_, dy_down) = field.sample(px, py - h, 0.0);
            let ddx_dx = (dx_right - dx_left) / (2.0 * h);
            let ddy_dy = (dy_up - dy_down) / (2.0 * h);
            let divergence = ddx_dx + ddy_dy;
            assert!(
                divergence.abs() < 0.1,
                "divergence too large at ({px}, {py}): {divergence}"
            );
        }
    }

    #[test]
    fn simplex_field_deterministic() {
        let field = SimplexField::new(1.0, 1.0, 99);
        let (dx1, dy1) = field.sample(1.5, 2.3, 0.7);
        let (dx2, dy2) = field.sample(1.5, 2.3, 0.7);
        assert_eq!(dx1, dx2, "simplex dx not deterministic");
        assert_eq!(dy1, dy2, "simplex dy not deterministic");
    }

    #[test]
    fn turbulence_field_with_one_octave_matches_base() {
        let turb = TurbulenceField::new(1.0, 1.0, 42, 1, 0.5, 2.0);
        let base = PerlinField::new(1.0, 1.0, 42);
        let (tdx, tdy) = turb.sample(1.0, 2.0, 0.5);
        let (bdx, bdy) = base.sample(1.0, 2.0, 0.5);
        assert!(
            (tdx - bdx).abs() < 1e-9,
            "1-octave turbulence dx ({tdx}) should match base ({bdx})"
        );
        assert!(
            (tdy - bdy).abs() < 1e-9,
            "1-octave turbulence dy ({tdy}) should match base ({bdy})"
        );
    }

    // =======================================================================
    // Noise golden-value test (pin exact bits for determinism)
    // =======================================================================

    /// Captures the golden value so we can pin it. Intentionally panics
    /// with the bit pattern to be hardcoded into `perlin_golden_value_seed_42`.
    #[test]
    #[ignore = "run once to capture golden bits, then pin in perlin_golden_value_seed_42"]
    fn perlin_capture_golden_bits() {
        let val = Perlin::new(42).get([1.3, 2.7, 0.5]);
        panic!(
            "GOLDEN: Perlin(42).get([1.3, 2.7, 0.5]) = {val} (bits: {:#018x})",
            val.to_bits()
        );
    }

    #[test]
    fn perlin_golden_value_seed_42() {
        // Use non-integer coordinates to avoid Perlin lattice zeros.
        let val = Perlin::new(42).get([1.3, 2.7, 0.5]);
        // Pin: the exact bit pattern for noise = "=0.9.0", Perlin::new(42).
        // If this changes, the noise crate output changed and all replay
        // files using Perlin noise are invalidated.
        // To recapture: cargo test -p art-engine-core -- --ignored perlin_capture_golden_bits --nocapture
        const GOLDEN_BITS: u64 = 0x3fd3_f04b_8ca2_cd01;
        let actual_bits = val.to_bits();
        assert_eq!(
            actual_bits, GOLDEN_BITS,
            "Perlin noise golden value changed! Got {val} (bits: {actual_bits:#018x}), \
             expected bits {GOLDEN_BITS:#018x}. Replay files may be invalidated.",
        );
    }

    // =======================================================================
    // Zero-radius / NaN guard tests
    // =======================================================================

    #[test]
    fn vortex_zero_radius_returns_zero() {
        let vortex = Vortex {
            x: 0.0,
            y: 0.0,
            strength: 1.0,
            radius: 0.0,
        };
        let (dx, dy) = vortex.sample(1.0, 0.0, 0.0);
        assert!(
            dx.abs() < 1e-9 && dy.abs() < 1e-9,
            "vortex with radius=0 should return (0,0), got ({dx}, {dy})"
        );
    }

    #[test]
    fn point_attractor_zero_radius_returns_zero() {
        let attr = PointAttractor {
            x: 5.0,
            y: 5.0,
            strength: 1.0,
            radius: 0.0,
        };
        let (dx, dy) = attr.sample(0.0, 0.0, 0.0);
        assert!(
            dx.abs() < 1e-9 && dy.abs() < 1e-9,
            "attractor with radius=0 should return (0,0), got ({dx}, {dy})"
        );
    }

    #[test]
    fn orbital_attractor_zero_radius_returns_zero() {
        let orbital = OrbitalAttractor {
            x: 0.0,
            y: 0.0,
            strength: 1.0,
            radius: 0.0,
        };
        let (dx, dy) = orbital.sample(3.0, 0.0, 0.0);
        assert!(
            dx.abs() < 1e-9 && dy.abs() < 1e-9,
            "orbital with radius=0 should return (0,0), got ({dx}, {dy})"
        );
    }

    #[test]
    fn curl_field_zero_scale_returns_zero() {
        let field = CurlField {
            noise: Perlin::new(42),
            scale: 0.0,
            strength: 1.0,
            eps: 0.001,
        };
        let (dx, dy) = field.sample(1.0, 1.0, 0.0);
        assert!(
            dx.abs() < 1e-9 && dy.abs() < 1e-9,
            "curl with scale=0 (eps*scale=0) should return (0,0), got ({dx}, {dy})"
        );
    }

    #[test]
    fn gravity_well_negative_mass_clamped() {
        let well = GravityWell {
            x: 0.0,
            y: 0.0,
            mass: -1.0,
        };
        let (dx, _dy) = well.sample(-1.0, 0.0, 0.0);
        // Negative mass produces repulsion (dx negative)
        assert!(dx < 0.0, "negative mass should repel, got dx={dx}");
        assert!(
            dx.abs() <= MAX_GRAVITY_FORCE,
            "force should be clamped, got |dx|={}",
            dx.abs()
        );
    }

    // =======================================================================
    // Vortex tests
    // =======================================================================

    #[test]
    fn vortex_creates_rotational_field() {
        let vortex = Vortex {
            x: 0.0,
            y: 0.0,
            strength: 1.0,
            radius: 5.0,
        };
        // At (1, 0), radial direction is (1, 0).
        // Rotational (perpendicular) should give dot product ~ 0 with radial.
        let (dx, dy) = vortex.sample(1.0, 0.0, 0.0);
        let dot = dx * 1.0 + dy * 0.0;
        assert!(
            dot.abs() < 1e-9,
            "vortex force should be perpendicular to radial, dot = {dot}"
        );
        let mag = (dx * dx + dy * dy).sqrt();
        assert!(mag > 1e-9, "vortex force should be non-zero, got {mag}");
    }

    #[test]
    fn vortex_at_center_returns_zero() {
        let vortex = Vortex {
            x: 3.0,
            y: 4.0,
            strength: 10.0,
            radius: 1.0,
        };
        let (dx, dy) = vortex.sample(3.0, 4.0, 0.0);
        assert!(
            dx.abs() < 1e-9 && dy.abs() < 1e-9,
            "vortex at center should return (0,0), got ({dx}, {dy})"
        );
    }

    #[test]
    fn vortex_falls_off_with_distance() {
        let vortex = Vortex {
            x: 0.0,
            y: 0.0,
            strength: 1.0,
            radius: 1.0,
        };
        let (dx_near, dy_near) = vortex.sample(0.5, 0.0, 0.0);
        let (dx_far, dy_far) = vortex.sample(5.0, 0.0, 0.0);
        let mag_near = (dx_near * dx_near + dy_near * dy_near).sqrt();
        let mag_far = (dx_far * dx_far + dy_far * dy_far).sqrt();
        assert!(
            mag_near > mag_far,
            "vortex should be stronger near center: near={mag_near}, far={mag_far}"
        );
    }

    // =======================================================================
    // CompositeField tests
    // =======================================================================

    #[test]
    fn empty_composite_returns_zero() {
        let composite = CompositeField::new();
        let (dx, dy) = composite.sample(1.0, 2.0, 3.0);
        assert!(
            dx.abs() < 1e-15 && dy.abs() < 1e-15,
            "empty composite should return (0,0), got ({dx}, {dy})"
        );
    }

    #[test]
    fn single_source_passes_through_composite() {
        let attr = PointAttractor {
            x: 10.0,
            y: 0.0,
            strength: 1.0,
            radius: 1.0,
        };
        let (expected_dx, expected_dy) = attr.sample(0.0, 0.0, 0.0);

        let composite = CompositeField::new().add(Box::new(PointAttractor {
            x: 10.0,
            y: 0.0,
            strength: 1.0,
            radius: 1.0,
        }));
        let (dx, dy) = composite.sample(0.0, 0.0, 0.0);
        assert!(
            (dx - expected_dx).abs() < 1e-15,
            "composite dx {dx} != expected {expected_dx}"
        );
        assert!(
            (dy - expected_dy).abs() < 1e-15,
            "composite dy {dy} != expected {expected_dy}"
        );
    }

    #[test]
    fn two_opposing_attractors_cancel_at_midpoint() {
        let composite = CompositeField::new()
            .add(Box::new(PointAttractor {
                x: -5.0,
                y: 0.0,
                strength: 1.0,
                radius: 1.0,
            }))
            .add(Box::new(PointAttractor {
                x: 5.0,
                y: 0.0,
                strength: 1.0,
                radius: 1.0,
            }));
        // At the midpoint (0, 0), equal-strength attractors should cancel
        let (dx, dy) = composite.sample(0.0, 0.0, 0.0);
        assert!(
            dx.abs() < 1e-9,
            "opposing attractors should cancel at midpoint, dx = {dx}"
        );
        assert!(
            dy.abs() < 1e-9,
            "opposing attractors should cancel at midpoint, dy = {dy}"
        );
    }

    #[test]
    fn composite_field_is_itself_a_field_source() {
        let inner = CompositeField::new().add(Box::new(PointAttractor {
            x: 5.0,
            y: 5.0,
            strength: 1.0,
            radius: 1.0,
        }));
        let outer = CompositeField::new().add(Box::new(inner));
        let (dx, dy) = outer.sample(0.0, 0.0, 0.0);
        assert!(dx > 0.0, "nested composite should produce non-zero dx");
        assert!(dy > 0.0, "nested composite should produce non-zero dy");
    }

    // =======================================================================
    // Property-based tests
    // =======================================================================

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        fn any_coord() -> impl Strategy<Value = f64> {
            prop::num::f64::NORMAL
                .prop_filter("finite", |v| v.is_finite())
                .prop_map(|v| v.clamp(-1e6, 1e6))
        }

        fn any_time() -> impl Strategy<Value = f64> {
            0.0_f64..100.0
        }

        proptest! {
            #[test]
            fn all_sources_return_finite_values(
                x in any_coord(),
                y in any_coord(),
                t in any_time(),
            ) {
                let sources: Vec<Box<dyn FieldSource>> = vec![
                    Box::new(PerlinField::new(1.0, 1.0, 42)),
                    Box::new(SimplexField::new(1.0, 1.0, 42)),
                    Box::new(CurlField::new(1.0, 1.0, 42)),
                    Box::new(WorleyField::new(1.0, 1.0, 42)),
                    Box::new(TurbulenceField::new(1.0, 1.0, 42, 4, 0.5, 2.0)),
                    Box::new(PointAttractor { x: 0.0, y: 0.0, strength: 1.0, radius: 1.0 }),
                    Box::new(PointRepulsor { x: 0.0, y: 0.0, strength: 1.0, radius: 1.0 }),
                    Box::new(OrbitalAttractor { x: 0.0, y: 0.0, strength: 1.0, radius: 1.0 }),
                    Box::new(GravityWell { x: 0.0, y: 0.0, mass: 1.0 }),
                    Box::new(Vortex { x: 0.0, y: 0.0, strength: 1.0, radius: 1.0 }),
                    Box::new(LineAttractor { x0: 0.0, y0: 0.0, x1: 1.0, y1: 1.0, strength: 1.0, radius: 1.0 }),
                ];
                for (i, source) in sources.iter().enumerate() {
                    let (dx, dy) = source.sample(x, y, t);
                    prop_assert!(
                        dx.is_finite(),
                        "source {i} returned non-finite dx={dx} at ({x}, {y}, {t})"
                    );
                    prop_assert!(
                        dy.is_finite(),
                        "source {i} returned non-finite dy={dy} at ({x}, {y}, {t})"
                    );
                }
            }

            #[test]
            fn point_attractor_always_points_toward_target(
                tx in any_coord(),
                ty in any_coord(),
                px in any_coord(),
                py in any_coord(),
            ) {
                let dist = ((tx - px).powi(2) + (ty - py).powi(2)).sqrt();
                prop_assume!(dist > 1e-6);

                let attr = PointAttractor {
                    x: tx, y: ty, strength: 1.0, radius: 1.0,
                };
                let (dx, dy) = attr.sample(px, py, 0.0);

                let dir_x = tx - px;
                let dir_y = ty - py;

                let dot = dx * dir_x + dy * dir_y;
                prop_assert!(
                    dot > 0.0,
                    "attractor at ({tx},{ty}) sampled at ({px},{py}): dot={dot}, (dx,dy)=({dx},{dy})"
                );
            }

            #[test]
            fn determinism_all_sources_same_output(
                x in any_coord(),
                y in any_coord(),
                t in any_time(),
            ) {
                let sources: Vec<Box<dyn FieldSource>> = vec![
                    Box::new(PerlinField::new(1.0, 1.0, 42)),
                    Box::new(SimplexField::new(1.0, 1.0, 42)),
                    Box::new(CurlField::new(1.0, 1.0, 42)),
                    Box::new(WorleyField::new(1.0, 1.0, 42)),
                    Box::new(TurbulenceField::new(1.0, 1.0, 42, 4, 0.5, 2.0)),
                    Box::new(PointAttractor { x: 1.0, y: 1.0, strength: 1.0, radius: 1.0 }),
                    Box::new(Vortex { x: 1.0, y: 1.0, strength: 1.0, radius: 1.0 }),
                ];
                let sources2: Vec<Box<dyn FieldSource>> = vec![
                    Box::new(PerlinField::new(1.0, 1.0, 42)),
                    Box::new(SimplexField::new(1.0, 1.0, 42)),
                    Box::new(CurlField::new(1.0, 1.0, 42)),
                    Box::new(WorleyField::new(1.0, 1.0, 42)),
                    Box::new(TurbulenceField::new(1.0, 1.0, 42, 4, 0.5, 2.0)),
                    Box::new(PointAttractor { x: 1.0, y: 1.0, strength: 1.0, radius: 1.0 }),
                    Box::new(Vortex { x: 1.0, y: 1.0, strength: 1.0, radius: 1.0 }),
                ];
                for (i, (s1, s2)) in sources.iter().zip(sources2.iter()).enumerate() {
                    let (dx1, dy1) = s1.sample(x, y, t);
                    let (dx2, dy2) = s2.sample(x, y, t);
                    prop_assert!(
                        dx1 == dx2,
                        "source {} dx not deterministic: {} vs {}", i, dx1, dx2
                    );
                    prop_assert!(
                        dy1 == dy2,
                        "source {} dy not deterministic: {} vs {}", i, dy1, dy2
                    );
                }
            }
        }
    }
}
