//! Cheap perceptual metrics computed from a rendered RGBA8 buffer.
//!
//! These are the explorer's read on *where in the possibility space* the
//! current composition sits — not its structural recipe (that's the genome)
//! but its perceptual character: how bright, how contrasty, how colourful,
//! how busy. They mirror the "always compute" tier of the engine's planned
//! stats system, kept self-contained here so the explorer has no dependency
//! on a stats crate that doesn't exist yet.
//!
//! All metrics operate on the raw byte values treated as `[0, 1]`. They are
//! relative coordinates for navigation, not colorimetric measurements, so no
//! gamma decode is applied.

/// Perceptual coordinates of one rendered frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    /// Mean luminance (Rec. 709), `[0, 1]`. Dark ↔ bright.
    pub luminance: f64,
    /// Luminance standard deviation, ~`[0, 0.5]`. Flat ↔ high-contrast.
    pub contrast: f64,
    /// Hasler–Suesstrunk colourfulness on normalised channels, ~`[0, 0.6]`.
    /// Monochrome ↔ vivid.
    pub colorfulness: f64,
    /// Fraction of pixels on a strong luminance edge, `[0, 1]`. Calm ↔ busy.
    pub edge_density: f64,
}

impl Metrics {
    /// One-line qualitative descriptor for each axis, for the diagnostics panel.
    pub fn describe(&self) -> String {
        format!(
            "luminance {:.2} ({})   contrast {:.2} ({})   colorfulness {:.2} ({})   edges {:.2} ({})",
            self.luminance,
            bucket(self.luminance, &["dark", "mid", "bright"], &[0.25, 0.6]),
            self.contrast,
            bucket(self.contrast, &["flat", "moderate", "punchy"], &[0.1, 0.22]),
            self.colorfulness,
            bucket(self.colorfulness, &["muted", "balanced", "vivid"], &[0.12, 0.28]),
            self.edge_density,
            bucket(self.edge_density, &["calm", "textured", "busy"], &[0.12, 0.32]),
        )
    }
}

/// Picks a label by thresholding `v` against ascending `cuts`.
/// `labels.len()` must equal `cuts.len() + 1`.
fn bucket(v: f64, labels: &[&'static str], cuts: &[f64]) -> &'static str {
    let idx = cuts.iter().take_while(|&&c| v >= c).count();
    labels[idx.min(labels.len() - 1)]
}

/// Computes [`Metrics`] from an RGBA8 buffer of `width × height` pixels.
///
/// Returns [`Metrics::default`] (all zero) for an empty or malformed buffer.
pub fn analyze(bytes: &[u8], width: usize, height: usize) -> Metrics {
    let n = width * height;
    if n == 0 || bytes.len() < n * 4 {
        return Metrics::default();
    }

    let inv = 1.0 / 255.0;
    let mut lum = vec![0.0f64; n];

    let mut sum_l = 0.0;
    let mut sum_l2 = 0.0;
    let mut sum_rg = 0.0;
    let mut sum_yb = 0.0;
    let mut sum_rg2 = 0.0;
    let mut sum_yb2 = 0.0;

    for i in 0..n {
        let r = bytes[i * 4] as f64 * inv;
        let g = bytes[i * 4 + 1] as f64 * inv;
        let b = bytes[i * 4 + 2] as f64 * inv;

        let l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        lum[i] = l; // retained for the Sobel edge pass below
        sum_l += l;
        sum_l2 += l * l;

        let rg = r - g;
        let yb = 0.5 * (r + g) - b;
        sum_rg += rg;
        sum_yb += yb;
        sum_rg2 += rg * rg;
        sum_yb2 += yb * yb;
    }

    let nf = n as f64;
    let mean_l = sum_l / nf;
    let contrast = (sum_l2 / nf - mean_l * mean_l).max(0.0).sqrt();

    let mean_rg = sum_rg / nf;
    let mean_yb = sum_yb / nf;
    let std_rg = (sum_rg2 / nf - mean_rg * mean_rg).max(0.0).sqrt();
    let std_yb = (sum_yb2 / nf - mean_yb * mean_yb).max(0.0).sqrt();
    let colorfulness = (std_rg * std_rg + std_yb * std_yb).sqrt()
        + 0.3 * (mean_rg * mean_rg + mean_yb * mean_yb).sqrt();

    let edge_density = sobel_edge_density(&lum, width, height, 0.18);

    Metrics {
        luminance: mean_l,
        contrast,
        colorfulness,
        edge_density,
    }
}

/// Fraction of interior pixels whose Sobel gradient magnitude exceeds `thresh`.
fn sobel_edge_density(lum: &[f64], width: usize, height: usize, thresh: f64) -> f64 {
    if width < 3 || height < 3 {
        return 0.0;
    }
    let at = |x: usize, y: usize| lum[y * width + x];
    let mut edges = 0usize;
    let interior = (width - 2) * (height - 2);

    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let gx = (at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1));
            let gy = (at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1));
            if (gx * gx + gy * gy).sqrt() > thresh {
                edges += 1;
            }
        }
    }
    edges as f64 / interior as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: usize, height: usize, rgb: [u8; 3]) -> Vec<u8> {
        let mut v = Vec::with_capacity(width * height * 4);
        for _ in 0..width * height {
            v.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
        v
    }

    #[test]
    fn empty_buffer_is_zero() {
        assert_eq!(analyze(&[], 0, 0), Metrics::default());
        assert_eq!(analyze(&[1, 2, 3], 4, 4), Metrics::default());
    }

    #[test]
    fn flat_gray_has_no_contrast_color_or_edges() {
        let m = analyze(&solid(16, 16, [128, 128, 128]), 16, 16);
        assert!((m.luminance - 128.0 / 255.0).abs() < 1e-6);
        // Single-pass variance (E[l²]-E[l]²) leaves float cancellation noise
        // on a constant image; 1e-6 still means "effectively no contrast".
        assert!(m.contrast < 1e-6, "contrast {}", m.contrast);
        assert!(m.colorfulness < 1e-9, "colorfulness {}", m.colorfulness);
        assert!(m.edge_density < 1e-9, "edges {}", m.edge_density);
    }

    #[test]
    fn black_vs_white_luminance() {
        let black = analyze(&solid(8, 8, [0, 0, 0]), 8, 8);
        let white = analyze(&solid(8, 8, [255, 255, 255]), 8, 8);
        assert!(black.luminance < 0.01);
        assert!(white.luminance > 0.99);
    }

    #[test]
    fn saturated_red_is_more_colorful_than_gray() {
        let gray = analyze(&solid(8, 8, [128, 128, 128]), 8, 8);
        let red = analyze(&solid(8, 8, [255, 0, 0]), 8, 8);
        assert!(red.colorfulness > gray.colorfulness);
        assert!(red.colorfulness > 0.1);
    }

    #[test]
    fn banded_image_has_high_edge_density() {
        // 4-pixel horizontal bands alternating black/white. Unlike a 1-pixel
        // checkerboard (a Sobel null at Nyquist), coarse bands produce strong
        // gradients at each band boundary.
        let (w, h) = (16, 16);
        let mut v = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for _x in 0..w {
                let c = if (y / 4) % 2 == 0 { 255 } else { 0 };
                v.extend_from_slice(&[c, c, c, 255]);
            }
        }
        let m = analyze(&v, w, h);
        let flat = analyze(&solid(w, h, [128, 128, 128]), w, h);
        assert!(m.edge_density > 0.2, "edge density {}", m.edge_density);
        assert!(m.edge_density > flat.edge_density);
        assert!(m.contrast > 0.3, "contrast {}", m.contrast);
    }

    #[test]
    fn describe_is_nonempty_and_labeled() {
        let m = analyze(&solid(8, 8, [200, 40, 40]), 8, 8);
        let d = m.describe();
        assert!(d.contains("luminance"));
        assert!(d.contains("colorfulness"));
        // No unresolved label slots.
        assert!(!d.contains("()"), "empty label bucket in: {d}");
    }
}
