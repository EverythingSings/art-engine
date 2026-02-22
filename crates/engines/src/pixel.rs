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
}
