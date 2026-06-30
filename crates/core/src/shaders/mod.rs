//! Built-in shader library for per-layer effects and post-processing.
//!
//! Each shader module provides GLSL ES 3.0 fragment source as a string
//! constant, paired with the fullscreen triangle vertex shader from
//! [`crate::render::fullscreen`]. The [`BuiltinShader`] enum serves as
//! the registry — string-based lookup via [`BuiltinShader::from_name`],
//! discovery via [`BuiltinShader::list`].
//!
//! # Shader categories
//!
//! **Layer effects** (applied per-layer via ping-pong on the layer FBO pair):
//! - [`feedback`] — Frame persistence / trails
//! - [`voronoi`] — Voronoi cell pattern generation
//! - [`kaleidoscope`] — Radial mirror symmetry
//! - [`flow`] — Audio-reactive organic flow field (storyboard backdrop)
//!
//! **Post-processing** (applied to the composite via the post-process ping-pong):
//! - [`bloom`] — Multi-pass glow (threshold + blur + combine)
//! - [`vignette`] — Radial corner darkening
//! - [`grain`] — Animated film grain
//! - [`color_grade`] — Lift / gamma / gain + saturation
//!
//! **Compositing** (used by the layer compositor for non-hardware blend modes):
//! - [`composite`] — Multiply / Screen / Overlay shader-based blends

pub mod aurora;
pub mod bloom;
pub mod bokeh;
pub mod branch;
pub mod caustics;
pub mod color_grade;
pub mod composite;
pub mod concentric;
pub mod constellation;
pub mod crosshatch;
pub mod crystal;
pub mod feedback;
pub mod flow;
pub mod grain;
pub mod halftone;
pub mod kaleidoscope;
pub mod lattice;
pub mod mandala;
pub mod moire;
pub mod mosaic;
pub mod noise_static;
pub mod particles;
pub mod phyllotaxis;
pub mod plasma;
pub mod ripple;
pub mod smoke;
pub mod solid;
pub mod spiral;
pub mod strands;
pub mod sun;
pub mod topo;
pub mod vector_field;
pub mod vignette;
pub mod voronoi;
pub mod wave;

/// A built-in shader effect from the engine's shader library.
///
/// Provides access to GLSL fragment sources and metadata. Constructed
/// by name via [`BuiltinShader::from_name`] for CLI/agent integration.
///
/// # Audio reactivity convention
///
/// Every animated backdrop in the registry accepts two optional audio
/// uniforms:
///
/// - `u_rms` — RMS loudness in `[0, 1]`. Standard mapping: multiplies
///   the final ink/intensity by `(1.0 + 0.35 * u_rms)` for a sustained
///   brightness lift on loud passages.
/// - `u_onset` — Transient hit indicator in `[0, 1]`, sharpest at the
///   instant of a percussive event. Standard mapping: pulses ONE
///   shader-specific parameter that fits the shader's symbolic register
///   (e.g. tightens cell edges, fattens line thickness, brightens dash
///   speed, triggers a fresh ripple).
///
/// Both default to `0.0` in [`crate::render::pipeline::default_uniform_schema`],
/// so a shader rendered without an audio track behaves identically to
/// the un-reactive version. New shaders added to this registry are
/// expected to declare and consume these uniforms following the same
/// convention. `Solid` is the only intentional opt-out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinShader {
    /// Frame feedback for trails and echoes.
    Feedback,
    /// Voronoi cell tessellation pattern.
    Voronoi,
    /// Radial mirror symmetry.
    Kaleidoscope,
    /// Audio-reactive organic flow field (storyboard backdrop).
    Flow,
    /// Solid color fill (storyboard backdrop / transition pad).
    Solid,
    /// Animated TV-static / breakdown noise (storyboard backdrop).
    NoiseStatic,
    /// Orthogonal mechanical grid (storyboard backdrop).
    Lattice,
    /// N-fold radial symmetry (storyboard backdrop).
    Mandala,
    /// Outward-radiating concentric rings (storyboard backdrop).
    Concentric,
    /// Vertical glowing filaments (storyboard backdrop).
    Strands,
    /// Horizontal sinusoidal bands (storyboard backdrop).
    Wave,
    /// Logarithmic spiral winding from center (storyboard backdrop).
    Spiral,
    /// Newsprint halftone dot pattern (storyboard backdrop).
    Halftone,
    /// Draftsman crosshatch — two diagonal line families (storyboard backdrop).
    Crosshatch,
    /// Topographic isolines of a slow scalar field (storyboard backdrop).
    Topo,
    /// Aurora — vertical curtains of light (storyboard backdrop).
    Aurora,
    /// Sun — singular luminous disc with rays + halo (storyboard backdrop).
    Sun,
    /// Particles — N orbiting points-of-light (storyboard backdrop).
    Particles,
    /// Branch — fractal tree silhouette via line-segment SDFs (storyboard backdrop).
    Branch,
    /// Caustics — light dapples through wavy water (storyboard backdrop).
    Caustics,
    /// Phyllotaxis — sunflower-seed packing on the golden angle (storyboard backdrop).
    Phyllotaxis,
    /// Constellation — bright nodes joined by faint edges (storyboard backdrop).
    Constellation,
    /// VectorField — streamlines of an unseen vector field with drifting dashes (storyboard backdrop).
    VectorField,
    /// Crystal — hard-faceted polygonal cells with quantised tone (storyboard backdrop).
    Crystal,
    /// Smoke — soft drifting volumetric haze (storyboard backdrop).
    Smoke,
    /// Moire — interference between two near-identical line patterns (storyboard backdrop).
    Moire,
    /// Ripple — propagating disturbances from localised origins (storyboard backdrop).
    Ripple,
    /// Plasma — fluid blobs that merge and separate via metaballs (storyboard backdrop).
    Plasma,
    /// Bokeh — soft out-of-focus circles at varied depths (storyboard backdrop).
    Bokeh,
    /// Mosaic — quantised regular tile grid with discrete tone per tile (storyboard backdrop).
    Mosaic,
    /// Multi-pass glow (post-processing).
    Bloom,
    /// Radial corner darkening (post-processing).
    Vignette,
    /// Film grain (post-processing).
    Grain,
    /// Lift / gamma / gain color grade (post-processing).
    ColorGrade,
}

impl BuiltinShader {
    /// Looks up a shader by name (case-insensitive).
    ///
    /// Returns `None` if the name doesn't match any built-in shader.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "feedback" => Some(Self::Feedback),
            "voronoi" => Some(Self::Voronoi),
            "kaleidoscope" => Some(Self::Kaleidoscope),
            "flow" => Some(Self::Flow),
            "solid" => Some(Self::Solid),
            "noise_static" | "noise-static" | "static" => Some(Self::NoiseStatic),
            "lattice" => Some(Self::Lattice),
            "mandala" => Some(Self::Mandala),
            "concentric" => Some(Self::Concentric),
            "strands" => Some(Self::Strands),
            "wave" => Some(Self::Wave),
            "spiral" => Some(Self::Spiral),
            "halftone" => Some(Self::Halftone),
            "crosshatch" => Some(Self::Crosshatch),
            "topo" => Some(Self::Topo),
            "aurora" => Some(Self::Aurora),
            "sun" => Some(Self::Sun),
            "particles" => Some(Self::Particles),
            "branch" => Some(Self::Branch),
            "caustics" => Some(Self::Caustics),
            "phyllotaxis" => Some(Self::Phyllotaxis),
            "constellation" => Some(Self::Constellation),
            "vector_field" | "vector-field" | "vectorfield" => Some(Self::VectorField),
            "crystal" => Some(Self::Crystal),
            "smoke" => Some(Self::Smoke),
            "moire" => Some(Self::Moire),
            "ripple" => Some(Self::Ripple),
            "plasma" => Some(Self::Plasma),
            "bokeh" => Some(Self::Bokeh),
            "mosaic" => Some(Self::Mosaic),
            "bloom" => Some(Self::Bloom),
            "vignette" => Some(Self::Vignette),
            "grain" => Some(Self::Grain),
            // Allow both `color_grade` and `color-grade` — CLI users will
            // commonly type the kebab form.
            "color_grade" | "color-grade" | "colorgrade" => Some(Self::ColorGrade),
            _ => None,
        }
    }

    /// Returns the shader's canonical name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Feedback => feedback::NAME,
            Self::Voronoi => voronoi::NAME,
            Self::Kaleidoscope => kaleidoscope::NAME,
            Self::Flow => flow::NAME,
            Self::Solid => solid::NAME,
            Self::NoiseStatic => noise_static::NAME,
            Self::Lattice => lattice::NAME,
            Self::Mandala => mandala::NAME,
            Self::Concentric => concentric::NAME,
            Self::Strands => strands::NAME,
            Self::Wave => wave::NAME,
            Self::Spiral => spiral::NAME,
            Self::Halftone => halftone::NAME,
            Self::Crosshatch => crosshatch::NAME,
            Self::Topo => topo::NAME,
            Self::Aurora => aurora::NAME,
            Self::Sun => sun::NAME,
            Self::Particles => particles::NAME,
            Self::Branch => branch::NAME,
            Self::Caustics => caustics::NAME,
            Self::Phyllotaxis => phyllotaxis::NAME,
            Self::Constellation => constellation::NAME,
            Self::VectorField => vector_field::NAME,
            Self::Crystal => crystal::NAME,
            Self::Smoke => smoke::NAME,
            Self::Moire => moire::NAME,
            Self::Ripple => ripple::NAME,
            Self::Plasma => plasma::NAME,
            Self::Bokeh => bokeh::NAME,
            Self::Mosaic => mosaic::NAME,
            Self::Bloom => bloom::NAME,
            Self::Vignette => vignette::NAME,
            Self::Grain => grain::NAME,
            Self::ColorGrade => color_grade::NAME,
        }
    }

    /// Returns the primary fragment shader source.
    ///
    /// For single-pass shaders (feedback, voronoi, kaleidoscope, vignette,
    /// grain, color_grade), this is the only fragment source needed. For
    /// multi-pass shaders (bloom), this returns the threshold pass — use
    /// [`bloom_sources`] for the full set.
    pub fn fragment_source(self) -> &'static str {
        match self {
            Self::Feedback => feedback::FRAGMENT_SOURCE,
            Self::Voronoi => voronoi::FRAGMENT_SOURCE,
            Self::Kaleidoscope => kaleidoscope::FRAGMENT_SOURCE,
            Self::Flow => flow::FRAGMENT_SOURCE,
            Self::Solid => solid::FRAGMENT_SOURCE,
            Self::NoiseStatic => noise_static::FRAGMENT_SOURCE,
            Self::Lattice => lattice::FRAGMENT_SOURCE,
            Self::Mandala => mandala::FRAGMENT_SOURCE,
            Self::Concentric => concentric::FRAGMENT_SOURCE,
            Self::Strands => strands::FRAGMENT_SOURCE,
            Self::Wave => wave::FRAGMENT_SOURCE,
            Self::Spiral => spiral::FRAGMENT_SOURCE,
            Self::Halftone => halftone::FRAGMENT_SOURCE,
            Self::Crosshatch => crosshatch::FRAGMENT_SOURCE,
            Self::Topo => topo::FRAGMENT_SOURCE,
            Self::Aurora => aurora::FRAGMENT_SOURCE,
            Self::Sun => sun::FRAGMENT_SOURCE,
            Self::Particles => particles::FRAGMENT_SOURCE,
            Self::Branch => branch::FRAGMENT_SOURCE,
            Self::Caustics => caustics::FRAGMENT_SOURCE,
            Self::Phyllotaxis => phyllotaxis::FRAGMENT_SOURCE,
            Self::Constellation => constellation::FRAGMENT_SOURCE,
            Self::VectorField => vector_field::FRAGMENT_SOURCE,
            Self::Crystal => crystal::FRAGMENT_SOURCE,
            Self::Smoke => smoke::FRAGMENT_SOURCE,
            Self::Moire => moire::FRAGMENT_SOURCE,
            Self::Ripple => ripple::FRAGMENT_SOURCE,
            Self::Plasma => plasma::FRAGMENT_SOURCE,
            Self::Bokeh => bokeh::FRAGMENT_SOURCE,
            Self::Mosaic => mosaic::FRAGMENT_SOURCE,
            Self::Bloom => bloom::THRESHOLD_SOURCE,
            Self::Vignette => vignette::FRAGMENT_SOURCE,
            Self::Grain => grain::FRAGMENT_SOURCE,
            Self::ColorGrade => color_grade::FRAGMENT_SOURCE,
        }
    }

    /// Returns whether this shader is a post-processing effect.
    ///
    /// Post-processing shaders run on the composite output after all
    /// layers are blended. Layer effects run per-layer before compositing.
    pub fn is_post_process(self) -> bool {
        matches!(
            self,
            Self::Bloom | Self::Vignette | Self::Grain | Self::ColorGrade
        )
    }

    /// Returns all available shader names, sorted alphabetically.
    pub fn list() -> &'static [&'static str] {
        &[
            "aurora",
            "bloom",
            "bokeh",
            "branch",
            "caustics",
            "color_grade",
            "concentric",
            "constellation",
            "crosshatch",
            "crystal",
            "feedback",
            "flow",
            "grain",
            "halftone",
            "kaleidoscope",
            "lattice",
            "mandala",
            "moire",
            "mosaic",
            "noise_static",
            "particles",
            "phyllotaxis",
            "plasma",
            "ripple",
            "smoke",
            "solid",
            "spiral",
            "strands",
            "sun",
            "topo",
            "vector_field",
            "vignette",
            "voronoi",
            "wave",
        ]
    }
}

/// The three fragment shader sources for the bloom post-processing pipeline.
///
/// Bloom requires three passes in order:
/// 1. Threshold — extract bright pixels
/// 2. Blur — separable Gaussian (run twice: H then V)
/// 3. Combine — additive blend with original
pub struct BloomSources {
    /// Brightness threshold extraction.
    pub threshold: &'static str,
    /// Separable Gaussian blur (set direction uniform per pass).
    pub blur: &'static str,
    /// Additive combine of bloom with original.
    pub combine: &'static str,
}

/// Returns the full set of bloom shader sources.
///
/// Use this instead of `BuiltinShader::Bloom.fragment_source()` when
/// setting up the multi-pass bloom pipeline.
pub fn bloom_sources() -> BloomSources {
    BloomSources {
        threshold: bloom::THRESHOLD_SOURCE,
        blur: bloom::BLUR_SOURCE,
        combine: bloom::COMBINE_SOURCE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_name_resolves_all_shaders() {
        assert_eq!(
            BuiltinShader::from_name("feedback"),
            Some(BuiltinShader::Feedback)
        );
        assert_eq!(
            BuiltinShader::from_name("voronoi"),
            Some(BuiltinShader::Voronoi)
        );
        assert_eq!(
            BuiltinShader::from_name("kaleidoscope"),
            Some(BuiltinShader::Kaleidoscope)
        );
        assert_eq!(
            BuiltinShader::from_name("flow"),
            Some(BuiltinShader::Flow)
        );
        assert_eq!(
            BuiltinShader::from_name("bloom"),
            Some(BuiltinShader::Bloom)
        );
        assert_eq!(
            BuiltinShader::from_name("vignette"),
            Some(BuiltinShader::Vignette)
        );
        assert_eq!(
            BuiltinShader::from_name("grain"),
            Some(BuiltinShader::Grain)
        );
        assert_eq!(
            BuiltinShader::from_name("color_grade"),
            Some(BuiltinShader::ColorGrade)
        );
    }

    #[test]
    fn from_name_accepts_kebab_color_grade() {
        assert_eq!(
            BuiltinShader::from_name("color-grade"),
            Some(BuiltinShader::ColorGrade)
        );
        assert_eq!(
            BuiltinShader::from_name("colorgrade"),
            Some(BuiltinShader::ColorGrade)
        );
    }

    #[test]
    fn from_name_is_case_insensitive() {
        assert_eq!(
            BuiltinShader::from_name("FEEDBACK"),
            Some(BuiltinShader::Feedback)
        );
        assert_eq!(
            BuiltinShader::from_name("Voronoi"),
            Some(BuiltinShader::Voronoi)
        );
        assert_eq!(
            BuiltinShader::from_name("KALEIDOSCOPE"),
            Some(BuiltinShader::Kaleidoscope)
        );
        assert_eq!(
            BuiltinShader::from_name("Bloom"),
            Some(BuiltinShader::Bloom)
        );
        assert_eq!(
            BuiltinShader::from_name("VIGNETTE"),
            Some(BuiltinShader::Vignette)
        );
    }

    #[test]
    fn from_name_returns_none_for_unknown() {
        assert_eq!(BuiltinShader::from_name("nonexistent"), None);
        assert_eq!(BuiltinShader::from_name(""), None);
    }

    #[test]
    fn name_roundtrips_through_from_name() {
        for &name in BuiltinShader::list() {
            let shader = BuiltinShader::from_name(name).expect("list() name should resolve");
            assert_eq!(shader.name(), name, "name() should match from_name() input");
        }
    }

    #[test]
    fn fragment_source_is_nonempty_for_all() {
        for &name in BuiltinShader::list() {
            let shader = BuiltinShader::from_name(name).unwrap();
            assert!(
                !shader.fragment_source().is_empty(),
                "fragment_source() for {name} should not be empty"
            );
        }
    }

    #[test]
    fn all_fragment_sources_are_glsl_es_300() {
        for &name in BuiltinShader::list() {
            let shader = BuiltinShader::from_name(name).unwrap();
            assert!(
                shader.fragment_source().contains("#version 300 es"),
                "{name} fragment source should be GLSL ES 3.0"
            );
        }
    }

    #[test]
    fn list_is_sorted_alphabetically() {
        let list = BuiltinShader::list();
        let mut sorted = list.to_vec();
        sorted.sort();
        assert_eq!(
            list,
            sorted.as_slice(),
            "list() should be alphabetically sorted"
        );
    }

    #[test]
    fn list_contains_all_variants() {
        let list = BuiltinShader::list();
        assert_eq!(list.len(), 34, "expected 34 built-in shaders");
        assert!(list.contains(&"aurora"));
        assert!(list.contains(&"bloom"));
        assert!(list.contains(&"bokeh"));
        assert!(list.contains(&"branch"));
        assert!(list.contains(&"caustics"));
        assert!(list.contains(&"color_grade"));
        assert!(list.contains(&"concentric"));
        assert!(list.contains(&"constellation"));
        assert!(list.contains(&"crosshatch"));
        assert!(list.contains(&"crystal"));
        assert!(list.contains(&"feedback"));
        assert!(list.contains(&"flow"));
        assert!(list.contains(&"grain"));
        assert!(list.contains(&"halftone"));
        assert!(list.contains(&"kaleidoscope"));
        assert!(list.contains(&"lattice"));
        assert!(list.contains(&"mandala"));
        assert!(list.contains(&"moire"));
        assert!(list.contains(&"mosaic"));
        assert!(list.contains(&"noise_static"));
        assert!(list.contains(&"particles"));
        assert!(list.contains(&"phyllotaxis"));
        assert!(list.contains(&"plasma"));
        assert!(list.contains(&"ripple"));
        assert!(list.contains(&"smoke"));
        assert!(list.contains(&"solid"));
        assert!(list.contains(&"spiral"));
        assert!(list.contains(&"strands"));
        assert!(list.contains(&"sun"));
        assert!(list.contains(&"topo"));
        assert!(list.contains(&"vector_field"));
        assert!(list.contains(&"vignette"));
        assert!(list.contains(&"voronoi"));
        assert!(list.contains(&"wave"));
    }

    #[test]
    fn post_process_set_matches_documentation() {
        assert!(!BuiltinShader::Feedback.is_post_process());
        assert!(!BuiltinShader::Voronoi.is_post_process());
        assert!(!BuiltinShader::Kaleidoscope.is_post_process());
        assert!(!BuiltinShader::Flow.is_post_process());
        assert!(!BuiltinShader::Solid.is_post_process());
        assert!(!BuiltinShader::NoiseStatic.is_post_process());
        assert!(!BuiltinShader::Lattice.is_post_process());
        assert!(!BuiltinShader::Mandala.is_post_process());
        assert!(!BuiltinShader::Concentric.is_post_process());
        assert!(!BuiltinShader::Strands.is_post_process());
        assert!(!BuiltinShader::Wave.is_post_process());
        assert!(!BuiltinShader::Spiral.is_post_process());
        assert!(!BuiltinShader::Halftone.is_post_process());
        assert!(!BuiltinShader::Crosshatch.is_post_process());
        assert!(!BuiltinShader::Topo.is_post_process());
        assert!(!BuiltinShader::Aurora.is_post_process());
        assert!(!BuiltinShader::Sun.is_post_process());
        assert!(!BuiltinShader::Particles.is_post_process());
        assert!(!BuiltinShader::Branch.is_post_process());
        assert!(!BuiltinShader::Caustics.is_post_process());
        assert!(!BuiltinShader::Phyllotaxis.is_post_process());
        assert!(!BuiltinShader::Constellation.is_post_process());
        assert!(!BuiltinShader::VectorField.is_post_process());
        assert!(!BuiltinShader::Crystal.is_post_process());
        assert!(!BuiltinShader::Smoke.is_post_process());
        assert!(!BuiltinShader::Moire.is_post_process());
        assert!(!BuiltinShader::Ripple.is_post_process());
        assert!(!BuiltinShader::Plasma.is_post_process());
        assert!(!BuiltinShader::Bokeh.is_post_process());
        assert!(!BuiltinShader::Mosaic.is_post_process());
        assert!(BuiltinShader::Bloom.is_post_process());
        assert!(BuiltinShader::Vignette.is_post_process());
        assert!(BuiltinShader::Grain.is_post_process());
        assert!(BuiltinShader::ColorGrade.is_post_process());
    }

    #[test]
    fn bloom_sources_returns_three_distinct_shaders() {
        let sources = bloom_sources();
        assert_ne!(sources.threshold, sources.blur);
        assert_ne!(sources.blur, sources.combine);
        assert_ne!(sources.threshold, sources.combine);
    }

    #[test]
    fn bloom_sources_are_all_glsl_es_300() {
        let sources = bloom_sources();
        assert!(sources.threshold.contains("#version 300 es"));
        assert!(sources.blur.contains("#version 300 es"));
        assert!(sources.combine.contains("#version 300 es"));
    }
}
