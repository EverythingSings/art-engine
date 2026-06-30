//! Layer compositing blend-mode fragment shaders.
//!
//! Every blend mode is expressed as a shader pass that samples the layer
//! being composited (`u_layer`) and the running composite below it
//! (`u_composite`), and writes back the blended result. Using shaders for
//! all five modes — including `Normal` and `Additive`, which could in
//! principle ride on hardware `glBlendFunc` — keeps the per-layer dispatch
//! a single uniform code path. The hardware fast path is a future
//! optimisation, not a correctness requirement.
//!
//! ## Uniforms (shared by all sources)
//!
//! | Name | Type | Default | Description |
//! |------|------|---------|-------------|
//! | `u_layer` | `sampler2D` | — | The layer being composited (top) |
//! | `u_composite` | `sampler2D` | — | Current composite below this layer |
//! | `u_opacity` | `float` | 1.0 | Layer opacity in `[0, 1]` |

/// Normal: alpha-weighted `mix(composite, layer, layer.a * opacity)`.
/// Identity for transparent layers; full overwrite for opaque ones.
pub const NORMAL_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform sampler2D u_layer;
uniform sampler2D u_composite;
uniform float u_opacity;

void main() {
    vec4 top = texture(u_layer, v_uv);
    vec4 bot = texture(u_composite, v_uv);
    float a = top.a * u_opacity;
    vec3 mixed = mix(bot.rgb, top.rgb, a);
    fragColor = vec4(mixed, max(bot.a, a));
}
"#;

/// Additive: `composite + layer * opacity`. Useful for glow / light.
pub const ADDITIVE_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform sampler2D u_layer;
uniform sampler2D u_composite;
uniform float u_opacity;

void main() {
    vec4 top = texture(u_layer, v_uv);
    vec4 bot = texture(u_composite, v_uv);
    float a = top.a * u_opacity;
    vec3 sum = bot.rgb + top.rgb * a;
    fragColor = vec4(sum, max(bot.a, a));
}
"#;

/// Multiply: `composite * layer`. Darkens; identity for white layers.
pub const MULTIPLY_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform sampler2D u_layer;
uniform sampler2D u_composite;
uniform float u_opacity;

void main() {
    vec4 top = texture(u_layer, v_uv);
    vec4 bot = texture(u_composite, v_uv);
    vec3 blended = bot.rgb * top.rgb;
    vec3 mixed = mix(bot.rgb, blended, top.a * u_opacity);
    fragColor = vec4(mixed, max(bot.a, top.a * u_opacity));
}
"#;

/// Screen: `1 - (1 - composite) * (1 - layer)`. Lightens; identity for black layers.
pub const SCREEN_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform sampler2D u_layer;
uniform sampler2D u_composite;
uniform float u_opacity;

void main() {
    vec4 top = texture(u_layer, v_uv);
    vec4 bot = texture(u_composite, v_uv);
    vec3 blended = vec3(1.0) - (vec3(1.0) - bot.rgb) * (vec3(1.0) - top.rgb);
    vec3 mixed = mix(bot.rgb, blended, top.a * u_opacity);
    fragColor = vec4(mixed, max(bot.a, top.a * u_opacity));
}
"#;

/// Overlay: multiply where the composite is dark, screen where it is light.
pub const OVERLAY_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform sampler2D u_layer;
uniform sampler2D u_composite;
uniform float u_opacity;

vec3 overlay(vec3 a, vec3 b) {
    vec3 mul = 2.0 * a * b;
    vec3 scr = vec3(1.0) - 2.0 * (vec3(1.0) - a) * (vec3(1.0) - b);
    return mix(mul, scr, step(vec3(0.5), a));
}

void main() {
    vec4 top = texture(u_layer, v_uv);
    vec4 bot = texture(u_composite, v_uv);
    vec3 blended = overlay(bot.rgb, top.rgb);
    vec3 mixed = mix(bot.rgb, blended, top.a * u_opacity);
    fragColor = vec4(mixed, max(bot.a, top.a * u_opacity));
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCES: [&str; 5] = [
        NORMAL_SOURCE,
        ADDITIVE_SOURCE,
        MULTIPLY_SOURCE,
        SCREEN_SOURCE,
        OVERLAY_SOURCE,
    ];

    #[test]
    fn all_sources_share_uniform_interface() {
        for src in SOURCES {
            assert!(src.contains("#version 300 es"));
            assert!(src.contains("uniform sampler2D u_layer"));
            assert!(src.contains("uniform sampler2D u_composite"));
            assert!(src.contains("uniform float u_opacity"));
        }
    }

    #[test]
    fn all_sources_output_frag_color() {
        for src in SOURCES {
            assert!(src.contains("out vec4 fragColor"));
            assert!(src.contains("fragColor ="));
        }
    }

    #[test]
    fn sources_are_distinct() {
        for (i, a) in SOURCES.iter().enumerate() {
            for b in SOURCES.iter().skip(i + 1) {
                assert_ne!(a, b);
            }
        }
    }
}
