//! art-engine explorer — native egui dashboard for exploring the engine's
//! possibility space.
//!
//! Layout: a large focus canvas on the left, a 3×3 mutation grid on the
//! right, and a genome inspector below. The grid holds near-neighbours of
//! the focus (one random change each). Click a tile to promote it to focus
//! and re-spawn the grid around it; "Random" rolls a fresh focus; "Reroll
//! grid" re-mutates around the current focus.
//!
//! All GPU rendering happens on a dedicated thread (see [`render`]); the UI
//! sends genomes and receives RGBA8 buffers it uploads as egui textures.

mod genome;
mod metrics;
mod render;

use std::path::{Path, PathBuf};
use std::process::Command;

use art_engine_core::prng::Xorshift64;
use eframe::egui;

use genome::{random_genome, vary, Genome};
use metrics::{analyze, Metrics};
use render::{RenderThread, Slot, ANIM_FRAMES};

/// Playback rate for animated tiles and the exported GIF.
const FPS: f64 = 12.0;

/// Square render resolution shared by all tiles. The UI scales textures for
/// display, so this doubles as the export resolution — keeping it the sole
/// resolution guarantees a clicked thumbnail is exactly what gets promoted
/// and exported. Quality/perf knob.
const RENDER_RES: u32 = 512;
/// Display size of the focus canvas.
const FOCUS_DISP: f32 = 480.0;
/// Display size of each grid thumbnail.
const GRID_DISP: f32 = 150.0;
/// Grid is GRID_N × GRID_N.
const GRID_N: usize = 3;
const GRID_COUNT: usize = GRID_N * GRID_N;
/// Cap on retained timeline entries (each is tiny — focus + 9 grid genomes).
const MAX_HISTORY: usize = 256;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 760.0])
            .with_title("art-engine explorer"),
        ..Default::default()
    };
    eframe::run_native(
        "art-engine explorer",
        native_options,
        Box::new(|cc| Ok(Box::new(ExplorerApp::new(cc)))),
    )
}

/// One grid cell: its genome and (once rendered) its texture.
struct Cell {
    genome: Genome,
    /// One texture per animation frame; empty until the render arrives.
    frames: Vec<egui::TextureHandle>,
}

impl Cell {
    fn new(genome: Genome) -> Self {
        Self {
            genome,
            frames: Vec::new(),
        }
    }
}

/// Returns the texture for the current playback frame of `frames`, cycling at
/// [`FPS`] off the egui clock. `None` while the slot is still rendering.
fn current_frame(frames: &[egui::TextureHandle], clock: f64) -> Option<&egui::TextureHandle> {
    if frames.is_empty() {
        return None;
    }
    let idx = (clock * FPS) as usize % frames.len();
    frames.get(idx)
}

/// One navigable point in the exploration: the focus composition and the
/// exact nine neighbours that were offered alongside it. Snapshotting the
/// grid (not just the focus) makes back/forward reproduce what was seen,
/// since the mutation operator is stochastic.
#[derive(Clone)]
struct HistoryEntry {
    focus: Genome,
    grid: Vec<Genome>,
}

/// A linear undo/redo history with a cursor. Committing a new entry while
/// the cursor is behind the end truncates the forward branch (browser-style).
struct Timeline<T> {
    entries: Vec<T>,
    cursor: usize,
}

impl<T> Timeline<T> {
    fn new(initial: T) -> Self {
        Self {
            entries: vec![initial],
            cursor: 0,
        }
    }

    fn current(&self) -> &T {
        &self.entries[self.cursor]
    }

    /// Appends `entry` after the cursor, discarding any forward history, then
    /// moves the cursor to it. Trims the oldest entries past [`MAX_HISTORY`].
    fn commit(&mut self, entry: T) {
        self.entries.truncate(self.cursor + 1);
        self.entries.push(entry);
        if self.entries.len() > MAX_HISTORY {
            let overflow = self.entries.len() - MAX_HISTORY;
            self.entries.drain(0..overflow);
        }
        self.cursor = self.entries.len() - 1;
    }

    fn back(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            true
        } else {
            false
        }
    }

    fn forward(&mut self) -> bool {
        if self.cursor + 1 < self.entries.len() {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn can_back(&self) -> bool {
        self.cursor > 0
    }

    fn can_forward(&self) -> bool {
        self.cursor + 1 < self.entries.len()
    }

    /// 1-based position and total, for a "step X / Y" indicator.
    fn position(&self) -> (usize, usize) {
        (self.cursor + 1, self.entries.len())
    }
}

struct ExplorerApp {
    render_thread: RenderThread,
    rng: Xorshift64,
    /// Navigable history of explored entries; its current entry drives the
    /// live `focus`/`grid` render state below.
    timeline: Timeline<HistoryEntry>,
    focus: Genome,
    /// One texture per focus animation frame.
    focus_frames: Vec<egui::TextureHandle>,
    /// Raw RGBA8 of every focus frame, retained for PNG/GIF export.
    focus_frame_bytes: Vec<Vec<u8>>,
    /// Perceptual metrics of the current focus render — "where in the space".
    focus_metrics: Option<Metrics>,
    /// When true, tiles render and play [`ANIM_FRAMES`] frames; else 1 still.
    animate: bool,
    grid: Vec<Cell>,
    /// Errors surfaced from the render thread, shown in-window.
    last_error: Option<String>,
    /// Transient status line (e.g. "exported to …"), shown in the toolbar.
    status: Option<String>,
    /// System clipboard handle, kept alive for the app's lifetime so the
    /// copied image stays available to other apps (X11/Wayland require the
    /// owning process to keep serving it). `None` if unavailable (headless).
    clipboard: Option<arboard::Clipboard>,
}

impl ExplorerApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Seed from the wall clock so each launch explores a different
        // region. The explorer isn't a determinism target; reproducibility
        // comes later via genome save/load.
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15)
            | 1;
        let mut rng = Xorshift64::new(seed);
        let render_thread = RenderThread::spawn(RENDER_RES);

        let focus = random_genome(&mut rng);
        let grid_genomes: Vec<Genome> = (0..GRID_COUNT).map(|_| vary(&focus, &mut rng)).collect();
        let timeline = Timeline::new(HistoryEntry {
            focus: focus.clone(),
            grid: grid_genomes.clone(),
        });
        let grid = grid_genomes.into_iter().map(Cell::new).collect();

        let mut app = Self {
            render_thread,
            rng,
            timeline,
            focus,
            focus_frames: Vec::new(),
            focus_frame_bytes: Vec::new(),
            focus_metrics: None,
            animate: true,
            grid,
            last_error: None,
            status: None,
            clipboard: arboard::Clipboard::new().ok(),
        };
        app.request_all();
        app
    }

    /// Frames requested per tile: a full animation or a single still.
    fn frame_count(&self) -> usize {
        if self.animate {
            ANIM_FRAMES
        } else {
            1
        }
    }

    /// Queues render requests for the focus and every grid cell, clearing
    /// stale frames so the UI shows a pending state until they arrive.
    fn request_all(&mut self) {
        let frames = self.frame_count();
        self.focus_frames.clear();
        self.focus_frame_bytes.clear();
        self.focus_metrics = None;
        self.render_thread
            .request(Slot::Focus, self.focus.clone(), frames);
        for (i, cell) in self.grid.iter_mut().enumerate() {
            cell.frames.clear();
            self.render_thread
                .request(Slot::Grid(i), cell.genome.clone(), frames);
        }
    }

    /// Builds nine fresh neighbour genomes around `focus`.
    fn make_grid(&mut self, focus: &Genome) -> Vec<Genome> {
        (0..GRID_COUNT).map(|_| vary(focus, &mut self.rng)).collect()
    }

    /// Records a new focus + grid as a timeline entry and renders it. This is
    /// the single path that advances the timeline (Random / Reroll / promote).
    fn commit(&mut self, focus: Genome, grid_genomes: Vec<Genome>) {
        self.timeline.commit(HistoryEntry {
            focus: focus.clone(),
            grid: grid_genomes.clone(),
        });
        self.focus = focus;
        self.grid = grid_genomes.into_iter().map(Cell::new).collect();
        self.request_all();
    }

    /// Loads the timeline's current entry into the live render state.
    fn load_current(&mut self) {
        let entry = self.timeline.current();
        self.focus = entry.focus.clone();
        let grid = entry.grid.clone();
        self.grid = grid.into_iter().map(Cell::new).collect();
        self.request_all();
    }

    /// Rolls a brand-new random focus and grid.
    fn randomize(&mut self) {
        let focus = random_genome(&mut self.rng);
        let grid = self.make_grid(&focus);
        self.commit(focus, grid);
    }

    /// Re-mutates a fresh grid around the unchanged focus.
    fn reroll_grid(&mut self) {
        let focus = self.focus.clone();
        let grid = self.make_grid(&focus);
        self.commit(focus, grid);
    }

    /// Promotes a grid cell to focus, then re-mutates the grid around it.
    fn promote(&mut self, index: usize) {
        let Some(focus) = self.grid.get(index).map(|c| c.genome.clone()) else {
            return;
        };
        let grid = self.make_grid(&focus);
        self.commit(focus, grid);
    }

    /// Steps one entry back in the timeline, if possible.
    fn go_back(&mut self) {
        if self.timeline.back() {
            self.load_current();
        }
    }

    /// Steps one entry forward in the timeline, if possible.
    fn go_forward(&mut self) {
        if self.timeline.forward() {
            self.load_current();
        }
    }

    /// Drains finished renders from the thread and uploads them as textures.
    fn collect_renders(&mut self, ctx: &egui::Context) {
        for result in self.render_thread.drain() {
            match result.frames {
                Ok(frames) if !frames.is_empty() => {
                    // For the focus, compute metrics on (and retain) every
                    // frame for PNG/GIF export before uploading textures.
                    if result.slot == Slot::Focus {
                        self.focus_metrics = Some(analyze(
                            &frames[0],
                            RENDER_RES as usize,
                            RENDER_RES as usize,
                        ));
                        self.focus_frame_bytes = frames.clone();
                    }
                    let textures = upload_frames(ctx, result.slot, &frames);
                    match result.slot {
                        Slot::Focus => self.focus_frames = textures,
                        Slot::Grid(i) => {
                            if let Some(cell) = self.grid.get_mut(i) {
                                cell.frames = textures;
                            }
                        }
                    }
                }
                Ok(_) => {} // empty frame list — nothing to show
                Err(e) => self.last_error = Some(e),
            }
        }
    }

    fn pending(&self) -> bool {
        self.focus_frames.is_empty() || self.grid.iter().any(|c| c.frames.is_empty())
    }

    /// Writes the first focus frame to a timestamped PNG in the working
    /// directory, recording the path (or error) in the status line.
    fn export_focus(&mut self) {
        let Some(bytes) = self.focus_frame_bytes.first() else {
            self.status = Some("nothing to export yet — still rendering".to_string());
            return;
        };
        match write_png(bytes, RENDER_RES) {
            Ok(path) => self.status = Some(format!("exported → {}", path.display())),
            Err(e) => self.status = Some(format!("export failed: {e}")),
        }
    }

    /// Encodes all focus frames into a looping animated GIF on disk.
    fn export_gif(&mut self) {
        if self.focus_frame_bytes.is_empty() {
            self.status = Some("nothing to export yet — still rendering".to_string());
            return;
        }
        match write_gif(&self.focus_frame_bytes, RENDER_RES) {
            Ok(path) => self.status = Some(format!("exported GIF → {}", path.display())),
            Err(e) => self.status = Some(format!("GIF export failed: {e}")),
        }
    }

    /// Copies the current focus render onto the system clipboard as an image,
    /// so it can be pasted directly into other applications.
    ///
    /// On WSL, arboard targets the Wayland/X11 clipboard, whose WSLg bridge to
    /// Windows does not carry image MIME types — so the copy silently fails to
    /// reach Windows apps. There we route through PowerShell's
    /// `Clipboard.SetImage` instead. Native Linux/macOS use arboard directly.
    fn copy_focus_image(&mut self) {
        let Some(bytes) = self.focus_frame_bytes.first().cloned() else {
            self.status = Some("nothing to copy yet — still rendering".to_string());
            return;
        };

        if is_wsl() {
            self.status = Some(match copy_image_via_windows(&bytes, RENDER_RES) {
                Ok(()) => "image copied to clipboard (Windows)".to_string(),
                Err(e) => format!("clipboard copy failed: {e}"),
            });
            return;
        }

        let Some(clipboard) = self.clipboard.as_mut() else {
            self.status = Some("clipboard unavailable on this system".to_string());
            return;
        };
        let image = arboard::ImageData {
            width: RENDER_RES as usize,
            height: RENDER_RES as usize,
            bytes: std::borrow::Cow::Owned(bytes),
        };
        match clipboard.set_image(image) {
            Ok(()) => self.status = Some("image copied to clipboard".to_string()),
            Err(e) => self.status = Some(format!("clipboard copy failed: {e}")),
        }
    }

    /// Copies the focus to the clipboard: the animated GIF when there are
    /// multiple frames, otherwise the still image. Dispatched by the adaptive
    /// copy button.
    fn copy_focus(&mut self) {
        if self.focus_frame_bytes.len() > 1 {
            self.copy_focus_gif();
        } else {
            self.copy_focus_image();
        }
    }

    /// Copies the focus animation as a GIF. A bitmap clipboard slot can only
    /// hold one frame, so the GIF is placed on the clipboard as a *file*
    /// (CF_HDROP) — apps that accept a pasted file (chat clients, browsers,
    /// Explorer, email) receive the animated GIF. Only the Windows clipboard
    /// supports this here; native falls back to copying the first frame.
    fn copy_focus_gif(&mut self) {
        if self.focus_frame_bytes.is_empty() {
            self.status = Some("nothing to copy yet — still rendering".to_string());
            return;
        }
        if !is_wsl() {
            self.copy_focus_image();
            self.status =
                Some("copied first frame (animated GIF clipboard needs Windows)".to_string());
            return;
        }
        let tmp = std::env::temp_dir().join("art-engine-clip.gif");
        if let Err(e) = encode_gif(&self.focus_frame_bytes, RENDER_RES, &tmp) {
            self.status = Some(format!("GIF copy failed: {e}"));
            return;
        }
        self.status = Some(match copy_file_via_windows(&tmp) {
            Ok(()) => "GIF copied — paste into a chat/file app".to_string(),
            Err(e) => format!("GIF copy failed: {e}"),
        });
    }
}

/// True when running under WSL, where the Linux→Windows clipboard bridge
/// can't carry images and we must route through PowerShell.
fn is_wsl() -> bool {
    if std::env::var_os("WSL_DISTRO_NAME").is_some() {
        return true;
    }
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| {
            let s = s.to_ascii_lowercase();
            s.contains("microsoft") || s.contains("wsl")
        })
        .unwrap_or(false)
}

/// Writes the image to a temp PNG and sets it on the Windows clipboard via
/// PowerShell. Blocks for the duration of the PowerShell call (~1s cold).
fn copy_image_via_windows(bytes: &[u8], res: u32) -> Result<(), String> {
    let tmp = std::env::temp_dir().join("art-engine-clip.png");
    encode_png(bytes, res, &tmp)?;

    // Translate the Linux temp path to a Windows path PowerShell can open.
    let win = Command::new("wslpath")
        .arg("-w")
        .arg(&tmp)
        .output()
        .map_err(|e| format!("wslpath: {e}"))?;
    if !win.status.success() {
        return Err("wslpath failed to translate the temp path".to_string());
    }
    let win_path = String::from_utf8_lossy(&win.stdout).trim().to_string();

    // SetImage requires an STA thread; -sta forces it. Single-quoting the
    // path keeps backslashes literal (temp paths contain no single quotes).
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; \
         Add-Type -AssemblyName System.Drawing; \
         $i=[System.Drawing.Image]::FromFile('{win_path}'); \
         [System.Windows.Forms.Clipboard]::SetImage($i); \
         $i.Dispose()"
    );
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-sta", "-Command", &script])
        .output()
        .map_err(|e| format!("powershell.exe: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Places a file on the Windows clipboard as a file drop (CF_HDROP) via
/// `Set-Clipboard`. Used to copy an animated GIF, which has no bitmap
/// clipboard representation — receiving apps paste it as the file.
fn copy_file_via_windows(path: &Path) -> Result<(), String> {
    let win = Command::new("wslpath")
        .arg("-w")
        .arg(path)
        .output()
        .map_err(|e| format!("wslpath: {e}"))?;
    if !win.status.success() {
        return Err("wslpath failed to translate the temp path".to_string());
    }
    let win_path = String::from_utf8_lossy(&win.stdout).trim().to_string();

    let script = format!("Set-Clipboard -LiteralPath '{win_path}'");
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| format!("powershell.exe: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Encodes an RGBA8 buffer to a PNG at `path`.
fn encode_png(bytes: &[u8], res: u32, path: &Path) -> Result<(), String> {
    let img = image::RgbaImage::from_raw(res, res, bytes.to_vec())
        .ok_or_else(|| "RGBA buffer size mismatch".to_string())?;
    img.save(path).map_err(|e| e.to_string())
}

/// Builds a timestamped export path with the given extension in the cwd.
fn export_path(ext: &str) -> PathBuf {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    std::env::current_dir()
        .unwrap_or_default()
        .join(format!("art-export-{ts}.{ext}"))
}

/// Saves an RGBA8 buffer as a PNG in the current directory.
fn write_png(bytes: &[u8], res: u32) -> Result<PathBuf, String> {
    let path = export_path("png");
    encode_png(bytes, res, &path)?;
    Ok(path)
}

/// Encodes RGBA8 frames into a looping animated GIF at `path`.
fn encode_gif(frames: &[Vec<u8>], res: u32, path: &Path) -> Result<(), String> {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame};

    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    // speed 1..=30, higher is faster/coarser; 20 keeps export snappy.
    let mut encoder = GifEncoder::new_with_speed(std::io::BufWriter::new(file), 20);
    encoder
        .set_repeat(Repeat::Infinite)
        .map_err(|e| e.to_string())?;
    let delay = Delay::from_numer_denom_ms(1000, FPS as u32);
    for bytes in frames {
        let img = image::RgbaImage::from_raw(res, res, bytes.clone())
            .ok_or_else(|| "RGBA buffer size mismatch".to_string())?;
        encoder
            .encode_frame(Frame::from_parts(img, 0, 0, delay))
            .map_err(|e| e.to_string())?;
    }
    drop(encoder); // flush the GIF trailer before returning
    Ok(())
}

/// Encodes a looping animated GIF in the current directory.
fn write_gif(frames: &[Vec<u8>], res: u32) -> Result<PathBuf, String> {
    let path = export_path("gif");
    encode_gif(frames, res, &path)?;
    Ok(path)
}

/// Uploads one RGBA8 frame buffer per animation frame as egui textures.
fn upload_frames(
    ctx: &egui::Context,
    slot: Slot,
    frames: &[Vec<u8>],
) -> Vec<egui::TextureHandle> {
    frames
        .iter()
        .enumerate()
        .map(|(i, bytes)| {
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [RENDER_RES as usize, RENDER_RES as usize],
                bytes,
            );
            let name = match slot {
                Slot::Focus => format!("focus-f{i}"),
                Slot::Grid(g) => format!("grid{g}-f{i}"),
            };
            ctx.load_texture(name, image, egui::TextureOptions::LINEAR)
        })
        .collect()
}

impl eframe::App for ExplorerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.collect_renders(ctx);

        // Arrow keys walk the timeline (no text fields to steal focus);
        // `clock` drives animation playback.
        let (key_back, key_fwd, clock) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
                i.time,
            )
        });
        if key_back {
            self.go_back();
        }
        if key_fwd {
            self.go_forward();
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("art-engine explorer");
                ui.separator();
                if ui.button("🎲 Random").clicked() {
                    self.randomize();
                }
                if ui.button("↻ Reroll grid").clicked() {
                    self.reroll_grid();
                }
                if ui
                    .checkbox(&mut self.animate, "Animate")
                    .on_hover_text("Render and play animated frames (slower)")
                    .changed()
                {
                    self.request_all();
                }
                ui.separator();
                // Timeline navigation.
                if ui
                    .add_enabled(self.timeline.can_back(), egui::Button::new("← Back"))
                    .on_hover_text("Previous step (←)")
                    .clicked()
                {
                    self.go_back();
                }
                if ui
                    .add_enabled(self.timeline.can_forward(), egui::Button::new("Forward →"))
                    .on_hover_text("Next step (→)")
                    .clicked()
                {
                    self.go_forward();
                }
                let (pos, total) = self.timeline.position();
                ui.label(format!("step {pos} / {total}"));
                if self.pending() {
                    ui.spinner();
                    ui.label("rendering…");
                }
            });
            if let Some(status) = &self.status {
                ui.colored_label(egui::Color32::LIGHT_GREEN, status);
            }
            if let Some(err) = &self.last_error {
                ui.colored_label(egui::Color32::RED, format!("render error: {err}"));
            }
        });

        egui::TopBottomPanel::bottom("inspector")
            .resizable(false)
            .min_height(150.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let have_image = !self.focus_frame_bytes.is_empty();
                    let have_anim = self.focus_frame_bytes.len() > 1;

                    // Primary action: copy the focus to the clipboard — the
                    // animated GIF when animating, otherwise the still image.
                    let (label, hover) = if have_anim {
                        (
                            "📋  Copy GIF",
                            "Copy the animated GIF to the clipboard (pastes as a file)",
                        )
                    } else {
                        ("📋  Copy image", "Copy the focus image to the clipboard")
                    };
                    let copy_btn = egui::Button::new(egui::RichText::new(label).size(18.0))
                        .min_size(egui::vec2(200.0, 44.0));
                    if ui
                        .add_enabled(have_image, copy_btn)
                        .on_hover_text(hover)
                        .clicked()
                    {
                        self.copy_focus();
                    }
                    // Save the focus as an animated GIF.
                    if ui
                        .add_enabled(have_anim, egui::Button::new("🎞 Export GIF"))
                        .on_hover_text("Save the focus animation to a GIF file")
                        .clicked()
                    {
                        self.export_gif();
                    }
                    // Save a still frame to a PNG file.
                    if ui
                        .add_enabled(have_image, egui::Button::new("💾 Export PNG"))
                        .on_hover_text("Save the focus image (first frame) to a PNG file")
                        .clicked()
                    {
                        self.export_focus();
                    }
                    // Copy the recipe for reproduction.
                    if ui
                        .button("📋 Copy genome")
                        .on_hover_text("Copy the focus composition recipe as JSON")
                        .clicked()
                    {
                        ui.ctx().copy_text(self.focus.to_json());
                        self.status = Some("genome JSON copied to clipboard".to_string());
                    }
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("structure").strong());
                        ui.monospace(self.focus.summary());
                    });
                });

                ui.add_space(8.0);
                ui.label(egui::RichText::new("position in space").strong());
                match &self.focus_metrics {
                    Some(m) => {
                        ui.monospace(m.describe());
                    }
                    None => {
                        ui.monospace("measuring…");
                    }
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                // Focus canvas.
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("focus").strong());
                    let size = egui::vec2(FOCUS_DISP, FOCUS_DISP);
                    match current_frame(&self.focus_frames, clock) {
                        Some(tex) => {
                            ui.image(egui::load::SizedTexture::new(tex.id(), size));
                        }
                        None => {
                            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                            ui.painter()
                                .rect_filled(rect, 4.0, egui::Color32::from_gray(20));
                        }
                    }
                });

                ui.separator();

                // Mutation grid.
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("mutations").strong());
                    let mut clicked: Option<usize> = None;
                    for row in 0..GRID_N {
                        ui.horizontal(|ui| {
                            for col in 0..GRID_N {
                                let i = row * GRID_N + col;
                                let frames =
                                    self.grid.get(i).map(|c| c.frames.as_slice()).unwrap_or(&[]);
                                if cell_button(ui, frames, clock) {
                                    clicked = Some(i);
                                }
                            }
                        });
                    }
                    if let Some(i) = clicked {
                        self.promote(i);
                    }
                });
            });
        });

        // Repaint to advance animation, or to poll for outstanding renders.
        if self.animate {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(1.0 / FPS));
        } else if self.pending() {
            ctx.request_repaint();
        }
    }
}

/// Draws one grid cell as a clickable image button playing the current
/// animation frame (or a placeholder while it renders). Returns true if
/// clicked this frame.
fn cell_button(ui: &mut egui::Ui, frames: &[egui::TextureHandle], clock: f64) -> bool {
    let size = egui::vec2(GRID_DISP, GRID_DISP);
    match current_frame(frames, clock) {
        Some(tex) => {
            let image = egui::Image::new(egui::load::SizedTexture::new(tex.id(), size));
            ui.add(egui::ImageButton::new(image)).clicked()
        }
        None => {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 4.0, egui::Color32::from_gray(20));
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_starts_at_first_entry() {
        let t = Timeline::new(10);
        assert_eq!(*t.current(), 10);
        assert_eq!(t.position(), (1, 1));
        assert!(!t.can_back());
        assert!(!t.can_forward());
    }

    #[test]
    fn commit_advances_and_back_forward_navigate() {
        let mut t = Timeline::new(1);
        t.commit(2);
        t.commit(3);
        assert_eq!(t.position(), (3, 3));
        assert_eq!(*t.current(), 3);

        assert!(t.back());
        assert_eq!(*t.current(), 2);
        assert!(t.back());
        assert_eq!(*t.current(), 1);
        assert!(!t.back(), "cannot go before the first entry");

        assert!(t.forward());
        assert_eq!(*t.current(), 2);
    }

    #[test]
    fn commit_after_back_truncates_forward_branch() {
        let mut t = Timeline::new(1);
        t.commit(2);
        t.commit(3);
        t.back(); // at 2, with 3 ahead
        t.commit(99); // discards 3
        assert_eq!(t.position(), (3, 3));
        assert_eq!(*t.current(), 99);
        assert!(!t.can_forward(), "forward branch should be gone");
        assert!(t.back());
        assert_eq!(*t.current(), 2);
    }

    #[test]
    fn history_is_capped_and_cursor_tracks_end() {
        let mut t = Timeline::new(0);
        for n in 1..=(MAX_HISTORY + 50) {
            t.commit(n as i32);
        }
        let (pos, total) = t.position();
        assert_eq!(total, MAX_HISTORY, "history should be capped");
        assert_eq!(pos, MAX_HISTORY, "cursor stays at newest entry");
        assert_eq!(*t.current(), (MAX_HISTORY + 50) as i32);
    }

    #[test]
    fn current_frame_cycles_with_clock() {
        // Three dummy frame slots can't be real TextureHandles without a GL
        // context, so test the index math on a stand-in slice length instead.
        let pick = |len: usize, clock: f64| -> Option<usize> {
            if len == 0 {
                None
            } else {
                Some((clock * FPS) as usize % len)
            }
        };
        assert_eq!(pick(0, 1.0), None);
        assert_eq!(pick(3, 0.0), Some(0));
        // At FPS frames/sec, t = 1/FPS advances exactly one frame.
        assert_eq!(pick(3, 1.0 / FPS), Some(1));
        assert_eq!(pick(3, 3.0 / FPS), Some(0), "wraps after len frames");
    }

    #[test]
    fn gif_round_trips_frame_count() {
        use image::AnimationDecoder;

        let res = 8u32;
        // Three solid frames: red, green, blue.
        let colors = [[255u8, 0, 0], [0, 255, 0], [0, 0, 255]];
        let frames: Vec<Vec<u8>> = colors
            .iter()
            .map(|c| {
                let mut v = Vec::with_capacity((res * res * 4) as usize);
                for _ in 0..res * res {
                    v.extend_from_slice(&[c[0], c[1], c[2], 255]);
                }
                v
            })
            .collect();

        let path = std::env::temp_dir().join("art-engine-test.gif");
        encode_gif(&frames, res, &path).expect("encode gif");

        let file = std::fs::File::open(&path).expect("open gif");
        let decoder =
            image::codecs::gif::GifDecoder::new(std::io::BufReader::new(file)).expect("decode gif");
        let decoded = decoder.into_frames().collect_frames().expect("frames");
        assert_eq!(decoded.len(), 3, "GIF should have three frames");
        let _ = std::fs::remove_file(&path);
    }
}
