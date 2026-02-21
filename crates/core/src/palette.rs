//! Palette of colors stored in OKLCh, sampled by interpolation.
//!
//! Interpolation happens in OKLCh space for perceptually uniform gradients.
//! Hue interpolation uses shortest-arc wrapping to avoid unexpected color
//! journeys through the color wheel.

use crate::color::{oklch_to_srgb, srgb_to_oklch, OkLch, Srgb};
use crate::error::EngineError;

/// A palette of colors stored in OKLCh, sampled by interpolation.
///
/// Colors are evenly spaced along the `t` parameter: `sample(0.0)` returns
/// the first color, `sample(1.0)` returns the last.
#[derive(Debug, Clone)]
pub struct Palette {
    colors: Vec<OkLch>,
}

impl Palette {
    /// Creates a new palette from a vector of OKLCh colors.
    ///
    /// Requires at least one color.
    pub fn new(colors: Vec<OkLch>) -> Result<Self, EngineError> {
        if colors.is_empty() {
            return Err(EngineError::InvalidPalette(
                "palette requires at least 1 color".to_string(),
            ));
        }
        Ok(Self { colors })
    }

    /// Creates a palette by parsing hex color strings and converting to OKLCh.
    ///
    /// Each string can be "#rrggbb" or "rrggbb" (case insensitive).
    /// Requires at least one color.
    pub fn from_hex(hexes: &[&str]) -> Result<Self, EngineError> {
        if hexes.is_empty() {
            return Err(EngineError::InvalidPalette(
                "palette requires at least 1 color".to_string(),
            ));
        }
        let colors: Result<Vec<OkLch>, EngineError> = hexes
            .iter()
            .map(|h| Srgb::from_hex(h).map(srgb_to_oklch))
            .collect();
        Self::new(colors?)
    }

    /// Returns the number of color stops in this palette.
    pub fn len(&self) -> usize {
        self.colors.len()
    }

    /// Returns true if this palette has no colors. (Always false for valid palettes.)
    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }

    /// Samples the palette at parameter `t` in [0, 1].
    ///
    /// Interpolates in OKLCh space with shortest-arc hue interpolation.
    /// For a single-color palette, returns that color for any `t`.
    /// The `t` parameter is clamped to [0, 1].
    pub fn sample(&self, t: f64) -> Srgb {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        let n = self.colors.len();

        if n == 1 {
            return oklch_to_srgb(self.colors[0]);
        }

        // Map t to segment index and local interpolation factor
        let scaled = t * (n - 1) as f64;
        let idx = (scaled as usize).min(n - 2);
        let frac = scaled - idx as f64;

        let c0 = &self.colors[idx];
        let c1 = &self.colors[idx + 1];

        let l = c0.l + frac * (c1.l - c0.l);
        let c = c0.c + frac * (c1.c - c0.c);
        let h = interpolate_hue(c0.h, c1.h, frac);

        oklch_to_srgb(OkLch { l, c, h })
    }

    // -- Palette generators --

    /// Creates an analogous palette: colors evenly spread around `base` hue
    /// within `spread` degrees.
    ///
    /// For `count=1`, returns just the base color. For `count=2`, returns
    /// base-spread/2 and base+spread/2. For larger counts, colors are evenly
    /// distributed across the spread.
    pub fn analogous(base: OkLch, spread: f64, count: usize) -> Self {
        if count <= 1 {
            return Self { colors: vec![base] };
        }
        let colors = (0..count)
            .map(|i| {
                let offset = -spread / 2.0 + spread * i as f64 / (count - 1) as f64;
                OkLch {
                    l: base.l,
                    c: base.c,
                    h: normalize_hue(base.h + offset),
                }
            })
            .collect();
        Self { colors }
    }

    /// Creates a complementary palette: base and base+180 degrees.
    pub fn complementary(base: OkLch) -> Self {
        Self {
            colors: vec![
                base,
                OkLch {
                    l: base.l,
                    c: base.c,
                    h: normalize_hue(base.h + 180.0),
                },
            ],
        }
    }

    /// Creates a triadic palette: base, base+120, base+240 degrees.
    pub fn triadic(base: OkLch) -> Self {
        Self {
            colors: vec![
                base,
                OkLch {
                    l: base.l,
                    c: base.c,
                    h: normalize_hue(base.h + 120.0),
                },
                OkLch {
                    l: base.l,
                    c: base.c,
                    h: normalize_hue(base.h + 240.0),
                },
            ],
        }
    }

    /// Creates a split-complementary palette: base, base+150, base+210 degrees.
    pub fn split_complementary(base: OkLch) -> Self {
        Self {
            colors: vec![
                base,
                OkLch {
                    l: base.l,
                    c: base.c,
                    h: normalize_hue(base.h + 150.0),
                },
                OkLch {
                    l: base.l,
                    c: base.c,
                    h: normalize_hue(base.h + 210.0),
                },
            ],
        }
    }

    /// Creates a gradient palette with `count` colors evenly spaced between
    /// `start` and `end` in OKLCh space.
    ///
    /// Uses shortest-arc hue interpolation. Requires `count >= 1`.
    pub fn gradient(start: OkLch, end: OkLch, count: usize) -> Self {
        if count <= 1 {
            return Self {
                colors: vec![start],
            };
        }
        let colors = (0..count)
            .map(|i| {
                let t = i as f64 / (count - 1) as f64;
                OkLch {
                    l: start.l + t * (end.l - start.l),
                    c: start.c + t * (end.c - start.c),
                    h: interpolate_hue(start.h, end.h, t),
                }
            })
            .collect();
        Self { colors }
    }

    // -- Built-in palettes --

    /// Deep blues to cyan.
    pub fn ocean() -> Self {
        Self::from_hex(&["#001f3f", "#003366", "#005f73", "#0a9396", "#94d2bd"])
            .expect("ocean palette hex values are valid")
    }

    /// Vibrant pinks, greens, yellows.
    pub fn neon() -> Self {
        Self::from_hex(&["#ff00ff", "#00ff41", "#ffff00", "#ff0080", "#00ffff"])
            .expect("neon palette hex values are valid")
    }

    /// Browns, greens, golds.
    pub fn earth() -> Self {
        Self::from_hex(&["#5c4033", "#8b6914", "#6b8e23", "#daa520", "#d2b48c"])
            .expect("earth palette hex values are valid")
    }

    /// Black to white via grays.
    pub fn monochrome() -> Self {
        Self::from_hex(&["#000000", "#404040", "#808080", "#c0c0c0", "#ffffff"])
            .expect("monochrome palette hex values are valid")
    }

    /// Pastel purples, pinks, teals.
    pub fn vapor() -> Self {
        Self::from_hex(&["#7b2d8e", "#c77dff", "#ff9ebb", "#80ced6", "#a0e7e5"])
            .expect("vapor palette hex values are valid")
    }

    /// Reds, oranges, yellows.
    pub fn fire() -> Self {
        Self::from_hex(&["#800000", "#cc0000", "#ff4500", "#ff8c00", "#ffd700"])
            .expect("fire palette hex values are valid")
    }
}

/// Interpolates hue using shortest-arc logic, handling wraparound at 360.
fn interpolate_hue(h0: f64, h1: f64, t: f64) -> f64 {
    let delta = match h1 - h0 {
        d if d > 180.0 => d - 360.0,
        d if d < -180.0 => d + 360.0,
        d => d,
    };
    (h0 + t * delta).rem_euclid(360.0)
}

/// Normalizes a hue angle to [0, 360).
fn normalize_hue(h: f64) -> f64 {
    h.rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{srgb_to_oklch, OkLch, Srgb};

    const EPSILON: f64 = 1e-5;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < EPSILON
    }

    // -- Construction tests --

    #[test]
    fn new_with_empty_vec_returns_error() {
        let result = Palette::new(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn new_with_one_color_succeeds() {
        let result = Palette::new(vec![OkLch {
            l: 0.5,
            c: 0.1,
            h: 180.0,
        }]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn from_hex_with_valid_colors_succeeds() {
        let result = Palette::from_hex(&["#ff0000", "#00ff00", "#0000ff"]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[test]
    fn from_hex_with_empty_slice_returns_error() {
        let result = Palette::from_hex(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn from_hex_with_invalid_hex_returns_error() {
        let result = Palette::from_hex(&["#ff0000", "#zzzzzz"]);
        assert!(result.is_err());
    }

    // -- Sampling tests --

    #[test]
    fn sample_at_zero_returns_first_color() {
        let palette = Palette::from_hex(&["#ff0000", "#00ff00", "#0000ff"]).unwrap();
        let first_srgb = oklch_to_srgb(srgb_to_oklch(Srgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
        }));
        let sampled = palette.sample(0.0);
        assert!(
            approx_eq(sampled.r, first_srgb.r),
            "r: {} vs {}",
            sampled.r,
            first_srgb.r
        );
        assert!(
            approx_eq(sampled.g, first_srgb.g),
            "g: {} vs {}",
            sampled.g,
            first_srgb.g
        );
        assert!(
            approx_eq(sampled.b, first_srgb.b),
            "b: {} vs {}",
            sampled.b,
            first_srgb.b
        );
    }

    #[test]
    fn sample_at_one_returns_last_color() {
        let palette = Palette::from_hex(&["#ff0000", "#00ff00", "#0000ff"]).unwrap();
        let last_srgb = oklch_to_srgb(srgb_to_oklch(Srgb {
            r: 0.0,
            g: 0.0,
            b: 1.0,
        }));
        let sampled = palette.sample(1.0);
        assert!(
            approx_eq(sampled.r, last_srgb.r),
            "r: {} vs {}",
            sampled.r,
            last_srgb.r
        );
        assert!(
            approx_eq(sampled.g, last_srgb.g),
            "g: {} vs {}",
            sampled.g,
            last_srgb.g
        );
        assert!(
            approx_eq(sampled.b, last_srgb.b),
            "b: {} vs {}",
            sampled.b,
            last_srgb.b
        );
    }

    #[test]
    fn single_color_palette_returns_that_color_for_any_t() {
        let color = OkLch {
            l: 0.7,
            c: 0.15,
            h: 200.0,
        };
        let palette = Palette::new(vec![color]).unwrap();
        let expected = oklch_to_srgb(color);

        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let sampled = palette.sample(t);
            assert!(
                approx_eq(sampled.r, expected.r)
                    && approx_eq(sampled.g, expected.g)
                    && approx_eq(sampled.b, expected.b),
                "single-color palette diverged at t={t}: {:?} vs {:?}",
                sampled,
                expected
            );
        }
    }

    #[test]
    fn sample_clamps_t_below_zero() {
        let palette = Palette::from_hex(&["#ff0000", "#0000ff"]).unwrap();
        let at_zero = palette.sample(0.0);
        let below = palette.sample(-0.5);
        assert!(approx_eq(at_zero.r, below.r));
        assert!(approx_eq(at_zero.g, below.g));
        assert!(approx_eq(at_zero.b, below.b));
    }

    #[test]
    fn sample_clamps_t_above_one() {
        let palette = Palette::from_hex(&["#ff0000", "#0000ff"]).unwrap();
        let at_one = palette.sample(1.0);
        let above = palette.sample(1.5);
        assert!(approx_eq(at_one.r, above.r));
        assert!(approx_eq(at_one.g, above.g));
        assert!(approx_eq(at_one.b, above.b));
    }

    // -- Hue wraparound tests --

    #[test]
    fn hue_wraparound_350_to_10_goes_through_zero() {
        // When interpolating from h=350 to h=10, the shortest arc goes through
        // 0 (distance=20), not backwards through 180 (distance=340).
        let h = interpolate_hue(350.0, 10.0, 0.5);
        // Midpoint should be 0 (or 360, normalized)
        assert!(
            approx_eq(h, 0.0) || approx_eq(h, 360.0),
            "midpoint hue should be 0/360, got {}",
            h
        );
    }

    #[test]
    fn hue_wraparound_10_to_350_goes_through_zero() {
        let h = interpolate_hue(10.0, 350.0, 0.5);
        assert!(
            approx_eq(h, 0.0) || approx_eq(h, 360.0),
            "midpoint hue should be 0/360, got {}",
            h
        );
    }

    #[test]
    fn hue_interpolation_no_wraparound() {
        // h=90 to h=180, midpoint should be 135
        let h = interpolate_hue(90.0, 180.0, 0.5);
        assert!(approx_eq(h, 135.0), "expected 135, got {}", h);
    }

    #[test]
    fn hue_interpolation_at_endpoints() {
        let h0 = interpolate_hue(100.0, 200.0, 0.0);
        let h1 = interpolate_hue(100.0, 200.0, 1.0);
        assert!(approx_eq(h0, 100.0), "t=0 should give h0, got {}", h0);
        assert!(approx_eq(h1, 200.0), "t=1 should give h1, got {}", h1);
    }

    // -- Palette generator tests --

    #[test]
    fn complementary_colors_are_180_degrees_apart() {
        let base = OkLch {
            l: 0.7,
            c: 0.15,
            h: 30.0,
        };
        let palette = Palette::complementary(base);
        assert_eq!(palette.len(), 2);
        let h0 = palette.colors[0].h;
        let h1 = palette.colors[1].h;
        let diff = (h1 - h0).abs();
        assert!(
            approx_eq(diff, 180.0),
            "complementary hue difference should be 180, got {}",
            diff
        );
    }

    #[test]
    fn complementary_wraps_correctly() {
        let base = OkLch {
            l: 0.7,
            c: 0.15,
            h: 200.0,
        };
        let palette = Palette::complementary(base);
        let h1 = palette.colors[1].h;
        // 200 + 180 = 380 -> normalized to 20
        assert!(approx_eq(h1, 20.0), "expected 20, got {}", h1);
    }

    #[test]
    fn triadic_colors_are_120_degrees_apart() {
        let base = OkLch {
            l: 0.7,
            c: 0.15,
            h: 60.0,
        };
        let palette = Palette::triadic(base);
        assert_eq!(palette.len(), 3);

        let h0 = palette.colors[0].h;
        let h1 = palette.colors[1].h;
        let h2 = palette.colors[2].h;

        assert!(approx_eq(h0, 60.0));
        assert!(approx_eq(h1, 180.0));
        assert!(approx_eq(h2, 300.0));
    }

    #[test]
    fn split_complementary_has_correct_angles() {
        let base = OkLch {
            l: 0.7,
            c: 0.15,
            h: 0.0,
        };
        let palette = Palette::split_complementary(base);
        assert_eq!(palette.len(), 3);
        assert!(approx_eq(palette.colors[0].h, 0.0));
        assert!(approx_eq(palette.colors[1].h, 150.0));
        assert!(approx_eq(palette.colors[2].h, 210.0));
    }

    #[test]
    fn gradient_with_count_2_returns_start_and_end() {
        let start = OkLch {
            l: 0.3,
            c: 0.1,
            h: 45.0,
        };
        let end = OkLch {
            l: 0.9,
            c: 0.2,
            h: 270.0,
        };
        let palette = Palette::gradient(start, end, 2);
        assert_eq!(palette.len(), 2);
        assert!(approx_eq(palette.colors[0].l, start.l));
        assert!(approx_eq(palette.colors[0].c, start.c));
        assert!(approx_eq(palette.colors[0].h, start.h));
        assert!(approx_eq(palette.colors[1].l, end.l));
        assert!(approx_eq(palette.colors[1].c, end.c));
        assert!(approx_eq(palette.colors[1].h, end.h));
    }

    #[test]
    fn gradient_with_count_1_returns_start() {
        let start = OkLch {
            l: 0.5,
            c: 0.1,
            h: 90.0,
        };
        let end = OkLch {
            l: 0.9,
            c: 0.2,
            h: 270.0,
        };
        let palette = Palette::gradient(start, end, 1);
        assert_eq!(palette.len(), 1);
        assert!(approx_eq(palette.colors[0].l, start.l));
    }

    #[test]
    fn gradient_midpoint_is_interpolated() {
        let start = OkLch {
            l: 0.2,
            c: 0.1,
            h: 100.0,
        };
        let end = OkLch {
            l: 0.8,
            c: 0.3,
            h: 200.0,
        };
        let palette = Palette::gradient(start, end, 3);
        assert_eq!(palette.len(), 3);
        let mid = &palette.colors[1];
        assert!(approx_eq(mid.l, 0.5), "mid L: {}", mid.l);
        assert!(approx_eq(mid.c, 0.2), "mid C: {}", mid.c);
        assert!(approx_eq(mid.h, 150.0), "mid h: {}", mid.h);
    }

    #[test]
    fn analogous_with_count_1_returns_base() {
        let base = OkLch {
            l: 0.7,
            c: 0.15,
            h: 120.0,
        };
        let palette = Palette::analogous(base, 60.0, 1);
        assert_eq!(palette.len(), 1);
        assert!(approx_eq(palette.colors[0].h, 120.0));
    }

    #[test]
    fn analogous_spreads_evenly() {
        let base = OkLch {
            l: 0.7,
            c: 0.15,
            h: 180.0,
        };
        // spread=60, count=3: hues at 150, 180, 210
        let palette = Palette::analogous(base, 60.0, 3);
        assert_eq!(palette.len(), 3);
        assert!(
            approx_eq(palette.colors[0].h, 150.0),
            "first: {}",
            palette.colors[0].h
        );
        assert!(
            approx_eq(palette.colors[1].h, 180.0),
            "mid: {}",
            palette.colors[1].h
        );
        assert!(
            approx_eq(palette.colors[2].h, 210.0),
            "last: {}",
            palette.colors[2].h
        );
    }

    // -- NaN guard --

    #[test]
    fn sample_nan_returns_valid_color() {
        let palette = Palette::from_hex(&["#ff0000", "#0000ff"]).unwrap();
        let srgb = palette.sample(f64::NAN);
        assert!(srgb.r >= 0.0 && srgb.r <= 1.0, "r out of range: {}", srgb.r);
        assert!(srgb.g >= 0.0 && srgb.g <= 1.0, "g out of range: {}", srgb.g);
        assert!(srgb.b >= 0.0 && srgb.b <= 1.0, "b out of range: {}", srgb.b);
    }

    // -- Built-in palette tests --

    #[test]
    fn builtin_palettes_have_at_least_2_colors() {
        let palettes = [
            ("ocean", Palette::ocean()),
            ("neon", Palette::neon()),
            ("earth", Palette::earth()),
            ("monochrome", Palette::monochrome()),
            ("vapor", Palette::vapor()),
            ("fire", Palette::fire()),
        ];
        for (name, palette) in &palettes {
            assert!(
                palette.len() >= 2,
                "{name} has only {} colors",
                palette.len()
            );
        }
    }

    #[test]
    fn builtin_palettes_sample_to_valid_srgb() {
        let palettes = [
            ("ocean", Palette::ocean()),
            ("neon", Palette::neon()),
            ("earth", Palette::earth()),
            ("monochrome", Palette::monochrome()),
            ("vapor", Palette::vapor()),
            ("fire", Palette::fire()),
        ];
        let sample_points = [0.0, 0.25, 0.5, 0.75, 1.0];

        for (name, palette) in &palettes {
            for &t in &sample_points {
                let srgb = palette.sample(t);
                assert!(
                    srgb.r >= 0.0 && srgb.r <= 1.0,
                    "{name} at t={t}: r={} out of range",
                    srgb.r
                );
                assert!(
                    srgb.g >= 0.0 && srgb.g <= 1.0,
                    "{name} at t={t}: g={} out of range",
                    srgb.g
                );
                assert!(
                    srgb.b >= 0.0 && srgb.b <= 1.0,
                    "{name} at t={t}: b={} out of range",
                    srgb.b
                );
            }
        }
    }

    // -- Property-based tests --

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn sample_always_produces_valid_srgb(
                t in -0.5_f64..=1.5,
            ) {
                // Use a fixed multi-color palette
                let palette = Palette::from_hex(&[
                    "#ff0000", "#00ff00", "#0000ff", "#ffff00",
                ]).unwrap();
                let srgb = palette.sample(t);
                prop_assert!(
                    srgb.r >= 0.0 && srgb.r <= 1.0,
                    "r out of range: {} at t={}", srgb.r, t
                );
                prop_assert!(
                    srgb.g >= 0.0 && srgb.g <= 1.0,
                    "g out of range: {} at t={}", srgb.g, t
                );
                prop_assert!(
                    srgb.b >= 0.0 && srgb.b <= 1.0,
                    "b out of range: {} at t={}", srgb.b, t
                );
            }

            #[test]
            fn hue_interpolation_stays_in_range(
                h0 in 0.0_f64..360.0,
                h1 in 0.0_f64..360.0,
                t in 0.0_f64..=1.0,
            ) {
                let h = interpolate_hue(h0, h1, t);
                prop_assert!(
                    h >= 0.0 && h < 360.0,
                    "hue {} out of [0, 360) for h0={h0}, h1={h1}, t={t}", h
                );
            }

            #[test]
            fn normalize_hue_always_in_range(h in -1000.0_f64..1000.0) {
                let n = normalize_hue(h);
                prop_assert!(
                    n >= 0.0 && n < 360.0,
                    "normalize_hue({h}) = {n}, not in [0, 360)"
                );
            }
        }
    }
}
