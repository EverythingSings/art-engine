# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Generative art engine in Rust, compiled to WASM for browser and native for server. Renders via WebGL2 with a composable layer/shader/post-processing pipeline. Exposes a CLI command interface. Two-agent system (Operator + Critic) can drive the CLI autonomously. Full architecture vision in `ARCHITECTURE.md`.

**Current state:** Phase 1 foundation in progress. Core workspace scaffolded with 11 crates. Engine trait, Field, and WebGL2 render module implemented.

## Build Commands

Use `xtask.sh` (or `make` if available) for all standard workflows:

```bash
bash xtask.sh check                                   # Full verification: fmt, clippy, test, doc
bash xtask.sh test                                     # Run all workspace tests
bash xtask.sh test art-engine-core                     # Run tests for a single crate
bash xtask.sh clippy                                   # Lint all crates
bash xtask.sh fmt                                      # Format check (no writes)
bash xtask.sh doc                                      # Build docs
bash xtask.sh build                                    # Build all crates (native)
bash xtask.sh wasm                                     # Build WASM target
```

Raw cargo commands (for reference / one-off use):

```bash
cargo build                                          # Build all crates
cargo build --target wasm32-unknown-unknown           # Build WASM
cargo test --all                                      # Run all workspace tests
cargo test -p art-engine-core                         # Run tests for a single crate
cargo test -p art-engine-core -- test_name            # Run a single test
cargo clippy --all                                    # Lint
cargo fmt --all                                       # Format
cargo run -p art-engine-cli --                        # Run CLI binary
wasm-pack build crates/wasm --target web              # Build WASM package for browser
```

## Development Standards

### Functional Programming

Write Rust in a functional style. Prefer:

- **Pure functions** over methods with side effects. Functions that take inputs and return outputs, no hidden mutation.
- **Immutable data** by default. Use `let` not `let mut` unless mutation is genuinely needed.
- **Iterator chains** (`map`, `filter`, `fold`, `flat_map`) over imperative loops with mutable accumulators.
- **Pattern matching** and `match` expressions over chains of `if/else`.
- **Sum types** (`enum`) for modeling state. Make invalid states unrepresentable.
- **`Result`/`Option` combinators** (`and_then`, `map`, `unwrap_or_else`, `?`) over manual `match` on every error.
- **No `unwrap()` or `expect()` in library code.** Return `Result` and let callers decide. `unwrap()` is acceptable only in tests and top-level CLI entry points.
- **Small, composable functions** over large monolithic ones.

### Test-Driven Development

Write tests first. The workflow is:

1. Write a failing test that describes the desired behavior.
2. Write the minimum code to make it pass.
3. Refactor while keeping tests green.

Every public function and every engine implementation must have tests. Use `#[cfg(test)]` modules within each source file. Integration tests go in `tests/` directories within each crate.

Property-based testing with `proptest` where applicable (e.g., field operations, color space conversions, PRNG distribution).

### Documentation

Keep documentation current with code changes:

- Update `CLAUDE.md` when adding new crates, changing build commands, or modifying architecture.
- Update `ARCHITECTURE.md` when making design decisions that deviate from or extend the spec.
- Every public type and function gets a doc comment (`///`). Focus on *why* and *invariants*, not restating the type signature.
- Use `cargo doc --all --no-deps` to verify doc comments compile.

### OSS-Only Dependencies

All dependencies must be open-source with permissive licenses (MIT, Apache-2.0, BSD, MPL-2.0, Zlib). No proprietary, AGPL, or GPL dependencies. This project is distributed as a Railway template — license compatibility matters. Check licenses before adding any new crate (`cargo license` or check crates.io).

### Project Management

Use **GitHub Issues** for all project management. Every non-trivial piece of work gets an issue. Reference issue numbers in commit messages (`fixes #12`, `ref #34`). Use labels for phases (`phase-1`, `phase-2`, etc.), categories (`engine`, `rendering`, `cli`, `backend`, `frontend`), and priority (`p0`, `p1`, `p2`).

## Technology Decisions

### Rendering: `glow` (WebGL2) for V1

Use the `glow` crate (GL on Whatever) targeting WebGL2 for V1. Rationale:

- WebGL2 has 97% browser coverage vs WebGPU at ~70%. A Railway template needs broad reach.
- `glow` enables desktop + WASM from one codebase (OpenGL 4.6 natively, WebGL2 on WASM). Enables headless server-side rendering (V2) without a separate code path.
- Shaders authored in GLSL ES 3.0 directly -- no WGSL translation layer, no Naga edge cases.
- The architecture's built-in shader library (voronoi, reaction-diffusion, kaleidoscope, etc.) is most naturally expressed in GLSL.
- `wgpu` (WebGPU) is the V2 target. The pipeline architecture (layers-as-textures, ping-pong post-processing, compositing pass) ports cleanly. Shaders would need GLSL-to-WGSL rewrite. Compute shaders for 100K+ particles become available on the WebGPU path.

Key dependencies:
```toml
glow = "0.16"
wasm-bindgen = "0.2"
web-sys = { version = "0.3", features = ["HtmlCanvasElement", "WebGl2RenderingContext", "OffscreenCanvas"] }
glam = "0.29"              # vec2, vec3, mat4
bytemuck = { version = "1", features = ["derive"] }
noise = "0.9"              # Perlin, simplex, worley (CPU-side)
```

All intermediate FBOs must use `RGBA16F` / `HALF_FLOAT` for HDR range (bloom thresholding, additive blending, banding prevention). Requires `EXT_color_buffer_float` extension at init.

### Frontend: OffscreenCanvas in Web Worker

The WASM engine + WebGL2 context runs entirely in a Web Worker via OffscreenCanvas:

- Main thread: UI, terminal, WebSocket, chat, gallery. Never blocked by engine computation.
- Worker thread: WASM engine + WebGL rendering at full speed.
- Communication: `postMessage` for CLI commands (main -> worker) and state updates (worker -> main).
- No pixel copying -- WebGL draws directly to the transferred canvas.

OffscreenCanvas is supported in Chrome (69+), Firefox (105+), Safari (16.4+).

### Backend: Axum with Dual Thread Pools

```rust
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]  // HTTP I/O only
async fn main() {
    let compute_pool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_cpus::get().saturating_sub(2))  // Leave cores for Tokio
        .build().unwrap();
    // ...
}
```

- `spawn_blocking` for one-off work (snapshot rendering, stats computation).
- Rayon thread pool for parallel particle/field computation.
- Long-running engine sessions: background task on blocking thread, `mpsc` for commands in, `oneshot` for results back, `watch` for state observation.

## Rendering Pipeline Architecture

### Per-Frame Render Order

```
1. For each layer (bottom to top):
   a. Bind layer FBO, clear to transparent
   b. Render content (particles as point sprites or instanced quads, shapes)
   c. Apply per-layer shader effects via ping-pong on layer's FBO pair
   d. Apply composition (mask, symmetry, tiling)

2. Composite all layers -> composite FBO
   (shader-based blend for multiply/screen/overlay; GL blendFunc for normal/additive)

3. Save composite to feedback texture (for echo/trails)

4. Post-processing stack via ping-pong FBOs
   bloom -> blur -> chromatic aberration -> grain -> vignette -> color grading

5. Tonemap + final blit to screen (default framebuffer)
```

### FBO Inventory

| Resource | Count | Purpose |
|----------|-------|---------|
| Layer FBOs | 2 per layer (ping-pong pair) | Content rendering + per-layer effects |
| Composite FBO | 1 + 1 copy | Layer blending (copy needed for shader-based blend modes) |
| Post-process FBOs | 2 (ping-pong pair) | Multi-pass post-processing |
| Feedback texture | 1 + FBO | Previous frame retention |
| Bloom mip chain | ~5 | Progressive downsample/upsample |

Typical setup (4 layers, 5-level bloom): ~19 FBOs, well within WebGL2 limits.

### Particles + Shaders Interaction

Particles render to the layer FBO first (vertex shader positions, fragment shader draws soft circles/glow). Then artistic effects (voronoi, wave distortion, etc.) apply as fullscreen passes reading that FBO. This is a two-pass-per-layer pattern:

1. Render particles -> layer texture
2. Apply effect shader: read layer texture -> write to ping-pong partner

This is necessary because screen-space effects need to sample neighboring pixels, which per-particle fragment shaders cannot do.

### Layer Compositing

WebGL2 hardware blend modes cannot express multiply/screen/overlay. These require a shader that samples both the layer texture and the current composite texture, computes the blend mathematically, and writes the result. This means:

- `blitFramebuffer` to copy composite before each layer blend (can't read and write same FBO)
- Simple modes (normal, additive) can use hardware `gl.blendFunc` as fast path

### Particle Trails

For layers with trails, skip `gl.clear()` between frames. Instead, draw a fullscreen quad sampling the previous frame at reduced alpha (e.g., 0.95 decay), then render new particle positions on top.

### Post-Processing Ping-Pong

```rust
struct PingPong {
    targets: [RenderTarget; 2],
    current: usize,
}

impl PingPong {
    fn src(&self) -> &RenderTarget { &self.targets[self.current] }
    fn dst(&self) -> &RenderTarget { &self.targets[1 - self.current] }
    fn swap(&mut self) { self.current = 1 - self.current; }
}
```

Bloom is special -- needs its own downsample/upsample mip chain (dual Kawase / progressive downsample pattern). Gaussian blur uses separable two-pass (horizontal then vertical).

### Fullscreen Triangle

Every post-processing and compositing pass uses a fullscreen triangle (more efficient than a quad -- no diagonal). Vertex shader generates positions from `gl_VertexID` with no VBO:

```glsl
#version 300 es
out vec2 v_uv;
void main() {
    v_uv = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
    gl_Position = vec4(v_uv * 2.0 - 1.0, 0.0, 1.0);
}
```

Draw with `gl.draw_arrays(TRIANGLES, 0, 3)` and an empty bound VAO.

## Particle Update Strategy (WebGL2)

At V1 particle counts (1K-10K per layer), CPU-side Rust/WASM update + VBO re-upload is sufficient. For 10K-50K, use WebGL2 transform feedback (vertex shader computes new positions, `RASTERIZER_DISCARD` enabled, ping-pong two VBOs). Compute shaders (100K+) require the V2 WebGPU port.

## Stats / Metrics System

The `stats` command provides structured JSON for the Critic agent feedback loop (Channel 3 in the architecture). Metrics are orders of magnitude cheaper than a vision API call.

### Metric Tiers (cost vs. signal)

**Always compute (near-zero cost, high signal):**
- Luminance statistics: mean, std_dev, skewness, dynamic_range
- Shannon entropy: grid-based (8x8 or 16x16), normalized to [0,1]
- Edge density: Sobel + threshold, fraction of edge pixels
- Colorfulness: Hasler-Suesstrunk formula (`sqrt(sigma_rg^2 + sigma_yb^2) + 0.3 * sqrt(mu_rg^2 + mu_yb^2)`)
- Visual balance: center of visual mass normalized to [0,1]
- Compression ratio: JPEG compressed / raw size

**Compute by default (cheap, good signal):**
- Dominant colors: k-means on 64x64 downsample, k=5
- Bilateral symmetry: flip + normalized cross-correlation
- Fractal dimension: box-counting on edge map (sweet spot: 1.3-1.5)

**Compute on request (moderate cost):**
- Color harmony: Matsuda template matching on hue histogram
- Radial symmetry, edge orientation entropy, power spectrum slope

### Key Thresholds for `summary.flags`

```
"too_uniform":   entropy < 0.2
"too_noisy":     entropy > 0.9 AND fractal_dimension > 1.7
"low_contrast":  dynamic_range < 0.2
"off_balance":   balance < 0.5
"color_clash":   harmony_score < 0.3
"too_dark":      luminance_mean < 0.1
"too_bright":    luminance_mean > 0.9
```

The agent uses flags to decide when to spend vision tokens: no flags = move on, flags present = investigate.

## Deterministic Replay

### WASM Float Determinism

Non-NaN results are **bit-identical** across all compliant WASM runtimes (Chrome, Firefox, Safari, Node). WASM has no transcendental instructions (`sin`, `cos`, `exp`) -- these compile to deterministic `libm` software implementations. Guard against NaN propagation at key computation points (division-by-zero, sqrt-of-negative).

### Replay Format (JSONL)

```jsonl
{"version":1,"seed":8675309,"canvas":[1024,1024],"engine_version":"0.1.0","wasm_hash":"sha256:abc..."}
{"f":0,"cmd":"canvas new 1024 1024 --bg #020210"}
{"f":0,"cmd":"layer add deep --type particles --count 1500"}
{"f":120,"input":{"type":"mouse","x":0.3,"y":0.6,"action":"click"}}
{"f":240,"cmd":"post add bloom --radius 12 --intensity 0.6"}
```

- Frame indices, not timestamps. The frame counter is the replay clock.
- Record: PRNG seed + frame-indexed CLI commands + frame-indexed input events.
- Do NOT record intermediate state or render output -- derived from seed + commands.
- Store WASM binary hash in header. Serve the same binary for gallery replay.

### Replay Guarantees

| Scenario | Guarantee |
|----------|-----------|
| Same WASM binary, same inputs | Bit-identical simulation state |
| Same WASM binary, different GPU | Visually indistinguishable (1-2 LSB pixel variance from GPU shader rounding) |
| Different WASM binary | Structural equivalence only. Warn user via hash mismatch. |

## Workspace Structure

```
art-engine/
  crates/
    core/          # Engine trait, Field, Palette (OKLab/OKLCh), PRNG (Xorshift64), Seed, params
    wasm/          # WASM bindings (wasm-bindgen), Lab struct wrapping EngineKind
    cli/           # CLI binary (clap): render, gif, info, list commands
    gray-scott/    # Gray-Scott reaction-diffusion
    physarum/      # Physarum polycephalum slime mold
    rose/          # Rose/parametric curve patterns
    microbe/       # Organism/cell simulation
    quantum/       # 2D quantum walk
    ising/         # Ising model (statistical mechanics)
    dla/           # Diffusion-limited aggregation
    attractor/     # Strange attractors (Lorenz, Henon, etc.)
  www/             # Minimal HTML/JS frontend (canvas + keyboard/mouse)
  pkg/             # Pre-built WASM artifacts
```

### Core Abstractions

- **`Engine` trait** (object-safe): `step()`, `field()`, `params()`, `param_schema()`, `hue_field()`. Each engine crate implements this. `dyn Engine` enables runtime engine switching.
- **`Field`**: 2D scalar field, row-major `Vec<f64>` in [0,1], toroidal wrapping. Used for visualization, nutrients, trails, hue modulation.
- **`Palette`**: OKLab/OKLCh color space for perceptually uniform gradients. Curated built-ins (ocean, neon, earth, vapor, etc.).
- **`Xorshift64`**: Deterministic PRNG. Same seed = reproducible art.
- **`Seed`**: Serializable struct (engine + dimensions + params + seed + steps) for reproducible specifications.

### Build Infrastructure Over Ad-Hoc Scripts

Never run one-off verification commands (e.g., `cargo test && cargo clippy && cargo fmt --check`) inline. Instead, build reusable infrastructure — Makefiles, shell scripts, CI configs — and invoke those. Verification should be a single repeatable command, not a chain pasted into the terminal.

### Code Conventions

- All crates use `#![deny(unsafe_code)]`
- All engine params extracted from `serde_json::Value` with defaults via `param_f64()` / `param_usize()` helpers
- Release profile: `lto = true`, `opt-level = 3` (native) or `opt-level = "s"` (WASM)

## Build Order (from ARCHITECTURE.md)

| Phase | Scope |
|-------|-------|
| 1 | Engine core: canvas, layers, particles, fields, shapes, palettes, transforms. WASM + WebGL2. CLI. Stats. |
| 2 | Shaders + post-processing: built-in shader library, post stack, composition (masking, symmetry, tiling). |
| 3 | Backend: Axum, Postgres, gallery API, auth (single admin password + session cookie). |
| 4 | Frontend: split-pane UI (WebGL canvas + terminal + gallery), WebSocket, stream-everything. |
| 5 | Railway template: Dockerfile, env vars, Postgres plugin, marketplace listing. |
| 6 | Two-agent system: Operator + Critic via OpenRouter. Screenshot capture. Orchestrator loop. |
