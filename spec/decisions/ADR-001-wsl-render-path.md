# ADR-001: WSL is the supported render path on Windows

- **Status**: accepted
- **Date**: 2026-05-11
- **Authority**: 85 (per foundation spec's `spec_hierarchy`)
- **Supersedes**: none
- **Superseded by**: none
- **Capability touched**: CAP-RENDER-EPISODE

## Context

`art-engine-core`'s native GPU pipeline (`crates/core/src/render/headless.rs`) creates a
headless OpenGL context via EGL using the `khronos-egl` crate. EGL is a first-class platform
on Linux (Mesa) and Android. On Windows, EGL is not part of the OS — applications that need
it ship the [ANGLE](https://github.com/google/angle) implementation as `libEGL.dll +
libGLESv2.dll + d3dcompiler_47.dll`.

During Phase A implementation (this session, 2026-05-11), the rendering pipeline was
exercised on Windows by attempting to copy Chrome and Edge's bundled ANGLE DLLs alongside
the binary. Both browsers' libEGL has hidden runtime dependencies on the surrounding
browser install (chrome_elf.dll, etc.), so loading fails with `LoadLibraryExW error 126` even
when the three documented DLLs are co-located with the executable.

The pragmatic alternative — running the renderer inside WSL2 Ubuntu — works out of the box.
WSL2 ships with Mesa's libEGL via WSLg, and software rendering is fast enough for
3-minute 1080×1920 YouTube Shorts at ~36 fps render. The shared filesystem at
`/mnt/c/...` means storyboards, audio, and outputs are accessible from both sides without
copying.

## Decision

**The supported render path on Windows is via WSL2 with the WSL-side `target-wsl/`
build directory.** The Windows-native render path is unsupported.

Concretely:

- `art-engine-episode` is built and run via `wsl -- bash -c "cd ... && cargo build --release ..."`
- A separate `CARGO_TARGET_DIR=target-wsl` keeps Windows-side and WSL-side build artifacts
  from colliding
- `examined-machine/scripts/render_episode.sh` is the canonical wrapper that does the WSL
  invocation for the user
- The `art-engine/CLAUDE.md` and `AGENTS.md` document this constraint

The Linux-native path (when running on actual Linux) and the browser WASM path remain
unchanged.

## Consequences

### Positive

- Zero distribution work: every Windows developer with WSL2 can build and run today
- Mesa software rendering is deterministic and good enough for the project's render volume
  (~3 min episodes, 1-2 renders per episode authoring iteration)
- The `target-wsl/` separation keeps Windows-side IDE workflows fast (no GPU artifacts in
  the Windows target dir)
- The Linux/WSL execution path is identical, so CI on Linux exercises the same code path

### Negative

- No native GPU acceleration on Windows. Mesa llvmpipe runs ~36 fps at 1080×1920 vs an
  RTX 3070 Ti which would do >500 fps. For 3-min episodes that's ~150s vs an estimated <10s.
- Two build directories on disk (Windows `target/` + WSL `target-wsl/`), each ~5 GB after
  a full debug build
- Windows-only contributors without WSL2 cannot run the renderer at all
- The `examined-machine/scripts/render_episode.sh` wrapper bakes the WSL invocation into
  the workflow, complicating any future cross-platform distribution

### Neutral

- The capability contract for `CAP-RENDER-EPISODE` documents `HeadlessGl` as an
  `integration_error` with exit code 6 and remediation pointing here

## Alternatives considered

### Option A — bundle ANGLE binaries from Google's CI releases

Pros: native Windows GPU acceleration; users don't need WSL.
Cons: requires us to redistribute ANGLE; the prebuilt CI artifacts target Chromium's
dependency stack and may still need extra DLLs (`vulkan-1.dll`, `dxcompiler.dll`, etc).
Adds a release pipeline step (download, vendor, version-lock) that doesn't exist yet. Not
clearly worth it for a small project's render volume.

### Option B — add a WGL backend to `art-engine-core`

Pros: native Windows OpenGL via the Windows native API; no DLL bundling.
Cons: real engineering work — write a parallel headless context module that uses
`glutin` with a hidden window or off-screen PBuffer surface. ~couple-hundred lines of
unsafe Win32 + glutin code, plus a feature flag to switch backends. The code path is then
exercised only on Windows, doubling the CI matrix needed to keep it honest.

### Option C — port to wgpu (WebGPU)

Pros: cross-platform with one backend selection at runtime; future-proof.
Cons: massive rewrite of the entire shader pipeline (GLSL → WGSL), the engine crates'
field-uniform binding, and the post-processing chain. Out of scope for any near-term
milestone.

### Option D — accept WSL-only (this ADR)

Pros: zero work, immediately functional, predictable performance.
Cons: noted in "Negative" above.

## Validation

- WSL2 Ubuntu with Mesa libEGL successfully renders `art-engine-cli render gray-scott --gpu`
  and `art-engine-episode render <storyboard>` (verified 2026-05-11)
- The Windows-native build of `art-engine-cli` compiles cleanly but fails at runtime in
  `create_headless_context()` with `LoadLibraryExW error 126`
- `art-engine-storyboard` and any tests not requiring GL build and pass on Windows native
  (validates the dependency rule — pure data crates do not need EGL)

## Follow-up items

- Add an `art-engine-episode doctor` command that runs `create_headless_context` and prints
  a useful failure message + this ADR's path on Windows-native invocation
- Investigate Option B (WGL backend) when a contributor needs Windows-native performance
- Investigate Option C (wgpu) only if the project moves toward a Windows desktop app
- Update `examined-machine/README.md` to mention WSL2 as a prerequisite
