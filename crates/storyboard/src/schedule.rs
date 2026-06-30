//! Frame-by-frame timeline resolution.
//!
//! Maps a frame index (or absolute time) to the active [`Scene`] from a
//! [`Storyboard`]. The renderer in `art-engine-episode` calls these
//! helpers per frame; everything here is pure data, no GL.

use crate::{Scene, Storyboard};

/// Returns the index of the scene active at time `t`, or `None` if `t`
/// falls in a gap between scenes (or before/after the timeline).
///
/// "Active" means `start <= t < end`. Scenes are assumed validated —
/// non-overlapping, ordered. Linear scan is fine for the small scene
/// counts a Short produces (<= 60).
pub fn scene_at(sb: &Storyboard, t: f32) -> Option<usize> {
    sb.scenes
        .iter()
        .position(|sc| t >= sc.start && t < sc.end)
}

/// Returns the active scene at `t`, or `None`.
pub fn scene<'a>(sb: &'a Storyboard, t: f32) -> Option<&'a Scene> {
    scene_at(sb, t).map(|i| &sb.scenes[i])
}

/// Returns the time-in-scene `t - scene.start` for the active scene at
/// time `t`, or `None`.
pub fn time_in_scene(sb: &Storyboard, t: f32) -> Option<f32> {
    scene(sb, t).map(|sc| t - sc.start)
}

/// Number of frames a storyboard renders to at its configured fps.
pub fn frame_count(sb: &Storyboard) -> u32 {
    (sb.duration() * sb.fps as f32).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    fn sb_two_scenes() -> Storyboard {
        Storyboard {
            audio: "x".into(),
            fps: 30,
            width: 1080,
            height: 1920,
            subtitles: None,
            header: None,
            sigil: None,
            scene_pips: None,
            scenes: vec![
                Scene {
                    start: 0.0,
                    end: 5.0,
                    backdrop: Backdrop::Solid { color: [0.0; 3] },
                    foreground: vec![],
                    transition_in: Transition::HardCut,
                    post: PostChain::default(),
                },
                Scene {
                    start: 5.0,
                    end: 10.0,
                    backdrop: Backdrop::Solid { color: [1.0; 3] },
                    foreground: vec![],
                    transition_in: Transition::HardCut,
                    post: PostChain::default(),
                },
            ],
        }
    }

    #[test]
    fn scene_at_picks_first_scene_at_t_zero() {
        assert_eq!(scene_at(&sb_two_scenes(), 0.0), Some(0));
    }

    #[test]
    fn scene_at_picks_second_scene_at_boundary_inclusive_left() {
        assert_eq!(scene_at(&sb_two_scenes(), 5.0), Some(1));
    }

    #[test]
    fn scene_at_returns_none_past_end() {
        assert_eq!(scene_at(&sb_two_scenes(), 10.0), None);
        assert_eq!(scene_at(&sb_two_scenes(), 99.0), None);
    }

    #[test]
    fn scene_at_picks_correctly_in_middle_of_each_scene() {
        let sb = sb_two_scenes();
        assert_eq!(scene_at(&sb, 2.5), Some(0));
        assert_eq!(scene_at(&sb, 7.5), Some(1));
    }

    #[test]
    fn time_in_scene_resets_at_each_boundary() {
        let sb = sb_two_scenes();
        assert!((time_in_scene(&sb, 0.0).unwrap() - 0.0).abs() < 1e-6);
        assert!((time_in_scene(&sb, 4.5).unwrap() - 4.5).abs() < 1e-6);
        assert!((time_in_scene(&sb, 5.0).unwrap() - 0.0).abs() < 1e-6);
        assert!((time_in_scene(&sb, 7.5).unwrap() - 2.5).abs() < 1e-6);
    }

    #[test]
    fn frame_count_matches_duration_times_fps() {
        // 10 seconds @ 30fps = 300 frames.
        assert_eq!(frame_count(&sb_two_scenes()), 300);
    }
}
