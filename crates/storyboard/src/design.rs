//! The show's design language.
//!
//! Lives as Rust constants because the design system is operated by hand
//! by the human in the loop (me) — when I want to change the show's look
//! globally I edit this file. Not a public-stable API.
//!
//! Initial values are seeded from ep1's thumbnail and karaoke subtitle
//! choices: deep indigo / dusty teal / warm amber / soft white, Arial
//! Black for display type, Inter for body. Subject to evolve.

/// Deep indigo, used as the lowest base value for backdrops and as the
/// "ink" behind text overlays.
pub const COLOR_INK: [f32; 3] = [0.04, 0.05, 0.10];
/// Dusty teal — the dominant cool tone in the Flow backdrop.
pub const COLOR_TEAL: [f32; 3] = [0.10, 0.32, 0.40];
/// Warm amber — the accent color used for highlighted words in karaoke
/// subtitles and as the high stop in the Flow backdrop palette.
pub const COLOR_AMBER: [f32; 3] = [0.96, 0.74, 0.36];
/// Hot construction-orange — the *chrome* accent used for system labels,
/// registration brackets, status banners. Distinct from `COLOR_AMBER`
/// (which stays inside the content layer) so the chrome reads as a
/// separate, louder typographic layer.
pub const COLOR_CHROME_ORANGE: [f32; 3] = [0.91, 0.30, 0.13];
/// Soft white, used for primary subtitle text and idle title-card type.
pub const COLOR_BONE: [f32; 3] = [0.95, 0.95, 0.93];

/// Display typeface (used for title cards, pull quotes, sigils).
pub const TYPE_DISPLAY: &str = "Arial Black";
/// Body typeface (used for kicker text, captions where applicable).
pub const TYPE_BODY: &str = "Inter";

/// Distance from the frame edge that text overlays must respect, in pixels
/// at the canonical 1080×1920 working resolution.
pub const SAFE_MARGIN: u32 = 60;

/// Bottom band reserved for karaoke subtitles, in pixels (keeps overlays
/// out of the caption area).
pub const CAPTION_BAND: u32 = 380;

/// The default three-stop palette used by the Flow backdrop when the
/// storyboard names `PaletteRef::TealAmber`.
pub const PALETTE_TEAL_AMBER: [[f32; 3]; 3] = [COLOR_INK, COLOR_TEAL, COLOR_AMBER];

#[cfg(test)]
mod tests {
    use super::*;

    /// Sanity check that the design system tokens are in the unit interval —
    /// they are linear-sRGB values and must be 0..=1.
    #[test]
    fn colors_are_in_unit_interval() {
        for c in [COLOR_INK, COLOR_TEAL, COLOR_AMBER, COLOR_BONE] {
            for v in c {
                assert!((0.0..=1.0).contains(&v), "color value out of range: {v}");
            }
        }
    }

    #[test]
    fn palette_teal_amber_is_three_distinct_stops() {
        assert_eq!(PALETTE_TEAL_AMBER[0], COLOR_INK);
        assert_eq!(PALETTE_TEAL_AMBER[1], COLOR_TEAL);
        assert_eq!(PALETTE_TEAL_AMBER[2], COLOR_AMBER);
    }
}
