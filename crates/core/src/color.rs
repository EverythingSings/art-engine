//! Color types and conversion functions for the art-engine.
//!
//! Provides four color types (`Srgb`, `LinearRgb`, `OkLab`, `OkLch`) and
//! pure conversion functions between them. All conversions are pure functions
//! (no methods with side effects). Uses `f64` throughout for precision.
//!
//! The OKLab color space provides perceptually uniform gradients, making it
//! ideal for generative art palette interpolation.

use crate::error::EngineError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// sRGB color with components in [0, 1].
///
/// Serializes as a hex string `"#rrggbb"` for human-readable formats.
/// The hex round-trip has 8-bit quantization (1/255 precision loss),
/// which is acceptable since hex colors are inherently 8-bit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Srgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

/// Linear RGB color (gamma-decoded).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearRgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

/// OKLab perceptual color space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OkLab {
    pub l: f64,
    pub a: f64,
    pub b: f64,
}

/// OKLCh (cylindrical form of OKLab).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OkLch {
    pub l: f64,
    pub c: f64,
    pub h: f64,
}

impl Srgb {
    /// Parses a hex color string like "#ff00aa" or "ff00aa" (case insensitive).
    ///
    /// Returns `EngineError::InvalidColor` if the input is not a valid 6-digit hex color.
    pub fn from_hex(hex: &str) -> Result<Srgb, EngineError> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        if hex.len() != 6 {
            return Err(EngineError::InvalidColor(format!(
                "expected 6 hex digits, got {}",
                hex.len()
            )));
        }
        let r = u8::from_str_radix(&hex[0..2], 16)
            .map_err(|e| EngineError::InvalidColor(format!("invalid red component: {e}")))?;
        let g = u8::from_str_radix(&hex[2..4], 16)
            .map_err(|e| EngineError::InvalidColor(format!("invalid green component: {e}")))?;
        let b = u8::from_str_radix(&hex[4..6], 16)
            .map_err(|e| EngineError::InvalidColor(format!("invalid blue component: {e}")))?;
        Ok(Srgb {
            r: r as f64 / 255.0,
            g: g as f64 / 255.0,
            b: b as f64 / 255.0,
        })
    }

    /// Converts the color to a hex string like `"#rrggbb"`.
    ///
    /// Components are quantized to 8-bit (0–255) with rounding.
    pub fn to_hex(self) -> String {
        let r = (self.r.clamp(0.0, 1.0) * 255.0).round() as u8;
        let g = (self.g.clamp(0.0, 1.0) * 255.0).round() as u8;
        let b = (self.b.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{r:02x}{g:02x}{b:02x}")
    }
}

impl Serialize for Srgb {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Srgb {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Srgb::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

/// Applies inverse sRGB gamma to convert a single sRGB component to linear.
fn srgb_component_to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Applies sRGB gamma to convert a single linear component to sRGB.
fn linear_component_to_srgb(c: f64) -> f64 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Converts sRGB to linear RGB by applying inverse sRGB gamma.
pub fn srgb_to_linear(c: Srgb) -> LinearRgb {
    LinearRgb {
        r: srgb_component_to_linear(c.r),
        g: srgb_component_to_linear(c.g),
        b: srgb_component_to_linear(c.b),
    }
}

/// Converts linear RGB to sRGB by applying sRGB gamma.
pub fn linear_to_srgb(c: LinearRgb) -> Srgb {
    Srgb {
        r: linear_component_to_srgb(c.r),
        g: linear_component_to_srgb(c.g),
        b: linear_component_to_srgb(c.b),
    }
}

/// Converts linear RGB to OKLab via the OKLab matrix transform.
pub fn linear_to_oklab(c: LinearRgb) -> OkLab {
    let l_ = 0.4122214708 * c.r + 0.5363325363 * c.g + 0.0514459929 * c.b;
    let m_ = 0.2119034982 * c.r + 0.6806995451 * c.g + 0.1073969566 * c.b;
    let s_ = 0.0883024619 * c.r + 0.2817188376 * c.g + 0.6299787005 * c.b;

    let l_c = l_.cbrt();
    let m_c = m_.cbrt();
    let s_c = s_.cbrt();

    OkLab {
        l: 0.2104542553 * l_c + 0.7936177850 * m_c - 0.0040720468 * s_c,
        a: 1.9779984951 * l_c - 2.4285922050 * m_c + 0.4505937099 * s_c,
        b: 0.0259040371 * l_c + 0.7827717662 * m_c - 0.8086757660 * s_c,
    }
}

/// Converts OKLab to linear RGB via the inverse OKLab matrix transform.
pub fn oklab_to_linear(c: OkLab) -> LinearRgb {
    let l_ = c.l + 0.3963377774 * c.a + 0.2158037573 * c.b;
    let m_ = c.l - 0.1055613458 * c.a - 0.0638541728 * c.b;
    let s_ = c.l - 0.0894841775 * c.a - 1.2914855480 * c.b;

    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    LinearRgb {
        r: 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        g: -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        b: -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    }
}

/// Converts OKLab to OKLCh (cylindrical form).
///
/// NaN guard: if chroma is less than 1e-10, hue is set to 0.0 to avoid
/// indeterminate `atan2(0, 0)` results.
pub fn oklab_to_oklch(c: OkLab) -> OkLch {
    let ch = (c.a * c.a + c.b * c.b).sqrt();
    let h = if ch < 1e-10 {
        0.0
    } else {
        c.b.atan2(c.a).to_degrees().rem_euclid(360.0)
    };
    OkLch { l: c.l, c: ch, h }
}

/// Converts OKLCh to OKLab.
pub fn oklch_to_oklab(c: OkLch) -> OkLab {
    let h_rad = c.h.to_radians();
    OkLab {
        l: c.l,
        a: c.c * h_rad.cos(),
        b: c.c * h_rad.sin(),
    }
}

/// Convenience: sRGB to OKLCh via the chain sRGB -> linear -> OKLab -> OKLCh.
pub fn srgb_to_oklch(c: Srgb) -> OkLch {
    oklab_to_oklch(linear_to_oklab(srgb_to_linear(c)))
}

/// Convenience: OKLCh to sRGB via the chain OKLCh -> OKLab -> linear -> sRGB,
/// with output clamped to [0, 1].
pub fn oklch_to_srgb(c: OkLch) -> Srgb {
    let srgb = linear_to_srgb(oklab_to_linear(oklch_to_oklab(c)));
    Srgb {
        r: srgb.r.clamp(0.0, 1.0),
        g: srgb.g.clamp(0.0, 1.0),
        b: srgb.b.clamp(0.0, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f64 = 1e-6;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < EPSILON
    }

    // -- sRGB <-> Linear round-trip tests --

    #[test]
    fn srgb_to_linear_black_is_zero() {
        let black = Srgb {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        };
        let lin = srgb_to_linear(black);
        assert!(approx_eq(lin.r, 0.0));
        assert!(approx_eq(lin.g, 0.0));
        assert!(approx_eq(lin.b, 0.0));
    }

    #[test]
    fn srgb_to_linear_white_is_one() {
        let white = Srgb {
            r: 1.0,
            g: 1.0,
            b: 1.0,
        };
        let lin = srgb_to_linear(white);
        assert!(approx_eq(lin.r, 1.0));
        assert!(approx_eq(lin.g, 1.0));
        assert!(approx_eq(lin.b, 1.0));
    }

    #[test]
    fn srgb_linear_round_trip_pure_red() {
        let red = Srgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
        };
        let round_tripped = linear_to_srgb(srgb_to_linear(red));
        assert!(approx_eq(round_tripped.r, 1.0));
        assert!(approx_eq(round_tripped.g, 0.0));
        assert!(approx_eq(round_tripped.b, 0.0));
    }

    #[test]
    fn srgb_linear_round_trip_mid_gray() {
        let gray = Srgb {
            r: 0.5,
            g: 0.5,
            b: 0.5,
        };
        let round_tripped = linear_to_srgb(srgb_to_linear(gray));
        assert!(approx_eq(round_tripped.r, 0.5));
        assert!(approx_eq(round_tripped.g, 0.5));
        assert!(approx_eq(round_tripped.b, 0.5));
    }

    #[test]
    fn srgb_gamma_boundary_at_0_04045() {
        // Value exactly at the boundary between linear and gamma segments.
        let boundary = Srgb {
            r: 0.04045,
            g: 0.0,
            b: 0.0,
        };
        let lin = srgb_to_linear(boundary);
        // The linear segment: 0.04045 / 12.92 = 0.003130804953...
        assert!(approx_eq(lin.r, 0.04045 / 12.92));

        // Just above the boundary should use the power function
        let above = Srgb {
            r: 0.04046,
            g: 0.0,
            b: 0.0,
        };
        let lin_above = srgb_to_linear(above);
        let expected = ((0.04046 + 0.055) / 1.055_f64).powf(2.4);
        assert!(approx_eq(lin_above.r, expected));
    }

    #[test]
    fn linear_to_srgb_boundary_at_0_0031308() {
        let boundary = LinearRgb {
            r: 0.0031308,
            g: 0.0,
            b: 0.0,
        };
        let srgb = linear_to_srgb(boundary);
        assert!(approx_eq(srgb.r, 0.0031308 * 12.92));

        let above = LinearRgb {
            r: 0.0031309,
            g: 0.0,
            b: 0.0,
        };
        let srgb_above = linear_to_srgb(above);
        let expected = 1.055 * 0.0031309_f64.powf(1.0 / 2.4) - 0.055;
        assert!(approx_eq(srgb_above.r, expected));
    }

    // -- OKLab / OKLCh conversion tests --

    #[test]
    fn white_in_oklab_has_l_near_one_and_zero_chroma() {
        let white = LinearRgb {
            r: 1.0,
            g: 1.0,
            b: 1.0,
        };
        let lab = linear_to_oklab(white);
        assert!(approx_eq(lab.l, 1.0), "expected L~1.0, got {}", lab.l);
        assert!(approx_eq(lab.a, 0.0), "expected a~0.0, got {}", lab.a);
        assert!(approx_eq(lab.b, 0.0), "expected b~0.0, got {}", lab.b);
    }

    #[test]
    fn black_in_oklab_has_l_near_zero() {
        let black = LinearRgb {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        };
        let lab = linear_to_oklab(black);
        assert!(approx_eq(lab.l, 0.0), "expected L~0.0, got {}", lab.l);
        assert!(approx_eq(lab.a, 0.0), "expected a~0.0, got {}", lab.a);
        assert!(approx_eq(lab.b, 0.0), "expected b~0.0, got {}", lab.b);
    }

    #[test]
    fn oklab_linear_round_trip_pure_red() {
        let red = LinearRgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
        };
        let round_tripped = oklab_to_linear(linear_to_oklab(red));
        assert!(
            approx_eq(round_tripped.r, 1.0),
            "expected r~1.0, got {}",
            round_tripped.r
        );
        assert!(
            approx_eq(round_tripped.g, 0.0),
            "expected g~0.0, got {}",
            round_tripped.g
        );
        assert!(
            approx_eq(round_tripped.b, 0.0),
            "expected b~0.0, got {}",
            round_tripped.b
        );
    }

    #[test]
    fn oklab_linear_round_trip_pure_green() {
        let green = LinearRgb {
            r: 0.0,
            g: 1.0,
            b: 0.0,
        };
        let round_tripped = oklab_to_linear(linear_to_oklab(green));
        assert!(approx_eq(round_tripped.r, 0.0), "r: {}", round_tripped.r);
        assert!(approx_eq(round_tripped.g, 1.0), "g: {}", round_tripped.g);
        assert!(approx_eq(round_tripped.b, 0.0), "b: {}", round_tripped.b);
    }

    #[test]
    fn oklab_linear_round_trip_pure_blue() {
        let blue = LinearRgb {
            r: 0.0,
            g: 0.0,
            b: 1.0,
        };
        let round_tripped = oklab_to_linear(linear_to_oklab(blue));
        assert!(approx_eq(round_tripped.r, 0.0), "r: {}", round_tripped.r);
        assert!(approx_eq(round_tripped.g, 0.0), "g: {}", round_tripped.g);
        assert!(approx_eq(round_tripped.b, 1.0), "b: {}", round_tripped.b);
    }

    #[test]
    fn oklch_pure_red_has_hue_near_29_degrees() {
        // sRGB red -> OKLCh should have hue approximately 29.2 degrees
        let red = Srgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
        };
        let lch = srgb_to_oklch(red);
        assert!(
            (lch.h - 29.2).abs() < 1.0,
            "expected red hue ~29.2, got {}",
            lch.h
        );
        assert!(lch.c > 0.0, "expected positive chroma for red");
    }

    #[test]
    fn oklch_pure_green_has_hue_near_142_degrees() {
        let green = Srgb {
            r: 0.0,
            g: 1.0,
            b: 0.0,
        };
        let lch = srgb_to_oklch(green);
        assert!(
            (lch.h - 142.5).abs() < 1.5,
            "expected green hue ~142.5, got {}",
            lch.h
        );
    }

    #[test]
    fn oklch_nan_guard_zero_chroma_sets_hue_to_zero() {
        // A color with a=0, b=0 should produce hue=0, not NaN.
        let achromatic = OkLab {
            l: 0.5,
            a: 0.0,
            b: 0.0,
        };
        let lch = oklab_to_oklch(achromatic);
        assert_eq!(lch.h, 0.0, "achromatic color should have hue=0");
        assert!(lch.c < 1e-10, "achromatic color should have chroma~0");
        assert!(!lch.h.is_nan(), "hue must not be NaN");
    }

    #[test]
    fn oklch_oklab_round_trip() {
        let original = OkLch {
            l: 0.7,
            c: 0.15,
            h: 250.0,
        };
        let round_tripped = oklab_to_oklch(oklch_to_oklab(original));
        assert!(
            approx_eq(round_tripped.l, original.l),
            "L: {} vs {}",
            round_tripped.l,
            original.l
        );
        assert!(
            approx_eq(round_tripped.c, original.c),
            "C: {} vs {}",
            round_tripped.c,
            original.c
        );
        assert!(
            approx_eq(round_tripped.h, original.h),
            "h: {} vs {}",
            round_tripped.h,
            original.h
        );
    }

    // -- Full pipeline round-trip: sRGB -> OKLCh -> sRGB --

    #[test]
    fn srgb_oklch_round_trip_known_colors() {
        let colors = [
            Srgb {
                r: 1.0,
                g: 0.0,
                b: 0.0,
            },
            Srgb {
                r: 0.0,
                g: 1.0,
                b: 0.0,
            },
            Srgb {
                r: 0.0,
                g: 0.0,
                b: 1.0,
            },
            Srgb {
                r: 1.0,
                g: 1.0,
                b: 1.0,
            },
            Srgb {
                r: 0.0,
                g: 0.0,
                b: 0.0,
            },
            Srgb {
                r: 0.5,
                g: 0.3,
                b: 0.8,
            },
        ];
        for (i, &color) in colors.iter().enumerate() {
            let round_tripped = oklch_to_srgb(srgb_to_oklch(color));
            assert!(
                approx_eq(round_tripped.r, color.r),
                "color {i}: r={} vs {}",
                round_tripped.r,
                color.r
            );
            assert!(
                approx_eq(round_tripped.g, color.g),
                "color {i}: g={} vs {}",
                round_tripped.g,
                color.g
            );
            assert!(
                approx_eq(round_tripped.b, color.b),
                "color {i}: b={} vs {}",
                round_tripped.b,
                color.b
            );
        }
    }

    #[test]
    fn oklch_to_srgb_clamps_out_of_gamut() {
        // Very high chroma at some hues can produce out-of-gamut linear RGB.
        // The result should be clamped to [0, 1].
        let out_of_gamut = OkLch {
            l: 0.9,
            c: 0.4,
            h: 150.0,
        };
        let srgb = oklch_to_srgb(out_of_gamut);
        assert!(srgb.r >= 0.0 && srgb.r <= 1.0, "r out of range: {}", srgb.r);
        assert!(srgb.g >= 0.0 && srgb.g <= 1.0, "g out of range: {}", srgb.g);
        assert!(srgb.b >= 0.0 && srgb.b <= 1.0, "b out of range: {}", srgb.b);
    }

    // -- Hex parsing tests --

    #[test]
    fn from_hex_parses_red_with_hash() {
        let red = Srgb::from_hex("#ff0000").unwrap();
        assert!(approx_eq(red.r, 1.0));
        assert!(approx_eq(red.g, 0.0));
        assert!(approx_eq(red.b, 0.0));
    }

    #[test]
    fn from_hex_parses_green_without_hash() {
        let green = Srgb::from_hex("00ff00").unwrap();
        assert!(approx_eq(green.r, 0.0));
        assert!(approx_eq(green.g, 1.0));
        assert!(approx_eq(green.b, 0.0));
    }

    #[test]
    fn from_hex_is_case_insensitive() {
        let upper = Srgb::from_hex("#FF00AA").unwrap();
        let lower = Srgb::from_hex("#ff00aa").unwrap();
        assert!(approx_eq(upper.r, lower.r));
        assert!(approx_eq(upper.g, lower.g));
        assert!(approx_eq(upper.b, lower.b));
    }

    #[test]
    fn from_hex_returns_error_for_invalid_hex() {
        assert!(Srgb::from_hex("#gggggg").is_err());
        assert!(Srgb::from_hex("#fff").is_err()); // too short
        assert!(Srgb::from_hex("").is_err());
        assert!(Srgb::from_hex("#ff00ff00").is_err()); // too long
    }

    #[test]
    fn from_hex_parses_arbitrary_color() {
        let color = Srgb::from_hex("#804020").unwrap();
        assert!(approx_eq(color.r, 0x80 as f64 / 255.0));
        assert!(approx_eq(color.g, 0x40 as f64 / 255.0));
        assert!(approx_eq(color.b, 0x20 as f64 / 255.0));
    }

    // -- to_hex tests --

    #[test]
    fn to_hex_pure_red() {
        let red = Srgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
        };
        assert_eq!(red.to_hex(), "#ff0000");
    }

    #[test]
    fn to_hex_pure_white() {
        let white = Srgb {
            r: 1.0,
            g: 1.0,
            b: 1.0,
        };
        assert_eq!(white.to_hex(), "#ffffff");
    }

    #[test]
    fn to_hex_pure_black() {
        let black = Srgb {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        };
        assert_eq!(black.to_hex(), "#000000");
    }

    #[test]
    fn to_hex_known_color() {
        let color = Srgb {
            r: 0x80 as f64 / 255.0,
            g: 0x40 as f64 / 255.0,
            b: 0x20 as f64 / 255.0,
        };
        assert_eq!(color.to_hex(), "#804020");
    }

    #[test]
    fn from_hex_to_hex_round_trip() {
        let original = "#c0ffee";
        let color = Srgb::from_hex(original).unwrap();
        assert_eq!(color.to_hex(), original);
    }

    // -- Serde tests --

    #[test]
    fn srgb_serializes_as_hex_string() {
        let red = Srgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
        };
        let json = serde_json::to_string(&red).unwrap();
        assert_eq!(json, "\"#ff0000\"");
    }

    #[test]
    fn srgb_deserializes_from_hex_string() {
        let json = "\"#00ff00\"";
        let green: Srgb = serde_json::from_str(json).unwrap();
        assert!(approx_eq(green.r, 0.0));
        assert!(approx_eq(green.g, 1.0));
        assert!(approx_eq(green.b, 0.0));
    }

    #[test]
    fn srgb_json_round_trip() {
        let original = Srgb {
            r: 0x80 as f64 / 255.0,
            g: 0x40 as f64 / 255.0,
            b: 0x20 as f64 / 255.0,
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: Srgb = serde_json::from_str(&json).unwrap();
        // 8-bit quantization means exact match within 1/255
        assert!((deserialized.r - original.r).abs() < 1.0 / 255.0 + 1e-10);
        assert!((deserialized.g - original.g).abs() < 1.0 / 255.0 + 1e-10);
        assert!((deserialized.b - original.b).abs() < 1.0 / 255.0 + 1e-10);
    }

    #[test]
    fn srgb_deserialize_rejects_invalid_hex() {
        let result: Result<Srgb, _> = serde_json::from_str("\"not-a-color\"");
        assert!(result.is_err());
    }

    #[test]
    fn to_hex_clamps_out_of_range() {
        let color = Srgb {
            r: 1.5,
            g: -0.1,
            b: 0.5,
        };
        let hex = color.to_hex();
        assert_eq!(hex, "#ff0080");
    }

    #[test]
    fn hex_round_trip_is_idempotent_after_first_quantization() {
        let original = Srgb {
            r: 0.123456,
            g: 0.654321,
            b: 0.999999,
        };
        // First quantization pass
        let once = Srgb::from_hex(&original.to_hex()).unwrap();
        // Second quantization pass
        let twice = Srgb::from_hex(&once.to_hex()).unwrap();
        // After the first quantization, further round-trips must be bit-identical
        assert_eq!(once.r.to_bits(), twice.r.to_bits());
        assert_eq!(once.g.to_bits(), twice.g.to_bits());
        assert_eq!(once.b.to_bits(), twice.b.to_bits());
    }

    // -- Property-based tests --

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        /// Strategy for sRGB component values in [0, 1].
        fn srgb_component() -> impl Strategy<Value = f64> {
            0.0_f64..=1.0
        }

        proptest! {
            #[test]
            fn srgb_to_oklch_round_trip_within_epsilon(
                r in srgb_component(),
                g in srgb_component(),
                b in srgb_component(),
            ) {
                let original = Srgb { r, g, b };
                let round_tripped = oklch_to_srgb(srgb_to_oklch(original));
                prop_assert!(
                    (round_tripped.r - original.r).abs() < 1e-5,
                    "r: {} vs {}", round_tripped.r, original.r
                );
                prop_assert!(
                    (round_tripped.g - original.g).abs() < 1e-5,
                    "g: {} vs {}", round_tripped.g, original.g
                );
                prop_assert!(
                    (round_tripped.b - original.b).abs() < 1e-5,
                    "b: {} vs {}", round_tripped.b, original.b
                );
            }

            #[test]
            fn srgb_linear_round_trip_within_epsilon(
                r in srgb_component(),
                g in srgb_component(),
                b in srgb_component(),
            ) {
                let original = Srgb { r, g, b };
                let round_tripped = linear_to_srgb(srgb_to_linear(original));
                prop_assert!(
                    (round_tripped.r - original.r).abs() < 1e-10,
                    "r: {} vs {}", round_tripped.r, original.r
                );
                prop_assert!(
                    (round_tripped.g - original.g).abs() < 1e-10,
                    "g: {} vs {}", round_tripped.g, original.g
                );
                prop_assert!(
                    (round_tripped.b - original.b).abs() < 1e-10,
                    "b: {} vs {}", round_tripped.b, original.b
                );
            }

            #[test]
            fn oklch_to_srgb_always_produces_valid_range(
                l in 0.0_f64..=1.0,
                c in 0.0_f64..=0.4,
                h in 0.0_f64..360.0,
            ) {
                let color = OkLch { l, c, h };
                let srgb = oklch_to_srgb(color);
                prop_assert!(
                    srgb.r >= 0.0 && srgb.r <= 1.0,
                    "r out of range: {}", srgb.r
                );
                prop_assert!(
                    srgb.g >= 0.0 && srgb.g <= 1.0,
                    "g out of range: {}", srgb.g
                );
                prop_assert!(
                    srgb.b >= 0.0 && srgb.b <= 1.0,
                    "b out of range: {}", srgb.b
                );
            }

            #[test]
            fn srgb_hex_round_trip_within_quantization(
                r in srgb_component(),
                g in srgb_component(),
                b in srgb_component(),
            ) {
                let original = Srgb { r, g, b };
                let round_tripped = Srgb::from_hex(&original.to_hex()).unwrap();
                // Hex is 8-bit: max error is 0.5/255
                let max_err = 0.5 / 255.0 + 1e-10;
                prop_assert!(
                    (round_tripped.r - original.r).abs() < max_err,
                    "r: {} vs {}", round_tripped.r, original.r
                );
                prop_assert!(
                    (round_tripped.g - original.g).abs() < max_err,
                    "g: {} vs {}", round_tripped.g, original.g
                );
                prop_assert!(
                    (round_tripped.b - original.b).abs() < max_err,
                    "b: {} vs {}", round_tripped.b, original.b
                );
            }

            #[test]
            fn oklch_hue_is_never_nan(
                l in 0.0_f64..=1.0,
                a in -0.5_f64..=0.5,
                b_val in -0.5_f64..=0.5,
            ) {
                let lab = OkLab { l, a, b: b_val };
                let lch = oklab_to_oklch(lab);
                prop_assert!(!lch.h.is_nan(), "hue is NaN for a={a}, b={b_val}");
                prop_assert!(!lch.c.is_nan(), "chroma is NaN for a={a}, b={b_val}");
                prop_assert!(lch.h >= 0.0 && lch.h < 360.0,
                    "hue {} out of [0, 360) for a={a}, b={b_val}", lch.h);
            }
        }
    }
}
