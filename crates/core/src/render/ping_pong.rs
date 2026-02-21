//! Ping-pong index logic for double-buffered render targets.
//!
//! `PingPong` tracks which of two buffers is the current source and which
//! is the destination. Calling `swap()` flips them. This is pure index
//! math with no GPU dependency, used by the post-processing pipeline
//! and per-layer effect passes.

/// Tracks the current read/write indices for a pair of double-buffered
/// render targets. The invariant `src_index() + dst_index() == 1` always holds.
pub struct PingPong {
    current: usize,
}

impl PingPong {
    /// Creates a new `PingPong` with source at index 0 and destination at index 1.
    pub fn new() -> Self {
        Self { current: 0 }
    }

    /// Returns the index of the current source (read) buffer.
    pub fn src_index(&self) -> usize {
        self.current
    }

    /// Returns the index of the current destination (write) buffer.
    pub fn dst_index(&self) -> usize {
        1 - self.current
    }

    /// Swaps source and destination, flipping which buffer is read from
    /// and which is written to.
    pub fn swap(&mut self) {
        self.current = 1 - self.current;
    }
}

impl Default for PingPong {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_at_index_zero() {
        let pp = PingPong::new();
        assert_eq!(pp.src_index(), 0);
    }

    #[test]
    fn initial_src_is_zero_dst_is_one() {
        let pp = PingPong::new();
        assert_eq!(pp.src_index(), 0);
        assert_eq!(pp.dst_index(), 1);
    }

    #[test]
    fn swap_flips_src_and_dst() {
        let mut pp = PingPong::new();
        pp.swap();
        assert_eq!(pp.src_index(), 1, "after swap, src should be 1");
        assert_eq!(pp.dst_index(), 0, "after swap, dst should be 0");
    }

    #[test]
    fn double_swap_returns_to_initial_state() {
        let mut pp = PingPong::new();
        pp.swap();
        pp.swap();
        assert_eq!(pp.src_index(), 0);
        assert_eq!(pp.dst_index(), 1);
    }

    #[test]
    fn src_plus_dst_invariant_holds_over_100_swaps() {
        let mut pp = PingPong::new();
        for i in 0..100 {
            assert_eq!(
                pp.src_index() + pp.dst_index(),
                1,
                "invariant broken at swap {i}"
            );
            pp.swap();
        }
        // Check once more after the last swap
        assert_eq!(pp.src_index() + pp.dst_index(), 1);
    }

    #[test]
    fn even_swap_count_restores_initial_odd_does_not() {
        let mut pp = PingPong::new();
        for _ in 0..50 {
            pp.swap();
        }
        assert_eq!(pp.src_index(), 0, "50 swaps (even) should restore src to 0");

        pp.swap();
        assert_eq!(pp.src_index(), 1, "51 swaps (odd) should flip src to 1");
    }
}
