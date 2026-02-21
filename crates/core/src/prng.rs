//! Deterministic PRNG based on the Xorshift64 algorithm.
//!
//! Provides a fast, seedable pseudo-random number generator suitable for
//! reproducible generative art. Same seed always produces the same sequence
//! of values across all platforms (pure integer arithmetic, no floating point
//! in the core algorithm).

use serde::{Deserialize, Serialize};

/// Xorshift64 deterministic PRNG. Same seed always produces the same sequence.
///
/// Uses the standard shift parameters (13, 7, 17) for good statistical
/// properties across the full 64-bit state space. Seed of 0 is automatically
/// replaced with a non-zero fallback to avoid the all-zeros fixed point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Xorshift64 {
    state: u64,
}

impl Xorshift64 {
    /// Fallback seed used when the caller provides 0, which is a fixed point
    /// of the xorshift algorithm.
    const FALLBACK_SEED: u64 = 0x5EED_DEAD_BEEF_CAFE;

    /// Creates a new PRNG with the given seed.
    ///
    /// If `seed` is 0, uses `0x5EED_DEAD_BEEF_CAFE` as a fallback to avoid
    /// the xorshift all-zeros fixed point.
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { Self::FALLBACK_SEED } else { seed },
        }
    }

    /// Advances the state and returns the next 64-bit value.
    ///
    /// Implements xorshift64 with shifts (13, 7, 17).
    pub fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    /// Returns a uniformly distributed f64 in [0, 1).
    ///
    /// Uses the upper 53 bits of `next_u64()` divided by 2^53 for
    /// full mantissa precision.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Returns a uniformly distributed f64 in [min, max).
    pub fn next_range(&mut self, min: f64, max: f64) -> f64 {
        min + self.next_f64() * (max - min)
    }

    /// Returns a uniformly distributed usize in [0, max).
    ///
    /// Uses simple modulo reduction. For non-power-of-two `max` values,
    /// this introduces negligible bias at 64-bit state width.
    ///
    /// # Panics
    ///
    /// Panics if `max` is 0 (division by zero in modulo).
    pub fn next_usize(&mut self, max: usize) -> usize {
        (self.next_u64() as usize) % max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Test 1: Golden value --

    #[test]
    fn next_u64_produces_known_golden_value_for_seed_42() {
        // Golden value for xorshift64(seed=42, shifts=13,7,17).
        // If this test breaks, the PRNG algorithm changed and all
        // replay files using this seed are invalidated.
        let mut rng = Xorshift64::new(42);
        assert_eq!(rng.next_u64(), 45_454_805_674);
    }

    // -- Test 2: Seed=0 guard --

    #[test]
    fn seed_zero_does_not_produce_all_zeros() {
        let mut rng = Xorshift64::new(0);
        // If seed=0 were used directly, xorshift would return 0 forever.
        // The guard must replace it, so the first value should be non-zero.
        let first = rng.next_u64();
        assert_ne!(first, 0, "seed=0 guard failed: first value is 0");
        // Verify a few more values are non-zero
        let second = rng.next_u64();
        let third = rng.next_u64();
        assert_ne!(second, 0);
        assert_ne!(third, 0);
    }

    // -- Test 3: Determinism --

    #[test]
    fn two_instances_with_same_seed_produce_identical_sequences() {
        let mut rng_a = Xorshift64::new(42);
        let mut rng_b = Xorshift64::new(42);
        for i in 0..1000 {
            assert_eq!(
                rng_a.next_u64(),
                rng_b.next_u64(),
                "sequences diverged at index {i}"
            );
        }
    }

    // -- Test 4: next_f64 range --

    #[test]
    fn next_f64_always_in_unit_interval() {
        let mut rng = Xorshift64::new(12345);
        for i in 0..10_000 {
            let v = rng.next_f64();
            assert!(
                (0.0..1.0).contains(&v),
                "next_f64() = {v} out of [0, 1) at iteration {i}"
            );
        }
    }

    // -- Test 5: next_range bounds --

    #[test]
    fn next_range_stays_within_specified_bounds() {
        let mut rng = Xorshift64::new(9999);
        for i in 0..10_000 {
            let v = rng.next_range(10.0, 20.0);
            assert!(
                (10.0..20.0).contains(&v),
                "next_range(10, 20) = {v} out of bounds at iteration {i}"
            );
        }
    }

    // -- Test 6: next_usize bounds --

    #[test]
    fn next_usize_always_less_than_max() {
        let mut rng = Xorshift64::new(7777);
        for i in 0..10_000 {
            let v = rng.next_usize(100);
            assert!(v < 100, "next_usize(100) = {v} >= 100 at iteration {i}");
        }
    }

    // -- Serialization roundtrip --

    #[test]
    fn serialization_roundtrip_preserves_state() {
        let mut rng = Xorshift64::new(42);
        // Advance state partway through a sequence
        for _ in 0..50 {
            rng.next_u64();
        }
        // Serialize mid-stream
        let json = serde_json::to_string(&rng).unwrap();
        let mut restored: Xorshift64 = serde_json::from_str(&json).unwrap();
        // Verify next 100 values match
        for i in 0..100 {
            assert_eq!(
                rng.next_u64(),
                restored.next_u64(),
                "sequences diverged after deserialization at index {i}"
            );
        }
    }

    // -- Property-based tests --

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            // -- Test 7: next_f64 in range for any seed --

            #[test]
            fn next_f64_in_unit_interval_for_any_seed(seed: u64) {
                let mut rng = Xorshift64::new(seed);
                for _ in 0..100 {
                    let v = rng.next_f64();
                    prop_assert!(
                        (0.0..1.0).contains(&v),
                        "next_f64() = {v} out of [0, 1) for seed {seed}"
                    );
                }
            }

            // -- Test 8: next_range in bounds for any seed and range --

            #[test]
            fn next_range_in_bounds_for_any_seed_and_range(
                seed: u64,
                min in -1e6_f64..1e6,
                max in -1e6_f64..1e6,
            ) {
                // Only test when min < max
                prop_assume!(min < max);
                let mut rng = Xorshift64::new(seed);
                for _ in 0..100 {
                    let v = rng.next_range(min, max);
                    prop_assert!(
                        v >= min && v < max,
                        "next_range({min}, {max}) = {v} out of bounds for seed {seed}"
                    );
                }
            }

            // -- Test 9: next_usize in bounds for any seed and max --

            #[test]
            fn next_usize_in_bounds_for_any_seed_and_max(
                seed: u64,
                max in 1_usize..10_000,
            ) {
                let mut rng = Xorshift64::new(seed);
                for _ in 0..100 {
                    let v = rng.next_usize(max);
                    prop_assert!(
                        v < max,
                        "next_usize({max}) = {v} >= max for seed {seed}"
                    );
                }
            }

            // -- Test 10: Approximate uniformity bucket test --

            #[test]
            fn next_f64_approximate_uniformity(seed: u64) {
                let mut rng = Xorshift64::new(seed);
                let mut buckets = [0u32; 10];
                for _ in 0..10_000 {
                    let v = rng.next_f64();
                    let idx = (v * 10.0).min(9.0) as usize;
                    buckets[idx] += 1;
                }
                // Each bucket should have at least 500 out of 10000 (expected ~1000).
                // This is a very loose bound to avoid flaky tests.
                for (i, &count) in buckets.iter().enumerate() {
                    prop_assert!(
                        count >= 500,
                        "bucket {i} has only {count} values (expected ~1000) for seed {seed}"
                    );
                }
            }
        }
    }
}
