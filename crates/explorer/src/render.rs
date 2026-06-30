//! Off-thread GPU rendering.
//!
//! eframe owns a GL context on the main thread; our [`GpuSession`] owns a
//! separate EGL context. Two GL contexts on one thread fight over "current",
//! so all art-engine rendering happens on a dedicated thread that owns the
//! session exclusively. The main (UI) thread talks to it over channels:
//! send a [`RenderRequest`], receive a [`RenderResult`] with the RGBA8 frames.
//!
//! # Animation
//!
//! Each genome is rendered as a sequence of frames. To keep this affordable
//! and reproducible, the engine field is *not* re-stepped per render: at
//! startup the thread warms the engine to a fixed step and captures
//! [`ANIM_FRAMES`] field snapshots at a fixed stride. Every genome animates
//! against those same snapshots plus a `u_time` sweep, so motion is identical
//! across tiles and across timeline navigation, and the (expensive) engine
//! stepping happens exactly once.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

use art_engine_core::engine::Engine;
use art_engine_core::field::Field;
use art_engine_core::palette::Palette;
use art_engine_engines::gpu_snapshot::GpuSession;
use art_engine_engines::EngineKind;

use crate::genome::Genome;

/// Frames captured for an animation. Also the upper bound on frames per
/// request; a static request asks for 1.
pub const ANIM_FRAMES: usize = 8;
/// Total `u_time` swept across the full frame sequence.
const TIME_SPAN: f32 = 6.0;
/// Max distinct (engine, seed) field sequences kept in the cache. A reroll
/// touches ≤ 10 keys, so this never thrashes mid-reroll.
const MAX_CACHE: usize = 16;

/// Per-engine evolution schedule: (warmup steps, steps between frames).
/// Different systems develop and move on different timescales — excitable
/// media needs far more steps per frame to show wave motion than gray-scott.
fn engine_schedule(engine: &str) -> (usize, usize) {
    match engine {
        "gray-scott" => (400, 8),
        "physarum" => (200, 6),
        "dla" => (600, 20),
        "differential" => (300, 10),
        "excitable" => (1500, 60),
        _ => (400, 12),
    }
}

/// Builds the [`ANIM_FRAMES`] field snapshots for one (engine, seed): warm the
/// engine, then capture a field every `stride` steps. Deterministic, so a
/// genome's animation is reproducible across tiles and timeline navigation.
fn build_fields(engine: &str, seed: u64, res: u32) -> Result<Vec<Field>, String> {
    let (warmup, stride) = engine_schedule(engine);
    let mut eng = EngineKind::from_name(engine, res as usize, res as usize, seed, &serde_json::json!({}))
        .map_err(|e| e.to_string())?;
    for _ in 0..warmup {
        eng.step().map_err(|e| e.to_string())?;
    }
    let mut fields = Vec::with_capacity(ANIM_FRAMES);
    for i in 0..ANIM_FRAMES {
        fields.push(eng.field().clone());
        if i + 1 < ANIM_FRAMES {
            for _ in 0..stride {
                eng.step().map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(fields)
}

/// Identifies which slot a render is for, so the UI can route the result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Focus,
    Grid(usize),
}

/// A request to render one genome into one slot as `frames` animation frames
/// (1 = a still).
pub struct RenderRequest {
    pub slot: Slot,
    pub genome: Genome,
    pub frames: usize,
}

/// The rendered frames (RGBA8, one Vec per frame) for one slot, or an error.
pub struct RenderResult {
    pub slot: Slot,
    pub frames: Result<Vec<Vec<u8>>, String>,
}

/// Handle to the render thread: push requests, drain results.
pub struct RenderThread {
    tx: Sender<RenderRequest>,
    rx: Receiver<RenderResult>,
}

impl RenderThread {
    /// Spawns the render thread. `res` is the square render resolution shared
    /// by every tile (the UI scales the texture for display). The field
    /// snapshots are computed once inside the thread.
    pub fn spawn(res: u32) -> Self {
        let (req_tx, req_rx) = std::sync::mpsc::channel::<RenderRequest>();
        let (res_tx, res_rx) = std::sync::mpsc::channel::<RenderResult>();

        thread::spawn(move || {
            let mut renderer = Renderer::new(res);
            while let Ok(req) = req_rx.recv() {
                let frames = match &mut renderer {
                    Ok(r) => r.render(&req.genome, req.frames),
                    Err(e) => Err(e.clone()),
                };
                if res_tx
                    .send(RenderResult {
                        slot: req.slot,
                        frames,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        Self {
            tx: req_tx,
            rx: res_rx,
        }
    }

    /// Queues a genome for rendering into the given slot as `frames` frames.
    pub fn request(&self, slot: Slot, genome: Genome, frames: usize) {
        let _ = self.tx.send(RenderRequest {
            slot,
            genome,
            frames,
        });
    }

    /// Returns any results that have arrived since the last call, without
    /// blocking.
    pub fn drain(&self) -> Vec<RenderResult> {
        self.rx.try_iter().collect()
    }
}

/// Owns the GPU session and a cache of field sequences keyed by (engine,
/// seed). Lives entirely on the render thread. Tiles that share a genome's
/// engine+seed (the common case — most mutations only touch shaders/palette)
/// reuse the same evolved fields, so engine stepping happens once per system.
struct Renderer {
    session: GpuSession,
    cache: HashMap<(String, u64), Vec<Field>>,
    /// `u_time` value per frame.
    times: Vec<f32>,
    res: u32,
}

impl Renderer {
    fn new(res: u32) -> Result<Self, String> {
        let times = (0..ANIM_FRAMES)
            .map(|i| i as f32 / ANIM_FRAMES as f32 * TIME_SPAN)
            .collect();
        let session = GpuSession::new(res, res).map_err(|e| e.to_string())?;
        Ok(Self {
            session,
            cache: HashMap::new(),
            times,
            res,
        })
    }

    fn render(&mut self, genome: &Genome, frames: usize) -> Result<Vec<Vec<u8>>, String> {
        let key = (genome.engine.clone(), genome.seed);
        if !self.cache.contains_key(&key) {
            let fields = build_fields(&genome.engine, genome.seed, self.res)?;
            if self.cache.len() >= MAX_CACHE {
                self.cache.clear();
            }
            self.cache.insert(key.clone(), fields);
        }

        let pal = Palette::from_name(&genome.palette).map_err(|e| e.to_string())?;
        self.session
            .rebake_palette(&pal)
            .map_err(|e| e.to_string())?;
        let canvas = genome
            .to_canvas(self.res as usize, self.res as usize)
            .map_err(|e| e.to_string())?;

        let fields = &self.cache[&key];
        let n = frames.clamp(1, ANIM_FRAMES);
        let mut out = Vec::with_capacity(n);
        for (field, &time) in fields.iter().zip(&self.times).take(n) {
            out.push(
                self.session
                    .render_to_rgba8_at(&canvas, field, time)
                    .map_err(|e| e.to_string())?,
            );
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{EffectSpec, LayerSpec};
    use art_engine_core::canvas::BlendMode;
    use art_engine_core::color::Srgb;
    use std::time::{Duration, Instant};

    fn looks_like_no_gl(err: &str) -> bool {
        err.contains("headless GL") || err.contains("EGL") || err.contains("libEGL")
    }

    fn green_solid() -> Genome {
        Genome {
            engine: "gray-scott".to_string(),
            seed: 1,
            palette: "ocean".to_string(),
            background: Srgb {
                r: 0.0,
                g: 0.0,
                b: 0.0,
            },
            layers: vec![LayerSpec {
                effects: vec![EffectSpec {
                    shader: "solid".to_string(),
                    params: serde_json::json!({"u_color": [0.0, 1.0, 0.0]}),
                }],
                blend: BlendMode::Normal,
                opacity: 1.0,
            }],
            post: vec![],
        }
    }

    /// Blocks (with timeout) for the first result, skipping if no GL.
    fn await_result(thread: &RenderThread) -> Option<RenderResult> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(r) = thread.drain().into_iter().next() {
                return Some(r);
            }
            if Instant::now() > deadline {
                panic!("render thread produced no result within timeout");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn render_thread_produces_a_still_frame() {
        let res = 32u32;
        let thread = RenderThread::spawn(res);
        thread.request(Slot::Focus, green_solid(), 1);
        let result = await_result(&thread).unwrap();
        match result.frames {
            Ok(frames) => {
                assert_eq!(frames.len(), 1, "one frame requested");
                let bytes = &frames[0];
                assert_eq!(bytes.len(), (res * res * 4) as usize);
                let center = ((res / 2) * res + (res / 2)) as usize * 4;
                assert!(bytes[center + 1] > 200, "G should be ~255");
                assert!(bytes[center] < 40 && bytes[center + 2] < 40);
            }
            Err(e) if looks_like_no_gl(&e) && std::env::var("ART_ENGINE_REQUIRE_GL").is_err() => {
                eprintln!("skipping: {e}");
            }
            Err(e) => panic!("render failed: {e}"),
        }
    }

    #[test]
    fn render_thread_produces_animation_frames() {
        let res = 32u32;
        let thread = RenderThread::spawn(res);
        thread.request(Slot::Focus, green_solid(), ANIM_FRAMES);
        let result = await_result(&thread).unwrap();
        match result.frames {
            Ok(frames) => {
                assert_eq!(frames.len(), ANIM_FRAMES, "all frames requested");
                for f in &frames {
                    assert_eq!(f.len(), (res * res * 4) as usize);
                }
            }
            Err(e) if looks_like_no_gl(&e) && std::env::var("ART_ENGINE_REQUIRE_GL").is_err() => {
                eprintln!("skipping: {e}");
            }
            Err(e) => panic!("render failed: {e}"),
        }
    }
}
