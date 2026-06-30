//! Bloom post-processing shaders.
//!
//! Bloom is a multi-pass effect that extracts bright pixels, blurs them,
//! and adds the result back to the original image for a glow effect.
//! Three fragment shaders work together:
//!
//! 1. **Threshold** — extract pixels above a brightness cutoff
//! 2. **Blur** — separable Gaussian blur (run twice: horizontal, then vertical)
//! 3. **Combine** — additively blend the blurred bloom with the original
//!
//! ## Pipeline
//!
//! ```text
//! original ──┬──────────────────────────────► combine ──► output
//!            │                                   ▲
//!            └──► threshold ──► blur_h ──► blur_v ┘
//! ```
//!
//! The blur pass uses a 9-tap Gaussian kernel. For wider bloom, run the
//! blur at progressively lower resolutions (downsample the threshold output
//! before blurring, upsample before combining).

/// GLSL ES 3.0 fragment shader: brightness threshold extraction.
///
/// Extracts pixels whose luminance exceeds `u_threshold`, with a soft
/// knee controlled by `u_soft_knee`. Below threshold, output is black.
/// Above threshold, output retains original color scaled by excess
/// brightness.
pub const THRESHOLD_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform sampler2D u_texture;
uniform float u_threshold;
uniform float u_soft_knee;

void main() {
    vec4 color = texture(u_texture, v_uv);

    // Perceived luminance (Rec. 709 coefficients).
    float luminance = dot(color.rgb, vec3(0.2126, 0.7152, 0.0722));

    // Soft threshold curve — avoids harsh cutoff artifacts.
    float knee = u_threshold * u_soft_knee;
    float soft = luminance - u_threshold + knee;
    soft = clamp(soft / (2.0 * knee + 0.0001), 0.0, 1.0);
    soft = soft * soft;

    float contribution = max(soft, step(u_threshold, luminance));
    fragColor = color * contribution;
}
"#;

/// GLSL ES 3.0 fragment shader: separable 9-tap Gaussian blur.
///
/// Set `u_direction` to `(1/width, 0)` for horizontal blur or
/// `(0, 1/height)` for vertical blur. Run both passes to get a
/// full 2D Gaussian. The kernel weights approximate sigma ~2.0.
pub const BLUR_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform sampler2D u_texture;
uniform vec2 u_direction;

// 9-tap Gaussian weights (sigma ~2.0, normalized).
const float weight[5] = float[](
    0.2270270270,
    0.1945945946,
    0.1216216216,
    0.0540540541,
    0.0162162162
);

void main() {
    vec4 result = texture(u_texture, v_uv) * weight[0];

    for (int i = 1; i < 5; i++) {
        vec2 offset = u_direction * float(i);
        result += texture(u_texture, v_uv + offset) * weight[i];
        result += texture(u_texture, v_uv - offset) * weight[i];
    }

    fragColor = result;
}
"#;

/// GLSL ES 3.0 fragment shader: additive bloom combine.
///
/// Blends the blurred bloom texture into the original scene with
/// a controllable intensity. Uses additive blending in HDR space
/// (RGBA16F), so values can exceed 1.0 before tonemapping.
pub const COMBINE_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform sampler2D u_texture;
uniform sampler2D u_bloom;
uniform float u_intensity;

void main() {
    vec4 original = texture(u_texture, v_uv);
    vec4 bloom = texture(u_bloom, v_uv);
    fragColor = original + bloom * u_intensity;
}
"#;

/// Name used in the shader registry.
pub const NAME: &str = "bloom";

#[cfg(test)]
mod tests {
    use super::*;

    // --- Threshold shader ---

    #[test]
    fn threshold_source_is_glsl_es_300() {
        assert!(THRESHOLD_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn threshold_source_uses_rec709_luminance() {
        // Rec. 709 coefficients for perceived brightness.
        assert!(THRESHOLD_SOURCE.contains("0.2126"));
        assert!(THRESHOLD_SOURCE.contains("0.7152"));
        assert!(THRESHOLD_SOURCE.contains("0.0722"));
    }

    #[test]
    fn threshold_source_has_soft_knee() {
        assert!(
            THRESHOLD_SOURCE.contains("u_soft_knee"),
            "expected soft knee uniform for smooth threshold transition"
        );
    }

    #[test]
    fn threshold_source_outputs_frag_color() {
        assert!(THRESHOLD_SOURCE.contains("out vec4 fragColor"));
        assert!(THRESHOLD_SOURCE.contains("fragColor ="));
    }

    // --- Blur shader ---

    #[test]
    fn blur_source_is_glsl_es_300() {
        assert!(BLUR_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn blur_source_is_separable_via_direction() {
        assert!(
            BLUR_SOURCE.contains("uniform vec2 u_direction"),
            "expected direction uniform for separable blur"
        );
    }

    #[test]
    fn blur_source_samples_symmetrically() {
        // A separable Gaussian must sample in both + and - directions.
        assert!(BLUR_SOURCE.contains("v_uv + offset"));
        assert!(BLUR_SOURCE.contains("v_uv - offset"));
    }

    #[test]
    fn blur_weights_sum_to_approximately_one() {
        let weights: [f64; 5] = [
            0.2270270270,
            0.1945945946,
            0.1216216216,
            0.0540540541,
            0.0162162162,
        ];
        // center + 2 * (sum of tails)
        let total = weights[0] + 2.0 * weights[1..].iter().sum::<f64>();
        assert!(
            (total - 1.0).abs() < 0.001,
            "Gaussian weights should sum to ~1.0, got {total}"
        );
    }

    #[test]
    fn blur_source_outputs_frag_color() {
        assert!(BLUR_SOURCE.contains("out vec4 fragColor"));
        assert!(BLUR_SOURCE.contains("fragColor ="));
    }

    // --- Combine shader ---

    #[test]
    fn combine_source_is_glsl_es_300() {
        assert!(COMBINE_SOURCE.contains("#version 300 es"));
    }

    #[test]
    fn combine_source_blends_two_textures() {
        assert!(COMBINE_SOURCE.contains("uniform sampler2D u_texture"));
        assert!(COMBINE_SOURCE.contains("uniform sampler2D u_bloom"));
    }

    #[test]
    fn combine_source_has_intensity_control() {
        assert!(
            COMBINE_SOURCE.contains("u_intensity"),
            "expected intensity uniform for bloom strength"
        );
    }

    #[test]
    fn combine_source_uses_additive_blending() {
        assert!(
            COMBINE_SOURCE.contains("original + bloom"),
            "expected additive blend of bloom into original"
        );
    }

    #[test]
    fn combine_source_outputs_frag_color() {
        assert!(COMBINE_SOURCE.contains("out vec4 fragColor"));
        assert!(COMBINE_SOURCE.contains("fragColor ="));
    }
}
