//! Pure-computation pixel buffer conversion from [`Field`] + [`Palette`].
//!
//! This module is always available (no feature gate) so that both the `png`
//! snapshot path and the WASM `ImageData` path can share the same conversion.

use art_engine_core::field::Field;
use art_engine_core::palette::Palette;

/// Maps field values through a palette to produce an RGBA8 pixel buffer.
///
/// Each field value `t` in [0, 1] is sampled from the palette and written as
/// four bytes (R, G, B, 255). The buffer length is `width * height * 4`.
pub fn field_to_rgba(field: &Field, palette: &Palette) -> Vec<u8> {
    field
        .data()
        .iter()
        .flat_map(|&t| {
            let srgb = palette.sample(t);
            let r = (srgb.r * 255.0).round() as u8;
            let g = (srgb.g * 255.0).round() as u8;
            let b = (srgb.b * 255.0).round() as u8;
            [r, g, b, 255u8]
        })
        .collect()
}

/// CPU-side post-processing parameters: scanlines, vignette, and grain.
///
/// All strengths are in [0, 1] and stack multiplicatively. A `PostFx::default()`
/// is a no-op (all zero). Each strength is clamped at apply time, so values
/// outside [0, 1] are accepted but bounded.
#[derive(Debug, Clone, Copy)]
pub struct PostFx {
    /// 0.0 = no scanlines, 1.0 = every other row fully blacked out.
    pub scanline_strength: f64,
    /// 0.0 = no vignette, 1.0 = corners fully blacked out.
    pub vignette_strength: f64,
    /// 0.0 = no grain, 1.0 = full-amplitude (+/- 50%) per-pixel modulation.
    pub grain_strength: f64,
    /// Deterministic seed for the grain pattern.
    pub grain_seed: u64,
}

impl Default for PostFx {
    fn default() -> Self {
        Self {
            scanline_strength: 0.0,
            vignette_strength: 0.0,
            grain_strength: 0.0,
            grain_seed: 0,
        }
    }
}

impl PostFx {
    /// A retro-CRT preset: visible scanlines, soft vignette, light grain.
    pub fn crt_amber() -> Self {
        Self {
            scanline_strength: 0.35,
            vignette_strength: 0.55,
            grain_strength: 0.10,
            grain_seed: 0xC07A_F33D_DEAD_BEEFu64,
        }
    }

    fn clamped(&self) -> Self {
        Self {
            scanline_strength: self.scanline_strength.clamp(0.0, 1.0),
            vignette_strength: self.vignette_strength.clamp(0.0, 1.0),
            grain_strength: self.grain_strength.clamp(0.0, 1.0),
            grain_seed: self.grain_seed,
        }
    }
}

/// Applies CRT-style post-processing to an RGBA8 buffer in-place.
///
/// Effects are applied in order: vignette, scanlines, grain. Each is
/// gated by its strength; passing `PostFx::default()` is a no-op.
/// Buffer length must be `width * height * 4`.
pub fn apply_postfx(rgba: &mut [u8], width: usize, height: usize, fx: &PostFx) {
    debug_assert_eq!(rgba.len(), width * height * 4);
    let fx = fx.clamped();
    if fx.scanline_strength == 0.0 && fx.vignette_strength == 0.0 && fx.grain_strength == 0.0 {
        return;
    }

    let cx = (width as f64 - 1.0) * 0.5;
    let cy = (height as f64 - 1.0) * 0.5;
    // Distance from center to corner; used to normalize vignette to [0, 1].
    let max_dist_sq = cx * cx + cy * cy;
    let inv_max_dist_sq = if max_dist_sq > 0.0 {
        1.0 / max_dist_sq
    } else {
        0.0
    };

    for y in 0..height {
        // Scanline factor: alternating rows attenuate. Strength 0 => 1.0,
        // strength 1 => 0.0 on odd rows.
        let scan_factor = if y & 1 == 1 {
            1.0 - fx.scanline_strength
        } else {
            1.0
        };

        for x in 0..width {
            let i = (y * width + x) * 4;

            // Vignette: smoothstep on normalized distance squared from center.
            let dx = x as f64 - cx;
            let dy = y as f64 - cy;
            let d2 = (dx * dx + dy * dy) * inv_max_dist_sq;
            // Vignette strength controls how aggressive the falloff is at
            // d2=1 (corner). Linear blend: 1 - strength * d2^1.5.
            let vignette_factor = (1.0 - fx.vignette_strength * d2.powf(1.5)).max(0.0);

            // Per-pixel grain: deterministic hash to value in [-0.5, 0.5].
            let grain_factor = if fx.grain_strength > 0.0 {
                let n = pixel_hash(x as u64, y as u64, fx.grain_seed);
                // Map to [-0.5, 0.5] then scale by strength so 1.0 = +/- 50%.
                let signed = (n as f64 / u64::MAX as f64) - 0.5;
                1.0 + fx.grain_strength * signed
            } else {
                1.0
            };

            let factor = scan_factor * vignette_factor * grain_factor;
            let factor = factor.clamp(0.0, 1.0);

            for c in 0..3 {
                let v = rgba[i + c] as f64 * factor;
                rgba[i + c] = v.round().clamp(0.0, 255.0) as u8;
            }
            // Alpha left untouched.
        }
    }
}

/// Cheap deterministic per-pixel hash (splitmix-ish), avoids per-pixel PRNG state.
fn pixel_hash(x: u64, y: u64, seed: u64) -> u64 {
    let mut z = x
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(y.wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add(seed.wrapping_mul(0x94D0_49BB_1331_11EB));
    z ^= z >> 30;
    z = z.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z ^= z >> 27;
    z = z.wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    z
}

#[cfg(test)]
mod tests {
    use super::*;
    use art_engine_core::field::Field;
    use art_engine_core::palette::Palette;

    #[test]
    fn field_to_rgba_correct_length() {
        let field = Field::new(8, 4).unwrap();
        let palette = Palette::ocean();
        let buf = field_to_rgba(&field, &palette);
        assert_eq!(buf.len(), 8 * 4 * 4);
    }

    #[test]
    fn field_to_rgba_alpha_always_255() {
        let field = Field::filled(4, 4, 0.5).unwrap();
        let palette = Palette::neon();
        let buf = field_to_rgba(&field, &palette);
        for (i, &byte) in buf.iter().enumerate() {
            if i % 4 == 3 {
                assert_eq!(byte, 255, "alpha at pixel {} should be 255", i / 4);
            }
        }
    }

    #[test]
    fn field_to_rgba_boundary_colors() {
        // t=0 should give first palette color, t=1 should give last
        let palette = Palette::monochrome(); // black -> white
        let field_zero = Field::filled(1, 1, 0.0).unwrap();
        let field_one = Field::filled(1, 1, 1.0).unwrap();

        let buf_zero = field_to_rgba(&field_zero, &palette);
        let buf_one = field_to_rgba(&field_one, &palette);

        // First color of monochrome is #000000 -> near 0
        assert!(buf_zero[0] < 10, "r at t=0: {}", buf_zero[0]);
        assert!(buf_zero[1] < 10, "g at t=0: {}", buf_zero[1]);
        assert!(buf_zero[2] < 10, "b at t=0: {}", buf_zero[2]);

        // Last color of monochrome is #ffffff -> near 255
        assert!(buf_one[0] > 245, "r at t=1: {}", buf_one[0]);
        assert!(buf_one[1] > 245, "g at t=1: {}", buf_one[1]);
        assert!(buf_one[2] > 245, "b at t=1: {}", buf_one[2]);
    }

    // -- PostFx tests --

    #[test]
    fn postfx_default_is_noop() {
        let mut buf = vec![128_u8; 16 * 16 * 4];
        let original = buf.clone();
        apply_postfx(&mut buf, 16, 16, &PostFx::default());
        assert_eq!(buf, original);
    }

    #[test]
    fn postfx_scanlines_darken_odd_rows() {
        let w = 4;
        let h = 4;
        let mut buf = vec![200_u8; w * h * 4];
        // Reset alpha to 255 for clarity (already 200; that's fine).
        let fx = PostFx {
            scanline_strength: 1.0,
            ..PostFx::default()
        };
        apply_postfx(&mut buf, w, h, &fx);
        // Row 0 (even): unchanged. Row 1 (odd): zeroed RGB, alpha intact.
        for x in 0..w {
            let i0 = x * 4;
            let i1 = (w + x) * 4;
            assert_eq!(buf[i0], 200, "row 0 should be unchanged");
            assert_eq!(buf[i1], 0, "row 1 RGB should be zero with full scanline");
            assert_eq!(buf[i1 + 3], 200, "alpha untouched");
        }
    }

    #[test]
    fn postfx_vignette_darkens_corners_more_than_center() {
        let w = 32;
        let h = 32;
        let mut buf = vec![200_u8; w * h * 4];
        let fx = PostFx {
            vignette_strength: 1.0,
            ..PostFx::default()
        };
        apply_postfx(&mut buf, w, h, &fx);
        let center = buf[((h / 2) * w + w / 2) * 4];
        let corner = buf[0];
        assert!(
            center > corner + 30,
            "center ({}) should be much brighter than corner ({}) with full vignette",
            center,
            corner
        );
    }

    #[test]
    fn postfx_grain_modulates_pixels_deterministically() {
        let w = 8;
        let h = 8;
        let mut a = vec![128_u8; w * h * 4];
        let mut b = vec![128_u8; w * h * 4];
        let fx = PostFx {
            grain_strength: 0.5,
            grain_seed: 12345,
            ..PostFx::default()
        };
        apply_postfx(&mut a, w, h, &fx);
        apply_postfx(&mut b, w, h, &fx);
        assert_eq!(a, b, "grain must be deterministic for same seed");

        let mut c = vec![128_u8; w * h * 4];
        let fx2 = PostFx {
            grain_seed: 67890,
            ..fx
        };
        apply_postfx(&mut c, w, h, &fx2);
        assert_ne!(a, c, "different seed must produce different grain");
    }

    #[test]
    fn postfx_alpha_untouched() {
        let w = 16;
        let h = 16;
        let mut buf = vec![200_u8; w * h * 4];
        for i in (3..buf.len()).step_by(4) {
            buf[i] = 217;
        }
        apply_postfx(&mut buf, w, h, &PostFx::crt_amber());
        for i in (3..buf.len()).step_by(4) {
            assert_eq!(buf[i], 217, "alpha at byte {i} was modified");
        }
    }

    #[test]
    fn postfx_clamps_out_of_range_strengths() {
        let w = 4;
        let h = 4;
        let mut buf = vec![200_u8; w * h * 4];
        let fx = PostFx {
            scanline_strength: 5.0,
            vignette_strength: -2.0,
            grain_strength: 99.0,
            grain_seed: 1,
        };
        // Should not panic. (u8 values are always in [0, 255] by type, but
        // we still want a non-empty assertion to anchor the test intent.)
        apply_postfx(&mut buf, w, h, &fx);
        assert_eq!(buf.len(), w * h * 4);
    }

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn field_to_rgba_length_invariant(
                w in 1_usize..64,
                h in 1_usize..64,
            ) {
                let field = Field::new(w, h).unwrap();
                let palette = Palette::ocean();
                let buf = field_to_rgba(&field, &palette);
                prop_assert_eq!(buf.len(), w * h * 4);
            }

            #[test]
            fn field_to_rgba_alpha_always_255_prop(
                w in 1_usize..32,
                h in 1_usize..32,
                t in 0.0_f64..=1.0,
            ) {
                let field = Field::filled(w, h, t).unwrap();
                let palette = Palette::earth();
                let buf = field_to_rgba(&field, &palette);
                for (i, chunk) in buf.chunks(4).enumerate() {
                    prop_assert!(chunk[3] == 255, "alpha at pixel {} should be 255", i);
                }
            }

            #[test]
            fn field_to_rgba_deterministic(
                w in 1_usize..16,
                h in 1_usize..16,
                t in 0.0_f64..=1.0,
            ) {
                let field = Field::filled(w, h, t).unwrap();
                let palette = Palette::neon();
                let buf1 = field_to_rgba(&field, &palette);
                let buf2 = field_to_rgba(&field, &palette);
                prop_assert_eq!(buf1, buf2);
            }
        }
    }
}
