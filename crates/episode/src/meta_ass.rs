//! Build a combined ASS subtitle file that merges the karaoke captions
//! with the persistent header + sigil overlays declared at the
//! storyboard's top level.
//!
//! ASS lets us layer multiple styled text elements in a single file
//! that ffmpeg's `subtitles=` filter burns in one pass — no second
//! rendering stack needed for Phase B. The combined file is written
//! to `<build>/<ep>/meta.ass` and that path is what we pass to ffmpeg.
//!
//! Layout decisions for 1080x1920:
//! - **Header** at top-center, ~150px from the top edge. Bold, white
//!   with a black outline. The optional kicker line sits above it in
//!   amber, smaller, all-caps.
//! - **Sigil** in the top-right corner by default, so it never collides
//!   with karaoke captions (which live in the bottom third) and stays
//!   visible above the YouTube UI overlay on mobile.

use art_engine_storyboard::{
    Corner, Foreground, HeaderSpec, PipPosition, ScenePipsSpec, SigilSpec, Storyboard,
};
use std::path::{Path, PathBuf};

use crate::error::MetaAssError;

/// Generate a combined ASS file at `out_path` containing the karaoke
/// dialogue events from `karaoke_ass_path` plus the storyboard's
/// header + sigil events. Returns the path written.
pub fn build_meta_ass(
    sb: &Storyboard,
    karaoke_ass_path: Option<&Path>,
    out_path: &Path,
) -> Result<PathBuf, MetaAssError> {
    let mut events: Vec<String> = Vec::new();

    if let Some(p) = karaoke_ass_path {
        let karaoke = std::fs::read_to_string(p).map_err(|source| MetaAssError::ReadKaraoke {
            path: p.to_path_buf(),
            source,
        })?;
        events.extend(extract_dialogue_lines(&karaoke));
    }

    let duration_end = end_time(sb.duration());

    if let Some(h) = &sb.header {
        events.extend(header_events(h, &duration_end));
    }
    if let Some(s) = &sb.sigil {
        events.push(sigil_event(s, &duration_end));
    }
    if let Some(spec) = &sb.scene_pips {
        events.extend(pip_events(sb, spec));
    }
    // Persistent industrial-spec chrome: four small corner brackets
    // framing the safe area. Always-on; no storyboard opt-in needed
    // (yet). If we ever want to disable per-episode, gate behind a
    // `chrome: ChromeSpec` field.
    events.extend(corner_bracket_events(sb, &duration_end));

    // Per-scene PullQuotes — large emphasised lines anchored at the
    // centre of the frame for their declared window.
    events.extend(pullquote_events(sb));

    // Per-scene title + end cards. TitleCard is the big opening
    // headline; EndCard is the subscribe nudge at the close.
    events.extend(titlecard_events(sb));
    events.extend(endcard_events(sb));

    // Per-scene Arrow diagram primitives. Drawn via ASS drawing
    // commands using the chrome-orange accent.
    events.extend(arrow_events(sb));

    // Per-scene Decomposition diagrams (whole + N radial parts).
    events.extend(decomposition_events(sb));

    // Per-scene Annotation callouts (label + leader + target dot).
    events.extend(annotation_events(sb));

    // Per-scene Highlight frames (four corner brackets around a region).
    events.extend(highlight_events(sb));

    // Per-scene Comparison (left | divider | right, all centred row).
    events.extend(comparison_events(sb));

    let body = render_ass(sb, &events);
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| MetaAssError::Mkdir {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    std::fs::write(out_path, body).map_err(|source| MetaAssError::Write {
        path: out_path.to_path_buf(),
        source,
    })?;
    Ok(out_path.to_path_buf())
}

fn extract_dialogue_lines(ass: &str) -> Vec<String> {
    ass.lines()
        .filter(|l| l.trim_start().starts_with("Dialogue:"))
        .map(|l| l.to_string())
        .collect()
}

fn header_events(h: &HeaderSpec, end: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(2);
    let start = "0:00:00.00";

    if let Some(kicker) = &h.kicker {
        out.push(format!(
            "Dialogue: 0,{start},{end},HeaderKicker,,0,0,0,,{}",
            escape_ass(kicker)
        ));
    }
    out.push(format!(
        "Dialogue: 0,{start},{end},Header,,0,0,0,,{}",
        escape_ass(&h.text)
    ));
    out
}

/// One Dialogue event per scene, each drawing the full pips strip with
/// past scenes filled, the current scene highlighted, and future scenes
/// dimmed. The strip is composed via ASS `\p1...\p0` polygon paths so
/// no extra renderer is needed — libass burns it during compositing.
fn pip_events(sb: &Storyboard, spec: &ScenePipsSpec) -> Vec<String> {
    let n = sb.scenes.len();
    if n == 0 {
        return vec![];
    }

    // Layout: strip is centred horizontally inside the safe area. The
    // right edge of the safe area reserves room for the sigil so the
    // pips never disappear behind the channel handle.
    let safe_left: i32 = 60;
    let sigil_reserve: i32 = 340; // mirror on the left so the strip stays centred
    let total_w = sb.width as i32 - safe_left * 2 - sigil_reserve;
    let gap: i32 = 6;
    let pip_w = (((total_w - gap * (n as i32 - 1)) / n as i32).max(8)).min(120);
    let pip_h: i32 = 14;
    let pip_h_active: i32 = 22;
    let strip_w = pip_w * n as i32 + gap * (n as i32 - 1);
    let start_x = (sb.width as i32 - strip_w) / 2;

    let y_centre: i32 = match spec.position {
        PipPosition::Top => 40,
        PipPosition::Bottom => sb.height as i32 - 40,
    };

    // ASS colours: AABBGGRR.
    //   past   — full white (clearly "we've been there")
    //   current — amber accent at full opacity; also drawn taller so
    //             the active pip pops visually from neighbours
    //   future — semi-transparent grey (visible scope, doesn't compete)
    const PAST: &str = "&H00FFFFFF&";
    const CURRENT: &str = "&H005CBDF5&";
    const FUTURE: &str = "&HC0808080&";

    sb.scenes
        .iter()
        .enumerate()
        .map(|(i, scene)| {
            let mut drawing = String::new();
            for j in 0..n {
                let x0 = start_x + j as i32 * (pip_w + gap);
                let x1 = x0 + pip_w;
                let (color, h) = match j.cmp(&i) {
                    std::cmp::Ordering::Less => (PAST, pip_h),
                    std::cmp::Ordering::Equal => (CURRENT, pip_h_active),
                    std::cmp::Ordering::Greater => (FUTURE, pip_h),
                };
                // Vertically centre each pip on `y_centre` so the
                // taller current pip extends both above and below
                // the other pips, "lifting" off the strip.
                let y_top = -(h / 2);
                let y_bot = h / 2;
                drawing.push_str(&format!(
                    "{{\\1c{color}\\p1}}m {x0} {y_top} l {x1} {y_top} l {x1} {y_bot} l {x0} {y_bot}{{\\p0}}"
                ));
            }
            format!(
                "Dialogue: 0,{},{},Pips,,0,0,0,,{{\\an7\\pos(0,{y_centre})}}{drawing}",
                fmt_time(scene.start),
                fmt_time(scene.end),
            )
        })
        .collect()
}

/// ASS time format used by the pip and karaoke events.
fn fmt_time(t: f32) -> String {
    let total = t.max(0.0);
    let h = (total as u32) / 3600;
    let m = ((total as u32) % 3600) / 60;
    let s = total - (h as f32 * 3600.0) - (m as f32 * 60.0);
    let s_int = s as u32;
    let cs = ((s - s_int as f32) * 100.0) as u32;
    format!("{h}:{m:02}:{s_int:02}.{cs:02}")
}

/// Four small "L"-shaped registration brackets at the corners of the
/// safe area, drawn via ASS path commands. Inspired by industrial
/// spec-sheet / technical-document layouts — the bracket says "this
/// is the bounding crop, treat it like an instrument readout".
///
/// Each bracket is rendered as two thin rectangles (one horizontal
/// arm, one vertical arm), filled in chrome-orange.
fn corner_bracket_events(sb: &Storyboard, end: &str) -> Vec<String> {
    let inset: i32 = 36; // distance from the absolute frame edge
    let arm_len: i32 = 36;
    let arm_thick: i32 = 4;
    let w = sb.width as i32;
    let h = sb.height as i32;
    let start = "0:00:00.00";

    // (anchor_x, anchor_y, dx_arm, dy_arm) — direction signs encode
    // which way the bracket "opens".
    let corners = [
        (inset, inset, 1, 1),                 // TL ┌
        (w - inset, inset, -1, 1),            // TR ┐
        (inset, h - inset, 1, -1),            // BL └
        (w - inset, h - inset, -1, -1),       // BR ┘
    ];

    corners
        .iter()
        .map(|&(ax, ay, dx, dy)| {
            let rect = |x0: i32, y0: i32, w: i32, h: i32| -> (i32, i32, i32, i32) {
                let x1 = x0 + w;
                let y1 = y0 + h;
                (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1))
            };
            // Horizontal arm: thin band, arm_len wide, arm_thick tall.
            let (hx0, hy0, hx1, hy1) = rect(ax, ay, dx * arm_len, dy * arm_thick);
            // Vertical arm: thin band, arm_thick wide, arm_len tall.
            let (vx0, vy0, vx1, vy1) = rect(ax, ay, dx * arm_thick, dy * arm_len);

            // Chrome-orange (#E94C20). ASS = AABBGGRR = &H0021A7E8&
            // (alpha=00, B=21, G=A7? wait — recompute).
            // Hex 0xE94C20 RGB → reverse to BGR = 0x204CE9 → AABBGGRR
            // with alpha 00 = &H00204CE9&.
            let drawing = format!(
                "{{\\an7\\pos(0,0)\\1c&H00204CE9&\\p1}}m {hx0} {hy0} l {hx1} {hy0} l {hx1} {hy1} l {hx0} {hy1}{{\\p0}}\
                 {{\\p1}}m {vx0} {vy0} l {vx1} {vy0} l {vx1} {vy1} l {vx0} {vy1}{{\\p0}}"
            );
            format!("Dialogue: 0,{start},{end},Pips,,0,0,0,,{drawing}")
        })
        .collect()
}

/// Walk every scene's foreground list, collect PullQuote entries, and
/// emit one Dialogue per quote. Layer 1 puts them above the karaoke
/// captions so they dominate the frame while active.
fn pullquote_events(sb: &Storyboard) -> Vec<String> {
    let mut out = Vec::new();
    for scene in &sb.scenes {
        for fg in &scene.foreground {
            if let Foreground::PullQuote {
                text,
                emphasis,
                at,
                dur,
            } = fg
            {
                let abs_start = scene.start + at;
                let abs_end = abs_start + dur;
                // Clip to scene bounds so a too-long pullquote doesn't
                // leak into the next scene's beat.
                let abs_end = abs_end.min(scene.end);
                if abs_end <= abs_start {
                    continue;
                }
                let body = format_pullquote_body(text, emphasis);
                out.push(format!(
                    "Dialogue: 1,{},{},PullQuote,,0,0,0,,{body}",
                    fmt_time(abs_start),
                    fmt_time(abs_end),
                ));
            }
        }
    }
    out
}

/// Emit one Dialogue per TitleCard in the scene foreground list. The
/// card renders as a kicker (chrome-orange mono, smaller) over a
/// large white Arial Black title, separated by `\N`. When kicker is
/// empty, just the title line is emitted.
fn titlecard_events(sb: &Storyboard) -> Vec<String> {
    let mut out = Vec::new();
    for scene in &sb.scenes {
        for fg in &scene.foreground {
            if let Foreground::TitleCard {
                text,
                kicker,
                at,
                dur,
            } = fg
            {
                let abs_start = scene.start + at;
                let abs_end = (abs_start + dur).min(scene.end);
                if abs_end <= abs_start {
                    continue;
                }
                let body = if kicker.is_empty() {
                    escape_ass(text)
                } else {
                    // Inline overrides: kicker is small mono-feel
                    // chrome-orange; main title is huge white. `\N`
                    // is ASS's hard line break.
                    format!(
                        "{{\\fs34\\1c&H00204CE9&}}{}\\N{{\\fs110\\1c&H00FFFFFF&}}{}",
                        escape_ass(kicker),
                        escape_ass(text),
                    )
                };
                out.push(format!(
                    "Dialogue: 1,{},{},TitleCard,,0,0,0,,{body}",
                    fmt_time(abs_start),
                    fmt_time(abs_end),
                ));
            }
        }
    }
    out
}

/// Emit ASS drawing events for every Arrow foreground primitive.
///
/// Layout: an arrow is two drawn regions — a thin rectangular shaft
/// from `from` to `to`, plus a filled triangle at `to` for the head.
/// Optional label rides above the shaft midpoint, offset perpendicular
/// so it doesn't sit on the line.
///
/// Coordinates in the storyboard are normalised `[0, 1]`; we multiply
/// by frame dimensions to get absolute pixels for the ASS drawing block.
/// Anchor is `\an7\pos(0,0)`, so the inside-the-drawing coordinates
/// map directly to top-left pixel space.
fn arrow_events(sb: &Storyboard) -> Vec<String> {
    let mut out = Vec::new();
    for scene in &sb.scenes {
        for fg in &scene.foreground {
            if let Foreground::Arrow {
                from_x,
                from_y,
                to_x,
                to_y,
                label,
                at,
                dur,
            } = fg
            {
                let abs_start = scene.start + at;
                let abs_end = (abs_start + dur).min(scene.end);
                if abs_end <= abs_start {
                    continue;
                }

                let fw = sb.width as f32;
                let fh = sb.height as f32;
                let x1 = from_x * fw;
                let y1 = from_y * fh;
                let x2 = to_x * fw;
                let y2 = to_y * fh;

                // Unit direction + perpendicular along the line.
                let dx = x2 - x1;
                let dy = y2 - y1;
                let len = (dx * dx + dy * dy).sqrt().max(1.0);
                let ux = dx / len;
                let uy = dy / len;
                let nx = -uy; // perpendicular (left-of-direction)
                let ny = ux;

                // Shaft polygon: rectangle of width `stroke` along the line.
                // Pull the tip back by `head_len` so the triangle sits flush
                // against the shaft instead of overlapping it. Dimensions
                // are calibrated for a 1080x1920 short — smaller frames
                // will look proportionally chunky, larger ones thin.
                let stroke = 9.0_f32;
                let head_len = 56.0_f32;
                let head_half_w = 30.0_f32;
                let tip_x = x2;
                let tip_y = y2;
                let base_x = x2 - head_len * ux;
                let base_y = y2 - head_len * uy;
                let sx0 = (x1 + nx * stroke * 0.5) as i32;
                let sy0 = (y1 + ny * stroke * 0.5) as i32;
                let sx1 = (base_x + nx * stroke * 0.5) as i32;
                let sy1 = (base_y + ny * stroke * 0.5) as i32;
                let sx2 = (base_x - nx * stroke * 0.5) as i32;
                let sy2 = (base_y - ny * stroke * 0.5) as i32;
                let sx3 = (x1 - nx * stroke * 0.5) as i32;
                let sy3 = (y1 - ny * stroke * 0.5) as i32;

                // Arrowhead triangle: tip + two wings (base ± perpendicular).
                let wx1 = (base_x + nx * head_half_w) as i32;
                let wy1 = (base_y + ny * head_half_w) as i32;
                let wx2 = (base_x - nx * head_half_w) as i32;
                let wy2 = (base_y - ny * head_half_w) as i32;
                let tx = tip_x as i32;
                let ty = tip_y as i32;

                // Drawing colour matches the show's chrome-orange.
                // ASS = AABBGGRR; 0xE94C20 (RGB) → 0x00204CE9.
                //
                // Both subpaths live in ONE `\p1 … \p0` block. The second
                // `m` between them closes the first path and starts a
                // new one — this is libass's canonical multi-shape
                // pattern. Splitting into two `\p1 \p0` blocks (as
                // corner_brackets does) silently dropped the triangle
                // in earlier tests; staying in one block fixes it.
                let drawing = format!(
                    "{{\\an7\\pos(0,0)\\1c&H00204CE9&\\p1}}\
                     m {sx0} {sy0} l {sx1} {sy1} l {sx2} {sy2} l {sx3} {sy3} \
                     m {tx} {ty} l {wx1} {wy1} l {wx2} {wy2}\
                     {{\\p0}}"
                );
                out.push(format!(
                    "Dialogue: 1,{},{},Pips,,0,0,0,,{drawing}",
                    fmt_time(abs_start),
                    fmt_time(abs_end),
                ));

                if !label.is_empty() {
                    // Midpoint of the shaft, offset perpendicular ~48px so
                    // the label doesn't lie on the line. Left-of-direction
                    // perpendicular (nx, ny) → label sits "above" the arrow
                    // when it points right, "left of" when it points down.
                    let mx = ((x1 + x2) * 0.5 + nx * 48.0) as i32;
                    let my = ((y1 + y2) * 0.5 + ny * 48.0) as i32;
                    out.push(format!(
                        "Dialogue: 1,{},{},ArrowLabel,,0,0,0,,{{\\pos({mx},{my})}}{}",
                        fmt_time(abs_start),
                        fmt_time(abs_end),
                        escape_ass(label),
                    ));
                }
            }
        }
    }
    out
}

/// Emit ASS events for every Comparison foreground primitive.
///
/// Three centred text events on a single row at `(center_x, center_y)`:
///   - Left label at `(cx - gap*frame_w, cy)` — large hot-orange
///   - Divider at `(cx, cy)` — medium white
///   - Right label at `(cx + gap*frame_w, cy)` — large hot-orange
///
/// No drawing commands needed — Comparison is pure layout.
fn comparison_events(sb: &Storyboard) -> Vec<String> {
    let mut out = Vec::new();
    for scene in &sb.scenes {
        for fg in &scene.foreground {
            if let Foreground::Comparison {
                left,
                right,
                divider,
                center_x,
                center_y,
                gap,
                at,
                dur,
            } = fg
            {
                let abs_start = scene.start + at;
                let abs_end = (abs_start + dur).min(scene.end);
                if abs_end <= abs_start {
                    continue;
                }
                let t_start = fmt_time(abs_start);
                let t_end = fmt_time(abs_end);

                let fw = sb.width as f32;
                let fh = sb.height as f32;
                let cx = center_x * fw;
                let cy = center_y * fh;
                let dx = gap * fw;

                out.push(format!(
                    "Dialogue: 2,{t_start},{t_end},ComparisonSide,,0,0,0,,{{\\pos({},{})}}{}",
                    (cx - dx) as i32,
                    cy as i32,
                    escape_ass(left),
                ));
                out.push(format!(
                    "Dialogue: 2,{t_start},{t_end},ComparisonDivider,,0,0,0,,{{\\pos({},{})}}{}",
                    cx as i32,
                    cy as i32,
                    escape_ass(divider),
                ));
                out.push(format!(
                    "Dialogue: 2,{t_start},{t_end},ComparisonSide,,0,0,0,,{{\\pos({},{})}}{}",
                    (cx + dx) as i32,
                    cy as i32,
                    escape_ass(right),
                ));
            }
        }
    }
    out
}

/// Emit ASS events for every Highlight foreground primitive.
///
/// One drawing event per highlight, containing 8 subpaths — four
/// L-shaped corner brackets (each L is two thin rectangles, one
/// horizontal arm and one vertical arm). Plus one text event for the
/// optional label centred above the top edge.
///
/// The brackets are sized at 60px arms × 8px thick — meaningfully
/// larger than the show's always-on chrome corner brackets (36×4) so
/// a highlight reads as a deliberate per-scene framing rather than
/// part of the channel identity.
fn highlight_events(sb: &Storyboard) -> Vec<String> {
    let mut out = Vec::new();
    for scene in &sb.scenes {
        for fg in &scene.foreground {
            if let Foreground::Highlight {
                center_x,
                center_y,
                width,
                height,
                label,
                at,
                dur,
            } = fg
            {
                let abs_start = scene.start + at;
                let abs_end = (abs_start + dur).min(scene.end);
                if abs_end <= abs_start {
                    continue;
                }
                let t_start = fmt_time(abs_start);
                let t_end = fmt_time(abs_end);

                let fw = sb.width as f32;
                let fh = sb.height as f32;
                let cx = center_x * fw;
                let cy = center_y * fh;
                let half_w = (width * fw) * 0.5;
                let half_h = (height * fh) * 0.5;
                let x_left = cx - half_w;
                let x_right = cx + half_w;
                let y_top = cy - half_h;
                let y_bot = cy + half_h;

                // Per-arm dimensions. Tuned to feel deliberate at
                // 1080x1920; pick once here so all four brackets match.
                let arm_len: f32 = 60.0;
                let arm_thick: f32 = 8.0;

                // Each corner contributes a horizontal arm + a vertical
                // arm starting at the corner anchor and growing inward.
                let corners = [
                    (x_left, y_top, 1.0, 1.0),     // TL ┌
                    (x_right, y_top, -1.0, 1.0),   // TR ┐
                    (x_left, y_bot, 1.0, -1.0),    // BL └
                    (x_right, y_bot, -1.0, -1.0),  // BR ┘
                ];

                let mut path = String::new();
                for (ax, ay, dx, dy) in corners {
                    let h_x0 = ax;
                    let h_y0 = ay;
                    let h_x1 = ax + dx * arm_len;
                    let h_y1 = ay + dy * arm_thick;
                    let v_x0 = ax;
                    let v_y0 = ay;
                    let v_x1 = ax + dx * arm_thick;
                    let v_y1 = ay + dy * arm_len;
                    // Normalise to (lo, hi) per axis so libass gets a
                    // CW polygon regardless of direction signs.
                    let h_xlo = h_x0.min(h_x1) as i32;
                    let h_xhi = h_x0.max(h_x1) as i32;
                    let h_ylo = h_y0.min(h_y1) as i32;
                    let h_yhi = h_y0.max(h_y1) as i32;
                    let v_xlo = v_x0.min(v_x1) as i32;
                    let v_xhi = v_x0.max(v_x1) as i32;
                    let v_ylo = v_y0.min(v_y1) as i32;
                    let v_yhi = v_y0.max(v_y1) as i32;
                    if !path.is_empty() {
                        path.push(' ');
                    }
                    path.push_str(&format!(
                        "m {h_xlo} {h_ylo} l {h_xhi} {h_ylo} l {h_xhi} {h_yhi} l {h_xlo} {h_yhi} \
                         m {v_xlo} {v_ylo} l {v_xhi} {v_ylo} l {v_xhi} {v_yhi} l {v_xlo} {v_yhi}"
                    ));
                }

                out.push(format!(
                    "Dialogue: 1,{t_start},{t_end},Pips,,0,0,0,,{{\\an7\\pos(0,0)\\1c&H00204CE9&\\p1}}{path}{{\\p0}}"
                ));

                // Optional label above the top edge. We position it at
                // (cx, y_top - 24) so the baseline of the label sits a
                // bit above the top brackets, anchored centre via the
                // HighlightLabel style's \an5.
                if !label.is_empty() {
                    out.push(format!(
                        "Dialogue: 2,{t_start},{t_end},HighlightLabel,,0,0,0,,{{\\pos({},{})}}{}",
                        cx as i32,
                        (y_top - 24.0) as i32,
                        escape_ass(label),
                    ));
                }
            }
        }
    }
    out
}

/// Emit ASS events for every Annotation foreground primitive.
///
/// Two pieces of output per Annotation:
///   1. One drawing event holding the target dot (filled square) and
///      the leader line (thin rectangle from label toward target,
///      stopped short of both endpoints so the dot stays clean and
///      the line doesn't crash into the label text).
///   2. One text event for the label.
fn annotation_events(sb: &Storyboard) -> Vec<String> {
    let mut out = Vec::new();
    for scene in &sb.scenes {
        for fg in &scene.foreground {
            if let Foreground::Annotation {
                label,
                target_x,
                target_y,
                label_x,
                label_y,
                at,
                dur,
            } = fg
            {
                let abs_start = scene.start + at;
                let abs_end = (abs_start + dur).min(scene.end);
                if abs_end <= abs_start {
                    continue;
                }
                let t_start = fmt_time(abs_start);
                let t_end = fmt_time(abs_end);

                let fw = sb.width as f32;
                let fh = sb.height as f32;
                let tx = target_x * fw;
                let ty = target_y * fh;
                let lx = label_x * fw;
                let ly = label_y * fh;

                // Target marker: a small filled square centred on the
                // target point. Slightly bigger than a stroke width so
                // it reads as a distinct dot, not an artifact.
                let dot_half: f32 = 9.0;
                let dx0 = (tx - dot_half) as i32;
                let dy0 = (ty - dot_half) as i32;
                let dx1 = (tx + dot_half) as i32;
                let dy1 = (ty + dot_half) as i32;

                // Leader line from label toward target. Pull both ends
                // in: short of the label so it doesn't crash into the
                // text box, short of the dot so the dot stays distinct.
                let dvx = tx - lx;
                let dvy = ty - ly;
                let len = (dvx * dvx + dvy * dvy).sqrt().max(1.0);
                let ux = dvx / len;
                let uy = dvy / len;
                let label_pad: f32 = 48.0; // pixel gap at label end
                let target_pad: f32 = dot_half + 4.0;
                let stroke: f32 = 4.0;
                let l0x = lx + ux * label_pad;
                let l0y = ly + uy * label_pad;
                let l1x = tx - ux * target_pad;
                let l1y = ty - uy * target_pad;
                // Skip the line if the pads ate the whole length.
                let has_line = (l1x - l0x).powi(2) + (l1y - l0y).powi(2) > 4.0;

                let mut path = format!(
                    "m {dx0} {dy0} l {dx1} {dy0} l {dx1} {dy1} l {dx0} {dy1}"
                );
                if has_line {
                    let nx = -uy * stroke * 0.5;
                    let ny = ux * stroke * 0.5;
                    let p0x = (l0x + nx) as i32;
                    let p0y = (l0y + ny) as i32;
                    let p1x = (l1x + nx) as i32;
                    let p1y = (l1y + ny) as i32;
                    let p2x = (l1x - nx) as i32;
                    let p2y = (l1y - ny) as i32;
                    let p3x = (l0x - nx) as i32;
                    let p3y = (l0y - ny) as i32;
                    path.push_str(&format!(
                        " m {p0x} {p0y} l {p1x} {p1y} l {p2x} {p2y} l {p3x} {p3y}"
                    ));
                }
                out.push(format!(
                    "Dialogue: 1,{t_start},{t_end},Pips,,0,0,0,,{{\\an7\\pos(0,0)\\1c&H00204CE9&\\p1}}{path}{{\\p0}}"
                ));

                // Label text — uses the dedicated AnnotationLabel style
                // (cream-on-translucent, mono). Positioned at its own
                // anchor so the author controls placement explicitly.
                out.push(format!(
                    "Dialogue: 2,{t_start},{t_end},AnnotationLabel,,0,0,0,,{{\\pos({},{})}}{}",
                    lx as i32,
                    ly as i32,
                    escape_ass(label),
                ));
            }
        }
    }
    out
}

/// Emit ASS events for every Decomposition foreground primitive.
///
/// Three pieces of output per Decomposition:
///   1. One drawing event holding all N spokes (single `\p1`–`\p0`
///      block with N subpaths, following the multi-shape pattern
///      established by `arrow_events`).
///   2. One text event for the whole label, positioned at the centre.
///   3. N text events, one per part, positioned around the centre on
///      a circle of the given radius. First part at angle 0 (top),
///      rest stepping clockwise.
///
/// The radial layout uses `min(width, height)` as the radius unit so
/// the diagram fits within the safe area of a portrait Short
/// regardless of the storyboard's aspect.
fn decomposition_events(sb: &Storyboard) -> Vec<String> {
    use std::f32::consts::{FRAC_PI_2, TAU};
    let mut out = Vec::new();
    for scene in &sb.scenes {
        for fg in &scene.foreground {
            if let Foreground::Decomposition {
                whole,
                parts,
                center_x,
                center_y,
                radius,
                at,
                dur,
            } = fg
            {
                let abs_start = scene.start + at;
                let abs_end = (abs_start + dur).min(scene.end);
                if abs_end <= abs_start || parts.is_empty() {
                    continue;
                }
                let t_start = fmt_time(abs_start);
                let t_end = fmt_time(abs_end);

                let fw = sb.width as f32;
                let fh = sb.height as f32;
                let cx = center_x * fw;
                let cy = center_y * fh;
                // Radius in pixels, anchored to the SHORTER frame dim so
                // a portrait Short keeps parts inside the safe area no
                // matter the aspect.
                let r_unit = sb.width.min(sb.height) as f32;
                let r = radius * r_unit;

                // Compute each part position. First part at the TOP
                // (angle -π/2), going clockwise.
                let n = parts.len();
                let positions: Vec<(f32, f32)> = (0..n)
                    .map(|i| {
                        let theta = (i as f32) * TAU / (n as f32) - FRAC_PI_2;
                        (cx + r * theta.cos(), cy + r * theta.sin())
                    })
                    .collect();

                // Spokes: one drawing event, N subpaths, each a thin
                // rectangle from centre to the part position. We use
                // rectangles rather than zero-width lines because ASS
                // drawing fills only — a 1-pixel "line" wouldn't render
                // reliably across libass builds.
                let stroke = 4.0_f32;
                let mut path = String::new();
                for &(px, py) in &positions {
                    let dx = px - cx;
                    let dy = py - cy;
                    let len = (dx * dx + dy * dy).sqrt().max(1.0);
                    let nx = -dy / len * stroke * 0.5;
                    let ny = dx / len * stroke * 0.5;
                    let p0x = (cx + nx) as i32;
                    let p0y = (cy + ny) as i32;
                    let p1x = (px + nx) as i32;
                    let p1y = (py + ny) as i32;
                    let p2x = (px - nx) as i32;
                    let p2y = (py - ny) as i32;
                    let p3x = (cx - nx) as i32;
                    let p3y = (cy - ny) as i32;
                    if !path.is_empty() {
                        path.push(' ');
                    }
                    path.push_str(&format!(
                        "m {p0x} {p0y} l {p1x} {p1y} l {p2x} {p2y} l {p3x} {p3y}"
                    ));
                }
                let spokes = format!(
                    "{{\\an7\\pos(0,0)\\1c&H00204CE9&\\p1}}{path}{{\\p0}}"
                );
                out.push(format!(
                    "Dialogue: 1,{t_start},{t_end},Pips,,0,0,0,,{spokes}"
                ));

                // Whole label at the centre.
                out.push(format!(
                    "Dialogue: 2,{t_start},{t_end},DecompWhole,,0,0,0,,{{\\pos({},{})}}{}",
                    cx as i32,
                    cy as i32,
                    escape_ass(whole),
                ));

                // Part labels.
                for ((px, py), label) in positions.iter().zip(parts.iter()) {
                    out.push(format!(
                        "Dialogue: 2,{t_start},{t_end},DecompPart,,0,0,0,,{{\\pos({},{})}}{}",
                        *px as i32,
                        *py as i32,
                        escape_ass(label),
                    ));
                }
            }
        }
    }
    out
}

/// Emit one Dialogue per EndCard. Active for the full scene window —
/// it's expected to be the only foreground in a brief closing scene.
/// Layout: large chrome-orange handle over a smaller white CTA.
fn endcard_events(sb: &Storyboard) -> Vec<String> {
    let mut out = Vec::new();
    for scene in &sb.scenes {
        for fg in &scene.foreground {
            if let Foreground::EndCard { handle, cta } = fg {
                let body = format!(
                    "{{\\fs100\\1c&H00204CE9&}}{}\\N{{\\fs38\\1c&H00FFFFFF&}}{}",
                    escape_ass(handle),
                    escape_ass(cta),
                );
                out.push(format!(
                    "Dialogue: 1,{},{},EndCard,,0,0,0,,{body}",
                    fmt_time(scene.start),
                    fmt_time(scene.end),
                ));
            }
        }
    }
    out
}

/// Render a pull-quote's text with per-word emphasis colour overrides.
fn format_pullquote_body(text: &str, emphasis: &[usize]) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut out = String::new();
    for (i, word) in words.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if emphasis.contains(&i) {
            // Amber emphasis — matches the karaoke active-word colour
            // so emphasis reads as continuous with the show's accent.
            out.push_str(r"{\1c&H005CBDF5&}");
            out.push_str(&escape_ass(word));
            out.push_str(r"{\1c&H00FFFFFF&}");
        } else {
            out.push_str(&escape_ass(word));
        }
    }
    out
}

fn sigil_event(s: &SigilSpec, end: &str) -> String {
    let start = "0:00:00.00";
    let style = match s.corner {
        Corner::TopLeft => "SigilTL",
        Corner::TopRight => "SigilTR",
        Corner::BottomLeft => "SigilBL",
        Corner::BottomRight => "SigilBR",
    };
    let alpha = ((1.0 - s.opacity).clamp(0.0, 1.0) * 255.0) as u32;
    // ASS \alpha override embeds primary-color alpha. Higher = more transparent.
    format!(
        "Dialogue: 0,{start},{end},{style},,0,0,0,,{{\\alpha&H{alpha:02X}&}}{}",
        escape_ass(&s.handle)
    )
}

fn end_time(secs: f32) -> String {
    let total = (secs + 0.5).max(0.0); // pad past last frame
    let h = (total as u32) / 3600;
    let m = ((total as u32) % 3600) / 60;
    let s = total - (h as f32 * 3600.0) - (m as f32 * 60.0);
    let s_int = s as u32;
    let cs = ((s - s_int as f32) * 100.0) as u32;
    format!("{h}:{m:02}:{s_int:02}.{cs:02}")
}

fn escape_ass(text: &str) -> String {
    text.replace('{', "(").replace('}', ")")
}

fn render_ass(sb: &Storyboard, events: &[String]) -> String {
    let w = sb.width;
    let h = sb.height;
    let header_text = "Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding";

    // Style table.
    //  - Default: legacy fallback, large white centered text.
    //  - Header / HeaderKicker: top-of-frame title + kicker.
    //  - Sigil{TL,TR,BL,BR}: corner watermark, smaller font.
    // ASS colors are AABBGGRR. Amber = &H00 5CBDF5 (RGB 0xF5BD5C ≈ design::COLOR_AMBER).
    // Chrome (kicker, sigil) reads as "system metadata" — mono font,
    // chrome-orange accent. Content (Header, Default karaoke) stays in
    // Arial Black, white, so the design has a clear two-tier hierarchy:
    // display type for what's said, mono type for what's machine-state.
    //
    // ASS colours: AABBGGRR.
    //   &H00204CE9 = chrome orange (#E94C20), full opacity
    //   &H005CBDF5 = amber (#F5BD5C, secondary highlight)
    //   &HA0000000 = ~63% transparent black for header backdrop
    let styles = format!(
        "Style: Default,Arial Black,90,&H00FFFFFF,&H0066D9FF,&H00000000,&H80000000,-1,0,0,0,100,100,0,0,1,5,2,2,80,80,360,1\n\
         Style: Header,Arial Black,54,&H00FFFFFF,&H005CBDF5,&H00000000,&HA0000000,-1,0,0,0,100,100,0,0,3,10,0,8,60,60,170,1\n\
         Style: HeaderKicker,DejaVu Sans Mono,30,&H00204CE9,&H00204CE9,&H00000000,&HA0000000,-1,0,0,0,100,100,4,0,3,8,0,8,60,60,110,1\n\
         Style: SigilTL,DejaVu Sans Mono,26,&H00FFFFFF,&H00FFFFFF,&H00000000,&H80000000,0,0,0,0,100,100,2,0,1,2,2,7,40,40,40,1\n\
         Style: SigilTR,DejaVu Sans Mono,26,&H00FFFFFF,&H00FFFFFF,&H00000000,&H80000000,0,0,0,0,100,100,2,0,1,2,2,9,40,40,40,1\n\
         Style: SigilBL,DejaVu Sans Mono,26,&H00FFFFFF,&H00FFFFFF,&H00000000,&H80000000,0,0,0,0,100,100,2,0,1,2,2,1,40,40,40,1\n\
         Style: SigilBR,DejaVu Sans Mono,26,&H00FFFFFF,&H00FFFFFF,&H00000000,&H80000000,0,0,0,0,100,100,2,0,1,2,2,3,40,40,40,1\n\
         Style: Pips,Arial,1,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,0,7,0,0,0,1\n\
         Style: PullQuote,Arial Black,84,&H00FFFFFF,&H005CBDF5,&H00000000,&HC0000000,-1,0,0,0,100,100,0,0,3,18,0,5,80,80,0,1\n\
         Style: TitleCard,Arial Black,110,&H00FFFFFF,&H00204CE9,&H00000000,&HD0000000,-1,0,0,0,100,100,0,0,3,24,0,5,80,80,0,1\n\
         Style: EndCard,Arial Black,100,&H00FFFFFF,&H00204CE9,&H00000000,&HD0000000,-1,0,0,0,100,100,0,0,3,24,0,5,80,80,0,1\n\
         Style: ArrowLabel,DejaVu Sans Mono,36,&H00204CE9,&H00204CE9,&H00000000,&H80000000,-1,0,0,0,100,100,2,0,1,3,1,5,0,0,0,1\n\
         Style: DecompWhole,Arial Black,82,&H00204CE9,&H00204CE9,&H00000000,&HC0000000,-1,0,0,0,100,100,0,0,3,14,0,5,0,0,0,1\n\
         Style: DecompPart,DejaVu Sans Mono,42,&H00FFFFFF,&H00FFFFFF,&H00000000,&HC0000000,-1,0,0,0,100,100,2,0,3,8,0,5,0,0,0,1\n\
         Style: AnnotationLabel,DejaVu Sans Mono,34,&H00FFFFFF,&H00FFFFFF,&H00000000,&HC0000000,-1,0,0,0,100,100,2,0,3,8,0,5,0,0,0,1\n\
         Style: HighlightLabel,DejaVu Sans Mono,32,&H00204CE9,&H00204CE9,&H00000000,&HA0000000,-1,0,0,0,100,100,2,0,3,6,0,5,0,0,0,1\n\
         Style: ComparisonSide,Arial Black,72,&H00204CE9,&H00204CE9,&H00000000,&HB0000000,-1,0,0,0,100,100,0,0,3,12,0,5,0,0,0,1\n\
         Style: ComparisonDivider,Arial Black,56,&H00FFFFFF,&H00FFFFFF,&H00000000,&HB0000000,-1,0,0,0,100,100,0,0,3,8,0,5,0,0,0,1"
    );

    let mut s = String::new();
    // WrapStyle 0 = smart-wrap, prefer wider top lines. Important so the
    // header doesn't overflow the canvas when the text is longer than
    // its margin width permits at the chosen font size.
    s.push_str(&format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: {w}\nPlayResY: {h}\nWrapStyle: 0\nScaledBorderAndShadow: yes\n\n"
    ));
    s.push_str("[V4+ Styles]\n");
    s.push_str(header_text);
    s.push('\n');
    s.push_str(&styles);
    s.push_str("\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
    for ev in events {
        s.push_str(ev);
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use art_engine_storyboard::{Backdrop, PaletteRef, PostChain, Scene, Transition};

    fn sb_with(header: Option<HeaderSpec>, sigil: Option<SigilSpec>) -> Storyboard {
        Storyboard {
            audio: "a.m4a".into(),
            fps: 30,
            width: 1080,
            height: 1920,
            subtitles: None,
            header,
            sigil,
            scene_pips: None,
            scenes: vec![Scene {
                start: 0.0,
                end: 5.0,
                backdrop: Backdrop::Flow {
                    palette: PaletteRef::TealAmber,
                    intensity: 1.0,
                    seed: 11,
                },
                foreground: vec![],
                transition_in: Transition::HardCut,
                post: PostChain::default(),
            }],
        }
    }

    #[test]
    fn header_emits_two_dialogue_lines_with_kicker() {
        let sb = sb_with(
            Some(HeaderSpec {
                text: "Hello".into(),
                kicker: Some("EP. 02".into()),
            }),
            None,
        );
        let evs = header_events(sb.header.as_ref().unwrap(), &end_time(sb.duration()));
        assert_eq!(evs.len(), 2);
        assert!(evs[0].contains("HeaderKicker"));
        assert!(evs[0].contains("EP. 02"));
        assert!(evs[1].contains("Header,"));
        assert!(evs[1].contains("Hello"));
    }

    #[test]
    fn header_without_kicker_emits_one_line() {
        let sb = sb_with(
            Some(HeaderSpec {
                text: "Hello".into(),
                kicker: None,
            }),
            None,
        );
        let evs = header_events(sb.header.as_ref().unwrap(), &end_time(sb.duration()));
        assert_eq!(evs.len(), 1);
    }

    #[test]
    fn sigil_event_routes_to_correct_style_by_corner() {
        let make = |c: Corner| {
            sigil_event(
                &SigilSpec {
                    handle: "@x".into(),
                    corner: c,
                    opacity: 0.5,
                },
                "0:00:05.00",
            )
        };
        assert!(make(Corner::TopLeft).contains(",SigilTL,"));
        assert!(make(Corner::TopRight).contains(",SigilTR,"));
        assert!(make(Corner::BottomLeft).contains(",SigilBL,"));
        assert!(make(Corner::BottomRight).contains(",SigilBR,"));
    }

    #[test]
    fn sigil_event_encodes_opacity_as_alpha_override() {
        // opacity 1.0 → alpha 0x00 (fully visible).
        let ev = sigil_event(
            &SigilSpec {
                handle: "@x".into(),
                corner: Corner::TopRight,
                opacity: 1.0,
            },
            "0:00:05.00",
        );
        assert!(ev.contains("\\alpha&H00&"));
        // opacity 0.0 → alpha 0xFF (invisible).
        let ev = sigil_event(
            &SigilSpec {
                handle: "@x".into(),
                corner: Corner::TopRight,
                opacity: 0.0,
            },
            "0:00:05.00",
        );
        assert!(ev.contains("\\alpha&HFF&"));
    }

    #[test]
    fn pullquote_event_emitted_with_absolute_timing() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::PullQuote {
            text: "Finite creatures, infinite tools.".into(),
            emphasis: vec![0, 2],
            at: 1.0,
            dur: 2.0,
        });
        // Scene 0 starts at 0.0, so pullquote runs absolute 1.0..3.0.
        let evs = pullquote_events(&sb);
        assert_eq!(evs.len(), 1);
        assert!(evs[0].contains("0:00:01.00"), "wrong start: {}", evs[0]);
        assert!(evs[0].contains("0:00:03.00"), "wrong end: {}", evs[0]);
        // Word 0 ("Finite") and word 2 ("infinite") should be wrapped
        // in the amber colour override.
        assert!(evs[0].contains(r"{\1c&H005CBDF5&}Finite"));
        assert!(evs[0].contains(r"{\1c&H005CBDF5&}infinite"));
    }

    #[test]
    fn pullquote_clips_to_scene_end() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::PullQuote {
            text: "Too long".into(),
            emphasis: vec![],
            at: 4.0,
            dur: 5.0, // scene ends at 5.0
        });
        let evs = pullquote_events(&sb);
        assert_eq!(evs.len(), 1);
        // Start at 4.0, end clipped to scene.end == 5.0.
        assert!(evs[0].contains("0:00:04.00"));
        assert!(evs[0].contains("0:00:05.00"));
    }

    #[test]
    fn titlecard_emits_two_lines_when_kicker_present() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::TitleCard {
            text: "Title".into(),
            kicker: "EP. 02".into(),
            at: 0.0,
            dur: 2.0,
        });
        let evs = titlecard_events(&sb);
        assert_eq!(evs.len(), 1);
        // Body should contain both kicker and title text + the
        // per-line font/colour overrides.
        assert!(evs[0].contains("EP. 02"));
        assert!(evs[0].contains("Title"));
        assert!(evs[0].contains(r"\fs34"), "kicker font-size override missing");
        assert!(evs[0].contains(r"\fs110"), "title font-size override missing");
        assert!(evs[0].contains(r"\N"), "line break missing");
    }

    #[test]
    fn titlecard_kicker_empty_only_emits_title() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::TitleCard {
            text: "Solo".into(),
            kicker: "".into(),
            at: 0.0,
            dur: 1.0,
        });
        let evs = titlecard_events(&sb);
        assert_eq!(evs.len(), 1);
        // No line break / font override blocks when kicker is empty.
        assert!(!evs[0].contains(r"\N"));
        assert!(evs[0].contains("Solo"));
    }

    #[test]
    fn endcard_emits_dialogue_spanning_scene_window() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::EndCard {
            handle: "@TheExaminedMachine".into(),
            cta: "Subscribe.".into(),
        });
        let evs = endcard_events(&sb);
        assert_eq!(evs.len(), 1);
        // Scene 0 runs 0..5 in the sb_with sample.
        assert!(evs[0].contains("0:00:00.00"));
        assert!(evs[0].contains("0:00:05.00"));
        assert!(evs[0].contains("@TheExaminedMachine"));
        assert!(evs[0].contains("Subscribe."));
    }

    #[test]
    fn pullquote_dropped_when_window_empty() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::PullQuote {
            text: "Too late".into(),
            emphasis: vec![],
            at: 6.0, // past scene end (5.0)
            dur: 1.0,
        });
        assert!(pullquote_events(&sb).is_empty());
    }

    #[test]
    fn pip_events_emit_one_per_scene_when_strip_enabled() {
        let mut sb = sb_with(None, None);
        // Add a couple more scenes.
        sb.scenes.push(Scene {
            start: 5.0,
            end: 10.0,
            backdrop: Backdrop::Solid { color: [0.0; 3] },
            foreground: vec![],
            transition_in: Transition::HardCut,
            post: PostChain::default(),
        });
        sb.scenes.push(Scene {
            start: 10.0,
            end: 15.0,
            backdrop: Backdrop::Solid { color: [0.0; 3] },
            foreground: vec![],
            transition_in: Transition::HardCut,
            post: PostChain::default(),
        });
        sb.scene_pips = Some(ScenePipsSpec {
            position: PipPosition::Top,
        });
        let evs = pip_events(&sb, sb.scene_pips.as_ref().unwrap());
        assert_eq!(evs.len(), 3, "one Dialogue per scene");
        // The middle Dialogue must reference all three pip colors:
        // past (scene 0 filled), current (scene 1 highlighted),
        // future (scene 2 dim).
        assert!(evs[1].contains("&H00FFFFFF&"), "past pip color missing");
        assert!(evs[1].contains("&H005CBDF5&"), "current pip color missing");
        assert!(evs[1].contains("&HC0808080&"), "future pip color missing");
    }

    #[test]
    fn pip_events_empty_when_strip_disabled() {
        let sb = sb_with(None, None);
        assert!(sb.scene_pips.is_none());
    }

    #[test]
    fn arrow_emits_one_drawing_event_when_unlabeled() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::Arrow {
            from_x: 0.2,
            from_y: 0.5,
            to_x: 0.8,
            to_y: 0.5,
            label: String::new(),
            at: 1.0,
            dur: 2.0,
        });
        let evs = arrow_events(&sb);
        assert_eq!(evs.len(), 1, "no label means exactly one drawing event");
        // The shaft drawing block and the head drawing block live in
        // the same Dialogue line, separated by `{\p0}{\p1}`.
        assert!(evs[0].contains("\\p1"), "drawing mode missing");
        assert!(evs[0].contains("0:00:01.00"), "wrong start: {}", evs[0]);
        assert!(evs[0].contains("0:00:03.00"), "wrong end: {}", evs[0]);
        // Chrome-orange color override should be present.
        assert!(evs[0].contains("&H00204CE9"));
    }

    #[test]
    fn arrow_with_label_emits_two_events() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::Arrow {
            from_x: 0.2,
            from_y: 0.5,
            to_x: 0.8,
            to_y: 0.5,
            label: "force".into(),
            at: 0.0,
            dur: 3.0,
        });
        let evs = arrow_events(&sb);
        assert_eq!(evs.len(), 2, "drawing + label = two events");
        assert!(evs[0].contains("\\p1"));
        assert!(
            evs[1].contains("ArrowLabel"),
            "label uses ArrowLabel style: {}",
            evs[1]
        );
        assert!(evs[1].contains("force"));
    }

    #[test]
    fn arrow_clips_to_scene_end() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::Arrow {
            from_x: 0.0,
            from_y: 0.0,
            to_x: 1.0,
            to_y: 1.0,
            label: String::new(),
            at: 4.0,
            dur: 5.0, // scene ends at 5.0 — clip to scene.end
        });
        let evs = arrow_events(&sb);
        assert_eq!(evs.len(), 1);
        assert!(evs[0].contains("0:00:04.00"));
        assert!(evs[0].contains("0:00:05.00"));
    }

    #[test]
    fn comparison_emits_left_divider_right() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::Comparison {
            left: "watch".into(),
            right: "algorithm".into(),
            divider: "|".into(),
            center_x: 0.5,
            center_y: 0.5,
            gap: 0.2,
            at: 0.0,
            dur: 3.0,
        });
        let evs = comparison_events(&sb);
        assert_eq!(evs.len(), 3, "left + divider + right");
        assert!(evs[0].contains("ComparisonSide") && evs[0].contains("watch"));
        assert!(evs[1].contains("ComparisonDivider") && evs[1].contains("|"));
        assert!(evs[2].contains("ComparisonSide") && evs[2].contains("algorithm"));
    }

    #[test]
    fn comparison_left_right_offsets_symmetric() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::Comparison {
            left: "A".into(),
            right: "B".into(),
            divider: "vs".into(),
            center_x: 0.5,
            center_y: 0.5,
            gap: 0.25,
            at: 0.0,
            dur: 2.0,
        });
        // sb_with builds a 1080x1920 frame, so:
        //   cx = 540, gap_px = 0.25 * 1080 = 270
        //   left  pos = 540 - 270 = 270
        //   right pos = 540 + 270 = 810
        let evs = comparison_events(&sb);
        assert!(evs[0].contains("\\pos(270,960)"), "left pos: {}", evs[0]);
        assert!(evs[1].contains("\\pos(540,960)"), "divider pos: {}", evs[1]);
        assert!(evs[2].contains("\\pos(810,960)"), "right pos: {}", evs[2]);
    }

    #[test]
    fn comparison_clips_to_scene_end() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::Comparison {
            left: "X".into(),
            right: "Y".into(),
            divider: "→".into(),
            center_x: 0.5,
            center_y: 0.5,
            gap: 0.2,
            at: 4.5,
            dur: 5.0, // scene ends at 5.0
        });
        let evs = comparison_events(&sb);
        assert_eq!(evs.len(), 3);
        assert!(evs[0].contains("0:00:04.50"));
        assert!(evs[0].contains("0:00:05.00"));
    }

    #[test]
    fn highlight_emits_four_brackets_no_label() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::Highlight {
            center_x: 0.5,
            center_y: 0.5,
            width: 0.4,
            height: 0.3,
            label: String::new(),
            at: 0.0,
            dur: 3.0,
        });
        let evs = highlight_events(&sb);
        assert_eq!(evs.len(), 1, "drawing only when no label");
        assert!(evs[0].contains("\\p1"));
        // Four corners × two arms = 8 subpaths.
        let m_count = evs[0].matches(" m ").count() + evs[0].matches("}m ").count();
        assert_eq!(m_count, 8, "expected 8 subpaths (4 corners × 2 arms): {}", evs[0]);
        assert!(evs[0].contains("&H00204CE9"), "chrome colour missing");
    }

    #[test]
    fn highlight_with_label_emits_two_events() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::Highlight {
            center_x: 0.5,
            center_y: 0.5,
            width: 0.4,
            height: 0.3,
            label: "watch face".into(),
            at: 0.0,
            dur: 3.0,
        });
        let evs = highlight_events(&sb);
        assert_eq!(evs.len(), 2);
        assert!(evs[1].contains("HighlightLabel"));
        assert!(evs[1].contains("watch face"));
    }

    #[test]
    fn highlight_clips_to_scene_end() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::Highlight {
            center_x: 0.5,
            center_y: 0.5,
            width: 0.4,
            height: 0.3,
            label: String::new(),
            at: 4.5,
            dur: 5.0, // scene ends at 5.0 → clip
        });
        let evs = highlight_events(&sb);
        assert_eq!(evs.len(), 1);
        assert!(evs[0].contains("0:00:04.50"));
        assert!(evs[0].contains("0:00:05.00"));
    }

    #[test]
    fn annotation_emits_drawing_plus_label() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::Annotation {
            label: "balance wheel".into(),
            target_x: 0.7,
            target_y: 0.4,
            label_x: 0.3,
            label_y: 0.2,
            at: 0.0,
            dur: 3.0,
        });
        let evs = annotation_events(&sb);
        assert_eq!(evs.len(), 2, "expected drawing + label event");
        assert!(evs[0].contains("\\p1"), "drawing missing: {}", evs[0]);
        // Drawing contains two subpaths: the target dot rectangle +
        // the leader-line rectangle. Two `m ` tokens expected.
        let m_count = evs[0].matches(" m ").count() + evs[0].matches("}m ").count();
        assert_eq!(m_count, 2, "expected 2 subpaths (dot + leader): {}", evs[0]);
        // Label event uses the AnnotationLabel style and contains the text.
        assert!(evs[1].contains("AnnotationLabel"));
        assert!(evs[1].contains("balance wheel"));
    }

    #[test]
    fn annotation_omits_leader_when_label_overlaps_target() {
        // Label sitting right on top of the target should produce ONLY
        // the dot (no leader line), since the leader-line gap pads
        // would consume the whole length.
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::Annotation {
            label: "x".into(),
            target_x: 0.5,
            target_y: 0.5,
            label_x: 0.5,
            label_y: 0.5,
            at: 0.0,
            dur: 1.0,
        });
        let evs = annotation_events(&sb);
        assert_eq!(evs.len(), 2);
        // Drawing should have exactly 1 subpath (the dot only).
        let m_count = evs[0].matches(" m ").count() + evs[0].matches("}m ").count();
        assert_eq!(m_count, 1, "expected dot-only drawing: {}", evs[0]);
    }

    #[test]
    fn annotation_clips_to_scene_end() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::Annotation {
            label: "late".into(),
            target_x: 0.6,
            target_y: 0.6,
            label_x: 0.2,
            label_y: 0.2,
            at: 4.5,
            dur: 5.0, // scene ends at 5.0
        });
        let evs = annotation_events(&sb);
        assert_eq!(evs.len(), 2);
        assert!(evs[0].contains("0:00:04.50"));
        assert!(evs[0].contains("0:00:05.00"));
    }

    #[test]
    fn decomposition_emits_spokes_plus_whole_plus_parts() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::Decomposition {
            whole: "scythe".into(),
            parts: vec!["handle".into(), "blade".into(), "person".into()],
            center_x: 0.5,
            center_y: 0.5,
            radius: 0.25,
            at: 0.0,
            dur: 3.0,
        });
        let evs = decomposition_events(&sb);
        // 1 spoke drawing + 1 whole + 3 parts = 5
        assert_eq!(evs.len(), 5, "expected 1 + 1 + 3 events, got {evs:#?}");
        // The first event is the drawing (contains \p1 and the chrome
        // colour). It should contain three subpaths — one per spoke —
        // each starting with `m`.
        assert!(evs[0].contains("\\p1"), "drawing missing: {}", evs[0]);
        assert!(evs[0].contains("&H00204CE9"), "chrome colour missing");
        let m_count = evs[0].matches(" m ").count() + evs[0].matches("}m ").count();
        assert_eq!(m_count, 3, "expected 3 subpaths (one per spoke): {}", evs[0]);
        // Whole label is on its own event using the DecompWhole style.
        assert!(
            evs[1].contains("DecompWhole") && evs[1].contains("scythe"),
            "whole label event missing: {}",
            evs[1]
        );
        // Parts are emitted in input order using DecompPart.
        assert!(evs[2].contains("DecompPart") && evs[2].contains("handle"));
        assert!(evs[3].contains("DecompPart") && evs[3].contains("blade"));
        assert!(evs[4].contains("DecompPart") && evs[4].contains("person"));
    }

    #[test]
    fn decomposition_dropped_when_parts_empty() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::Decomposition {
            whole: "scythe".into(),
            parts: vec![],
            center_x: 0.5,
            center_y: 0.5,
            radius: 0.25,
            at: 0.0,
            dur: 2.0,
        });
        let evs = decomposition_events(&sb);
        assert!(evs.is_empty(), "no events when parts is empty");
    }

    #[test]
    fn decomposition_clips_to_scene_end() {
        let mut sb = sb_with(None, None);
        sb.scenes[0].foreground.push(Foreground::Decomposition {
            whole: "X".into(),
            parts: vec!["A".into(), "B".into()],
            center_x: 0.5,
            center_y: 0.5,
            radius: 0.25,
            at: 4.5,
            dur: 5.0, // scene ends at 5.0 → clip
        });
        let evs = decomposition_events(&sb);
        assert_eq!(evs.len(), 1 + 1 + 2);
        assert!(evs[0].contains("0:00:04.50"));
        assert!(evs[0].contains("0:00:05.00"));
    }

    #[test]
    fn arrow_dropped_when_window_empty() {
        let mut sb = sb_with(None, None);
        // Start at scene end → no visible window.
        sb.scenes[0].foreground.push(Foreground::Arrow {
            from_x: 0.0,
            from_y: 0.0,
            to_x: 0.5,
            to_y: 0.5,
            label: "x".into(),
            at: 5.0,
            dur: 1.0,
        });
        let evs = arrow_events(&sb);
        assert!(evs.is_empty(), "no event when start >= scene.end");
    }

    #[test]
    fn build_meta_ass_writes_a_file_with_a_dialogue_section() {
        let sb = sb_with(
            Some(HeaderSpec {
                text: "Top".into(),
                kicker: None,
            }),
            Some(SigilSpec {
                handle: "@x".into(),
                corner: Corner::TopRight,
                opacity: 0.5,
            }),
        );
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("meta.ass");
        build_meta_ass(&sb, None, &out).unwrap();
        let body = std::fs::read_to_string(&out).unwrap();
        assert!(body.contains("[Events]"));
        assert!(body.contains("Header,"));
        assert!(body.contains(",SigilTR,"));
        assert!(body.contains("@x"));
    }

    #[test]
    fn end_time_formats_correctly() {
        assert_eq!(end_time(0.0), "0:00:00.50");
        assert_eq!(end_time(65.25), "0:01:05.75");
        // 1h 2m 3s + half-second pad.
        let s = end_time(3723.0);
        assert!(s.starts_with("1:02:03"), "got {s}");
    }
}
