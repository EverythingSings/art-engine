//! Headless EGL context for native (non-WASM) GL rendering.
//!
//! Creates an EGL display + GLES 3.x context with no window or surface
//! attached, then wraps the result in a [`GpuContext`]. All actual
//! rendering happens to user-allocated framebuffer objects (`RenderTarget`s)
//! — the default framebuffer is never used or sampled.
//!
//! Available only when the `native-gpu` feature is enabled and the target
//! is not `wasm32`. On WASM, the browser-supplied `WebGl2RenderingContext`
//! is used instead (see `crates/wasm`).
//!
//! # Platform notes
//!
//! - Linux + Mesa: works via the standard EGL platform. WSLg ships Mesa
//!   with software fallback (llvmpipe), so this works inside WSL2 even
//!   without GPU passthrough.
//! - macOS / Windows native: untested. macOS lacks EGL by default;
//!   Windows requires ANGLE or a vendor EGL implementation.

#![cfg(all(feature = "native-gpu", not(target_arch = "wasm32")))]
#![allow(unsafe_code)]

use khronos_egl as egl;
use thiserror::Error;

use crate::render::GpuContext;

/// Errors produced while bringing up a headless GPU context.
#[derive(Debug, Error)]
pub enum HeadlessError {
    /// The EGL shared library could not be loaded.
    #[error("failed to load libEGL: {0}")]
    LoadFailed(String),
    /// EGL initialisation or query failed.
    #[error("EGL initialisation failed: {0}")]
    Init(String),
    /// `eglChooseConfig` returned no matching configurations.
    #[error("no EGL configuration matched the requested attributes")]
    NoConfig,
    /// An EGL operation reported an error.
    #[error("EGL error: {0:?}")]
    Egl(#[from] egl::Error),
    /// The wrapping `GpuContext::new` rejected the context (e.g. missing extension).
    #[error("GpuContext rejected the EGL context: {0}")]
    Context(String),
}

/// A live headless GPU context, holding the EGL handles needed to keep it
/// current. Drop this to release the EGL context and terminate the display.
pub struct HeadlessGpu {
    /// The wrapped `GpuContext` ready for rendering.
    context: GpuContext,
    egl: egl::DynamicInstance<egl::EGL1_4>,
    display: egl::Display,
    egl_context: egl::Context,
}

impl HeadlessGpu {
    /// Returns the wrapped `GpuContext`.
    pub fn context(&self) -> &GpuContext {
        &self.context
    }

    /// Returns a mutable reference to the wrapped `GpuContext`.
    pub fn context_mut(&mut self) -> &mut GpuContext {
        &mut self.context
    }
}

impl Drop for HeadlessGpu {
    fn drop(&mut self) {
        // Best-effort cleanup. If any of these fail we still want to
        // continue dropping; logging would require a logger we don't carry.
        let _ = self.egl.make_current(self.display, None, None, None);
        let _ = self.egl.destroy_context(self.display, self.egl_context);
        let _ = self.egl.terminate(self.display);
    }
}

/// Creates a headless EGL/GLES 3 context and wraps it in a `GpuContext`.
///
/// The returned `HeadlessGpu` keeps the EGL context current on the calling
/// thread for the duration of its lifetime. All GL calls must happen on
/// the same thread that called this function.
pub fn create_headless_context() -> Result<HeadlessGpu, HeadlessError> {
    // Load libEGL dynamically. khronos-egl's `dynamic` feature pulls in
    // `libloading` and searches the standard library paths.
    let egl = unsafe {
        egl::DynamicInstance::<egl::EGL1_4>::load_required()
            .map_err(|e| HeadlessError::LoadFailed(format!("{e:?}")))?
    };

    // SAFETY: get_display is unsafe because it returns a handle whose
    // validity depends on the EGL implementation. We immediately validate
    // by calling initialize() and bail out on failure.
    let display = unsafe {
        egl.get_display(egl::DEFAULT_DISPLAY)
            .ok_or_else(|| HeadlessError::Init("no default EGL display".into()))?
    };
    egl.initialize(display)?;

    egl.bind_api(egl::OPENGL_ES_API)?;

    // Pixel buffer surface type so we can create the context without a
    // window. We never actually use the pbuffer — all rendering targets
    // are user-managed FBOs — but EGL refuses GLES contexts without a
    // valid surface_type bit on the chosen config.
    let attribs: [i32; 13] = [
        egl::SURFACE_TYPE,
        egl::PBUFFER_BIT,
        egl::RENDERABLE_TYPE,
        egl::OPENGL_ES3_BIT,
        egl::RED_SIZE,
        8,
        egl::GREEN_SIZE,
        8,
        egl::BLUE_SIZE,
        8,
        egl::ALPHA_SIZE,
        8,
        egl::NONE,
    ];
    let config = egl
        .choose_first_config(display, &attribs)?
        .ok_or(HeadlessError::NoConfig)?;

    // Request GLES 3.0. The shader library is authored against
    // `#version 300 es`, which maps to GLES 3.0. Higher versions are
    // backwards-compatible.
    let context_attribs: [i32; 3] = [egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE];
    let egl_context = egl.create_context(display, config, None, &context_attribs)?;

    egl.make_current(display, None, None, Some(egl_context))?;

    let gl = unsafe {
        glow::Context::from_loader_function(|name| {
            egl.get_proc_address(name)
                .map(|p| p as *const _)
                .unwrap_or(std::ptr::null())
        })
    };

    let context = GpuContext::new(gl).map_err(HeadlessError::Context)?;

    Ok(HeadlessGpu {
        context,
        egl,
        display,
        egl_context,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This test attempts a real headless GL context. On systems without
    /// a working EGL (no WSLg, no Mesa, no display server), it is skipped
    /// rather than failed — set `ART_ENGINE_REQUIRE_GL=1` in CI to upgrade
    /// missing GL to a hard failure.
    #[test]
    fn create_headless_context_succeeds_or_is_skipped() {
        match create_headless_context() {
            Ok(gpu) => {
                // Sanity: the wrapped context is real and the required
                // extension is present (GpuContext::new enforces this).
                assert!(gpu.context().supports_color_buffer_float());
            }
            Err(e) => {
                if std::env::var("ART_ENGINE_REQUIRE_GL").is_ok() {
                    panic!("headless GL required by env but failed: {e}");
                }
                eprintln!("headless GL unavailable in this environment: {e} — skipping");
            }
        }
    }
}
