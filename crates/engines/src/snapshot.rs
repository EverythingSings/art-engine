//! CPU-side PNG rendering of a [`Field`].
//!
//! This module is feature-gated behind `png` (default on) so that WASM builds
//! can depend on the `engines` crate without pulling in the `image` crate.
//! The pixel buffer conversion itself lives in [`crate::pixel`] (always available).

use art_engine_core::error::EngineError;
use art_engine_core::field::Field;
use art_engine_core::palette::Palette;
use std::path::Path;

use crate::pixel::field_to_rgba;

/// Writes a field as a PNG image, mapping values through the given palette.
///
/// Returns `EngineError::InvalidDimensions` if the field dimensions overflow
/// `u32`, or `EngineError::Io` on write failure.
pub fn write_png(field: &Field, palette: &Palette, path: &Path) -> Result<(), EngineError> {
    let rgba = field_to_rgba(field, palette);
    let w = u32::try_from(field.width()).map_err(|_| EngineError::InvalidDimensions)?;
    let h = u32::try_from(field.height()).map_err(|_| EngineError::InvalidDimensions)?;
    let img = image::RgbaImage::from_raw(w, h, rgba)
        .ok_or_else(|| EngineError::Io("RGBA buffer size mismatch".into()))?;
    img.save(path).map_err(|e| EngineError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use art_engine_core::field::Field;
    use art_engine_core::palette::Palette;

    #[test]
    fn write_png_round_trip() {
        let field = Field::filled(16, 16, 0.3).unwrap();
        let palette = Palette::ocean();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.png");

        write_png(&field, &palette, &path).unwrap();

        // Verify the file exists and can be read back
        let img = image::open(&path).unwrap().to_rgba8();
        assert_eq!(img.width(), 16);
        assert_eq!(img.height(), 16);
    }
}
