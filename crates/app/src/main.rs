mod crop_tool;
mod editor_screen;
mod export_screen;
mod processing;
mod project_screen;
mod recording_indicator;
mod selection_overlay;
mod text_tool;
mod theme;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use eframe::egui;
use editor::{Frame, FrameList};
use editor_screen::{EditorAction, EditorScreen};
use export_screen::{start_export, ExportJob};
use global_hotkey::{
    hotkey::{Code, HotKey},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use processing::{start_edit_processing, start_initial_processing, EditOp, ProcessingJob};
use project_screen::{ProjectAction, ProjectScreen};

enum AppState {
    Project(ProjectScreen),
    SelectingArea {
        fps: u32,
        bounds: capture::Region,
        backdrop: Vec<(capture::Region, egui::TextureHandle)>,
        drag_start: Option<(f32, f32)>,
    },
    Recording {
        stop_flag: Arc<AtomicBool>,
        handle: JoinHandle<Result<(), capture::CaptureError>>,
        rx: Receiver<Frame>,
        frames: Vec<Frame>,
        started_at: Instant,
    },
    /// Building filmstrip thumbnails (right after a recording stops, or
    /// after a crop/blur/text edit) on a background thread — see
    /// `processing`. Keeps the UI thread responsive instead of blocking on
    /// hundreds of frames' worth of image work.
    Processing {
        job: ProcessingJob,
    },
    Editing {
        frames: FrameList,
        screen: EditorScreen,
    },
    Exporting {
        frames: FrameList,
        screen: EditorScreen,
        job: ExportJob,
        output_path: PathBuf,
    },
    Done {
        output_path: PathBuf,
    },
}

/// Renders the editor's top bar, "Nova gravação" row, error message, and
/// body (tool panel, preview, filmstrip). Shared by `Editing` (fully
/// interactive) and `Exporting` (wrapped in `add_enabled_ui(false)` by the
/// caller, so the export in progress stays visible instead of replacing
/// the whole screen with a bare progress bar).
fn show_editing_body(
    ui: &mut egui::Ui,
    frames: &FrameList,
    screen: &mut EditorScreen,
    logo: &egui::TextureHandle,
    last_error: &Option<String>,
    should_return_to_project: &mut bool,
    should_start_export: &mut Option<PathBuf>,
) -> Option<EditorAction> {
    ui.horizontal(|ui| {
        ui.add(egui::Image::new(logo).fit_to_exact_size(egui::vec2(22.0, 22.0)));
        ui.heading("AyeAye");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Exportar").clicked() {
                if let Some(path) =
                    rfd::FileDialog::new().add_filter("GIF", &["gif"]).set_file_name("recording.gif").save_file()
                {
                    *should_start_export = Some(path);
                }
            }
        });
    });
    // Frame count now lives in the editor's own status bar (see
    // EditorScreen::show) rather than being duplicated up here.
    if ui.link("< Nova gravação").clicked() {
        *should_return_to_project = true;
    }
    if let Some(msg) = last_error {
        ui.colored_label(egui::Color32::RED, msg);
    }
    ui.separator();
    screen.show(ui, frames)
}

struct App {
    /// `None` on Wayland — `global-hotkey`'s Linux backend is X11-only, so
    /// there's nothing to hold there. The recording indicator's "Parar"
    /// button is the only way to stop a recording on Wayland.
    _hotkey_manager: Option<GlobalHotKeyManager>,
    toggle_hotkey: Option<HotKey>,
    state: AppState,
    /// Set when a background operation (capture, export) fails partway
    /// through, so the current screen can show a warning without losing
    /// whatever work was already done. Cleared when the user starts a new
    /// attempt.
    last_error: Option<String>,
    /// The app icon, uploaded once at startup and reused everywhere it's
    /// shown in the UI (project screen header, editor top bar) — cheap to
    /// clone, since `TextureHandle` is just a ref-counted handle.
    logo: egui::TextureHandle,
}

impl App {
    fn new(logo: egui::TextureHandle) -> Self {
        let (hotkey_manager, toggle_hotkey) = if capture::session_type() == capture::SessionType::X11 {
            let manager = GlobalHotKeyManager::new().expect("failed to create global hotkey manager");
            let toggle_hotkey = HotKey::new(None, Code::F9);
            manager
                .register(toggle_hotkey)
                .expect("failed to register F9 hotkey (is another app using it?)");
            (Some(manager), Some(toggle_hotkey))
        } else {
            (None, None)
        };
        Self {
            _hotkey_manager: hotkey_manager,
            toggle_hotkey,
            state: AppState::Project(ProjectScreen::default()),
            last_error: None,
            logo,
        }
    }
}

impl App {
    fn start_recording(&mut self, region: capture::Region, fps: u32) {
        self.last_error = None;
        let (tx, rx) = channel();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let handle = capture::start_capture(region, fps, tx, stop_flag.clone());
        self.state = AppState::Recording { stop_flag, handle, rx, frames: Vec::new(), started_at: Instant::now() };
    }

    /// Stops the in-progress recording (if any) and transitions to
    /// `Processing`, then raises and focuses the main window — the app
    /// window is hidden during `Recording`, so without this the user has
    /// no way to tell where the recording went.
    fn stop_recording(&mut self, ctx: &egui::Context) {
        self.state = match std::mem::replace(&mut self.state, AppState::Project(ProjectScreen::default())) {
            AppState::Recording { stop_flag, handle, mut frames, rx, .. } => {
                stop_flag.store(true, Ordering::Relaxed);
                // A capture error only stops future frames — everything already
                // sent through `rx` is still collected below, so a mid-recording
                // failure never discards frames the user already captured.
                if let Err(e) = handle.join().expect("capture thread panicked") {
                    self.last_error = Some(format!("A gravação parou antes do esperado: {e}"));
                }
                frames.extend(rx.try_iter());
                let job = start_initial_processing(FrameList::new(frames));
                AppState::Processing { job }
            }
            other => other,
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if let Some(toggle_hotkey) = self.toggle_hotkey {
            if let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                if event.id == toggle_hotkey.id() && event.state == global_hotkey::HotKeyState::Pressed {
                    if matches!(self.state, AppState::Recording { .. }) {
                        self.stop_recording(&ctx);
                    }
                }
            }
        }
        ctx.request_repaint();

        let hide_main_window = matches!(self.state, AppState::Recording { .. } | AppState::SelectingArea { .. });
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(!hide_main_window));

        if let AppState::Recording { rx, frames, .. } = &mut self.state {
            frames.extend(rx.try_iter());
        }

        let mut should_start_full_screen = false;
        let mut should_start_export: Option<PathBuf> = None;
        let mut should_stop_recording = false;
        let mut should_start_region_recording: Option<(capture::Region, u32)> = None;
        let mut should_return_to_project = false;
        let mut should_process_edit: Option<EditOp> = None;
        let logo = self.logo.clone();

        egui::CentralPanel::default().show(ui, |ui| match &mut self.state {
            AppState::Project(screen) => match screen.show(ui, &logo) {
                Some(ProjectAction::StartFullScreen) => should_start_full_screen = true,
                Some(ProjectAction::StartAreaSelection) => {
                    let fps = screen.fps;
                    // On Wayland the overlay can only cover one monitor (see
                    // `selection_overlay::show`) — the one the app window is
                    // on, same restriction "Tela Inteira" already has. On
                    // X11 it still spans every monitor, as before.
                    let (bounds, snapshots): (capture::Region, Vec<(capture::Region, image::RgbaImage)>) =
                        if capture::session_type() == capture::SessionType::Wayland {
                            let window_center = ctx
                                .input(|i| i.viewport().inner_rect)
                                .expect("window position is unavailable on this platform")
                                .center();
                            let bounds = capture::monitor_bounds_at(window_center.x as i32, window_center.y as i32)
                                .expect("could not determine the monitor under the app window");
                            let image = capture::snapshot_monitor(bounds).expect("could not capture a desktop snapshot");
                            (bounds, vec![(bounds, image)])
                        } else {
                            let bounds = capture::virtual_screen_bounds().expect("could not enumerate monitors");
                            let snapshots = capture::snapshot_monitors().expect("could not capture a desktop snapshot");
                            (bounds, snapshots)
                        };
                    // Captured once, up front — used as the overlay's real
                    // backdrop instead of relying on window transparency,
                    // which isn't reliably honored by every window manager.
                    let backdrop = snapshots
                        .into_iter()
                        .enumerate()
                        .map(|(i, (region, image))| {
                            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                                [image.width() as usize, image.height() as usize],
                                image.as_raw(),
                            );
                            let texture =
                                ctx.load_texture(format!("overlay-backdrop-{i}"), color_image, egui::TextureOptions::default());
                            (region, texture)
                        })
                        .collect();
                    self.state = AppState::SelectingArea { fps, bounds, backdrop, drag_start: None };
                }
                None => {}
            },
            AppState::SelectingArea { fps, bounds, backdrop, drag_start } => {
                let fullscreen = capture::session_type() == capture::SessionType::Wayland;
                if let Some(region) = selection_overlay::show(&ctx, *bounds, fullscreen, backdrop, drag_start) {
                    should_start_region_recording = Some((region, *fps));
                }
            }
            AppState::Recording { frames, started_at, .. } => {
                ui.centered_and_justified(|ui| {
                    ui.label("Gravando...");
                });
                if recording_indicator::show(&ctx, started_at.elapsed().as_secs(), frames.len()) {
                    should_stop_recording = true;
                }
            }
            AppState::Processing { job } => {
                let (current, total) = *job.progress.lock().unwrap();
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Spinner::new().size(32.0));
                        ui.add_space(8.0);
                        ui.label("Processando gravação...");
                        ui.add_space(4.0);
                        ui.add(
                            egui::ProgressBar::new(if total == 0 { 0.0 } else { current as f32 / total as f32 })
                                .desired_width(240.0)
                                .text(format!("{current}/{total}")),
                        );
                    });
                });
            }
            AppState::Editing { frames, screen } => {
                let action = show_editing_body(
                    ui,
                    frames,
                    screen,
                    &logo,
                    &self.last_error,
                    &mut should_return_to_project,
                    &mut should_start_export,
                );
                if let Some(action) = action {
                    match action {
                        EditorAction::Delete(i) => {
                            frames.delete(i).expect("index came from the UI, must be valid");
                            screen.apply_delete(i);
                        }
                        EditorAction::Reorder(from, to) => {
                            frames.reorder(from, to).expect("index came from the UI, must be valid");
                            screen.apply_reorder(from, to);
                        }
                        EditorAction::Duplicate(i) => {
                            frames.duplicate(i).expect("index came from the UI, must be valid");
                            screen.apply_duplicate(i);
                        }
                        EditorAction::Crop(rect) => {
                            should_process_edit = Some(EditOp::Crop(rect));
                        }
                        EditorAction::Blur(rect, sigma) => {
                            should_process_edit = Some(EditOp::Blur(rect, sigma));
                        }
                        EditorAction::AddText { position, text, font_size } => {
                            should_process_edit = Some(EditOp::AddText { position, text, font_size });
                        }
                    }
                }
            }
            AppState::Exporting { frames, screen, job, .. } => {
                // The editing screen stays visible (just disabled) instead
                // of being replaced by a bare progress screen — the user
                // can still see their work, and lands right back on it,
                // fully interactive again, the moment export finishes.
                ui.add_enabled_ui(false, |ui| {
                    let _ = show_editing_body(
                        ui,
                        frames,
                        screen,
                        &logo,
                        &self.last_error,
                        &mut should_return_to_project,
                        &mut should_start_export,
                    );
                });

                let (current, total) = *job.progress.lock().unwrap();
                egui::Window::new("export_progress_overlay")
                    .title_bar(false)
                    .resizable(false)
                    .collapsible(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                    .show(ui.ctx(), |ui| {
                        ui.set_width(240.0);
                        ui.vertical_centered(|ui| {
                            ui.add_space(8.0);
                            ui.add(egui::Spinner::new().size(24.0));
                            ui.add_space(8.0);
                            ui.label("Exportando...");
                            ui.add_space(4.0);
                            ui.add(
                                egui::ProgressBar::new(if total == 0 { 0.0 } else { current as f32 / total as f32 })
                                    .text(format!("{current}/{total}")),
                            );
                            ui.add_space(8.0);
                        });
                    });
            }
            AppState::Done { output_path } => {
                ui.label(format!("Salvo em: {}", output_path.display()));
            }
        });

        if should_start_full_screen {
            if let AppState::Project(screen) = &self.state {
                let fps = screen.fps;
                let window_center = ctx
                    .input(|i| i.viewport().inner_rect)
                    .expect("window position is unavailable on this platform")
                    .center();
                let region = capture::monitor_bounds_at(window_center.x as i32, window_center.y as i32)
                    .expect("could not determine the monitor under the app window");
                self.start_recording(region, fps);
            }
        }

        if let Some((region, fps)) = should_start_region_recording {
            self.start_recording(region, fps);
        }

        if should_stop_recording {
            self.stop_recording(&ctx);
        }

        if should_return_to_project {
            self.state = AppState::Project(ProjectScreen::default());
        }

        if let Some(op) = should_process_edit {
            self.state = match std::mem::replace(&mut self.state, AppState::Project(ProjectScreen::default())) {
                AppState::Editing { frames, .. } => {
                    let job = start_edit_processing(frames, op);
                    AppState::Processing { job }
                }
                other => other,
            };
        }

        if let AppState::Processing { job } = &self.state {
            if job.handle.is_finished() {
                self.state = match std::mem::replace(&mut self.state, AppState::Project(ProjectScreen::default())) {
                    AppState::Processing { job } => {
                        let prepared = job.handle.join().expect("processing thread panicked");
                        let screen = EditorScreen::from_thumbnail_images(&ctx, &prepared.thumbnails);
                        AppState::Editing { frames: prepared.frames, screen }
                    }
                    other => other,
                };
            }
        }

        if let Some(path) = should_start_export {
            self.last_error = None;
            self.state = match std::mem::replace(&mut self.state, AppState::Project(ProjectScreen::default())) {
                AppState::Editing { frames, screen } => {
                    let job = start_export(&frames, path.clone());
                    AppState::Exporting { frames, screen, job, output_path: path }
                }
                other => other,
            };
        }

        if let AppState::Exporting { job, .. } = &self.state {
            if job.handle.is_finished() {
                self.state = match std::mem::replace(&mut self.state, AppState::Project(ProjectScreen::default())) {
                    AppState::Exporting { frames, screen, job, output_path } => {
                        match job.handle.join().expect("export thread panicked") {
                            Ok(()) => AppState::Done { output_path },
                            Err(e) => {
                                self.last_error = Some(format!("Falha ao exportar: {e}"));
                                AppState::Editing { frames, screen }
                            }
                        }
                    }
                    other => other,
                };
            }
        }
    }
}

/// Decodes the bundled app icon (embedded at compile time, so the binary
/// stays self-contained) into raw RGBA pixels.
fn load_logo_image() -> image::RgbaImage {
    let bytes = include_bytes!("../assets/icon/window_icon_256.png");
    image::load_from_memory(bytes).expect("bundled app icon is a valid PNG").into_rgba8()
}

fn load_icon_data() -> egui::IconData {
    let image = load_logo_image();
    let (width, height) = image.dimensions();
    egui::IconData { rgba: image.into_raw(), width, height }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(true)
            .with_resizable(true)
            .with_transparent(false)
            // 30% larger than the original 480x420 default.
            .with_inner_size([624.0, 546.0])
            .with_icon(load_icon_data()),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "AyeAye",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            let logo_image = load_logo_image();
            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                [logo_image.width() as usize, logo_image.height() as usize],
                logo_image.as_raw(),
            );
            let logo = cc.egui_ctx.load_texture("app-logo", color_image, egui::TextureOptions::default());
            Ok(Box::new(App::new(logo)))
        }),
    )
}
