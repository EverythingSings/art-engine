//! Architecture-boundary test: `art-engine-storyboard` must remain pure data.
//!
//! Spec authority: LAW-002 (Rust core owns the product engine) +
//! `spec/seed.yaml > architecture_notes.crates.storyboard` ("Pure data;
//! no GL dep").
//!
//! Mechanism: shell out to `cargo tree -p art-engine-storyboard
//! --no-default-features` and grep the output for any known
//! GL / GPU crate name. If one appears, the storyboard crate has
//! grown a GL dependency — fail with a pointer at the policy.
//!
//! This test runs only on platforms where `cargo` is on PATH (which is
//! every Cargo-driven test environment by construction).

use std::process::Command;

const BANNED_CRATES: &[&str] = &[
    "glow",          // OpenGL bindings used by art-engine-core's render module
    "khronos-egl",   // EGL platform layer used by art-engine-core's headless module
    "glutin",        // future Windows headless backend, if added
    "wgpu",          // future WebGPU path, if added
    "ash",           // raw Vulkan
    "wayland-client", // X11/Wayland surface — pulls in window-system deps
];

#[test]
fn storyboard_dependency_tree_has_no_gl_crates() {
    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "-p",
            "art-engine-storyboard",
            "--no-default-features",
            "--edges",
            "all",
            "--prefix",
            "none",
        ])
        .output()
        .expect("invoke cargo tree");

    assert!(
        output.status.success(),
        "cargo tree failed (rc={}):\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let tree = String::from_utf8_lossy(&output.stdout);

    // The output has one crate per line in the form `name vX.Y.Z [path]`.
    // We split on whitespace and check the first token of each line.
    let crates_in_tree: Vec<&str> = tree
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect();

    for banned in BANNED_CRATES {
        let found = crates_in_tree.iter().any(|c| *c == *banned);
        assert!(
            !found,
            "\nart-engine-storyboard must remain GL-free (spec LAW-002).\n\
             Found dependency on `{banned}` in the resolved tree.\n\
             \n\
             The storyboard crate is the typed authoring surface that the episode\n\
             renderer and the future WASM adapter both depend on. Keeping it pure\n\
             data lets it compile on any target without a GL toolchain.\n\
             \n\
             If you genuinely need GL types in storyboard, either:\n\
               1. Move the new feature into art-engine-core (the engine crate), or\n\
               2. File an ADR under spec/decisions/ explaining the architectural shift,\n\
                  bump the seed.yaml architecture_notes accordingly, and update this\n\
                  test's BANNED_CRATES list.\n",
        );
    }
}
