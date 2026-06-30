//! Doctor — environment readiness probe.
//!
//! Runs the same fragile bring-up steps the renderer does, but in
//! isolation and with friendly remediation messages. Each probe is
//! independent and continues running even if an earlier one failed,
//! so the report lists *all* problems at once.
//!
//! Spec authority: closes the LAW-011 follow-up in
//! `spec/decisions/ADR-001-wsl-render-path.md` ("Add an
//! `art-engine-episode doctor` command that runs `create_headless_context`
//! and prints a useful failure message + this ADR's path on
//! Windows-native invocation").

use serde::Serialize;
use std::process::Command;

/// Stable schema version for the JSON output. Bump when changing the
/// shape (adding/removing fields, renaming probes). Document the bump
/// in `spec/capabilities/doctor.yaml`.
pub const SCHEMA_VERSION: u32 = 1;

/// Result of running one environment probe.
#[derive(Debug, Serialize)]
pub struct Probe {
    /// Short stable name (snake_case). Treat as a compatibility surface.
    pub name: String,
    /// Did the probe succeed?
    pub ok: bool,
    /// One-line human summary.
    pub message: String,
    /// Optional next-action hint when `ok == false`.
    pub remediation: Option<String>,
}

/// Top-level report. Serialised as JSON when `--format json`.
#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub schema_version: u32,
    /// `true` iff every probe succeeded.
    pub overall_ok: bool,
    pub probes: Vec<Probe>,
}

/// Run every probe and return the combined report.
pub fn run() -> DoctorReport {
    let probes = vec![
        probe_ffmpeg(),
        #[cfg(feature = "gpu")]
        probe_libegl_loadable(),
        #[cfg(feature = "gpu")]
        probe_headless_gl_context(),
        probe_tempdir_writable(),
    ];
    let overall_ok = probes.iter().all(|p| p.ok);
    DoctorReport {
        schema_version: SCHEMA_VERSION,
        overall_ok,
        probes,
    }
}

/// Render the report as a short human-readable block on stdout.
pub fn human_format(r: &DoctorReport) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let banner = if r.overall_ok { "OK" } else { "PROBLEMS" };
    writeln!(&mut out, "art-engine-episode doctor — {banner}").unwrap();
    writeln!(&mut out).unwrap();
    for p in &r.probes {
        let mark = if p.ok { "✓" } else { "✗" };
        writeln!(&mut out, "  {mark} {:<22} {}", p.name, p.message).unwrap();
        if let Some(rem) = &p.remediation {
            writeln!(&mut out, "      → {rem}").unwrap();
        }
    }
    out
}

/// Exit code for a doctor report. 0 if all probes pass; otherwise 6
/// (unavailable dependency) per the spec's exit-code policy, so scripts
/// can distinguish "system not ready" from a real error.
pub fn exit_code(r: &DoctorReport) -> i32 {
    if r.overall_ok {
        0
    } else {
        6
    }
}

// ── individual probes ─────────────────────────────────────────────────

fn probe_ffmpeg() -> Probe {
    match Command::new("ffmpeg").arg("-version").output() {
        Ok(out) if out.status.success() => {
            let first_line = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("ffmpeg present")
                .to_string();
            Probe {
                name: "ffmpeg".into(),
                ok: true,
                message: first_line,
                remediation: None,
            }
        }
        Ok(out) => Probe {
            name: "ffmpeg".into(),
            ok: false,
            message: format!("ffmpeg exited with {}", out.status),
            remediation: Some(
                "Try `ffmpeg -version` directly to inspect the failure.".into(),
            ),
        },
        Err(_) => Probe {
            name: "ffmpeg".into(),
            ok: false,
            message: "ffmpeg not found on PATH".into(),
            remediation: Some(
                "Install ffmpeg (apt install ffmpeg, brew install ffmpeg, or winget install Gyan.FFmpeg).".into(),
            ),
        },
    }
}

#[cfg(feature = "gpu")]
fn probe_libegl_loadable() -> Probe {
    // Try to bring up a headless context; if that fails specifically
    // at the libEGL load step, surface a Windows-aware remediation.
    use art_engine_core::render::headless::{create_headless_context, HeadlessError};
    match create_headless_context() {
        Ok(_) => Probe {
            name: "libegl".into(),
            ok: true,
            message: "libEGL loaded and headless context created".into(),
            remediation: None,
        },
        Err(HeadlessError::LoadFailed(msg)) => Probe {
            name: "libegl".into(),
            ok: false,
            message: format!("libEGL failed to load: {msg}"),
            remediation: Some(
                "On Linux: install Mesa (libegl-dev or equivalent). \
                 On Windows: run inside WSL2 — see spec/decisions/ADR-001-wsl-render-path.md."
                    .into(),
            ),
        },
        Err(other) => Probe {
            name: "libegl".into(),
            ok: false,
            message: format!("headless GL failed: {other}"),
            remediation: Some(
                "libEGL loaded but context creation failed. Check DRI3 / Mesa version.".into(),
            ),
        },
    }
}

#[cfg(feature = "gpu")]
fn probe_headless_gl_context() -> Probe {
    // Independent of probe_libegl_loadable so callers can see whether
    // the context creation step itself has issues even when load worked.
    use art_engine_core::render::headless::create_headless_context;
    use art_engine_core::render::pipeline::Pipeline;
    let gpu = match create_headless_context() {
        Ok(g) => g,
        Err(_) => {
            // libegl probe already covered this. Don't double-report.
            return Probe {
                name: "gl_pipeline".into(),
                ok: false,
                message: "skipped — libegl probe failed first".into(),
                remediation: None,
            };
        }
    };
    match Pipeline::new(gpu.context(), 64, 64) {
        Ok(_) => Probe {
            name: "gl_pipeline".into(),
            ok: true,
            message: "Pipeline initialised at 64x64".into(),
            remediation: None,
        },
        Err(e) => Probe {
            name: "gl_pipeline".into(),
            ok: false,
            message: format!("Pipeline::new failed: {e}"),
            remediation: Some(
                "GL extension or driver issue. Try a different GL implementation (Mesa, llvmpipe).".into(),
            ),
        },
    }
}

fn probe_tempdir_writable() -> Probe {
    let dir = std::env::temp_dir();
    let test_path = dir.join(format!("art-engine-doctor-{}.tmp", std::process::id()));
    match std::fs::write(&test_path, b"probe") {
        Ok(()) => {
            // Best-effort cleanup.
            let _ = std::fs::remove_file(&test_path);
            Probe {
                name: "tempdir_write".into(),
                ok: true,
                message: format!("temp dir writable at {}", dir.display()),
                remediation: None,
            }
        }
        Err(e) => Probe {
            name: "tempdir_write".into(),
            ok: false,
            message: format!("cannot write to temp dir {}: {e}", dir.display()),
            remediation: Some(
                "Free disk space, fix permissions, or set TMPDIR / TMP / TEMP to a writable location.".into(),
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_stable() {
        assert_eq!(SCHEMA_VERSION, 1);
    }

    #[test]
    fn tempdir_probe_should_pass_on_test_runner() {
        // Test runner has a writable temp dir by construction.
        let p = probe_tempdir_writable();
        assert!(p.ok, "tempdir probe failed: {p:?}");
        assert_eq!(p.name, "tempdir_write");
    }

    #[test]
    fn human_format_marks_failures_visibly() {
        let report = DoctorReport {
            schema_version: SCHEMA_VERSION,
            overall_ok: false,
            probes: vec![
                Probe {
                    name: "alpha".into(),
                    ok: true,
                    message: "ok".into(),
                    remediation: None,
                },
                Probe {
                    name: "beta".into(),
                    ok: false,
                    message: "missing".into(),
                    remediation: Some("install it".into()),
                },
            ],
        };
        let s = human_format(&report);
        assert!(s.contains("PROBLEMS"));
        assert!(s.contains("✓ alpha"));
        assert!(s.contains("✗ beta"));
        assert!(s.contains("→ install it"));
    }

    #[test]
    fn exit_code_six_when_any_probe_fails() {
        let r_ok = DoctorReport {
            schema_version: SCHEMA_VERSION,
            overall_ok: true,
            probes: vec![],
        };
        let r_bad = DoctorReport {
            schema_version: SCHEMA_VERSION,
            overall_ok: false,
            probes: vec![Probe {
                name: "x".into(),
                ok: false,
                message: "no".into(),
                remediation: None,
            }],
        };
        assert_eq!(exit_code(&r_ok), 0);
        assert_eq!(exit_code(&r_bad), 6);
    }

    #[test]
    fn report_is_json_serialisable() {
        let r = DoctorReport {
            schema_version: SCHEMA_VERSION,
            overall_ok: true,
            probes: vec![Probe {
                name: "test".into(),
                ok: true,
                message: "ok".into(),
                remediation: None,
            }],
        };
        let json = serde_json::to_string_pretty(&r).unwrap();
        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.contains("\"overall_ok\": true"));
        assert!(json.contains("\"name\": \"test\""));
    }
}
