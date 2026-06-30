# AGENTS.md — art-engine

Repo-local operating rules for human + agent contributors. Specialises the
engineering-wide foundation spec at
`engineering/rust_core_cli_iced_spec_seed_v0_3_stable.yaml` for this project.
Spec authority: this file is **80**; the project's `spec/seed.yaml` is **100**;
foundation laws there override any local convention.

## Start here

Read in this order before making changes:

1. `spec/seed.yaml` — project overlay (mission, target platforms, non-goals, open drift)
2. `spec/capabilities/*.yaml` — current capability contracts (only `render-episode.yaml` so far)
3. `spec/decisions/*.md` — ADRs (only ADR-001 so far)
4. `CLAUDE.md` — historical Claude Code guidance (architecture sketch + build commands)
5. This file
6. `Cargo.toml` for the workspace layout
7. `ARCHITECTURE.md` for the long-form architecture vision

## What this codebase is

A Rust workspace with two adjacent products:

- **`art-engine`** (this repo) — generative art engine with a Canvas/Layer/Field/BuiltinShader
  data model and a Storyboard authoring layer on top. Compiles to native (CLI + episode
  renderer) and WASM (browser).
- **`examined-machine`** (sibling at `../examined-machine`) — a YouTube Shorts pipeline
  that uses `art-engine-episode` to render episodes of "The Examined Machine". Holds the
  per-episode `storyboard.ron` files, Python prep scripts, and rendered outputs.

The two are coupled but live in separate directories. art-engine doesn't know episode
content; examined-machine doesn't know shader internals.

## Workspace layout

```
art-engine/
├── Cargo.toml                 # workspace + members
├── CLAUDE.md                  # historical guidance (Claude Code project file)
├── ARCHITECTURE.md            # long-form architecture vision
├── README.md                  # (TODO: create)
├── AGENTS.md                  # this file
├── spec/
│   ├── seed.yaml              # project overlay (authority 100)
│   ├── capabilities/*.yaml    # capability contracts (authority 90)
│   └── decisions/*.md         # ADRs (authority 85)
├── xtask.sh                   # bash equivalent of an xtask crate (validation gates)
├── crates/
│   ├── core/                  # art-engine-core: Canvas / Layer / Field / Palette / BuiltinShader (23 shaders)
│   ├── engines/               # art-engine-engines: engine registry + snapshot writers
│   ├── storyboard/            # art-engine-storyboard: Storyboard / Backdrop / Foreground / Transition data
│   ├── episode/               # art-engine-episode: CLI binary that renders a storyboard → mp4
│   ├── cli/                   # art-engine-cli: generic engine-driven render commands
│   ├── wasm/                  # art-engine-wasm: browser WebGL2 binding
│   └── {gray-scott,physarum,particles,attractor,mandelbrot,rose,microbe,quantum,ising,dla,differential}
│                               # one crate per engine, all `impl Engine`
└── target-wsl/                # WSL build output (see ADR-001 — native render path requires Linux/WSL)
```

## Validation gates

The bash wrapper:

```bash
bash xtask.sh check    # full: fmt + clippy + test + doc + wasm
bash xtask.sh test
bash xtask.sh clippy
bash xtask.sh fmt
bash xtask.sh doc
bash xtask.sh wasm
```

CI runs `bash xtask.sh check` on every push and PR. Agents claiming completion must report
which gates ran and which passed. The xtask is a bash script rather than a Rust crate per the
foundation spec — see `spec/seed.yaml > architecture_notes > workspace_layout_overlay`.

Per-crate test run:

```bash
cargo test -p art-engine-core --features render
cargo test -p art-engine-storyboard
cargo test -p art-engine-episode --features gpu
```

### Supply-chain / license enforcement

```bash
cargo install cargo-deny      # one-time
cargo deny check              # license + advisory + bans + sources
cargo deny check licenses     # licenses only — fast iteration
```

The policy lives in `deny.toml` at the workspace root. It codifies the OSS-only
license rule from `CLAUDE.md` (allow MIT/Apache-2.0/BSD/MPL-2.0/Zlib/ISC/etc;
deny GPL/AGPL/LGPL/proprietary). Run `cargo deny check` before adding a new
dependency. The check is not yet wired into `xtask.sh check` — it's a
recommended gate (per the foundation seed's `validation_gates.recommended_when_available`)
that becomes required once we have a CI pipeline running it.

WSL-side build of the episode binary (see ADR-001):

```bash
wsl -- bash -c "source ~/.cargo/env && cd /mnt/c/Users/Trist/engineering/art-engine \
  && CARGO_TARGET_DIR=target-wsl cargo build --release -p art-engine-episode"
```

### Environment readiness check

Before trying a render, run the doctor command:

```bash
art-engine-episode doctor                    # human-readable
art-engine-episode doctor --format json      # JSON for scripts
```

Probes: ffmpeg on PATH, libEGL loadable, headless GL pipeline init,
tempdir writable. Exit 0 if all OK; exit 6 if any probe fails. The
failure messages point at remediations (e.g. "run inside WSL2 per
ADR-001" when libEGL fails on Windows native).

## Common workflows

### Adding a backdrop shader

1. Write `crates/core/src/shaders/<name>.rs` with `FRAGMENT_SOURCE: &str` and `NAME: &str`
   constants, plus a `#[cfg(test)] mod tests` block.
2. Register the variant in `crates/core/src/shaders/mod.rs::BuiltinShader`:
   - Add `pub mod <name>;`
   - Add the enum variant
   - Wire it into `from_name`, `name`, `fragment_source`, `list`
   - Update `list_contains_all_variants` test (`expected N` count)
   - Update `post_process_set_matches_documentation` test
3. Add the uniform schema in `crates/core/src/render/pipeline.rs::default_uniform_schema`.
4. If exposing as a storyboard backdrop, add the variant to
   `crates/storyboard/src/lib.rs::Backdrop` with default-fn'd fields.
5. Wire dispatch in `crates/episode/src/render.rs::effect_for_scene` +
   `update_dynamic_uniforms` + `crates/episode/src/main.rs::short_backdrop`.
6. Smoke-test with `art-engine-cli render gray-scott --gpu --shader '<name>:{...}'`.

### Adding a typed error variant

`crates/episode/src/error.rs` owns the closed error sets for the episode binary. New variant
must come with:

- A matching exit code in `RenderError::exit_code()` (or its sibling enum's, mapped to spec policy)
- An entry in `spec/capabilities/render-episode.yaml` under `errors.closed_set` with
  `code`, `category`, `recoverable`, `exit_code`, `remediation`
- A test in `tests` mod (or a test that exercises the failure path)

### Authoring a new episode

`examined-machine/` side, not art-engine:

```bash
cd ../examined-machine
uv run scripts/prep_episode.py <audio>.m4a
# Edit episodes/<ep>.storyboard.ron
bash scripts/render_episode.sh <ep>
```

The starter storyboard template (in `prep_episode.py`) inherits the show's chrome (header +
sigil + scene_pips).

## Spec drift policy

Behavior changes that touch:

- A capability's inputs / outputs / errors / authority / lifecycle → update the relevant
  `spec/capabilities/<cap>.yaml` in the same change
- The architecture (e.g. splitting episode into service + bin crates) → file an ADR under
  `spec/decisions/ADR-NNN-<slug>.md`
- The CLI's exit codes, command shape, or JSON schema (when JSON ships) → bump the
  capability's version and document in a changelog

Generated docs are disposable. Canonical sources (seed.yaml, capabilities, ADRs) require
intentional edits.

## Open drift, queued for resolution

Tracked in `examined-machine/out/spec_review.md`. The largest open items:

- **No CLI JSON output**. `art-engine-episode plan --format json` doesn't exist; both `plan`
  and `render` are human-text-only. Will be addressed in a future ADR/capability update.
<!-- deny.toml landed 2026-05-11; LAW-014 now codified. Keeping this note as a placeholder until the project gets a CI pipeline that actually runs `cargo deny check` on PR. -->
- **`cargo deny check` not yet in CI**. The policy is codified in `deny.toml`,
  but enforcement is still manual — `xtask.sh check` doesn't include it because
  the cargo-deny binary may not be installed locally. Add when a CI pipeline
  exists.
- **No service/bin split for art-engine-episode**. The render function and the CLI binary
  live in the same crate. The split is queued (LAW-003 alignment).
<!-- 2026-05-11: architecture-boundary test landed; see crates/storyboard/tests/no_gl_in_tree.rs. -->
- **Boundary enforcement coverage is shallow.** Only the storyboard ↔ GL boundary is
  guarded (by `crates/storyboard/tests/no_gl_in_tree.rs`). Other dependency-rule edges
  (e.g. core ↔ clap, core ↔ tokio) are not yet enforced. Add boundary tests as more
  adapters land.
- **No release packaging**. Native binaries aren't built into release artifacts. WSL-only
  render path means cross-platform packaging is non-trivial; see ADR-001.

## Forbidden patterns (project-specific specialisations)

Inherits the foundation's `forbidden_patterns` plus:

- Adding a backdrop variant to `Storyboard::Backdrop` without adding the matching shader
  schema in `pipeline.rs` (the shader will use default uniforms and silently look wrong).
- Storing `Result<_, String>` at any module boundary in `art-engine-episode`. Use the typed
  enums in `error.rs` and add new variants there with matching capability-contract entries.
- Mixing GPU types into `art-engine-storyboard`. It must stay pure data + serde.
- Hardcoding `1080`/`1920` in the render path outside the storyboard's `width`/`height`
  fields. Storyboard owns canvas dimensions.
- Burning chrome (header / sigil / pips / brackets) into shaders. Chrome is composed in
  `meta_ass.rs` via ASS overlays so it stays editable without re-rendering frames.

## Agent operating rules

Inherits all rules from the foundation seed's `agent_context.operating_rules`. Project-local
additions:

- Verify WSL is available before attempting `art-engine-episode` work; see ADR-001 for why.
- When in doubt about the storyboard schema, run `art-engine-episode plan <story.ron>` —
  it's the cheapest validation path. (`plan` returns exit code 2 on bad input today; future
  JSON output will give structured feedback.)
- Don't commit `target-wsl/` (already in `.gitignore` if present; if not, add it).

## Contact

Authority chain:

- Project seed: `spec/seed.yaml` (owner: @TheExaminedMachine)
- Foundation seed: `../rust_core_cli_iced_spec_seed_v0_3_stable.yaml`
