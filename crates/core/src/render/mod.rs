//! WebGL2 rendering infrastructure.
//!
//! This module is only available when the `render` feature is enabled.
//! It provides shader compilation, texture management, render targets
//! with ping-pong double buffering, and GPU context initialization.
//!
//! # Module overview
//!
//! - [`ping_pong`] -- Index tracking for double-buffered render targets.
//! - [`shader`] -- Shader compilation, linking, and error formatting.
//! - [`fullscreen`] -- Fullscreen triangle vertex shader constant.
//! - [`texture`] -- Texture configuration and creation helpers.
//! - [`target`] -- FBO + texture render targets.
//! - [`context`] -- GPU context wrapper with capability detection.
//! - [`uniforms`] -- Typed uniform setters and JSON-driven uniform application.

pub mod context;
pub mod fullscreen;
#[cfg(all(feature = "native-gpu", not(target_arch = "wasm32")))]
pub mod headless;
pub mod ping_pong;
pub mod pipeline;
pub mod shader;
pub mod target;
pub mod texture;
pub mod uniforms;

// Re-export key types at the render module level for convenience.
pub use context::GpuContext;
pub use fullscreen::FULLSCREEN_VERTEX_SHADER;
pub use ping_pong::PingPong;
pub use shader::{compile_program, compile_shader, format_shader_error, link_program, ShaderError};
pub use target::RenderTarget;
pub use texture::{create_texture, pixel_type_for_format, TextureConfig};
pub use uniforms::{apply_params, UniformDefault, UniformSchema, Uniforms};
