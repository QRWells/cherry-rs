use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use cherry_app::{
    DEFAULT_SPECTRAL_EXPOSURE, RuntimeRenderConfig, build_animated_scene_provider,
    build_registry_with_config, initialize_gpu,
};
use cherry_core::{Color, FrameRequest};
use cherry_render::{BackendId, FrameEvent, FrameSink, RenderStats, color_to_rgb8, render_frame};
use eframe::egui;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderParams {
    pub backend_id: String,
    pub width: u32,
    pub height: u32,
    pub samples_per_pixel: u32,
    pub max_bounces: u32,
    pub cpu_threads: Option<usize>,
    pub init_gpu: bool,
}

impl Default for RenderParams {
    fn default() -> Self {
        Self {
            backend_id: "ray.normal".to_string(),
            width: 320,
            height: 180,
            samples_per_pixel: 1,
            max_bounces: 3,
            cpu_threads: None,
            init_gpu: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderDoneStats {
    pub backend_id: String,
    pub frame_index: u32,
    pub elapsed: Duration,
    pub samples_per_pixel: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderStatus {
    Idle,
    Rendering {
        backend_id: String,
        scanlines_done: u32,
        total_scanlines: u32,
    },
    Done(RenderDoneStats),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewBuffer {
    width: u32,
    height: u32,
    pixels_rgba: Vec<u8>,
}

impl PreviewBuffer {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels_rgba: vec![0; width as usize * height as usize * 4],
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels_rgba
    }

    pub fn apply_scanline(&mut self, y: u32, pixels: &[Color]) -> bool {
        if y >= self.height {
            return false;
        }

        let pixel_count = self.width as usize;
        let write_count = pixel_count.min(pixels.len());
        if write_count == 0 {
            return false;
        }

        let row_offset = y as usize * pixel_count * 4;
        for (x, color) in pixels.iter().take(write_count).enumerate() {
            let rgb = color_to_rgb8(*color);
            let offset = row_offset + x * 4;
            self.pixels_rgba[offset] = rgb.0[0];
            self.pixels_rgba[offset + 1] = rgb.0[1];
            self.pixels_rgba[offset + 2] = rgb.0[2];
            self.pixels_rgba[offset + 3] = 255;
        }

        true
    }
}

#[derive(Debug, Clone)]
pub enum WorkerMessage {
    Begin {
        backend_id: String,
        request: FrameRequest,
    },
    Scanline {
        y: u32,
        pixels: Vec<Color>,
    },
    End {
        stats: RenderStats,
    },
    Error(String),
}

struct ChannelFrameSink {
    tx: Sender<WorkerMessage>,
}

impl ChannelFrameSink {
    fn new(tx: Sender<WorkerMessage>) -> Self {
        Self { tx }
    }
}

impl FrameSink for ChannelFrameSink {
    fn on_event(&mut self, event: FrameEvent) {
        match event {
            FrameEvent::Begin { backend, request } => {
                let _ = self.tx.send(WorkerMessage::Begin {
                    backend_id: backend.id.as_str().to_string(),
                    request,
                });
            }
            FrameEvent::Scanline { y, pixels, .. } => {
                let _ = self.tx.send(WorkerMessage::Scanline { y, pixels });
            }
            FrameEvent::End { stats } => {
                let _ = self.tx.send(WorkerMessage::End { stats });
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiState {
    pub params: RenderParams,
    pub status: RenderStatus,
    pub preview: Option<PreviewBuffer>,
}

impl GuiState {
    pub fn new(params: RenderParams) -> Self {
        Self {
            params,
            status: RenderStatus::Idle,
            preview: None,
        }
    }

    pub fn is_rendering(&self) -> bool {
        matches!(self.status, RenderStatus::Rendering { .. })
    }

    pub fn mark_rendering_requested(&mut self) {
        self.status = RenderStatus::Rendering {
            backend_id: self.params.backend_id.clone(),
            scanlines_done: 0,
            total_scanlines: self.params.height,
        };
        self.preview = None;
    }

    pub fn apply_worker_event(&mut self, event: WorkerMessage) -> bool {
        match event {
            WorkerMessage::Begin {
                backend_id,
                request,
            } => {
                self.preview = Some(PreviewBuffer::new(request.width, request.height));
                self.status = RenderStatus::Rendering {
                    backend_id,
                    scanlines_done: 0,
                    total_scanlines: request.height,
                };
                false
            }
            WorkerMessage::Scanline { y, pixels } => {
                let preview_dirty = self
                    .preview
                    .as_mut()
                    .map(|preview| preview.apply_scanline(y, &pixels))
                    .unwrap_or(false);

                if let RenderStatus::Rendering {
                    scanlines_done,
                    total_scanlines,
                    ..
                } = &mut self.status
                    && *scanlines_done < *total_scanlines
                {
                    *scanlines_done += 1;
                }

                preview_dirty
            }
            WorkerMessage::End { stats } => {
                self.status = RenderStatus::Done(RenderDoneStats {
                    backend_id: stats.backend_id.as_str().to_string(),
                    frame_index: stats.frame_index,
                    elapsed: stats.elapsed,
                    samples_per_pixel: stats.samples_per_pixel,
                });
                false
            }
            WorkerMessage::Error(message) => {
                self.status = RenderStatus::Error(message);
                false
            }
        }
    }
}

fn run_render_job_with_initializer(
    params: RenderParams,
    tx: Sender<WorkerMessage>,
    initialize_gpu_fn: impl Fn() -> Result<(), String>,
) {
    if params.init_gpu
        && let Err(error) = initialize_gpu_fn()
    {
        let _ = tx.send(WorkerMessage::Error(error));
        return;
    }

    let registry = build_registry_with_config(RuntimeRenderConfig {
        exposure: DEFAULT_SPECTRAL_EXPOSURE,
        cpu_threads: params.cpu_threads,
    });
    let provider = build_animated_scene_provider(params.width as f32 / params.height as f32);
    let backend_id = BackendId::new(params.backend_id);

    let request = FrameRequest {
        width: params.width,
        height: params.height,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: params.samples_per_pixel,
        max_bounces: params.max_bounces,
    };

    let mut sink = ChannelFrameSink::new(tx.clone());
    if let Err(error) = render_frame(&registry, &provider, &backend_id, &request, &mut sink) {
        let _ = tx.send(WorkerMessage::Error(error.to_string()));
    }
}

pub fn run_render_job(params: RenderParams, tx: Sender<WorkerMessage>) {
    run_render_job_with_initializer(params, tx, || {
        initialize_gpu()
            .map(|_| ())
            .map_err(|error| error.to_string())
    });
}

struct CherryGuiApp {
    state: GuiState,
    backend_options: Vec<String>,
    worker_rx: Option<Receiver<WorkerMessage>>,
    worker_handle: Option<thread::JoinHandle<()>>,
    preview_texture: Option<egui::TextureHandle>,
    preview_dirty: bool,
}

impl CherryGuiApp {
    fn new() -> Self {
        let backend_options = build_registry_with_config(RuntimeRenderConfig {
            exposure: DEFAULT_SPECTRAL_EXPOSURE,
            cpu_threads: None,
        })
        .list_ids()
        .into_iter()
        .map(|id| id.as_str().to_string())
        .collect::<Vec<_>>();

        let mut state = GuiState::new(RenderParams::default());
        if !backend_options.contains(&state.params.backend_id)
            && let Some(first) = backend_options.first()
        {
            state.params.backend_id = first.clone();
        }

        Self {
            state,
            backend_options,
            worker_rx: None,
            worker_handle: None,
            preview_texture: None,
            preview_dirty: false,
        }
    }

    fn start_render(&mut self, ctx: &egui::Context) {
        if self.state.is_rendering() {
            return;
        }

        self.state.mark_rendering_requested();
        self.preview_texture = None;
        self.preview_dirty = false;

        let params = self.state.params.clone();
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            run_render_job(params, tx);
        });

        self.worker_rx = Some(rx);
        self.worker_handle = Some(handle);
        ctx.request_repaint();
    }

    fn process_worker_events(&mut self, ctx: &egui::Context) {
        let mut clear_worker = false;

        if let Some(rx) = &self.worker_rx {
            while let Ok(event) = rx.try_recv() {
                let terminal_event =
                    matches!(event, WorkerMessage::End { .. } | WorkerMessage::Error(_));
                if self.state.apply_worker_event(event) {
                    self.preview_dirty = true;
                }
                if terminal_event {
                    clear_worker = true;
                }
            }
        }

        if self.preview_dirty {
            self.upload_preview_texture(ctx);
            self.preview_dirty = false;
        }

        if self.state.is_rendering() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }

        if clear_worker {
            self.worker_rx = None;
            if let Some(handle) = self.worker_handle.take() {
                let _ = handle.join();
            }
            ctx.request_repaint();
        }
    }

    fn upload_preview_texture(&mut self, ctx: &egui::Context) {
        let Some(preview) = &self.state.preview else {
            return;
        };

        let image = egui::ColorImage::from_rgba_unmultiplied(
            [preview.width as usize, preview.height as usize],
            preview.pixels(),
        );

        if let Some(texture) = &mut self.preview_texture {
            texture.set(image, egui::TextureOptions::LINEAR);
        } else {
            self.preview_texture =
                Some(ctx.load_texture("cherry-preview", image, egui::TextureOptions::LINEAR));
        }
    }

    fn status_text(&self) -> String {
        match &self.state.status {
            RenderStatus::Idle => "Idle. Adjust parameters, then click Render.".to_string(),
            RenderStatus::Rendering {
                backend_id,
                scanlines_done,
                total_scanlines,
            } => format!("Rendering {backend_id}: {scanlines_done}/{total_scanlines} scanlines"),
            RenderStatus::Done(stats) => format!(
                "Done: {} frame {} in {:.2?} (spp={})",
                stats.backend_id, stats.frame_index, stats.elapsed, stats.samples_per_pixel
            ),
            RenderStatus::Error(message) => format!("Error: {message}"),
        }
    }

    fn draw_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    ui.label("Save/Export: coming soon");
                });
                ui.menu_button("Render", |ui| {
                    ui.label("Queue/Cancel: coming soon");
                });
                ui.menu_button("Animation", |ui| {
                    ui.label("Timeline: coming soon");
                });
                ui.separator();

                if ui
                    .add_enabled(!self.state.is_rendering(), egui::Button::new("Render"))
                    .clicked()
                {
                    self.start_render(ctx);
                }
            });
        });
    }

    fn draw_controls(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("left_controls")
            .resizable(true)
            .default_width(260.0)
            .show(ctx, |ui| {
                ui.heading("Render Controls");
                ui.separator();

                ui.add_enabled_ui(!self.state.is_rendering(), |ui| {
                    egui::ComboBox::from_label("Backend")
                        .selected_text(self.state.params.backend_id.clone())
                        .show_ui(ui, |ui| {
                            for backend_id in &self.backend_options {
                                ui.selectable_value(
                                    &mut self.state.params.backend_id,
                                    backend_id.clone(),
                                    backend_id,
                                );
                            }
                        });

                    ui.horizontal(|ui| {
                        ui.label("Width");
                        ui.add(egui::DragValue::new(&mut self.state.params.width).range(1..=4096));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Height");
                        ui.add(egui::DragValue::new(&mut self.state.params.height).range(1..=4096));
                    });
                    ui.horizontal(|ui| {
                        ui.label("SPP");
                        ui.add(
                            egui::DragValue::new(&mut self.state.params.samples_per_pixel)
                                .range(1..=4096),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Max Bounces");
                        ui.add(
                            egui::DragValue::new(&mut self.state.params.max_bounces).range(1..=64),
                        );
                    });
                    ui.horizontal(|ui| {
                        let mut auto_threads = self.state.params.cpu_threads.is_none();
                        if ui.checkbox(&mut auto_threads, "Auto CPU Threads").changed() {
                            self.state.params.cpu_threads =
                                if auto_threads { None } else { Some(1) };
                        }
                    });
                    if let Some(cpu_threads) = self.state.params.cpu_threads {
                        let mut thread_count = cpu_threads.max(1) as u32;
                        ui.horizontal(|ui| {
                            ui.label("CPU Threads");
                            if ui
                                .add(egui::DragValue::new(&mut thread_count).range(1..=256))
                                .changed()
                            {
                                self.state.params.cpu_threads = Some(thread_count as usize);
                            }
                        });
                    }
                    ui.checkbox(
                        &mut self.state.params.init_gpu,
                        "Initialize GPU before render",
                    );
                });

                ui.separator();
                ui.heading("Animation");
                ui.label("Frame sequencing and playback are reserved for a future milestone.");

                ui.separator();
                ui.heading("Menus & Tools");
                ui.label("Advanced panels, scene controls, and export tools are coming soon.");
            });
    }

    fn draw_preview(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Preview");
            ui.separator();

            let Some(texture) = &self.preview_texture else {
                ui.centered_and_justified(|ui| {
                    ui.label("No preview yet. Click Render to start.");
                });
                return;
            };

            let Some(preview) = &self.state.preview else {
                ui.centered_and_justified(|ui| {
                    ui.label("Preparing preview buffer...");
                });
                return;
            };

            let source = egui::vec2(preview.width as f32, preview.height as f32);
            let available = ui.available_size();
            let scale = (available.x / source.x)
                .min(available.y / source.y)
                .max(0.01);
            let target = source * scale;

            ui.centered_and_justified(|ui| {
                ui.image((texture.id(), target));
            });
        });
    }

    fn draw_status_bar(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.label(self.status_text());
        });
    }
}

impl eframe::App for CherryGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_worker_events(ctx);
        self.draw_top_bar(ctx);
        self.draw_controls(ctx);
        self.draw_preview(ctx);
        self.draw_status_bar(ctx);
    }
}

pub fn run() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Cherry GUI Preview")
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([960.0, 540.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Cherry GUI Preview",
        native_options,
        Box::new(|_cc| Ok(Box::new(CherryGuiApp::new()))),
    )
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, time::Duration};

    use cherry_core::{Color, FrameRequest};
    use cherry_render::{BackendId, RenderStats};

    use super::{
        GuiState, PreviewBuffer, RenderDoneStats, RenderParams, RenderStatus, WorkerMessage,
    };

    #[test]
    fn preview_buffer_writes_scanline_at_expected_offset() {
        let mut preview = PreviewBuffer::new(3, 2);

        let changed = preview.apply_scanline(
            1,
            &[
                Color::new(1.0, 0.0, 0.0),
                Color::new(0.0, 1.0, 0.0),
                Color::new(0.0, 0.0, 1.0),
            ],
        );

        assert!(changed);
        let bytes = preview.pixels();
        assert_eq!(bytes[(3 * 4)..(3 * 4 + 4)], [255, 0, 0, 255]);
        assert_eq!(bytes[(4 * 4)..(4 * 4 + 4)], [0, 255, 0, 255]);
        assert_eq!(bytes[(5 * 4)..(5 * 4 + 4)], [0, 0, 255, 255]);
    }

    #[test]
    fn gui_state_transitions_idle_rendering_done() {
        let mut state = GuiState::new(RenderParams::default());
        state.mark_rendering_requested();
        assert!(state.is_rendering());

        state.apply_worker_event(WorkerMessage::Begin {
            backend_id: "raster.simple".to_string(),
            request: FrameRequest {
                width: 4,
                height: 2,
                frame_index: 0,
                time: 0.0,
                samples_per_pixel: 1,
                max_bounces: 1,
            },
        });

        state.apply_worker_event(WorkerMessage::Scanline {
            y: 0,
            pixels: vec![Color::new(0.2, 0.3, 0.4); 4],
        });

        state.apply_worker_event(WorkerMessage::End {
            stats: RenderStats {
                backend_id: BackendId::new("raster.simple"),
                frame_index: 0,
                elapsed: Duration::from_millis(12),
                samples_per_pixel: 1,
            },
        });

        assert_eq!(
            state.status,
            RenderStatus::Done(RenderDoneStats {
                backend_id: "raster.simple".to_string(),
                frame_index: 0,
                elapsed: Duration::from_millis(12),
                samples_per_pixel: 1,
            })
        );
    }

    #[test]
    fn event_flow_updates_scanline_progress() {
        let mut state = GuiState::new(RenderParams::default());
        state.apply_worker_event(WorkerMessage::Begin {
            backend_id: "ray.normal".to_string(),
            request: FrameRequest {
                width: 3,
                height: 2,
                frame_index: 0,
                time: 0.0,
                samples_per_pixel: 1,
                max_bounces: 1,
            },
        });

        state.apply_worker_event(WorkerMessage::Scanline {
            y: 0,
            pixels: vec![Color::new(0.1, 0.1, 0.1); 3],
        });

        match state.status {
            RenderStatus::Rendering { scanlines_done, .. } => assert_eq!(scanlines_done, 1),
            _ => panic!("expected rendering status"),
        }
    }

    #[test]
    fn worker_job_emits_begin_scanline_end_without_panic() {
        let params = RenderParams {
            backend_id: "raster.simple".to_string(),
            width: 16,
            height: 16,
            samples_per_pixel: 1,
            max_bounces: 1,
            cpu_threads: None,
            init_gpu: false,
        };

        let (tx, rx) = mpsc::channel();
        super::run_render_job(params.clone(), tx);

        let mut labels = Vec::new();
        loop {
            let event = rx
                .recv_timeout(Duration::from_secs(2))
                .expect("expected render event");

            match event {
                WorkerMessage::Begin { .. } => labels.push("begin"),
                WorkerMessage::Scanline { .. } => labels.push("scanline"),
                WorkerMessage::End { .. } => {
                    labels.push("end");
                    break;
                }
                WorkerMessage::Error(message) => panic!("unexpected render error: {message}"),
            }
        }

        assert_eq!(labels.first(), Some(&"begin"));
        assert_eq!(labels.last(), Some(&"end"));
        assert_eq!(
            labels.iter().filter(|label| **label == "scanline").count(),
            params.height as usize
        );
    }

    #[test]
    fn default_params_keep_gpu_init_disabled_and_cpu_threads_auto() {
        let params = RenderParams::default();
        assert_eq!(params.cpu_threads, None);
        assert!(!params.init_gpu);
    }

    #[test]
    fn worker_job_reports_error_when_gpu_init_fails() {
        let params = RenderParams {
            backend_id: "raster.simple".to_string(),
            width: 8,
            height: 8,
            samples_per_pixel: 1,
            max_bounces: 1,
            cpu_threads: None,
            init_gpu: true,
        };

        let (tx, rx) = mpsc::channel();
        super::run_render_job_with_initializer(params, tx, || {
            Err("gpu init failed for test".to_string())
        });

        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(WorkerMessage::Error(message)) => {
                assert!(message.contains("gpu init failed for test"));
            }
            Ok(other) => panic!("expected error event, got {:?}", other),
            Err(err) => panic!("expected event, got channel error: {err}"),
        }
    }
}
