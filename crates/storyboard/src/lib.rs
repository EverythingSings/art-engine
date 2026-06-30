#![deny(unsafe_code)]
//! Scene-composition primitives and the `.ron` storyboard format for
//! turning a transcribed audio episode into a YouTube-Shorts-shaped video.
//!
//! A [`Storyboard`] is the per-episode authored artifact: an ordered list
//! of [`Scene`]s, each pinning a [`Backdrop`] (a generative shader) and
//! a list of [`Foreground`] overlays (typography, sigil, captions). The
//! renderer walks the timeline at the configured `fps` and produces a
//! frame stream that the `art-engine-episode` binary muxes with the
//! original audio via ffmpeg.
//!
//! This crate intentionally has *no* GPU dependency. Parsing, validation,
//! and timeline resolution are pure data — the render half lives in
//! `art-engine-episode` so this crate stays cheap to compile and test.

pub mod design;
pub mod schedule;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// One episode's complete visual plan.
///
/// A `Storyboard` references the audio file (so the renderer can mux it
/// in unchanged), the target frame rate, and the ordered list of scenes.
/// Adjacent scenes' `[start, end)` intervals must not overlap and should
/// cover the audio duration without gaps; the renderer fills any gap
/// with a black frame and warns.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Storyboard {
    /// Path to the audio file, relative to the storyboard or absolute.
    pub audio: PathBuf,
    /// Output frame rate. Default 30 for YouTube Shorts.
    #[serde(default = "default_fps")]
    pub fps: u32,
    /// Vertical-aspect output width (default 1080).
    #[serde(default = "default_width")]
    pub width: u32,
    /// Vertical-aspect output height (default 1920).
    #[serde(default = "default_height")]
    pub height: u32,
    /// Optional subtitle (.ass) file to burn in at composite time.
    /// When present, the renderer instructs ffmpeg to apply its
    /// `subtitles=` filter; when absent, no captions are rendered.
    #[serde(default)]
    pub subtitles: Option<PathBuf>,
    /// Optional static header that sits at the top of every frame for
    /// the full duration. Serves as a "what is this video about" hook.
    #[serde(default)]
    pub header: Option<HeaderSpec>,
    /// Optional persistent channel-handle watermark in a corner.
    #[serde(default)]
    pub sigil: Option<SigilSpec>,
    /// Optional scope-indicator strip showing all scenes as small pills
    /// at the top (or bottom) of the frame. Past scenes filled, current
    /// scene highlighted in the accent color, future scenes dim. Signals
    /// to a feed viewer that there's more variety coming, which keeps
    /// them watching past the first scene.
    #[serde(default)]
    pub scene_pips: Option<ScenePipsSpec>,
    /// Ordered list of scenes that compose the episode.
    pub scenes: Vec<Scene>,
}

/// Configuration for the scene-pips scope indicator strip.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScenePipsSpec {
    #[serde(default = "default_pip_position")]
    pub position: PipPosition,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum PipPosition {
    /// Strip at the top edge of the frame (above the header).
    #[default]
    Top,
    /// Strip at the bottom edge, just above the karaoke band.
    Bottom,
}

fn default_pip_position() -> PipPosition { PipPosition::Top }

/// Static header text shown at the top of every frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HeaderSpec {
    /// The text to display.
    pub text: String,
    /// Optional kicker line above the main text (smaller).
    #[serde(default)]
    pub kicker: Option<String>,
}

/// Persistent channel watermark.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SigilSpec {
    pub handle: String,
    #[serde(default = "default_sigil_corner")]
    pub corner: Corner,
    #[serde(default = "default_sigil_opacity")]
    pub opacity: f32,
}

fn default_fps() -> u32 { 30 }
fn default_width() -> u32 { 1080 }
fn default_height() -> u32 { 1920 }

/// One contiguous slice of the timeline with a single backdrop and any
/// number of foreground overlays.
///
/// `start` and `end` are absolute times in seconds. The active scene at
/// time `t` is the unique scene with `start <= t < end`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Scene {
    pub start: f32,
    pub end: f32,
    pub backdrop: Backdrop,
    #[serde(default)]
    pub foreground: Vec<Foreground>,
    #[serde(default)]
    pub transition_in: Transition,
    #[serde(default)]
    pub post: PostChain,
}

/// The full-screen generative layer for a scene.
///
/// Each variant maps to a fragment shader; the `params` are the JSON-
/// shaped uniform overrides the existing `art-engine-core` shader
/// uniform schema understands.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Backdrop {
    /// Smooth flowing field — contemplative, time, organic. Ports the
    /// Python `render_flow` from the examined-machine prototype.
    Flow {
        #[serde(default = "default_palette")]
        palette: PaletteRef,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_seed")]
        seed: u32,
    },
    /// A solid color — useful for title cards and as a transition pad.
    Solid { color: [f32; 3] },
    /// Animated cellular tessellation. Snappy on transients; evokes
    /// decomposition / parts-of-a-whole moments.
    Voronoi {
        #[serde(default = "default_voronoi_scale")]
        scale: f32,
        #[serde(default = "default_voronoi_edge")]
        edge_width: f32,
        #[serde(default = "default_voronoi_jitter")]
        jitter: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// TV-static / breakdown noise. Evokes opacity, signal-loss,
    /// "we don't know how this works" moments.
    NoiseStatic {
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_unit")]
        density: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Mechanical orthogonal grid. For classical-machine, geometry,
    /// "transparent / discrete-parts" moments.
    Lattice {
        #[serde(default = "default_lattice_density")]
        density: f32,
        #[serde(default = "default_lattice_thickness")]
        thickness: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// N-fold radial symmetry. For pattern-as-system, capability
    /// chasing capability, structured emergence.
    Mandala {
        #[serde(default = "default_mandala_segments")]
        segments: f32,
        #[serde(default = "default_mandala_freq")]
        freq: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Outward-radiating concentric rings. For reflective pauses,
    /// held questions, resonance.
    Concentric {
        #[serde(default = "default_concentric_freq")]
        freq: f32,
        #[serde(default = "default_unit")]
        speed: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Vertical glowing filaments. For circuits, threads, connections,
    /// "things running quietly in the background".
    Strands {
        #[serde(default = "default_strands_density")]
        density: f32,
        #[serde(default = "default_strands_thickness")]
        thickness: f32,
        #[serde(default = "default_strands_jitter")]
        jitter: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Horizontal sinusoidal bands. For signal, waveform, broadcast,
    /// steady rhythm.
    Wave {
        #[serde(default = "default_wave_density")]
        density: f32,
        #[serde(default = "default_wave_freq")]
        freq: f32,
        #[serde(default = "default_wave_amp")]
        amplitude: f32,
        #[serde(default = "default_unit")]
        speed: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Logarithmic spiral from center. For recursion, depth, "going
    /// down the rabbit hole", self-reference.
    Spiral {
        #[serde(default = "default_spiral_arms")]
        arms: f32,
        #[serde(default = "default_unit")]
        tightness: f32,
        #[serde(default = "default_unit")]
        speed: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Newsprint halftone dot pattern. For "this has been published /
    /// reproduced / transmitted" moments.
    Halftone {
        #[serde(default = "default_halftone_cell")]
        cell: f32,
        #[serde(default = "default_unit")]
        strength: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Draftsman crosshatch — two diagonal line families. For blueprint,
    /// schematic, "drawing it out by hand" moments.
    Crosshatch {
        #[serde(default = "default_crosshatch_spacing")]
        spacing: f32,
        #[serde(default = "default_crosshatch_thickness")]
        thickness: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Topographic isolines. For data-map / bird's-eye-view moments.
    Topo {
        #[serde(default = "default_topo_scale")]
        scale: f32,
        #[serde(default = "default_topo_density")]
        density: f32,
        #[serde(default = "default_topo_thickness")]
        thickness: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Aurora — vertical curtains of light. For atmospheric, awe,
    /// "something larger than us" moments.
    Aurora {
        #[serde(default = "default_aurora_curtains")]
        curtains: f32,
        #[serde(default = "default_unit")]
        speed: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Sun — singular luminous focal disc. For revelation / answer /
    /// "the source" moments.
    Sun {
        #[serde(default = "default_sun_radius")]
        radius: f32,
        #[serde(default = "default_sun_rays")]
        rays: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Particles — N orbiting points of light. For swarm /
    /// distributed-agents / plurality moments.
    Particles {
        #[serde(default = "default_particles_count")]
        count: f32,
        #[serde(default = "default_particles_glow")]
        glow: f32,
        #[serde(default = "default_unit")]
        speed: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Branch — fractal tree silhouette. For dendritic / growth /
    /// decision-tree / organic-complexity moments.
    Branch {
        #[serde(default = "default_branch_branches")]
        branches: f32,
        #[serde(default = "default_branch_thickness")]
        thickness: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Caustics — light dapples through wavy water. For perception-
    /// through-a-medium / surface-vs-depth / "what reaches you isn't
    /// the source" moments.
    Caustics {
        #[serde(default = "default_caustics_scale")]
        scale: f32,
        #[serde(default = "default_caustics_sharpness")]
        sharpness: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Phyllotaxis — sunflower seed packing on the golden angle. For
    /// natural-mathematical-order / order-without-designer /
    /// "emergence from a one-line rule" moments.
    Phyllotaxis {
        #[serde(default = "default_phyllotaxis_count")]
        count: f32,
        #[serde(default = "default_phyllotaxis_radius_scale")]
        radius_scale: f32,
        #[serde(default = "default_phyllotaxis_seed_radius")]
        seed_radius: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Constellation — nodes connected by faint edges. For relations
    /// between specifics, mapping, "this leads to that", graph-shaped
    /// explanation moments.
    Constellation {
        #[serde(default = "default_constellation_node_glow")]
        node_glow: f32,
        #[serde(default = "default_constellation_edge_glow")]
        edge_glow: f32,
        #[serde(default = "default_constellation_edge_strength")]
        edge_strength: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// VectorField — streamlines + dashes of an unseen flow. For
    /// invisible-forces / influence / "what's shaping what you see"
    /// moments (gravity, momentum, tendency).
    VectorField {
        #[serde(default = "default_vector_field_scale")]
        scale: f32,
        #[serde(default = "default_vector_field_freq")]
        freq: f32,
        #[serde(default = "default_vector_field_density")]
        density: f32,
        #[serde(default = "default_vector_field_thickness")]
        thickness: f32,
        #[serde(default = "default_vector_field_dash_speed")]
        dash_speed: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Crystal — hard-faceted polygonal cells with quantised tone. For
    /// clarity-crystallising / a-frame-snapping-into-place moments.
    Crystal {
        #[serde(default = "default_crystal_scale")]
        scale: f32,
        #[serde(default = "default_crystal_levels")]
        levels: f32,
        #[serde(default = "default_crystal_edge_width")]
        edge_width: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Smoke — soft drifting volumetric haze. For obscurity /
    /// what-hides-between-you-and-the-thing / held-question moments.
    Smoke {
        #[serde(default = "default_smoke_scale")]
        scale: f32,
        #[serde(default = "default_smoke_warp")]
        warp: f32,
        #[serde(default = "default_unit")]
        speed: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Moire — interference between two near-identical line patterns.
    /// For two-systems-colliding / friction-at-a-seam / "almost-the-
    /// same-but-not" moments.
    Moire {
        #[serde(default = "default_moire_freq")]
        freq: f32,
        #[serde(default = "default_moire_angle_delta")]
        angle_delta: f32,
        #[serde(default = "default_moire_thickness")]
        thickness: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Ripple — disturbances propagating from localised origins. For
    /// cause-then-consequence / "a-decision-rippling-outward" /
    /// originating-event moments.
    Ripple {
        #[serde(default = "default_ripple_freq")]
        freq: f32,
        #[serde(default = "default_ripple_speed")]
        speed: f32,
        #[serde(default = "default_ripple_decay")]
        decay: f32,
        #[serde(default = "default_ripple_sharpness")]
        sharpness: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Plasma — fluid blobs that merge and separate. For energy-state /
    /// transformation / "two-flows-becoming-one" moments.
    Plasma {
        #[serde(default = "default_plasma_count")]
        count: f32,
        #[serde(default = "default_plasma_radius")]
        radius: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Bokeh — soft out-of-focus circles at varied depths. For
    /// attention / focal-depth / "what's foregrounded vs blurred"
    /// moments.
    Bokeh {
        #[serde(default = "default_bokeh_count")]
        count: f32,
        #[serde(default = "default_bokeh_radius")]
        radius: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
    /// Mosaic — quantised regular tile grid with discrete tone per
    /// tile. For finite-resolution-representation / sketch /
    /// "this is just the map at this scale" moments.
    Mosaic {
        #[serde(default = "default_mosaic_grid")]
        grid: f32,
        #[serde(default = "default_mosaic_levels")]
        levels: f32,
        #[serde(default = "default_mosaic_gap")]
        gap: f32,
        #[serde(default = "default_unit")]
        intensity: f32,
        #[serde(default = "default_palette")]
        palette: PaletteRef,
    },
}

fn default_branch_branches() -> f32 { 4.0 }
fn default_branch_thickness() -> f32 { 0.012 }

fn default_caustics_scale() -> f32 { 3.0 }
fn default_caustics_sharpness() -> f32 { 7.0 }

fn default_phyllotaxis_count() -> f32 { 140.0 }
fn default_phyllotaxis_radius_scale() -> f32 { 0.030 }
fn default_phyllotaxis_seed_radius() -> f32 { 90.0 }

fn default_constellation_node_glow() -> f32 { 240.0 }
fn default_constellation_edge_glow() -> f32 { 620.0 }
fn default_constellation_edge_strength() -> f32 { 0.55 }

fn default_vector_field_scale() -> f32 { 2.5 }
fn default_vector_field_freq() -> f32 { 1.3 }
fn default_vector_field_density() -> f32 { 6.0 }
fn default_vector_field_thickness() -> f32 { 0.06 }
fn default_vector_field_dash_speed() -> f32 { 4.0 }

fn default_crystal_scale() -> f32 { 7.0 }
fn default_crystal_levels() -> f32 { 5.0 }
fn default_crystal_edge_width() -> f32 { 0.03 }

fn default_smoke_scale() -> f32 { 2.2 }
fn default_smoke_warp() -> f32 { 0.7 }

fn default_moire_freq() -> f32 { 80.0 }
fn default_moire_angle_delta() -> f32 { 0.18 }
fn default_moire_thickness() -> f32 { 0.35 }

fn default_ripple_freq() -> f32 { 18.0 }
fn default_ripple_speed() -> f32 { 1.2 }
fn default_ripple_decay() -> f32 { 2.0 }
fn default_ripple_sharpness() -> f32 { 3.0 }

fn default_plasma_count() -> f32 { 6.0 }
fn default_plasma_radius() -> f32 { 0.20 }

fn default_bokeh_count() -> f32 { 9.0 }
fn default_bokeh_radius() -> f32 { 0.18 }

fn default_mosaic_grid() -> f32 { 14.0 }
fn default_mosaic_levels() -> f32 { 5.0 }
fn default_mosaic_gap() -> f32 { 0.06 }

fn default_sun_radius() -> f32 { 0.18 }
fn default_sun_rays() -> f32 { 24.0 }
fn default_particles_count() -> f32 { 16.0 }
fn default_particles_glow() -> f32 { 0.025 }

fn default_topo_scale() -> f32 { 3.0 }
fn default_topo_density() -> f32 { 8.0 }
fn default_topo_thickness() -> f32 { 0.04 }
fn default_aurora_curtains() -> f32 { 3.0 }

fn default_halftone_cell() -> f32 { 22.0 }
fn default_crosshatch_spacing() -> f32 { 14.0 }
fn default_crosshatch_thickness() -> f32 { 1.3 }

fn default_strands_density() -> f32 { 48.0 }
fn default_strands_thickness() -> f32 { 0.18 }
fn default_strands_jitter() -> f32 { 0.6 }
fn default_wave_density() -> f32 { 6.0 }
fn default_wave_freq() -> f32 { 1.5 }
fn default_wave_amp() -> f32 { 0.4 }
fn default_spiral_arms() -> f32 { 3.0 }

fn default_lattice_density() -> f32 { 12.0 }
fn default_lattice_thickness() -> f32 { 0.06 }
fn default_mandala_segments() -> f32 { 8.0 }
fn default_mandala_freq() -> f32 { 12.0 }
fn default_concentric_freq() -> f32 { 18.0 }

fn default_voronoi_scale() -> f32 { 8.0 }
fn default_voronoi_edge() -> f32 { 0.05 }
fn default_voronoi_jitter() -> f32 { 1.0 }

fn default_palette() -> PaletteRef { PaletteRef::TealAmber }
fn default_unit() -> f32 { 1.0 }
fn default_seed() -> u32 { 11 }

/// A named palette from the design system, or an inline custom one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PaletteRef {
    /// The show's default: deep indigo → dusty teal → warm amber.
    TealAmber,
    /// Inline three-stop palette in linear sRGB.
    Custom([[f32; 3]; 3]),
}

/// An overlay drawn on top of the backdrop for a particular scene.
///
/// Video-wide overlays (channel sigil, top header) live as top-level
/// fields on the `Storyboard` so they don't have to be repeated on
/// every scene. Phase B implements `KaraokeCaptions`. Other variants
/// are reserved so storyboards can already reference them; the
/// renderer no-ops on unimplemented variants.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Foreground {
    /// Render the storyboard's `subtitles` ASS file as burn-in captions
    /// for the duration of this scene. Today this is a passthrough flag
    /// the renderer reads; later it can take per-scene styling overrides.
    KaraokeCaptions,
    /// A reserved title card primitive (Phase B).
    TitleCard {
        text: String,
        #[serde(default)]
        kicker: String,
        #[serde(default)]
        at: f32,
        #[serde(default = "default_card_dur")]
        dur: f32,
    },
    /// A reserved pull-quote primitive (Phase B).
    PullQuote {
        text: String,
        #[serde(default)]
        emphasis: Vec<usize>,
        #[serde(default)]
        at: f32,
        #[serde(default = "default_card_dur")]
        dur: f32,
    },
    /// Reserved end-card / outro primitive (Phase B).
    EndCard { handle: String, cta: String },
    /// Diagram arrow — drawn line + arrowhead with optional label.
    ///
    /// `from`/`to` coordinates are normalised to `[0.0, 1.0]` of the
    /// frame, so a storyboard doesn't care about resolution: `from_y =
    /// 0.5` always reads as "the middle of the frame." Use this to
    /// literally point at things while the speaker names them — the
    /// arrow is the first diagram primitive, building block for
    /// Decomposition / Annotation later.
    ///
    /// Style is fixed to the show's chrome accent (hot orange) so
    /// every episode's arrows read as part of the same family. If you
    /// need a visually-distinct arrow per scene, add an arrow style
    /// parameter later — not in v1.
    Arrow {
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
        #[serde(default)]
        label: String,
        #[serde(default)]
        at: f32,
        #[serde(default = "default_card_dur")]
        dur: f32,
    },
    /// Comparison — two labelled sides separated by a divider.
    /// Visual shorthand for "X vs Y" / "before → after" / "transparent
    /// | opaque" beats.
    ///
    /// Three text events centred at `(center_x, center_y)`: left
    /// label at `cx - gap`, divider at `cx`, right label at `cx + gap`.
    /// All three baselines align. `gap` is normalised to frame width.
    /// `divider` is whatever string fits the contrast — `"|"`, `"vs"`,
    /// `"→"`, `"≠"` — picked by the author per beat.
    Comparison {
        left: String,
        right: String,
        #[serde(default = "default_comparison_divider")]
        divider: String,
        center_x: f32,
        center_y: f32,
        #[serde(default = "default_comparison_gap")]
        gap: f32,
        #[serde(default)]
        at: f32,
        #[serde(default = "default_card_dur")]
        dur: f32,
    },
    /// Highlight — corner-bracket frame around a rectangular region.
    /// Visual shorthand for "this whole area is what I'm talking about."
    ///
    /// Renders as four L-shaped brackets at the rect's corners (not a
    /// closed outline) so the framed content stays visible inside.
    /// Distinct from the always-on chrome corner brackets — Highlight's
    /// brackets are larger and only present during the active window.
    ///
    /// Coordinates of `center` are normalised `[0, 1]`; `width` and
    /// `height` are normalised to the same axes (so width=0.4 means
    /// 40% of the frame's width). Optional label sits centred above
    /// the rect's top edge.
    Highlight {
        center_x: f32,
        center_y: f32,
        width: f32,
        height: f32,
        #[serde(default)]
        label: String,
        #[serde(default)]
        at: f32,
        #[serde(default = "default_card_dur")]
        dur: f32,
    },
    /// Annotation — labelled callout that points at a specific point
    /// in the frame via a thin leader line. Visual shorthand for
    /// "this point is called X."
    ///
    /// Distinct from `Arrow`: no arrowhead, a small dot at the target
    /// instead. The dot signals "naming a point" whereas an arrow's
    /// head signals "going from A to B." Both render via ASS drawing
    /// commands.
    ///
    /// Coordinates of both `target` and `label` are normalised `[0, 1]`.
    /// `label_x`/`label_y` is where the text sits; `target_x`/`target_y`
    /// is what the leader line points at. The renderer trims the line
    /// so it stops short of the label box (so it doesn't run through
    /// the text) and short of the target dot (so the dot stays clean).
    Annotation {
        label: String,
        target_x: f32,
        target_y: f32,
        label_x: f32,
        label_y: f32,
        #[serde(default)]
        at: f32,
        #[serde(default = "default_card_dur")]
        dur: f32,
    },
    /// Decomposition — a whole label surrounded by N part labels,
    /// each connected to the whole by a thin spoke. The visual
    /// shorthand for "X has these N pieces."
    ///
    /// Layout is radial: parts are arranged evenly around a circle
    /// of `radius` (normalised to the shorter frame dimension), with
    /// the first part at the top (angle 0) and the rest stepping
    /// clockwise. For 2 parts you get top + bottom; for 3 parts a
    /// triangle starting from the top; for 4 a diamond; etc.
    ///
    /// Coordinates of the whole are normalised to `[0, 1]`. A typical
    /// composition is `center_x: 0.5, center_y: 0.45` (slightly above
    /// the karaoke band) with `radius: 0.30` for 3-5 short part names.
    Decomposition {
        whole: String,
        parts: Vec<String>,
        center_x: f32,
        center_y: f32,
        #[serde(default = "default_decomposition_radius")]
        radius: f32,
        #[serde(default)]
        at: f32,
        #[serde(default = "default_card_dur")]
        dur: f32,
    },
}

fn default_decomposition_radius() -> f32 { 0.30 }

fn default_comparison_divider() -> String { "|".to_string() }
fn default_comparison_gap() -> f32 { 0.18 }

fn default_sigil_corner() -> Corner { Corner::BottomRight }
fn default_sigil_opacity() -> f32 { 0.4 }
fn default_card_dur() -> f32 { 2.5 }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Corner { TopLeft, TopRight, BottomLeft, BottomRight }

/// Inter-scene transition. Phase A implements `HardCut` only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum Transition {
    /// Instant boundary; no blend. Default.
    #[default]
    HardCut,
    /// Linear crossfade over `dur` seconds (Phase B).
    Crossfade { dur: f32 },
    /// Glitchy break effect over `dur` seconds (Phase B).
    GlitchBreak { dur: f32, intensity: f32 },
}

/// Per-scene post-processing chain. Phase A leaves this at default;
/// downstream phases can populate grain / vignette / color-grade levels
/// that map onto the existing `BuiltinShader::{Grain, Vignette, ColorGrade}`
/// post effects.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PostChain {
    #[serde(default)]
    pub grain: Option<f32>,
    #[serde(default)]
    pub vignette: Option<f32>,
    #[serde(default)]
    pub color_grade: Option<ColorGrade>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ColorGrade {
    #[serde(default)]
    pub saturation: Option<f32>,
    #[serde(default)]
    pub gamma: Option<[f32; 3]>,
}

// ─── Errors / loading ────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum StoryboardError {
    #[error("io: {0}")]
    Io(String),
    #[error("ron: {0}")]
    Ron(String),
    #[error("validation: {0}")]
    Validation(String),
}

impl Storyboard {
    /// Loads + validates a storyboard from a `.ron` file.
    pub fn load(path: &std::path::Path) -> Result<Self, StoryboardError> {
        let s = std::fs::read_to_string(path).map_err(|e| StoryboardError::Io(e.to_string()))?;
        Self::from_ron(&s)
    }

    /// Parses + validates a storyboard from a RON string.
    pub fn from_ron(s: &str) -> Result<Self, StoryboardError> {
        let sb: Storyboard = ron::from_str(s).map_err(|e| StoryboardError::Ron(e.to_string()))?;
        sb.validate()?;
        Ok(sb)
    }

    /// Total duration covered by the scene timeline (max scene end).
    pub fn duration(&self) -> f32 {
        self.scenes.iter().map(|s| s.end).fold(0.0, f32::max)
    }

    /// Validates structural invariants:
    /// - at least one scene
    /// - every scene has `start < end`
    /// - scenes are ordered and non-overlapping
    /// - fps > 0, dimensions > 0
    pub fn validate(&self) -> Result<(), StoryboardError> {
        if self.fps == 0 {
            return Err(StoryboardError::Validation("fps must be > 0".into()));
        }
        if self.width == 0 || self.height == 0 {
            return Err(StoryboardError::Validation("width and height must be > 0".into()));
        }
        if self.scenes.is_empty() {
            return Err(StoryboardError::Validation("scenes is empty".into()));
        }
        let mut prev_end = 0.0_f32;
        for (i, sc) in self.scenes.iter().enumerate() {
            if !(sc.end > sc.start) {
                return Err(StoryboardError::Validation(format!(
                    "scene[{i}]: end ({}) must be > start ({})",
                    sc.end, sc.start
                )));
            }
            if sc.start + 1e-4 < prev_end {
                return Err(StoryboardError::Validation(format!(
                    "scene[{i}]: start ({}) overlaps previous end ({prev_end})",
                    sc.start
                )));
            }
            prev_end = sc.end;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Storyboard {
        Storyboard {
            audio: "ep1.m4a".into(),
            fps: 30,
            width: 1080,
            height: 1920,
            subtitles: Some("ep1.subs.ass".into()),
            header: Some(HeaderSpec {
                text: "What makes a system transparent?".into(),
                kicker: Some("EP. 01 · THE EXAMINED MACHINE".into()),
            }),
            sigil: Some(SigilSpec {
                handle: "@TheExaminedMachine".into(),
                corner: Corner::TopRight,
                opacity: 0.7,
            }),
            scene_pips: Some(ScenePipsSpec {
                position: PipPosition::Top,
            }),
            scenes: vec![
                Scene {
                    start: 0.0,
                    end: 10.0,
                    backdrop: Backdrop::Flow {
                        palette: PaletteRef::TealAmber,
                        intensity: 0.8,
                        seed: 11,
                    },
                    foreground: vec![Foreground::KaraokeCaptions],
                    transition_in: Transition::HardCut,
                    post: PostChain::default(),
                },
                Scene {
                    start: 10.0,
                    end: 20.0,
                    backdrop: Backdrop::Solid { color: [0.04, 0.05, 0.10] },
                    foreground: vec![],
                    transition_in: Transition::Crossfade { dur: 0.5 },
                    post: PostChain::default(),
                },
            ],
        }
    }

    #[test]
    fn ron_roundtrip() {
        let sb = sample();
        let s = ron::ser::to_string_pretty(&sb, ron::ser::PrettyConfig::default()).unwrap();
        let parsed = Storyboard::from_ron(&s).unwrap();
        assert_eq!(parsed, sb);
    }

    #[test]
    fn duration_returns_max_end() {
        assert_eq!(sample().duration(), 20.0);
    }

    #[test]
    fn validate_rejects_empty_scenes() {
        let mut sb = sample();
        sb.scenes.clear();
        assert!(sb.validate().is_err());
    }

    #[test]
    fn validate_rejects_overlapping_scenes() {
        let mut sb = sample();
        sb.scenes[1].start = 5.0; // overlaps first scene's end (10.0)
        let err = sb.validate().unwrap_err();
        assert!(err.to_string().contains("overlap"));
    }

    #[test]
    fn validate_rejects_zero_duration_scene() {
        let mut sb = sample();
        sb.scenes[0].end = sb.scenes[0].start;
        assert!(sb.validate().is_err());
    }

    #[test]
    fn validate_rejects_zero_fps() {
        let mut sb = sample();
        sb.fps = 0;
        assert!(sb.validate().is_err());
    }
}
