//! Canvas and layer data model for the art-engine.
//!
//! A [`Canvas`] holds dimensions, a background color, and an ordered stack of
//! [`Layer`]s. Layers are identified by unique names and rendered bottom-to-top
//! (index 0 = bottom).

use serde::{Deserialize, Serialize};

use crate::color::Srgb;
use crate::error::EngineError;

/// Blend mode used when compositing a layer onto the canvas.
///
/// `Normal` and `Additive` can use hardware `gl.blendFunc` as a fast path.
/// `Multiply`, `Screen`, and `Overlay` require shader-based compositing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlendMode {
    #[default]
    Normal,
    Additive,
    Multiply,
    Screen,
    Overlay,
}

/// The kind of content a layer renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Particles,
    Shapes,
    Field,
}

/// A single layer in the canvas stack.
///
/// Layers are identified by unique names within a [`Canvas`]. Each layer has
/// a blend mode, opacity, visibility flag, and content type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Layer {
    name: String,
    blend_mode: BlendMode,
    opacity: f64,
    visible: bool,
    content_type: ContentType,
}

impl Layer {
    /// Creates a new layer with the given name and content type.
    ///
    /// Defaults: `BlendMode::Normal`, opacity `1.0`, visible `true`.
    pub fn new(name: impl Into<String>, content_type: ContentType) -> Self {
        Self {
            name: name.into(),
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            visible: true,
            content_type,
        }
    }

    /// Returns the layer name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the blend mode.
    pub fn blend_mode(&self) -> BlendMode {
        self.blend_mode
    }

    /// Sets the blend mode.
    pub fn set_blend_mode(&mut self, mode: BlendMode) {
        self.blend_mode = mode;
    }

    /// Returns the opacity in [0.0, 1.0].
    pub fn opacity(&self) -> f64 {
        self.opacity
    }

    /// Sets the opacity, clamping to [0.0, 1.0].
    pub fn set_opacity(&mut self, opacity: f64) {
        self.opacity = opacity.clamp(0.0, 1.0);
    }

    /// Returns whether the layer is visible.
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// Sets the visibility flag.
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Returns the content type.
    pub fn content_type(&self) -> ContentType {
        self.content_type
    }

    /// Returns a new layer with the given blend mode.
    pub fn with_blend_mode(mut self, mode: BlendMode) -> Self {
        self.blend_mode = mode;
        self
    }

    /// Returns a new layer with the given opacity, clamped to [0.0, 1.0].
    pub fn with_opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Returns a new layer with the given visibility.
    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }
}

/// A canvas with dimensions, background color, and an ordered layer stack.
///
/// Layers are stored bottom-to-top: index 0 is the bottom layer, rendered
/// first. Layer names must be unique within a canvas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Canvas {
    width: usize,
    height: usize,
    background: Srgb,
    layers: Vec<Layer>,
}

impl Canvas {
    /// Creates a new canvas with the given dimensions and background color.
    ///
    /// Returns `EngineError::InvalidDimensions` if width or height is zero,
    /// or if `width * height` would overflow `usize`.
    pub fn new(width: usize, height: usize, background: Srgb) -> Result<Self, EngineError> {
        if width == 0 || height == 0 {
            return Err(EngineError::InvalidDimensions);
        }
        width
            .checked_mul(height)
            .ok_or(EngineError::InvalidDimensions)?;
        Ok(Self {
            width,
            height,
            background,
            layers: Vec::new(),
        })
    }

    /// Returns the canvas width.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Returns the canvas height.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Returns the background color.
    pub fn background(&self) -> Srgb {
        self.background
    }

    /// Sets the background color.
    pub fn set_background(&mut self, background: Srgb) {
        self.background = background;
    }

    /// Returns the number of layers.
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Returns a slice of all layers (bottom-to-top order).
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// Adds a layer to the top of the stack.
    ///
    /// Returns `EngineError::DuplicateLayerName` if a layer with the same
    /// name already exists.
    pub fn add_layer(&mut self, layer: Layer) -> Result<(), EngineError> {
        let has_duplicate = self.layers.iter().any(|l| l.name == layer.name);
        if has_duplicate {
            return Err(EngineError::DuplicateLayerName(layer.name));
        }
        self.layers.push(layer);
        Ok(())
    }

    /// Removes a layer by name and returns it.
    ///
    /// Returns `EngineError::LayerNotFound` if no layer with the given name exists.
    pub fn remove_layer(&mut self, name: &str) -> Result<Layer, EngineError> {
        let idx = self.index_of(name)?;
        Ok(self.layers.remove(idx))
    }

    /// Returns a reference to the layer with the given name.
    ///
    /// Returns `EngineError::LayerNotFound` if not found.
    pub fn layer(&self, name: &str) -> Result<&Layer, EngineError> {
        self.layers
            .iter()
            .find(|l| l.name == name)
            .ok_or_else(|| EngineError::LayerNotFound(name.to_string()))
    }

    /// Returns a mutable reference to the layer with the given name.
    ///
    /// Returns `EngineError::LayerNotFound` if not found.
    pub fn layer_mut(&mut self, name: &str) -> Result<&mut Layer, EngineError> {
        self.layers
            .iter_mut()
            .find(|l| l.name == name)
            .ok_or_else(|| EngineError::LayerNotFound(name.to_string()))
    }

    /// Moves a layer to the given index in the stack.
    ///
    /// Index 0 is the bottom. If `index >= layer_count()`, the layer moves
    /// to the top.
    ///
    /// Returns `EngineError::LayerNotFound` if the layer doesn't exist.
    pub fn move_layer_to(&mut self, name: &str, index: usize) -> Result<(), EngineError> {
        let idx = self.index_of(name)?;
        let layer = self.layers.remove(idx);
        let target = index.min(self.layers.len());
        self.layers.insert(target, layer);
        Ok(())
    }

    /// Moves a layer one position up (toward the top) in the stack.
    ///
    /// If the layer is already at the top, this is a no-op.
    ///
    /// Returns `EngineError::LayerNotFound` if the layer doesn't exist.
    pub fn move_layer_up(&mut self, name: &str) -> Result<(), EngineError> {
        let idx = self.index_of(name)?;
        if idx + 1 < self.layers.len() {
            self.layers.swap(idx, idx + 1);
        }
        Ok(())
    }

    /// Moves a layer one position down (toward the bottom) in the stack.
    ///
    /// If the layer is already at the bottom, this is a no-op.
    ///
    /// Returns `EngineError::LayerNotFound` if the layer doesn't exist.
    pub fn move_layer_down(&mut self, name: &str) -> Result<(), EngineError> {
        let idx = self.index_of(name)?;
        if idx > 0 {
            self.layers.swap(idx, idx - 1);
        }
        Ok(())
    }

    /// Finds the index of a layer by name.
    fn index_of(&self, name: &str) -> Result<usize, EngineError> {
        self.layers
            .iter()
            .position(|l| l.name == name)
            .ok_or_else(|| EngineError::LayerNotFound(name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn black() -> Srgb {
        Srgb {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        }
    }

    fn white() -> Srgb {
        Srgb {
            r: 1.0,
            g: 1.0,
            b: 1.0,
        }
    }

    // ── BlendMode tests ────────────────────────────────────────────

    #[test]
    fn blend_mode_default_is_normal() {
        assert_eq!(BlendMode::default(), BlendMode::Normal);
    }

    #[test]
    fn blend_mode_serde_round_trip() {
        let modes = [
            BlendMode::Normal,
            BlendMode::Additive,
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
        ];
        for mode in &modes {
            let json = serde_json::to_string(mode).unwrap();
            let deserialized: BlendMode = serde_json::from_str(&json).unwrap();
            assert_eq!(*mode, deserialized);
        }
    }

    #[test]
    fn blend_mode_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&BlendMode::Normal).unwrap(),
            "\"normal\""
        );
        assert_eq!(
            serde_json::to_string(&BlendMode::Additive).unwrap(),
            "\"additive\""
        );
        assert_eq!(
            serde_json::to_string(&BlendMode::Multiply).unwrap(),
            "\"multiply\""
        );
        assert_eq!(
            serde_json::to_string(&BlendMode::Screen).unwrap(),
            "\"screen\""
        );
        assert_eq!(
            serde_json::to_string(&BlendMode::Overlay).unwrap(),
            "\"overlay\""
        );
    }

    // ── ContentType tests ──────────────────────────────────────────

    #[test]
    fn content_type_serde_round_trip() {
        let types = [
            ContentType::Particles,
            ContentType::Shapes,
            ContentType::Field,
        ];
        for ct in &types {
            let json = serde_json::to_string(ct).unwrap();
            let deserialized: ContentType = serde_json::from_str(&json).unwrap();
            assert_eq!(*ct, deserialized);
        }
    }

    #[test]
    fn content_type_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&ContentType::Particles).unwrap(),
            "\"particles\""
        );
        assert_eq!(
            serde_json::to_string(&ContentType::Shapes).unwrap(),
            "\"shapes\""
        );
        assert_eq!(
            serde_json::to_string(&ContentType::Field).unwrap(),
            "\"field\""
        );
    }

    // ── Layer tests ────────────────────────────────────────────────

    #[test]
    fn layer_new_has_expected_defaults() {
        let layer = Layer::new("bg", ContentType::Particles);
        assert_eq!(layer.name(), "bg");
        assert_eq!(layer.blend_mode(), BlendMode::Normal);
        assert_eq!(layer.opacity(), 1.0);
        assert!(layer.visible());
        assert_eq!(layer.content_type(), ContentType::Particles);
    }

    #[test]
    fn layer_set_blend_mode() {
        let mut layer = Layer::new("fx", ContentType::Shapes);
        layer.set_blend_mode(BlendMode::Overlay);
        assert_eq!(layer.blend_mode(), BlendMode::Overlay);
    }

    #[test]
    fn layer_set_opacity_clamps_above_one() {
        let mut layer = Layer::new("test", ContentType::Field);
        layer.set_opacity(1.5);
        assert_eq!(layer.opacity(), 1.0);
    }

    #[test]
    fn layer_set_opacity_clamps_below_zero() {
        let mut layer = Layer::new("test", ContentType::Field);
        layer.set_opacity(-0.5);
        assert_eq!(layer.opacity(), 0.0);
    }

    #[test]
    fn layer_set_opacity_accepts_valid_value() {
        let mut layer = Layer::new("test", ContentType::Particles);
        layer.set_opacity(0.75);
        assert_eq!(layer.opacity(), 0.75);
    }

    #[test]
    fn layer_set_visible() {
        let mut layer = Layer::new("test", ContentType::Shapes);
        assert!(layer.visible());
        layer.set_visible(false);
        assert!(!layer.visible());
    }

    #[test]
    fn layer_with_builder_chain() {
        let layer = Layer::new("fx", ContentType::Shapes)
            .with_blend_mode(BlendMode::Overlay)
            .with_opacity(0.75)
            .with_visible(false);
        assert_eq!(layer.name(), "fx");
        assert_eq!(layer.blend_mode(), BlendMode::Overlay);
        assert_eq!(layer.opacity(), 0.75);
        assert!(!layer.visible());
        assert_eq!(layer.content_type(), ContentType::Shapes);
    }

    #[test]
    fn layer_with_opacity_clamps() {
        let above = Layer::new("a", ContentType::Field).with_opacity(2.0);
        assert_eq!(above.opacity(), 1.0);
        let below = Layer::new("b", ContentType::Field).with_opacity(-1.0);
        assert_eq!(below.opacity(), 0.0);
    }

    #[test]
    fn layer_serde_round_trip() {
        let layer = Layer::new("deep", ContentType::Particles)
            .with_blend_mode(BlendMode::Additive)
            .with_opacity(0.8)
            .with_visible(false);

        let json = serde_json::to_string(&layer).unwrap();
        let deserialized: Layer = serde_json::from_str(&json).unwrap();
        assert_eq!(layer, deserialized);
    }

    // ── Canvas construction tests ──────────────────────────────────

    #[test]
    fn canvas_new_creates_empty_canvas() {
        let canvas = Canvas::new(1024, 768, black()).unwrap();
        assert_eq!(canvas.width(), 1024);
        assert_eq!(canvas.height(), 768);
        assert_eq!(canvas.background(), black());
        assert_eq!(canvas.layer_count(), 0);
    }

    #[test]
    fn canvas_new_rejects_zero_width() {
        let result = Canvas::new(0, 100, black());
        assert!(matches!(result, Err(EngineError::InvalidDimensions)));
    }

    #[test]
    fn canvas_new_rejects_zero_height() {
        let result = Canvas::new(100, 0, black());
        assert!(matches!(result, Err(EngineError::InvalidDimensions)));
    }

    #[test]
    fn canvas_new_rejects_overflow_dimensions() {
        let result = Canvas::new(usize::MAX, 2, black());
        assert!(matches!(result, Err(EngineError::InvalidDimensions)));
    }

    #[test]
    fn canvas_set_background() {
        let mut canvas = Canvas::new(100, 100, black()).unwrap();
        canvas.set_background(white());
        assert_eq!(canvas.background(), white());
    }

    // ── Layer add/remove tests ─────────────────────────────────────

    #[test]
    fn canvas_add_layer_adds_to_top() {
        let mut canvas = Canvas::new(100, 100, black()).unwrap();
        canvas
            .add_layer(Layer::new("bottom", ContentType::Field))
            .unwrap();
        canvas
            .add_layer(Layer::new("top", ContentType::Particles))
            .unwrap();
        assert_eq!(canvas.layer_count(), 2);
        assert_eq!(canvas.layers()[0].name(), "bottom");
        assert_eq!(canvas.layers()[1].name(), "top");
    }

    #[test]
    fn canvas_add_duplicate_layer_returns_error() {
        let mut canvas = Canvas::new(100, 100, black()).unwrap();
        canvas
            .add_layer(Layer::new("bg", ContentType::Field))
            .unwrap();
        let result = canvas.add_layer(Layer::new("bg", ContentType::Particles));
        assert!(matches!(result, Err(EngineError::DuplicateLayerName(_))));
    }

    #[test]
    fn canvas_remove_layer_returns_layer() {
        let mut canvas = Canvas::new(100, 100, black()).unwrap();
        canvas
            .add_layer(Layer::new("fg", ContentType::Shapes))
            .unwrap();
        let removed = canvas.remove_layer("fg").unwrap();
        assert_eq!(removed.name(), "fg");
        assert_eq!(canvas.layer_count(), 0);
    }

    #[test]
    fn canvas_remove_nonexistent_layer_returns_error() {
        let mut canvas = Canvas::new(100, 100, black()).unwrap();
        let result = canvas.remove_layer("nope");
        assert!(matches!(result, Err(EngineError::LayerNotFound(_))));
    }

    #[test]
    fn canvas_remove_preserves_order() {
        let mut canvas = Canvas::new(100, 100, black()).unwrap();
        canvas
            .add_layer(Layer::new("a", ContentType::Field))
            .unwrap();
        canvas
            .add_layer(Layer::new("b", ContentType::Particles))
            .unwrap();
        canvas
            .add_layer(Layer::new("c", ContentType::Shapes))
            .unwrap();
        canvas.remove_layer("b").unwrap();
        let names: Vec<&str> = canvas.layers().iter().map(|l| l.name()).collect();
        assert_eq!(names, vec!["a", "c"]);
    }

    // ── Layer lookup tests ─────────────────────────────────────────

    #[test]
    fn canvas_layer_finds_by_name() {
        let mut canvas = Canvas::new(100, 100, black()).unwrap();
        canvas
            .add_layer(Layer::new("bg", ContentType::Field))
            .unwrap();
        let layer = canvas.layer("bg").unwrap();
        assert_eq!(layer.name(), "bg");
    }

    #[test]
    fn canvas_layer_not_found() {
        let canvas = Canvas::new(100, 100, black()).unwrap();
        let result = canvas.layer("missing");
        assert!(matches!(result, Err(EngineError::LayerNotFound(_))));
    }

    #[test]
    fn canvas_layer_mut_modifies_layer() {
        let mut canvas = Canvas::new(100, 100, black()).unwrap();
        canvas
            .add_layer(Layer::new("fx", ContentType::Particles))
            .unwrap();
        canvas
            .layer_mut("fx")
            .unwrap()
            .set_blend_mode(BlendMode::Screen);
        assert_eq!(canvas.layer("fx").unwrap().blend_mode(), BlendMode::Screen);
    }

    // ── Reorder tests ──────────────────────────────────────────────

    #[test]
    fn move_layer_up_swaps_with_above() {
        let mut canvas = Canvas::new(100, 100, black()).unwrap();
        canvas
            .add_layer(Layer::new("a", ContentType::Field))
            .unwrap();
        canvas
            .add_layer(Layer::new("b", ContentType::Particles))
            .unwrap();
        canvas
            .add_layer(Layer::new("c", ContentType::Shapes))
            .unwrap();
        canvas.move_layer_up("a").unwrap();
        let names: Vec<&str> = canvas.layers().iter().map(|l| l.name()).collect();
        assert_eq!(names, vec!["b", "a", "c"]);
    }

    #[test]
    fn move_layer_up_at_top_is_noop() {
        let mut canvas = Canvas::new(100, 100, black()).unwrap();
        canvas
            .add_layer(Layer::new("a", ContentType::Field))
            .unwrap();
        canvas
            .add_layer(Layer::new("b", ContentType::Particles))
            .unwrap();
        canvas.move_layer_up("b").unwrap();
        let names: Vec<&str> = canvas.layers().iter().map(|l| l.name()).collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn move_layer_down_swaps_with_below() {
        let mut canvas = Canvas::new(100, 100, black()).unwrap();
        canvas
            .add_layer(Layer::new("a", ContentType::Field))
            .unwrap();
        canvas
            .add_layer(Layer::new("b", ContentType::Particles))
            .unwrap();
        canvas
            .add_layer(Layer::new("c", ContentType::Shapes))
            .unwrap();
        canvas.move_layer_down("c").unwrap();
        let names: Vec<&str> = canvas.layers().iter().map(|l| l.name()).collect();
        assert_eq!(names, vec!["a", "c", "b"]);
    }

    #[test]
    fn move_layer_down_at_bottom_is_noop() {
        let mut canvas = Canvas::new(100, 100, black()).unwrap();
        canvas
            .add_layer(Layer::new("a", ContentType::Field))
            .unwrap();
        canvas
            .add_layer(Layer::new("b", ContentType::Particles))
            .unwrap();
        canvas.move_layer_down("a").unwrap();
        let names: Vec<&str> = canvas.layers().iter().map(|l| l.name()).collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn move_layer_to_repositions_correctly() {
        let mut canvas = Canvas::new(100, 100, black()).unwrap();
        canvas
            .add_layer(Layer::new("a", ContentType::Field))
            .unwrap();
        canvas
            .add_layer(Layer::new("b", ContentType::Particles))
            .unwrap();
        canvas
            .add_layer(Layer::new("c", ContentType::Shapes))
            .unwrap();
        // Move "c" from top (index 2) to bottom (index 0)
        canvas.move_layer_to("c", 0).unwrap();
        let names: Vec<&str> = canvas.layers().iter().map(|l| l.name()).collect();
        assert_eq!(names, vec!["c", "a", "b"]);
    }

    #[test]
    fn move_layer_to_beyond_end_moves_to_top() {
        let mut canvas = Canvas::new(100, 100, black()).unwrap();
        canvas
            .add_layer(Layer::new("a", ContentType::Field))
            .unwrap();
        canvas
            .add_layer(Layer::new("b", ContentType::Particles))
            .unwrap();
        canvas.move_layer_to("a", 100).unwrap();
        let names: Vec<&str> = canvas.layers().iter().map(|l| l.name()).collect();
        assert_eq!(names, vec!["b", "a"]);
    }

    #[test]
    fn move_layer_to_nonexistent_returns_error() {
        let mut canvas = Canvas::new(100, 100, black()).unwrap();
        let result = canvas.move_layer_to("nope", 0);
        assert!(matches!(result, Err(EngineError::LayerNotFound(_))));
    }

    #[test]
    fn move_layer_up_nonexistent_returns_error() {
        let mut canvas = Canvas::new(100, 100, black()).unwrap();
        let result = canvas.move_layer_up("nope");
        assert!(matches!(result, Err(EngineError::LayerNotFound(_))));
    }

    #[test]
    fn move_layer_down_nonexistent_returns_error() {
        let mut canvas = Canvas::new(100, 100, black()).unwrap();
        let result = canvas.move_layer_down("nope");
        assert!(matches!(result, Err(EngineError::LayerNotFound(_))));
    }

    // ── Full Canvas serde round-trip ───────────────────────────────

    #[test]
    fn canvas_serde_round_trip() {
        let mut canvas = Canvas::new(512, 512, Srgb::from_hex("#020210").unwrap()).unwrap();
        canvas
            .add_layer(
                Layer::new("deep", ContentType::Particles)
                    .with_blend_mode(BlendMode::Additive)
                    .with_opacity(0.9),
            )
            .unwrap();
        canvas
            .add_layer(
                Layer::new("shapes", ContentType::Shapes)
                    .with_blend_mode(BlendMode::Multiply)
                    .with_visible(false),
            )
            .unwrap();

        let json = serde_json::to_string_pretty(&canvas).unwrap();
        let deserialized: Canvas = serde_json::from_str(&json).unwrap();
        assert_eq!(canvas, deserialized);
    }

    #[test]
    fn canvas_json_contains_expected_structure() {
        let mut canvas = Canvas::new(256, 256, black()).unwrap();
        canvas
            .add_layer(Layer::new("bg", ContentType::Field))
            .unwrap();

        let json = serde_json::to_string(&canvas).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["width"], 256);
        assert_eq!(value["height"], 256);
        assert_eq!(value["background"], "#000000");
        assert!(value["layers"].is_array());
        assert_eq!(value["layers"][0]["name"], "bg");
        assert_eq!(value["layers"][0]["content_type"], "field");
    }

    // ── Iteration tests ────────────────────────────────────────────

    #[test]
    fn layers_iter_yields_bottom_to_top() {
        let mut canvas = Canvas::new(100, 100, black()).unwrap();
        canvas
            .add_layer(Layer::new("bottom", ContentType::Field))
            .unwrap();
        canvas
            .add_layer(Layer::new("middle", ContentType::Particles))
            .unwrap();
        canvas
            .add_layer(Layer::new("top", ContentType::Shapes))
            .unwrap();
        let names: Vec<&str> = canvas.layers().iter().map(|l| l.name()).collect();
        assert_eq!(names, vec!["bottom", "middle", "top"]);
    }

    #[test]
    fn layers_iter_empty_canvas() {
        let canvas = Canvas::new(100, 100, black()).unwrap();
        assert_eq!(canvas.layers().iter().count(), 0);
    }

    // ── Property-based tests ───────────────────────────────────────

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn opacity_always_clamped(value in -10.0_f64..=10.0) {
                let mut layer = Layer::new("test", ContentType::Particles);
                layer.set_opacity(value);
                prop_assert!(layer.opacity() >= 0.0);
                prop_assert!(layer.opacity() <= 1.0);
            }

            #[test]
            fn add_then_remove_restores_count(
                name1 in "[a-z]{1,8}",
                name2 in "[a-z]{1,8}",
            ) {
                // Ensure distinct names
                prop_assume!(name1 != name2);

                let mut canvas = Canvas::new(100, 100, Srgb { r: 0.0, g: 0.0, b: 0.0 }).unwrap();
                canvas.add_layer(Layer::new(&name1, ContentType::Particles)).unwrap();
                canvas.add_layer(Layer::new(&name2, ContentType::Shapes)).unwrap();
                prop_assert_eq!(canvas.layer_count(), 2);

                canvas.remove_layer(&name1).unwrap();
                prop_assert_eq!(canvas.layer_count(), 1);

                canvas.remove_layer(&name2).unwrap();
                prop_assert_eq!(canvas.layer_count(), 0);
            }

            #[test]
            fn duplicate_name_always_rejected(name in "[a-z]{1,8}") {
                let mut canvas = Canvas::new(100, 100, Srgb { r: 0.0, g: 0.0, b: 0.0 }).unwrap();
                canvas.add_layer(Layer::new(&name, ContentType::Field)).unwrap();
                let result = canvas.add_layer(Layer::new(&name, ContentType::Particles));
                prop_assert!(result.is_err());
            }
        }
    }
}
