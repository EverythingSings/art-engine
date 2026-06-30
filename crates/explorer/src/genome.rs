//! The genome: a dimension-independent recipe for one composition.
//!
//! A [`Genome`] is everything needed to build a [`Canvas`] *except* its pixel
//! dimensions — palette, background, an ordered layer stack, and a post
//! stack. Keeping it dimension-free lets the same genome render at focus
//! resolution and thumbnail resolution from one description, and gives the
//! mutation operators a clean value to perturb.
//!
//! Each layer is an *effect chain*: zero or more generator/transformer
//! passes. An empty chain renders the raw engine field through the palette
//! LUT; a generator (e.g. `aurora`, `voronoi`) synthesises content; a
//! transformer (e.g. `kaleidoscope`) reshapes whatever the chain produced
//! beneath it. Stacking a transformer on a generator multiplies the
//! reachable space combinatorially.
//!
//! The engine, seed, and step count are intentionally *not* part of the
//! genome in this phase: every variant shares one engine field so the grid
//! stays cheap to render. Engine-level mutation is a later phase.

use art_engine_core::canvas::{BlendMode, Canvas, ContentType, Layer, ShaderEffectDesc};
use art_engine_core::color::Srgb;
use art_engine_core::error::EngineError;
use art_engine_core::palette::Palette;
use art_engine_core::prng::Xorshift64;
use serde::{Deserialize, Serialize};

/// Built-in palette names the random generator draws from.
pub const PALETTES: &[&str] = &[
    "ocean",
    "neon",
    "earth",
    "monochrome",
    "vapor",
    "fire",
    "amber",
];

/// Engines the explorer navigates across — the workspace's far-from-equilibrium
/// "dissipative structure" family. Each produces a `Field` the layers render.
pub const ENGINES: &[&str] = &[
    "gray-scott",   // reaction-diffusion Turing patterns
    "physarum",     // slime-mold transport networks
    "dla",          // diffusion-limited aggregation
    "differential", // differential growth
    "excitable",    // excitable media — spiral/target waves
];

/// Content-generating shaders suitable as standalone layer content. These
/// synthesise an image rather than transforming `u_texture`, so each is a
/// valid first pass in a layer's chain.
pub const GENERATORS: &[&str] = &[
    "flow",
    "lattice",
    "mandala",
    "concentric",
    "strands",
    "wave",
    "spiral",
    "halftone",
    "crosshatch",
    "topo",
    "aurora",
    "sun",
    "particles",
    "branch",
    "caustics",
    "phyllotaxis",
    "constellation",
    "vector_field",
    "crystal",
    "smoke",
    "moire",
    "ripple",
    "plasma",
    "bokeh",
    "mosaic",
    "noise_static",
    "voronoi",
];

/// Shaders that reshape the content beneath them by sampling `u_texture`.
/// Appended after a generator (or the raw field) to fold/mirror it.
pub const TRANSFORMERS: &[&str] = &["kaleidoscope"];

/// Blend modes available to layers above the base.
pub const UPPER_BLENDS: &[BlendMode] = &[
    BlendMode::Additive,
    BlendMode::Screen,
    BlendMode::Normal,
    BlendMode::Multiply,
    BlendMode::Overlay,
];

/// Largest layer stack the random generator / mutator will build.
const MAX_LAYERS: usize = 4;

/// One shader pass within a layer's chain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectSpec {
    pub shader: String,
    pub params: serde_json::Value,
}

impl EffectSpec {
    fn is_transformer(&self) -> bool {
        TRANSFORMERS.contains(&self.shader.as_str())
    }
}

/// One layer's recipe: an ordered effect chain plus compositing settings.
/// An empty `effects` chain renders the raw engine field via the palette LUT.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerSpec {
    pub effects: Vec<EffectSpec>,
    pub blend: BlendMode,
    pub opacity: f64,
}

impl LayerSpec {
    fn ends_with_transformer(&self) -> bool {
        self.effects.last().is_some_and(EffectSpec::is_transformer)
    }
}

/// A complete, dimension-independent composition recipe.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Genome {
    /// Engine that produces the field layers render against (see [`ENGINES`]).
    pub engine: String,
    /// Engine seed — fixes the simulation, so the genome is reproducible.
    pub seed: u64,
    pub palette: String,
    pub background: Srgb,
    pub layers: Vec<LayerSpec>,
    pub post: Vec<ShaderEffectDesc>,
}

impl Genome {
    /// Builds a concrete [`Canvas`] at the given dimensions from this recipe.
    pub fn to_canvas(&self, width: usize, height: usize) -> Result<Canvas, EngineError> {
        let mut canvas = Canvas::new(width, height, self.background)?;
        for (i, spec) in self.layers.iter().enumerate() {
            let mut layer = Layer::new(format!("L{i}"), ContentType::Field)
                .with_blend_mode(spec.blend)
                .with_opacity(spec.opacity);
            for effect in &spec.effects {
                layer = layer.with_effect(ShaderEffectDesc::new(&effect.shader, effect.params.clone()));
            }
            canvas.add_layer(layer)?;
        }
        for effect in &self.post {
            canvas.push_post(effect.clone());
        }
        Ok(canvas)
    }

    /// Serialises the genome to pretty JSON for the clipboard / save-to-file.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| format!("// serialise failed: {e}"))
    }

    /// A short human-readable summary for the inspector panel.
    pub fn summary(&self) -> String {
        let layers: Vec<String> = self
            .layers
            .iter()
            .map(|l| {
                let chain = if l.effects.is_empty() {
                    "field".to_string()
                } else {
                    l.effects
                        .iter()
                        .map(|e| e.shader.as_str())
                        .collect::<Vec<_>>()
                        .join("→")
                };
                format!("{chain}/{:?}", l.blend)
            })
            .collect();
        let post: Vec<&str> = self.post.iter().map(|p| p.name.as_str()).collect();
        format!(
            "engine: {} (seed {})\npalette: {}\nlayers: {}\npost: {}",
            self.engine,
            self.seed,
            self.palette,
            layers.join(", "),
            if post.is_empty() {
                "—".to_string()
            } else {
                post.join(", ")
            }
        )
    }
}

// ── Random building blocks ─────────────────────────────────────────────

fn sample(pal: &Palette, t: f64) -> Srgb {
    pal.sample(t.clamp(0.0, 1.0))
}

fn color_array(c: Srgb) -> serde_json::Value {
    serde_json::json!([c.r, c.g, c.b])
}

/// Random generator params: recolour from the palette (covering both the
/// `lo/hi` and `a/b` colour conventions) and vary the common intensity /
/// speed knobs. Shaders ignore any uniform they don't declare.
fn random_generator_params(pal: &Palette, rng: &mut Xorshift64) -> serde_json::Value {
    let c1 = sample(pal, rng.next_f64());
    let c2 = sample(pal, rng.next_f64());
    serde_json::json!({
        "u_intensity": rng.next_range(0.6, 1.15),
        "u_speed": rng.next_range(0.2, 1.0),
        "u_color_lo": color_array(c1),
        "u_color_hi": color_array(c2),
        "u_color_a": color_array(c1),
        "u_color_b": color_array(c2),
    })
}

/// Random kaleidoscope params: fold count, rotation, zoom.
fn random_transformer_params(rng: &mut Xorshift64) -> serde_json::Value {
    serde_json::json!({
        "u_segments": (3 + rng.next_usize(10)) as f64, // 3..=12 folds
        "u_rotation": rng.next_range(0.0, std::f64::consts::TAU),
        "u_zoom": rng.next_range(0.7, 1.6),
        "u_center": [rng.next_range(0.35, 0.65), rng.next_range(0.35, 0.65)],
    })
}

fn random_generator(pal: &Palette, rng: &mut Xorshift64) -> EffectSpec {
    EffectSpec {
        shader: GENERATORS[rng.next_usize(GENERATORS.len())].to_string(),
        params: random_generator_params(pal, rng),
    }
}

fn random_transformer(rng: &mut Xorshift64) -> EffectSpec {
    EffectSpec {
        shader: TRANSFORMERS[rng.next_usize(TRANSFORMERS.len())].to_string(),
        params: random_transformer_params(rng),
    }
}

/// Builds one layer's effect chain. The base layer is occasionally the raw
/// field; otherwise a generator, optionally folded by a transformer.
fn random_chain(is_base: bool, pal: &Palette, rng: &mut Xorshift64) -> Vec<EffectSpec> {
    let mut effects = Vec::new();
    let raw_field = is_base && rng.next_f64() < 0.35;
    if !raw_field {
        effects.push(random_generator(pal, rng));
    }
    // ~35% of layers fold their content through a transformer.
    if rng.next_f64() < 0.35 {
        effects.push(random_transformer(rng));
    }
    effects
}

fn random_background(pal: &Palette, rng: &mut Xorshift64) -> Srgb {
    let c = sample(pal, rng.next_range(0.0, 0.25));
    let scale = rng.next_range(0.06, 0.3);
    Srgb {
        r: c.r * scale,
        g: c.g * scale,
        b: c.b * scale,
    }
}

fn random_upper_blend(rng: &mut Xorshift64) -> BlendMode {
    UPPER_BLENDS[rng.next_usize(UPPER_BLENDS.len())]
}

fn random_post(rng: &mut Xorshift64) -> Vec<ShaderEffectDesc> {
    let mut post = Vec::new();
    if rng.next_f64() < 0.75 {
        post.push(ShaderEffectDesc::new(
            "bloom",
            serde_json::json!({
                "intensity": rng.next_range(0.35, 0.8),
                "threshold": rng.next_range(0.4, 0.7),
                "radius": rng.next_range(3.0, 9.0),
            }),
        ));
    }
    if rng.next_f64() < 0.6 {
        post.push(ShaderEffectDesc::new(
            "vignette",
            serde_json::json!({
                "strength": rng.next_range(0.25, 0.6),
                "radius": rng.next_range(0.65, 0.9),
                "softness": 0.45,
            }),
        ));
    }
    if rng.next_f64() < 0.35 {
        post.push(ShaderEffectDesc::new(
            "color_grade",
            serde_json::json!({"saturation": rng.next_range(0.8, 1.5)}),
        ));
    }
    if rng.next_f64() < 0.3 {
        post.push(ShaderEffectDesc::new(
            "grain",
            serde_json::json!({"amount": rng.next_range(0.01, 0.04)}),
        ));
    }
    post
}

/// Generates a fresh random composition.
pub fn random_genome(rng: &mut Xorshift64) -> Genome {
    let palette = PALETTES[rng.next_usize(PALETTES.len())].to_string();
    let pal = Palette::from_name(&palette).expect("builtin palette name");
    let background = random_background(&pal, rng);

    let n_layers = 1 + rng.next_usize(MAX_LAYERS); // 1..=MAX_LAYERS
    let layers = (0..n_layers)
        .map(|i| {
            let is_base = i == 0;
            LayerSpec {
                effects: random_chain(is_base, &pal, rng),
                blend: if is_base {
                    BlendMode::Normal
                } else {
                    random_upper_blend(rng)
                },
                opacity: if is_base { 1.0 } else { rng.next_range(0.4, 1.0) },
            }
        })
        .collect();

    Genome {
        engine: ENGINES[rng.next_usize(ENGINES.len())].to_string(),
        seed: rng.next_u64(),
        palette,
        background,
        layers,
        post: random_post(rng),
    }
}

/// Picks an engine from [`ENGINES`] that differs from `current` when possible.
fn different_engine(current: &str, rng: &mut Xorshift64) -> String {
    let others: Vec<&&str> = ENGINES.iter().filter(|&&e| e != current).collect();
    if others.is_empty() {
        current.to_string()
    } else {
        others[rng.next_usize(others.len())].to_string()
    }
}

// ── Mutation ───────────────────────────────────────────────────────────

/// Multiplies every scalar number in a params object by a random factor,
/// leaving colour arrays untouched. Drives fine "jitter" exploration.
fn jitter_scalars(params: &mut serde_json::Value, rng: &mut Xorshift64) {
    if let Some(obj) = params.as_object_mut() {
        for value in obj.values_mut() {
            if let Some(f) = value.as_f64() {
                *value = serde_json::json!(f * rng.next_range(0.65, 1.4));
            }
        }
    }
}

fn pick_layer<'a>(layers: &'a mut [LayerSpec], rng: &mut Xorshift64) -> Option<&'a mut LayerSpec> {
    if layers.is_empty() {
        return None;
    }
    let idx = rng.next_usize(layers.len());
    layers.get_mut(idx)
}

/// Applies exactly one random change to the genome in place.
fn apply_one_change(g: &mut Genome, rng: &mut Xorshift64) {
    let pal = Palette::from_name(&g.palette).expect("builtin palette name");
    match rng.next_usize(11) {
        9 => {
            // Re-seed the same engine — a different instance of the same system.
            g.seed = rng.next_u64();
        }
        10 => {
            // Jump to a different dissipative engine (and a fresh seed).
            g.engine = different_engine(&g.engine, rng);
            g.seed = rng.next_u64();
        }
        0 => {
            // New palette + recolour every generator + background.
            g.palette = PALETTES[rng.next_usize(PALETTES.len())].to_string();
            let new_pal = Palette::from_name(&g.palette).expect("builtin palette name");
            for layer in &mut g.layers {
                for effect in &mut layer.effects {
                    if !effect.is_transformer() {
                        effect.params = random_generator_params(&new_pal, rng);
                    }
                }
            }
            g.background = random_background(&new_pal, rng);
        }
        1 => {
            // Swap one layer's generator (the first non-transformer pass).
            if let Some(layer) = pick_layer(&mut g.layers, rng) {
                let new_gen = random_generator(&pal, rng);
                match layer.effects.iter_mut().find(|e| !e.is_transformer()) {
                    Some(slot) => *slot = new_gen,
                    None => layer.effects.insert(0, new_gen),
                }
            }
        }
        2 => {
            // Flip one upper layer's blend mode.
            let upper = g.layers.len().saturating_sub(1);
            if upper >= 1 {
                let idx = 1 + rng.next_usize(upper);
                g.layers[idx].blend = random_upper_blend(rng);
            }
        }
        3 => {
            // Nudge one layer's opacity.
            if let Some(layer) = pick_layer(&mut g.layers, rng) {
                layer.opacity = rng.next_range(0.35, 1.0);
            }
        }
        4 => {
            // Re-roll the post stack.
            g.post = random_post(rng);
        }
        5 => {
            // Add or drop a layer.
            if g.layers.len() >= MAX_LAYERS || (g.layers.len() > 1 && rng.next_f64() < 0.4) {
                let idx = rng.next_usize(g.layers.len());
                g.layers.remove(idx);
            } else {
                g.layers.push(LayerSpec {
                    effects: random_chain(false, &pal, rng),
                    blend: random_upper_blend(rng),
                    opacity: rng.next_range(0.5, 1.0),
                });
            }
        }
        6 => {
            // Toggle a transformer (kaleidoscope fold) on a layer.
            if let Some(layer) = pick_layer(&mut g.layers, rng) {
                if layer.ends_with_transformer() {
                    layer.effects.pop();
                } else {
                    layer.effects.push(random_transformer(rng));
                }
            }
        }
        7 => {
            // Jitter the params of one random effect for fine variation.
            if let Some(layer) = pick_layer(&mut g.layers, rng) {
                if !layer.effects.is_empty() {
                    let idx = rng.next_usize(layer.effects.len());
                    jitter_scalars(&mut layer.effects[idx].params, rng);
                }
            }
        }
        _ => {
            // Re-tint the background.
            g.background = random_background(&pal, rng);
        }
    }
}

/// Produces a neighbour of `base` by applying one to three random changes.
/// The variable change-count widens the spread of the mutation grid while
/// keeping most variants recognisably related to the focus.
pub fn vary(base: &Genome, rng: &mut Xorshift64) -> Genome {
    let mut g = base.clone();
    // 1 change always, +1 half the time, +1 a fifth of the time → 1–3.
    let mut changes = 1;
    if rng.next_f64() < 0.5 {
        changes += 1;
    }
    if rng.next_f64() < 0.2 {
        changes += 1;
    }
    for _ in 0..changes {
        apply_one_change(&mut g, rng);
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_genome_is_buildable() {
        let mut rng = Xorshift64::new(1);
        for _ in 0..100 {
            let g = random_genome(&mut rng);
            assert!(ENGINES.contains(&g.engine.as_str()), "engine not in pool: {}", g.engine);
            assert!(!g.layers.is_empty());
            assert!(g.layers.len() <= MAX_LAYERS);
            assert_eq!(g.layers[0].blend, BlendMode::Normal);
            // A transformer is only ever the final pass of a chain.
            for layer in &g.layers {
                let t = layer.effects.iter().filter(|e| e.is_transformer()).count();
                assert!(t <= 1, "more than one transformer in a layer");
                if let Some(pos) = layer.effects.iter().position(|e| e.is_transformer()) {
                    assert_eq!(pos, layer.effects.len() - 1, "transformer not last");
                }
            }
            let canvas = g.to_canvas(64, 64).expect("to_canvas");
            assert_eq!(canvas.layer_count(), g.layers.len());
        }
    }

    #[test]
    fn vary_changes_something_but_stays_valid() {
        let mut rng = Xorshift64::new(7);
        let base = random_genome(&mut rng);
        let mut differed = 0;
        for _ in 0..50 {
            let v = vary(&base, &mut rng);
            assert!(!v.layers.is_empty());
            assert!(v.layers.len() <= MAX_LAYERS);
            v.to_canvas(64, 64).expect("varied genome builds");
            if v != base {
                differed += 1;
            }
        }
        assert!(differed > 45, "vary rarely changed anything: {differed}/50");
    }

    #[test]
    fn transformers_can_appear() {
        // Across many random genomes, at least some layers fold through a
        // transformer — proving the new shader possibility is reachable.
        let mut rng = Xorshift64::new(123);
        let mut with_transformer = 0;
        for _ in 0..200 {
            let g = random_genome(&mut rng);
            if g
                .layers
                .iter()
                .any(|l| l.effects.iter().any(EffectSpec::is_transformer))
            {
                with_transformer += 1;
            }
        }
        assert!(with_transformer > 10, "transformers never appeared");
    }

    #[test]
    fn genome_json_round_trips_structurally() {
        let mut rng = Xorshift64::new(99);
        for _ in 0..20 {
            let g = random_genome(&mut rng);
            let back: Genome = serde_json::from_str(&g.to_json()).expect("deserialize genome");
            assert_eq!(g.engine, back.engine);
            assert_eq!(g.seed, back.seed);
            assert_eq!(g.palette, back.palette);
            assert_eq!(g.layers.len(), back.layers.len());
            for (a, b) in g.layers.iter().zip(&back.layers) {
                let names_a: Vec<&str> = a.effects.iter().map(|e| e.shader.as_str()).collect();
                let names_b: Vec<&str> = b.effects.iter().map(|e| e.shader.as_str()).collect();
                assert_eq!(names_a, names_b);
                assert_eq!(a.blend, b.blend);
                assert!((a.opacity - b.opacity).abs() < 1e-9);
            }
            let post_a: Vec<&str> = g.post.iter().map(|p| p.name.as_str()).collect();
            let post_b: Vec<&str> = back.post.iter().map(|p| p.name.as_str()).collect();
            assert_eq!(post_a, post_b);
        }
    }

    #[test]
    fn every_shader_name_resolves_in_the_engine_registry() {
        // Guards against drift: if a name in GENERATORS/TRANSFORMERS or a post
        // builder is misspelled or removed from the engine, fail here rather
        // than silently rendering nothing at runtime.
        use art_engine_core::shaders::BuiltinShader;
        for name in GENERATORS.iter().chain(TRANSFORMERS) {
            assert!(
                BuiltinShader::from_name(name).is_some(),
                "unknown shader name in genome pool: {name}"
            );
        }
        // Names emitted by random_post / the render bridge.
        for name in ["bloom", "vignette", "color_grade", "grain", "solid"] {
            assert!(
                BuiltinShader::from_name(name).is_some(),
                "unknown post/content shader name: {name}"
            );
        }
    }

    #[test]
    fn every_engine_name_resolves() {
        // The render bridge builds these via EngineKind::from_name; a typo or
        // an unregistered engine would fail to render at runtime.
        use art_engine_engines::EngineKind;
        for name in ENGINES {
            assert!(
                EngineKind::from_name(name, 16, 16, 1, &serde_json::json!({})).is_ok(),
                "engine not registered: {name}"
            );
        }
    }

    #[test]
    fn every_palette_name_resolves() {
        for name in PALETTES {
            assert!(
                Palette::from_name(name).is_ok(),
                "unknown palette name in pool: {name}"
            );
        }
    }

    #[test]
    fn to_canvas_preserves_layer_and_post_counts() {
        let mut rng = Xorshift64::new(42);
        let g = random_genome(&mut rng);
        let canvas = g.to_canvas(128, 128).unwrap();
        assert_eq!(canvas.layer_count(), g.layers.len());
        assert_eq!(canvas.post_stack().len(), g.post.len());
    }
}
