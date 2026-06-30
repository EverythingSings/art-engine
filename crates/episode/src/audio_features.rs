//! Per-frame audio features loaded from the JSON written by
//! `examined-machine/scripts/extract_features.py`.

use serde::Deserialize;
use std::path::Path;

use crate::error::FeatureError;

/// Audio features sampled once per output frame.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct FeatureTrack {
    pub fps: u32,
    #[serde(default)]
    pub duration: f32,
    #[serde(default)]
    pub n_frames: u32,
    pub rms: Vec<f32>,
    pub onset: Vec<f32>,
    pub centroid: Vec<f32>,
}

impl FeatureTrack {
    /// Load + sanity-check a features.json file.
    pub fn load(path: &Path) -> Result<Self, FeatureError> {
        let s = std::fs::read_to_string(path).map_err(|source| FeatureError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let track: Self = serde_json::from_str(&s)?;
        if track.rms.len() != track.onset.len() || track.rms.len() != track.centroid.len() {
            return Err(FeatureError::LengthMismatch {
                rms: track.rms.len(),
                onset: track.onset.len(),
                centroid: track.centroid.len(),
            });
        }
        Ok(track)
    }

    /// Returns `(rms, onset, centroid)` at the given frame index, clamped
    /// to the available range. Index-out-of-bounds isn't an error — the
    /// last valid sample is returned, which keeps the renderer simple
    /// when the storyboard's duration nudges slightly past the audio.
    pub fn at_frame(&self, idx: usize) -> (f32, f32, f32) {
        let n = self.rms.len();
        if n == 0 {
            return (0.0, 0.0, 0.5);
        }
        let i = idx.min(n - 1);
        (self.rms[i], self.onset[i], self.centroid[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_frame_clamps_out_of_range() {
        let t = FeatureTrack {
            fps: 30,
            duration: 0.1,
            n_frames: 3,
            rms: vec![0.1, 0.2, 0.3],
            onset: vec![0.0, 0.5, 1.0],
            centroid: vec![0.4, 0.5, 0.6],
        };
        assert_eq!(t.at_frame(0), (0.1, 0.0, 0.4));
        assert_eq!(t.at_frame(2), (0.3, 1.0, 0.6));
        // Out of range clamps to last sample.
        assert_eq!(t.at_frame(99), (0.3, 1.0, 0.6));
    }

    #[test]
    fn at_frame_returns_safe_default_when_empty() {
        let t = FeatureTrack {
            fps: 30,
            duration: 0.0,
            n_frames: 0,
            rms: vec![],
            onset: vec![],
            centroid: vec![],
        };
        assert_eq!(t.at_frame(0), (0.0, 0.0, 0.5));
    }
}
