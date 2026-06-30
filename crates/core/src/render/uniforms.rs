//! Typed uniform setters for shader programs.
//!
//! Wraps glow's name-based uniform API behind typed methods that hide the
//! `Option<UniformLocation>` lookup. Missing uniforms are treated as
//! benign (the GLSL compiler often optimises away unused uniforms), so
//! the setters are best-effort: they silently no-op on miss.
//!
//! [`apply_params`] is the JSON adapter — given a [`UniformSchema`]
//! describing each uniform's type and default, it reads keys from a
//! `serde_json::Value` (the `params` field of a `ShaderEffectDesc`)
//! and applies them.

use serde_json::Value;

use crate::params::param_f64;

/// A uniform's type and default value.
///
/// Used by [`apply_params`] both to decide which `glow` setter to call
/// and to fill in absent JSON keys.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UniformDefault {
    /// `uniform float` — JSON `number`.
    F32(f32),
    /// `uniform vec2` — JSON `[x, y]` array.
    Vec2([f32; 2]),
    /// `uniform vec3` — JSON `[r, g, b]` or `[x, y, z]` array.
    Vec3([f32; 3]),
    /// `uniform vec4` — JSON `[x, y, z, w]` array.
    Vec4([f32; 4]),
    /// `uniform int` — JSON integer.
    I32(i32),
}

/// A static list of `(uniform_name, default)` pairs describing one shader's
/// JSON-controllable parameters. Sampler uniforms are bound separately
/// in the pipeline and are *not* part of the schema.
pub type UniformSchema = &'static [(&'static str, UniformDefault)];

/// Wraps a `glow::Context` + `glow::Program` for ergonomic uniform setting.
///
/// The program must be active (`gl.use_program(Some(program))`) before
/// the setters take effect.
pub struct Uniforms<'a> {
    gl: &'a glow::Context,
    program: glow::Program,
}

impl<'a> Uniforms<'a> {
    /// Creates a new `Uniforms` view over the given program.
    pub fn new(gl: &'a glow::Context, program: glow::Program) -> Self {
        Self { gl, program }
    }

    /// Sets a `float` uniform; silently no-ops if the uniform is missing.
    pub fn try_set_f32(&self, name: &str, value: f32) {
        if let Some(loc) = self.location(name) {
            // SAFETY: glow GL calls are unsafe. The location was just
            // returned by glow for the active program; the value is plain
            // POD; no unsafety beyond what glow itself wraps.
            #[allow(unsafe_code)]
            unsafe {
                use glow::HasContext;
                self.gl.uniform_1_f32(Some(&loc), value);
            }
        }
    }

    /// Sets a `vec2` uniform from a `[f32; 2]`.
    pub fn try_set_vec2(&self, name: &str, v: [f32; 2]) {
        if let Some(loc) = self.location(name) {
            #[allow(unsafe_code)]
            unsafe {
                use glow::HasContext;
                self.gl.uniform_2_f32(Some(&loc), v[0], v[1]);
            }
        }
    }

    /// Sets a `vec3` uniform from a `[f32; 3]`.
    pub fn try_set_vec3(&self, name: &str, v: [f32; 3]) {
        if let Some(loc) = self.location(name) {
            #[allow(unsafe_code)]
            unsafe {
                use glow::HasContext;
                self.gl.uniform_3_f32(Some(&loc), v[0], v[1], v[2]);
            }
        }
    }

    /// Sets a `vec4` uniform from a `[f32; 4]`.
    pub fn try_set_vec4(&self, name: &str, v: [f32; 4]) {
        if let Some(loc) = self.location(name) {
            #[allow(unsafe_code)]
            unsafe {
                use glow::HasContext;
                self.gl.uniform_4_f32(Some(&loc), v[0], v[1], v[2], v[3]);
            }
        }
    }

    /// Sets an `int` (or sampler binding) uniform.
    pub fn try_set_i32(&self, name: &str, value: i32) {
        if let Some(loc) = self.location(name) {
            #[allow(unsafe_code)]
            unsafe {
                use glow::HasContext;
                self.gl.uniform_1_i32(Some(&loc), value);
            }
        }
    }

    /// Binds a sampler uniform to a texture unit (e.g. `0` for `GL_TEXTURE0`).
    pub fn try_set_sampler(&self, name: &str, texture_unit: i32) {
        self.try_set_i32(name, texture_unit);
    }

    fn location(&self, name: &str) -> Option<glow::UniformLocation> {
        // SAFETY: glow GL calls are unsafe; the program handle is owned
        // by the caller and assumed valid for the lifetime of this view.
        #[allow(unsafe_code)]
        unsafe {
            use glow::HasContext;
            self.gl.get_uniform_location(self.program, name)
        }
    }
}

/// Reads each uniform from `params` according to `schema` and applies it
/// via `uniforms`. Missing or malformed JSON keys fall back to the schema's
/// declared default. Missing uniforms in the linked program are silently
/// skipped (treated as optimised-away).
pub fn apply_params(uniforms: &Uniforms, params: &Value, schema: UniformSchema) {
    for (name, default) in schema {
        match *default {
            UniformDefault::F32(d) => {
                let v = param_f64(params, name, d as f64) as f32;
                uniforms.try_set_f32(name, v);
            }
            UniformDefault::I32(d) => {
                let v = read_i32(params, name, d);
                uniforms.try_set_i32(name, v);
            }
            UniformDefault::Vec2(d) => {
                uniforms.try_set_vec2(name, read_vec_n::<2>(params, name, d));
            }
            UniformDefault::Vec3(d) => {
                uniforms.try_set_vec3(name, read_vec_n::<3>(params, name, d));
            }
            UniformDefault::Vec4(d) => {
                uniforms.try_set_vec4(name, read_vec_n::<4>(params, name, d));
            }
        }
    }
}

fn read_i32(params: &Value, name: &str, default: i32) -> i32 {
    params
        .get(name)
        .and_then(Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
        .unwrap_or(default)
}

fn read_vec_n<const N: usize>(params: &Value, name: &str, default: [f32; N]) -> [f32; N] {
    let arr = params.get(name).and_then(Value::as_array);
    let Some(a) = arr else {
        return default;
    };
    if a.len() != N {
        return default;
    }
    let mut out = default;
    for i in 0..N {
        if let Some(x) = a[i].as_f64() {
            out[i] = x as f32;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- read_i32 --

    #[test]
    fn read_i32_extracts_existing_integer() {
        let p = json!({"k": 7});
        assert_eq!(read_i32(&p, "k", 0), 7);
    }

    #[test]
    fn read_i32_returns_default_when_missing() {
        let p = json!({});
        assert_eq!(read_i32(&p, "k", 42), 42);
    }

    #[test]
    fn read_i32_returns_default_for_float() {
        let p = json!({"k": 1.5});
        assert_eq!(read_i32(&p, "k", 99), 99);
    }

    #[test]
    fn read_i32_returns_default_for_oob() {
        let big: i64 = i64::from(i32::MAX) + 100;
        let p = json!({"k": big});
        assert_eq!(read_i32(&p, "k", 5), 5);
    }

    // -- read_vec_n --

    #[test]
    fn read_vec2_from_array() {
        let p = json!({"k": [1.0, 2.5]});
        assert_eq!(read_vec_n::<2>(&p, "k", [0.0, 0.0]), [1.0, 2.5]);
    }

    #[test]
    fn read_vec3_from_array() {
        let p = json!({"k": [0.1, 0.2, 0.3]});
        assert_eq!(
            read_vec_n::<3>(&p, "k", [9.0, 9.0, 9.0]),
            [0.1f32, 0.2, 0.3],
        );
    }

    #[test]
    fn read_vec_n_returns_default_for_wrong_arity() {
        let p = json!({"k": [1.0, 2.0]});
        assert_eq!(read_vec_n::<3>(&p, "k", [9.0, 9.0, 9.0]), [9.0, 9.0, 9.0]);
    }

    #[test]
    fn read_vec_n_returns_default_for_missing_key() {
        let p = json!({});
        assert_eq!(read_vec_n::<2>(&p, "k", [3.0, 4.0]), [3.0, 4.0]);
    }

    #[test]
    fn read_vec_n_falls_back_per_element_for_non_numeric_entries() {
        // Non-numeric entries should keep the default for that slot only.
        let p = json!({"k": [1.0, "nope", 3.0]});
        assert_eq!(read_vec_n::<3>(&p, "k", [9.0, 9.0, 9.0]), [1.0, 9.0, 3.0],);
    }

    // -- UniformDefault sanity --

    #[test]
    fn uniform_default_is_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<UniformDefault>();
    }

    #[test]
    fn uniform_default_partial_eq() {
        assert_eq!(UniformDefault::F32(1.0), UniformDefault::F32(1.0));
        assert_ne!(UniformDefault::F32(1.0), UniformDefault::F32(2.0));
        assert_ne!(UniformDefault::F32(1.0), UniformDefault::I32(1),);
    }
}
