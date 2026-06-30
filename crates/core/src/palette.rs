//! Palette of colors stored in OKLCh, sampled by interpolation.
//!
//! Interpolation happens in OKLCh space for perceptually uniform gradients.
//! Hue interpolation uses shortest-arc wrapping to avoid unexpected color
//! journeys through the color wheel.

use crate::color::{oklch_to_srgb, srgb_to_oklch, OkLch, Srgb};
use crate::error::EngineError;

/// All built-in palette names, kept in sync with `from_name`.
const BUILTIN_PALETTE_NAMES: &[&str] = &[
    "ocean",
    "neon",
    "earth",
    "monochrome",
    "vapor",
    "fire",
    "amber",
];

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
    #[allow(clippy::excessive_precision)]
    pub fn ocean() -> Self {
        Self {
            colors: vec![
                OkLch {
                    l: 0.23812082136511145,
                    c: 0.07127959462771918,
                    h: 252.01432913571119343,
                }, // #001f3f
                OkLch {
                    l: 0.32333781228455999,
                    c: 0.10254380255141139,
                    h: 253.88517968214827647,
                }, // #003366
                OkLch {
                    l: 0.44847408232621344,
                    c: 0.08096082677814632,
                    h: 218.73432188477929117,
                }, // #005f73
                OkLch {
                    l: 0.60236934811109399,
                    c: 0.10097609551536746,
                    h: 197.39604317217774110,
                }, // #0a9396
                OkLch {
                    l: 0.81568863652537782,
                    c: 0.07011388691813732,
                    h: 171.43006696968956248,
                }, // #94d2bd
            ],
        }
    }

    /// Vibrant pinks, greens, yellows.
    #[allow(clippy::excessive_precision)]
    pub fn neon() -> Self {
        Self {
            colors: vec![
                OkLch {
                    l: 0.70167385587179243,
                    c: 0.32249096477516426,
                    h: 328.36341792345143631,
                }, // #ff00ff
                OkLch {
                    l: 0.86856225013429800,
                    c: 0.27758436123611802,
                    h: 144.46611623296706739,
                }, // #00ff41
                OkLch {
                    l: 0.96798272032678734,
                    c: 0.21100590772552355,
                    h: 109.76923207652123438,
                }, // #ffff00
                OkLch {
                    l: 0.64534933791255666,
                    c: 0.26034308371573917,
                    h: 2.47076075330204237,
                }, // #ff0080
                OkLch {
                    l: 0.90539923005576761,
                    c: 0.15455001106436869,
                    h: 194.76894793196385081,
                }, // #00ffff
            ],
        }
    }

    /// Browns, greens, golds.
    #[allow(clippy::excessive_precision)]
    pub fn earth() -> Self {
        Self {
            colors: vec![
                OkLch {
                    l: 0.39927207159385364,
                    c: 0.04471991685340046,
                    h: 45.87584833921376060,
                }, // #5c4033
                OkLch {
                    l: 0.54091619216423759,
                    c: 0.10413400389378805,
                    h: 84.44556495690922304,
                }, // #8b6914
                OkLch {
                    l: 0.59948384089518736,
                    c: 0.13738435092056211,
                    h: 126.32247662228316187,
                }, // #6b8e23
                OkLch {
                    l: 0.75157231639668942,
                    c: 0.14693369873682238,
                    h: 83.98811694668086147,
                }, // #daa520
                OkLch {
                    l: 0.78618663498834429,
                    c: 0.06382105558060952,
                    h: 74.61902764202896776,
                }, // #d2b48c
            ],
        }
    }

    /// Black to white via grays.
    #[allow(clippy::excessive_precision)]
    pub fn monochrome() -> Self {
        Self {
            colors: vec![
                OkLch {
                    l: 0.0,
                    c: 0.0,
                    h: 0.0,
                }, // #000000
                OkLch {
                    l: 0.37149494360518853,
                    c: 0.00000001384710093,
                    h: 89.87556330123183557,
                }, // #404040
                OkLch {
                    l: 0.59987080170711771,
                    c: 0.00000002235958163,
                    h: 89.87556235475514654,
                }, // #808080
                OkLch {
                    l: 0.80779623257237509,
                    c: 0.00000003010979339,
                    h: 89.87556362272995614,
                }, // #c0c0c0
                OkLch {
                    l: 0.99999999347354618,
                    c: 0.00000003727399554,
                    h: 89.87556309590243586,
                }, // #ffffff
            ],
        }
    }

    /// Pastel purples, pinks, teals.
    #[allow(clippy::excessive_precision)]
    pub fn vapor() -> Self {
        Self {
            colors: vec![
                OkLch {
                    l: 0.45254128296043711,
                    c: 0.16429144447389848,
                    h: 319.15903804977205027,
                }, // #7b2d8e
                OkLch {
                    l: 0.71970087298314189,
                    c: 0.19304222812119678,
                    h: 308.60056204777129096,
                }, // #c77dff
                OkLch {
                    l: 0.80539982278616506,
                    c: 0.11977682165104597,
                    h: 0.81858934155943142,
                }, // #ff9ebb
                OkLch {
                    l: 0.80385653909481602,
                    c: 0.07766707272151088,
                    h: 204.07761663758176951,
                }, // #80ced6
                OkLch {
                    l: 0.88067983293648833,
                    c: 0.07067221527030268,
                    h: 193.71902766763105319,
                }, // #a0e7e5
            ],
        }
    }

    /// Reds, oranges, yellows.
    #[allow(clippy::excessive_precision)]
    pub fn fire() -> Self {
        Self {
            colors: vec![
                OkLch {
                    l: 0.37669208806659682,
                    c: 0.15457669340706801,
                    h: 29.23388519234261196,
                }, // #800000
                OkLch {
                    l: 0.53076184644870028,
                    c: 0.21779966665019315,
                    h: 29.23388519234264393,
                }, // #cc0000
                OkLch {
                    l: 0.66019948425910413,
                    c: 0.22935607863610208,
                    h: 35.40251385253340288,
                }, // #ff4500
                OkLch {
                    l: 0.75054424731978897,
                    c: 0.17911451445506674,
                    h: 58.28268612175613583,
                }, // #ff8c00
                OkLch {
                    l: 0.88677107343929762,
                    c: 0.18218604275663958,
                    h: 95.33049348702482462,
                }, // #ffd700
            ],
        }
    }

    /// Monochromatic CRT-amber gradient: black through deep amber to bright glow.
    ///
    /// Tuned for retro-futuristic / phosphor-display aesthetics. All stops
    /// share roughly the same hue family (~55-90 degrees in OKLCh) so
    /// gradients stay monochromatic across the full t in [0, 1] range.
    #[allow(clippy::excessive_precision)]
    pub fn amber() -> Self {
        Self {
            colors: vec![
                OkLch {
                    l: 0.00000000000000000,
                    c: 0.00000000000000000,
                    h: 0.00000000000000000,
                }, // #000000
                OkLch {
                    l: 0.16610759630976832,
                    c: 0.03828581367845601,
                    h: 61.82965724866916446,
                }, // #1a0a00
                OkLch {
                    l: 0.34758304667402595,
                    c: 0.08817712922849860,
                    h: 53.03972690888107877,
                }, // #5c2a00
                OkLch {
                    l: 0.59932948726694502,
                    c: 0.14985184979870461,
                    h: 54.19415344612159657,
                }, // #c26200
                OkLch {
                    l: 0.81241940272516822,
                    c: 0.17037847630693131,
                    h: 76.39076890134664666,
                }, // #ffb000
                OkLch {
                    l: 0.93098070675064348,
                    c: 0.08331433919815009,
                    h: 88.57704902420216797,
                }, // #ffe6a8
            ],
        }
    }

    // -- Registry --

    /// Returns a slice of all built-in palette names.
    pub fn list_names() -> &'static [&'static str] {
        BUILTIN_PALETTE_NAMES
    }

    /// Constructs a built-in palette by name.
    ///
    /// Returns `EngineError::UnknownPalette` if the name is not recognized.
    pub fn from_name(name: &str) -> Result<Self, EngineError> {
        match name {
            "ocean" => Ok(Self::ocean()),
            "neon" => Ok(Self::neon()),
            "earth" => Ok(Self::earth()),
            "monochrome" => Ok(Self::monochrome()),
            "vapor" => Ok(Self::vapor()),
            "fire" => Ok(Self::fire()),
            "amber" => Ok(Self::amber()),
            _ => Err(EngineError::UnknownPalette(name.to_string())),
        }
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

    // -- NaN / infinity guard --

    #[test]
    fn sample_infinity_returns_last_color() {
        let palette = Palette::from_hex(&["#ff0000", "#0000ff"]).unwrap();
        let at_inf = palette.sample(f64::INFINITY);
        let at_one = palette.sample(1.0);
        assert!(approx_eq(at_inf.r, at_one.r));
        assert!(approx_eq(at_inf.g, at_one.g));
        assert!(approx_eq(at_inf.b, at_one.b));
    }

    #[test]
    fn sample_neg_infinity_returns_first_color() {
        let palette = Palette::from_hex(&["#ff0000", "#0000ff"]).unwrap();
        let at_neg_inf = palette.sample(f64::NEG_INFINITY);
        let at_zero = palette.sample(0.0);
        assert!(approx_eq(at_neg_inf.r, at_zero.r));
        assert!(approx_eq(at_neg_inf.g, at_zero.g));
        assert!(approx_eq(at_neg_inf.b, at_zero.b));
    }

    #[test]
    fn sample_nan_returns_valid_color() {
        let palette = Palette::from_hex(&["#ff0000", "#0000ff"]).unwrap();
        let srgb = palette.sample(f64::NAN);
        assert!(srgb.r >= 0.0 && srgb.r <= 1.0, "r out of range: {}", srgb.r);
        assert!(srgb.g >= 0.0 && srgb.g <= 1.0, "g out of range: {}", srgb.g);
        assert!(srgb.b >= 0.0 && srgb.b <= 1.0, "b out of range: {}", srgb.b);
    }

    // -- Registry tests --

    #[test]
    fn list_names_returns_expected_count() {
        assert_eq!(Palette::list_names().len(), 7);
    }

    #[test]
    fn from_name_succeeds_for_all_listed_names() {
        for name in Palette::list_names() {
            assert!(
                Palette::from_name(name).is_ok(),
                "from_name failed for listed palette: {name}"
            );
        }
    }

    #[test]
    fn from_name_returns_error_for_unknown() {
        let result = Palette::from_name("rainbow");
        assert!(matches!(
            result,
            Err(EngineError::UnknownPalette(ref n)) if n == "rainbow"
        ));
    }

    // -- Capture helper (run once to generate OkLch literals) --

    #[test]
    #[ignore = "run once to capture OkLch values: cargo test -p art-engine-core -- --ignored capture_palette_oklch --nocapture"]
    fn capture_palette_oklch() {
        let palettes: &[(&str, &[&str])] = &[
            (
                "ocean",
                &["#001f3f", "#003366", "#005f73", "#0a9396", "#94d2bd"],
            ),
            (
                "neon",
                &["#ff00ff", "#00ff41", "#ffff00", "#ff0080", "#00ffff"],
            ),
            (
                "earth",
                &["#5c4033", "#8b6914", "#6b8e23", "#daa520", "#d2b48c"],
            ),
            (
                "monochrome",
                &["#000000", "#404040", "#808080", "#c0c0c0", "#ffffff"],
            ),
            (
                "vapor",
                &["#7b2d8e", "#c77dff", "#ff9ebb", "#80ced6", "#a0e7e5"],
            ),
            (
                "fire",
                &["#800000", "#cc0000", "#ff4500", "#ff8c00", "#ffd700"],
            ),
            (
                "amber",
                &[
                    "#000000", "#1a0a00", "#5c2a00", "#c26200", "#ffb000", "#ffe6a8",
                ],
            ),
        ];
        for (name, hexes) in palettes {
            println!("// {name}:");
            for h in *hexes {
                let oklch = srgb_to_oklch(Srgb::from_hex(h).unwrap());
                println!(
                    "    OkLch {{ l: {:.17}, c: {:.17}, h: {:.17} }}, // {h}",
                    oklch.l, oklch.c, oklch.h
                );
            }
        }
    }

    // -- Built-in palette OkLch literal verification --

    /// Verifies that a built-in palette's OkLch literals match the original hex definitions.
    ///
    /// Tolerances are loose around grey: when chroma is effectively zero
    /// (well below any perceptible threshold) the hue is undefined, and
    /// `atan2(0, 0)`-style rounding can flip its low bits between runs.
    /// We compare hue only when chroma is meaningful.
    fn verify_palette_matches_hex(palette: &Palette, hexes: &[&str]) {
        const L_TOL: f64 = 1e-9;
        const C_TOL: f64 = 1e-6;
        const H_TOL: f64 = 1e-6;
        const C_HUE_THRESHOLD: f64 = 1e-4;
        assert_eq!(palette.len(), hexes.len());
        for (i, hex) in hexes.iter().enumerate() {
            let expected = srgb_to_oklch(Srgb::from_hex(hex).unwrap());
            let actual = palette.colors[i];
            let l_ok = (actual.l - expected.l).abs() < L_TOL;
            let c_ok = (actual.c - expected.c).abs() < C_TOL;
            let h_ok = if expected.c < C_HUE_THRESHOLD || actual.c < C_HUE_THRESHOLD {
                true
            } else {
                (actual.h - expected.h).abs() < H_TOL
            };
            assert!(
                l_ok && c_ok && h_ok,
                "palette color {i} ({hex}) mismatch: {:?} vs {:?}",
                actual,
                expected
            );
        }
    }

    #[test]
    fn builtin_ocean_matches_hex_definition() {
        verify_palette_matches_hex(
            &Palette::ocean(),
            &["#001f3f", "#003366", "#005f73", "#0a9396", "#94d2bd"],
        );
    }

    #[test]
    fn builtin_neon_matches_hex_definition() {
        verify_palette_matches_hex(
            &Palette::neon(),
            &["#ff00ff", "#00ff41", "#ffff00", "#ff0080", "#00ffff"],
        );
    }

    #[test]
    fn builtin_earth_matches_hex_definition() {
        verify_palette_matches_hex(
            &Palette::earth(),
            &["#5c4033", "#8b6914", "#6b8e23", "#daa520", "#d2b48c"],
        );
    }

    #[test]
    fn builtin_monochrome_matches_hex_definition() {
        verify_palette_matches_hex(
            &Palette::monochrome(),
            &["#000000", "#404040", "#808080", "#c0c0c0", "#ffffff"],
        );
    }

    #[test]
    fn builtin_vapor_matches_hex_definition() {
        verify_palette_matches_hex(
            &Palette::vapor(),
            &["#7b2d8e", "#c77dff", "#ff9ebb", "#80ced6", "#a0e7e5"],
        );
    }

    #[test]
    fn builtin_fire_matches_hex_definition() {
        verify_palette_matches_hex(
            &Palette::fire(),
            &["#800000", "#cc0000", "#ff4500", "#ff8c00", "#ffd700"],
        );
    }

    #[test]
    fn builtin_amber_matches_hex_definition() {
        verify_palette_matches_hex(
            &Palette::amber(),
            &[
                "#000000", "#1a0a00", "#5c2a00", "#c26200", "#ffb000", "#ffe6a8",
            ],
        );
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
            ("amber", Palette::amber()),
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
            ("amber", Palette::amber()),
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
