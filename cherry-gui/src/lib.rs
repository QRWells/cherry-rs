mod scene_editor;

use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use cherry_app::{
    CameraConfig, DEFAULT_SPECTRAL_EXPOSURE, RuntimeRenderConfig, build_registry_with_config,
    initialize_gpu,
};
use cherry_core::{Color, FrameRequest, PathTracingConfig, StaticSceneProvider};
use cherry_render::{BackendId, FrameEvent, FrameSink, RenderStats, color_to_rgb8, render_frame};
use eframe::egui;
use nalgebra::{Point3, Vector3};
use scene_editor::{
    AuthoredLight, AuthoredLightKind, AuthoredMaterial, AuthoredObject, AuthoredObjectKind,
    AuthoredScene, SceneSelection,
};

const RASTER_PREVIEW_BACKEND_ID: &str = "raster.simple";

#[derive(Debug, Clone, PartialEq)]
pub struct RenderParams {
    pub backend_id: String,
    pub width: u32,
    pub height: u32,
    pub samples_per_pixel: u32,
    pub max_bounces: u32,
    pub rr_start_depth: u32,
    pub rr_min_survival: f32,
    pub indirect_clamp: f32,
    pub direct_lighting: bool,
    pub cpu_threads: Option<usize>,
    pub init_gpu: bool,
    pub camera_look_from_x: f32,
    pub camera_look_from_y: f32,
    pub camera_look_from_z: f32,
    pub camera_look_at_x: f32,
    pub camera_look_at_y: f32,
    pub camera_look_at_z: f32,
    pub camera_view_up_x: f32,
    pub camera_view_up_y: f32,
    pub camera_view_up_z: f32,
    pub camera_fov_degrees: f32,
    pub camera_aperture: f32,
    pub camera_focal_distance: Option<f32>,
}

impl Default for RenderParams {
    fn default() -> Self {
        Self {
            backend_id: "ray.normal".to_string(),
            width: 320,
            height: 180,
            samples_per_pixel: 1,
            max_bounces: 3,
            rr_start_depth: 3,
            rr_min_survival: 0.05,
            indirect_clamp: 10.0,
            direct_lighting: true,
            cpu_threads: None,
            init_gpu: false,
            camera_look_from_x: 0.0,
            camera_look_from_y: 0.0,
            camera_look_from_z: 2.6,
            camera_look_at_x: 0.0,
            camera_look_at_y: -0.1,
            camera_look_at_z: -0.25,
            camera_view_up_x: 0.0,
            camera_view_up_y: 1.0,
            camera_view_up_z: 0.0,
            camera_fov_degrees: 38.0,
            camera_aperture: 0.0,
            camera_focal_distance: None,
        }
    }
}

impl RenderParams {
    fn camera_config(&self) -> CameraConfig {
        CameraConfig {
            look_from: Point3::new(
                self.camera_look_from_x,
                self.camera_look_from_y,
                self.camera_look_from_z,
            ),
            look_at: Point3::new(
                self.camera_look_at_x,
                self.camera_look_at_y,
                self.camera_look_at_z,
            ),
            view_up: Vector3::new(
                self.camera_view_up_x,
                self.camera_view_up_y,
                self.camera_view_up_z,
            ),
            fov_degrees: self.camera_fov_degrees,
            aperture: self.camera_aperture,
            focal_distance: self.camera_focal_distance,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobMode {
    Preview,
    Render,
}

impl JobMode {
    fn label(self) -> &'static str {
        match self {
            Self::Preview => "Preview",
            Self::Render => "Render",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderDoneStats {
    pub mode: JobMode,
    pub backend_id: String,
    pub frame_index: u32,
    pub elapsed: Duration,
    pub samples_per_pixel: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderStatus {
    Idle,
    Running {
        mode: JobMode,
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
        mode: JobMode,
        backend_id: String,
        request: FrameRequest,
    },
    Scanline {
        y: u32,
        pixels: Vec<Color>,
    },
    End {
        mode: JobMode,
        stats: RenderStats,
    },
    Error(String),
}

struct ChannelFrameSink {
    tx: Sender<WorkerMessage>,
    mode: JobMode,
}

impl ChannelFrameSink {
    fn new(tx: Sender<WorkerMessage>, mode: JobMode) -> Self {
        Self { tx, mode }
    }
}

impl FrameSink for ChannelFrameSink {
    fn on_event(&mut self, event: FrameEvent) {
        match event {
            FrameEvent::Begin { backend, request } => {
                let _ = self.tx.send(WorkerMessage::Begin {
                    mode: self.mode,
                    backend_id: backend.id.as_str().to_string(),
                    request,
                });
            }
            FrameEvent::Scanline { y, pixels, .. } => {
                let _ = self.tx.send(WorkerMessage::Scanline { y, pixels });
            }
            FrameEvent::End { stats } => {
                let _ = self.tx.send(WorkerMessage::End {
                    mode: self.mode,
                    stats,
                });
            }
        }
    }
}

#[derive(Clone)]
pub struct PreparedRenderJob {
    pub mode: JobMode,
    pub backend_id: BackendId,
    pub request: FrameRequest,
    runtime_config: RuntimeRenderConfig,
    init_gpu: bool,
    snapshot: cherry_core::SceneSnapshot,
}

impl PreparedRenderJob {
    pub fn prepare(
        mode: JobMode,
        params: RenderParams,
        scene: AuthoredScene,
    ) -> Result<Self, String> {
        let camera = params
            .camera_config()
            .to_camera(params.width as f32 / params.height as f32)?;
        let snapshot = scene.to_snapshot(camera)?;
        let backend_id = match mode {
            JobMode::Preview => BackendId::new(RASTER_PREVIEW_BACKEND_ID),
            JobMode::Render => BackendId::new(params.backend_id.clone()),
        };
        let request = FrameRequest {
            width: params.width,
            height: params.height,
            frame_index: 0,
            time: 0.0,
            samples_per_pixel: params.samples_per_pixel,
            max_bounces: params.max_bounces,
            path_tracing: PathTracingConfig {
                rr_start_depth: params.rr_start_depth,
                rr_min_survival: params.rr_min_survival,
                indirect_clamp: params.indirect_clamp,
                direct_lighting: params.direct_lighting,
            },
        };
        Ok(Self {
            mode,
            backend_id,
            request,
            runtime_config: RuntimeRenderConfig {
                exposure: DEFAULT_SPECTRAL_EXPOSURE,
                cpu_threads: params.cpu_threads,
            },
            init_gpu: params.init_gpu,
            snapshot,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuiState {
    pub params: RenderParams,
    pub scene: AuthoredScene,
    pub selected: Option<SceneSelection>,
    pub status: RenderStatus,
    pub preview: Option<PreviewBuffer>,
}

impl GuiState {
    pub fn new(params: RenderParams) -> Self {
        Self {
            params,
            scene: AuthoredScene::default(),
            selected: None,
            status: RenderStatus::Idle,
            preview: None,
        }
    }

    pub fn is_busy(&self) -> bool {
        matches!(self.status, RenderStatus::Running { .. })
    }

    pub fn mark_job_requested(&mut self, mode: JobMode, backend_id: String) {
        self.status = RenderStatus::Running {
            mode,
            backend_id,
            scanlines_done: 0,
            total_scanlines: self.params.height,
        };
        self.preview = None;
    }

    pub fn add_object(&mut self, kind: AuthoredObjectKind) -> u64 {
        let id = self.scene.add_object(
            format!("{} {}", kind.label(), self.scene.objects.len() + 1),
            kind,
            default_object_material(),
        );
        self.selected = Some(SceneSelection::Object(id));
        id
    }

    pub fn add_light(&mut self, kind: AuthoredLightKind) -> u64 {
        let id = self.scene.add_light(
            format!("{} {}", kind.label(), self.scene.lights.len() + 1),
            kind,
        );
        self.selected = Some(SceneSelection::Light(id));
        id
    }

    pub fn remove_selected(&mut self) {
        match self.selected {
            Some(SceneSelection::Object(id)) => {
                if self.scene.remove_object(id) {
                    self.selected = self
                        .scene
                        .objects
                        .first()
                        .map(|object| SceneSelection::Object(object.id))
                        .or_else(|| {
                            self.scene
                                .lights
                                .first()
                                .map(|light| SceneSelection::Light(light.id))
                        });
                }
            }
            Some(SceneSelection::Light(id)) => {
                if self.scene.remove_light(id) {
                    self.selected = self
                        .scene
                        .lights
                        .first()
                        .map(|light| SceneSelection::Light(light.id))
                        .or_else(|| {
                            self.scene
                                .objects
                                .first()
                                .map(|object| SceneSelection::Object(object.id))
                        });
                }
            }
            None => {}
        }
    }

    pub fn apply_worker_event(&mut self, event: WorkerMessage) -> bool {
        match event {
            WorkerMessage::Begin {
                mode,
                backend_id,
                request,
            } => {
                self.preview = Some(PreviewBuffer::new(request.width, request.height));
                self.status = RenderStatus::Running {
                    mode,
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
                if let RenderStatus::Running {
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
            WorkerMessage::End { mode, stats } => {
                self.status = RenderStatus::Done(RenderDoneStats {
                    mode,
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

fn default_object_material() -> AuthoredMaterial {
    AuthoredMaterial::opaque(Color::new(0.7, 0.7, 0.7), 0.0, 0.35)
}

fn default_sphere_kind() -> AuthoredObjectKind {
    AuthoredObjectKind::Sphere {
        center: Point3::new(0.0, -0.45, -0.1),
        radius: 0.28,
    }
}

fn default_cuboid_kind() -> AuthoredObjectKind {
    AuthoredObjectKind::Cuboid {
        min: Point3::new(-0.25, -1.0, -0.25),
        max: Point3::new(0.25, -0.35, 0.25),
    }
}

fn default_point_light_kind() -> AuthoredLightKind {
    AuthoredLightKind::Point {
        position: Point3::new(0.0, 0.65, 0.0),
        intensity: Color::new(1.0, 1.0, 1.0),
    }
}

fn default_directional_light_kind() -> AuthoredLightKind {
    AuthoredLightKind::Directional {
        direction: Vector3::new(0.2, -1.0, -0.15),
        intensity: Color::new(0.8, 0.8, 0.8),
    }
}

fn run_render_job_with_initializer(
    job: PreparedRenderJob,
    tx: Sender<WorkerMessage>,
    initialize_gpu_fn: impl Fn() -> Result<(), String>,
) {
    if job.init_gpu
        && let Err(error) = initialize_gpu_fn()
    {
        let _ = tx.send(WorkerMessage::Error(error));
        return;
    }

    let registry = build_registry_with_config(job.runtime_config);
    let provider = StaticSceneProvider::new(job.snapshot);
    let mut sink = ChannelFrameSink::new(tx.clone(), job.mode);
    if let Err(error) = render_frame(
        &registry,
        &provider,
        &job.backend_id,
        &job.request,
        &mut sink,
    ) {
        let _ = tx.send(WorkerMessage::Error(error.to_string()));
    }
}

pub fn run_render_job(job: PreparedRenderJob, tx: Sender<WorkerMessage>) {
    run_render_job_with_initializer(job, tx, || {
        initialize_gpu()
            .map(|_| ())
            .map_err(|error| error.to_string())
    });
}

fn supports_path_tracing_controls(backend_id: &str) -> bool {
    matches!(backend_id, "ray.montecarlo" | "ray.spectral")
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

    fn start_job(&mut self, ctx: &egui::Context, mode: JobMode) {
        if self.state.is_busy() {
            return;
        }
        let job = match PreparedRenderJob::prepare(
            mode,
            self.state.params.clone(),
            self.state.scene.clone(),
        ) {
            Ok(job) => job,
            Err(message) => {
                self.state.status = RenderStatus::Error(message);
                ctx.request_repaint();
                return;
            }
        };
        self.state
            .mark_job_requested(job.mode, job.backend_id.as_str().to_string());
        self.preview_texture = None;
        self.preview_dirty = false;

        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || run_render_job(job, tx));
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
        if self.state.is_busy() {
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
            [preview.width() as usize, preview.height() as usize],
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
            RenderStatus::Idle => {
                "Idle. Adjust the scene or render settings, then click Preview or Render."
                    .to_string()
            }
            RenderStatus::Running {
                mode,
                backend_id,
                scanlines_done,
                total_scanlines,
            } => format!(
                "{} {}: {}/{} scanlines",
                mode.label(),
                backend_id,
                scanlines_done,
                total_scanlines
            ),
            RenderStatus::Done(stats) => format!(
                "{} complete: {} frame {} in {:.2?} (spp={})",
                stats.mode.label(),
                stats.backend_id,
                stats.frame_index,
                stats.elapsed,
                stats.samples_per_pixel
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
                    .add_enabled(!self.state.is_busy(), egui::Button::new("Preview"))
                    .clicked()
                {
                    self.start_job(ctx, JobMode::Preview);
                }
                if ui
                    .add_enabled(!self.state.is_busy(), egui::Button::new("Render"))
                    .clicked()
                {
                    self.start_job(ctx, JobMode::Render);
                }
            });
        });
    }

    fn draw_scene_panel(&mut self, ctx: &egui::Context) {
        #[derive(Clone, Copy)]
        enum SceneAction {
            AddCuboid,
            AddSphere,
            AddPointLight,
            AddDirectionalLight,
            RemoveSelected,
        }

        let busy = self.state.is_busy();
        let mut action = None;
        egui::SidePanel::left("scene_panel")
            .resizable(true)
            .default_width(320.0)
            .show(ctx, |ui| {
                ui.heading("Scene");
                ui.separator();

                ui.add_enabled_ui(!busy, |ui| {
                    edit_color(ui, "Background", &mut self.state.scene.background);

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Add Cuboid").clicked() {
                            action = Some(SceneAction::AddCuboid);
                        }
                        if ui.button("Add Sphere").clicked() {
                            action = Some(SceneAction::AddSphere);
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Add Point Light").clicked() {
                            action = Some(SceneAction::AddPointLight);
                        }
                        if ui.button("Add Sun Light").clicked() {
                            action = Some(SceneAction::AddDirectionalLight);
                        }
                    });

                    ui.separator();
                    ui.label("Objects");
                    egui::ScrollArea::vertical()
                        .max_height(160.0)
                        .show(ui, |ui| {
                            for object in &self.state.scene.objects {
                                let selected =
                                    self.state.selected == Some(SceneSelection::Object(object.id));
                                if ui
                                    .selectable_label(
                                        selected,
                                        format!("{} [{}]", object.name, object.kind_label()),
                                    )
                                    .clicked()
                                {
                                    self.state.selected = Some(SceneSelection::Object(object.id));
                                }
                            }
                        });

                    ui.separator();
                    ui.label("Lights");
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .show(ui, |ui| {
                            for light in &self.state.scene.lights {
                                let selected =
                                    self.state.selected == Some(SceneSelection::Light(light.id));
                                if ui
                                    .selectable_label(
                                        selected,
                                        format!("{} [{}]", light.name, light.kind_label()),
                                    )
                                    .clicked()
                                {
                                    self.state.selected = Some(SceneSelection::Light(light.id));
                                }
                            }
                        });

                    ui.separator();
                    let mut remove_selected = false;
                    match self.state.selected {
                        Some(SceneSelection::Object(id)) => {
                            if let Some(object) = self.state.scene.object_mut(id) {
                                Self::draw_object_inspector(ui, object);
                                remove_selected = ui.button("Remove Selected").clicked();
                            } else {
                                self.state.selected = None;
                            }
                        }
                        Some(SceneSelection::Light(id)) => {
                            if let Some(light) = self.state.scene.light_mut(id) {
                                Self::draw_light_inspector(ui, light);
                                remove_selected = ui.button("Remove Selected").clicked();
                            } else {
                                self.state.selected = None;
                            }
                        }
                        None => {
                            ui.label("Select an object or light to edit it.");
                        }
                    }

                    if remove_selected {
                        action = Some(SceneAction::RemoveSelected);
                    }
                });
            });

        if let Some(action) = action {
            match action {
                SceneAction::AddCuboid => {
                    self.state.add_object(default_cuboid_kind());
                }
                SceneAction::AddSphere => {
                    self.state.add_object(default_sphere_kind());
                }
                SceneAction::AddPointLight => {
                    self.state.add_light(default_point_light_kind());
                }
                SceneAction::AddDirectionalLight => {
                    self.state.add_light(default_directional_light_kind());
                }
                SceneAction::RemoveSelected => self.state.remove_selected(),
            }
        }
    }

    fn draw_object_inspector(ui: &mut egui::Ui, object: &mut AuthoredObject) {
        ui.heading("Object");
        ui.text_edit_singleline(&mut object.name);
        ui.label(object.kind_label());

        match &mut object.kind {
            AuthoredObjectKind::Cuboid { min, max } => {
                edit_point3(ui, "Min", min);
                edit_point3(ui, "Max", max);
            }
            AuthoredObjectKind::Sphere { center, radius } => {
                edit_point3(ui, "Center", center);
                ui.horizontal(|ui| {
                    ui.label("Radius");
                    ui.add(
                        egui::DragValue::new(radius)
                            .range(0.001..=1000.0)
                            .speed(0.01),
                    );
                });
            }
        }

        ui.separator();
        Self::draw_material_editor(ui, &mut object.material);
    }

    fn draw_light_inspector(ui: &mut egui::Ui, light: &mut AuthoredLight) {
        ui.heading("Light");
        ui.text_edit_singleline(&mut light.name);
        ui.label(light.kind_label());

        match &mut light.kind {
            AuthoredLightKind::Point {
                position,
                intensity,
            } => {
                edit_point3(ui, "Position", position);
                edit_color(ui, "Intensity", intensity);
            }
            AuthoredLightKind::Directional {
                direction,
                intensity,
            } => {
                edit_vector3(ui, "Direction", direction);
                edit_color(ui, "Intensity", intensity);
            }
        }
    }

    fn draw_material_editor(ui: &mut egui::Ui, material: &mut AuthoredMaterial) {
        ui.heading("Material");
        edit_color(ui, "Base Color", &mut material.base_color);
        edit_color(ui, "Emissive", &mut material.emissive);
        ui.horizontal(|ui| {
            ui.label("Metallic");
            ui.add(
                egui::DragValue::new(&mut material.metallic)
                    .range(0.0..=1.0)
                    .speed(0.01),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Roughness");
            ui.add(
                egui::DragValue::new(&mut material.roughness)
                    .range(0.0..=1.0)
                    .speed(0.01),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Transmission");
            ui.add(
                egui::DragValue::new(&mut material.transmission)
                    .range(0.0..=1.0)
                    .speed(0.01),
            );
        });
        ui.horizontal(|ui| {
            ui.label("IOR");
            ui.add(
                egui::DragValue::new(&mut material.ior)
                    .range(1.0..=4.0)
                    .speed(0.01),
            );
        });
    }

    fn draw_controls(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("right_controls")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.heading("Render Controls");
                ui.separator();

                ui.add_enabled_ui(!self.state.is_busy(), |ui| {
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

                    ui.separator();
                    ui.label("Camera");
                    drag_f32_triplet(
                        ui,
                        "Look From",
                        [
                            &mut self.state.params.camera_look_from_x,
                            &mut self.state.params.camera_look_from_y,
                            &mut self.state.params.camera_look_from_z,
                        ],
                    );
                    drag_f32_triplet(
                        ui,
                        "Look At",
                        [
                            &mut self.state.params.camera_look_at_x,
                            &mut self.state.params.camera_look_at_y,
                            &mut self.state.params.camera_look_at_z,
                        ],
                    );
                    drag_f32_triplet(
                        ui,
                        "View Up",
                        [
                            &mut self.state.params.camera_view_up_x,
                            &mut self.state.params.camera_view_up_y,
                            &mut self.state.params.camera_view_up_z,
                        ],
                    );
                    ui.horizontal(|ui| {
                        ui.label("FOV");
                        ui.add(
                            egui::DragValue::new(&mut self.state.params.camera_fov_degrees)
                                .range(1.0..=179.0)
                                .speed(0.1),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Aperture");
                        ui.add(
                            egui::DragValue::new(&mut self.state.params.camera_aperture)
                                .range(0.0..=16.0)
                                .speed(0.01),
                        );
                    });
                    ui.horizontal(|ui| {
                        let mut auto_focal = self.state.params.camera_focal_distance.is_none();
                        if ui
                            .checkbox(&mut auto_focal, "Auto Focal Distance")
                            .changed()
                        {
                            self.state.params.camera_focal_distance = if auto_focal {
                                None
                            } else {
                                let dx = self.state.params.camera_look_from_x
                                    - self.state.params.camera_look_at_x;
                                let dy = self.state.params.camera_look_from_y
                                    - self.state.params.camera_look_at_y;
                                let dz = self.state.params.camera_look_from_z
                                    - self.state.params.camera_look_at_z;
                                Some((dx * dx + dy * dy + dz * dz).sqrt().max(1e-3))
                            };
                        }
                    });
                    if let Some(focal_distance) = &mut self.state.params.camera_focal_distance {
                        ui.horizontal(|ui| {
                            ui.label("Focal Distance");
                            ui.add(
                                egui::DragValue::new(focal_distance)
                                    .range(0.001..=1_000.0)
                                    .speed(0.01),
                            );
                        });
                    }

                    if supports_path_tracing_controls(&self.state.params.backend_id) {
                        ui.separator();
                        ui.label("Path Tracing");
                        ui.horizontal(|ui| {
                            ui.label("RR Start Depth");
                            ui.add(
                                egui::DragValue::new(&mut self.state.params.rr_start_depth)
                                    .range(0..=64),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("RR Min Survival");
                            ui.add(
                                egui::DragValue::new(&mut self.state.params.rr_min_survival)
                                    .range(0.0..=1.0)
                                    .speed(0.01),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("Indirect Clamp");
                            ui.add(
                                egui::DragValue::new(&mut self.state.params.indirect_clamp)
                                    .speed(0.1),
                            );
                        });
                        ui.checkbox(&mut self.state.params.direct_lighting, "Direct Lighting");
                    }
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
                ui.heading("Preview Notes");
                ui.label("Preview uses raster.simple for fast geometry and base-color feedback.");
                ui.label(
                    "Authored lights, shadows, and higher-fidelity material behavior still require Render.",
                );
            });
    }

    fn draw_preview(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Preview");
            ui.separator();

            let Some(texture) = &self.preview_texture else {
                ui.centered_and_justified(|ui| {
                    ui.label("No preview yet. Click Preview or Render to start.");
                });
                return;
            };
            let Some(preview) = &self.state.preview else {
                ui.centered_and_justified(|ui| {
                    ui.label("Preparing preview buffer...");
                });
                return;
            };

            let source = egui::vec2(preview.width() as f32, preview.height() as f32);
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
        self.draw_scene_panel(ctx);
        self.draw_controls(ctx);
        self.draw_preview(ctx);
        self.draw_status_bar(ctx);
    }
}

fn edit_point3(ui: &mut egui::Ui, label: &str, point: &mut Point3<f32>) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(&mut point.x).speed(0.01));
        ui.add(egui::DragValue::new(&mut point.y).speed(0.01));
        ui.add(egui::DragValue::new(&mut point.z).speed(0.01));
    });
}

fn edit_vector3(ui: &mut egui::Ui, label: &str, vector: &mut Vector3<f32>) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(&mut vector.x).speed(0.01));
        ui.add(egui::DragValue::new(&mut vector.y).speed(0.01));
        ui.add(egui::DragValue::new(&mut vector.z).speed(0.01));
    });
}

fn edit_color(ui: &mut egui::Ui, label: &str, color: &mut Color) {
    let mut rgb = [color.x, color.y, color.z];
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.color_edit_button_rgb(&mut rgb).changed() {
            *color = Color::new(rgb[0], rgb[1], rgb[2]);
        }
    });
}

fn drag_f32_triplet(ui: &mut egui::Ui, label: &str, values: [&mut f32; 3]) {
    ui.horizontal(|ui| {
        ui.label(label);
        for value in values {
            ui.add(egui::DragValue::new(value).speed(0.01));
        }
    });
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
    use nalgebra::Point3;

    use super::{
        GuiState, JobMode, PreparedRenderJob, PreviewBuffer, RenderDoneStats, RenderParams,
        RenderStatus, WorkerMessage, default_directional_light_kind, default_sphere_kind,
        supports_path_tracing_controls,
    };
    use crate::scene_editor::{AuthoredObjectKind, SceneSelection};

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
    fn gui_state_transitions_idle_running_done() {
        let mut state = GuiState::new(RenderParams::default());
        state.mark_job_requested(JobMode::Render, "raster.simple".to_string());
        assert!(state.is_busy());

        state.apply_worker_event(WorkerMessage::Begin {
            mode: JobMode::Render,
            backend_id: "raster.simple".to_string(),
            request: FrameRequest {
                width: 4,
                height: 2,
                frame_index: 0,
                time: 0.0,
                samples_per_pixel: 1,
                max_bounces: 1,
                path_tracing: Default::default(),
            },
        });
        state.apply_worker_event(WorkerMessage::Scanline {
            y: 0,
            pixels: vec![Color::new(0.2, 0.3, 0.4); 4],
        });
        state.apply_worker_event(WorkerMessage::End {
            mode: JobMode::Render,
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
                mode: JobMode::Render,
                backend_id: "raster.simple".to_string(),
                frame_index: 0,
                elapsed: Duration::from_millis(12),
                samples_per_pixel: 1,
            })
        );
        assert!(!state.is_busy());
    }

    #[test]
    fn event_flow_updates_scanline_progress() {
        let mut state = GuiState::new(RenderParams::default());
        state.apply_worker_event(WorkerMessage::Begin {
            mode: JobMode::Preview,
            backend_id: "ray.normal".to_string(),
            request: FrameRequest {
                width: 3,
                height: 2,
                frame_index: 0,
                time: 0.0,
                samples_per_pixel: 1,
                max_bounces: 1,
                path_tracing: Default::default(),
            },
        });
        state.apply_worker_event(WorkerMessage::Scanline {
            y: 0,
            pixels: vec![Color::new(0.1, 0.1, 0.1); 3],
        });

        match state.status {
            RenderStatus::Running { scanlines_done, .. } => assert_eq!(scanlines_done, 1),
            _ => panic!("expected running status"),
        }
    }

    #[test]
    fn worker_job_emits_begin_scanline_end_without_panic() {
        let state = GuiState::new(RenderParams::default());
        let job =
            PreparedRenderJob::prepare(JobMode::Preview, state.params.clone(), state.scene.clone())
                .expect("preview job should prepare");

        let (tx, rx) = mpsc::channel();
        super::run_render_job(job.clone(), tx);

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
            job.request.height as usize
        );
    }

    #[test]
    fn default_params_keep_gpu_init_disabled_and_cpu_threads_auto() {
        let params = RenderParams::default();
        assert_eq!(params.rr_start_depth, 3);
        assert!((params.rr_min_survival - 0.05).abs() < f32::EPSILON);
        assert!((params.indirect_clamp - 10.0).abs() < f32::EPSILON);
        assert!(params.direct_lighting);
        assert_eq!(params.cpu_threads, None);
        assert!(!params.init_gpu);
        assert!((params.camera_look_from_z - 2.6).abs() < f32::EPSILON);
        assert!(params.camera_focal_distance.is_none());
    }

    #[test]
    fn worker_job_reports_error_when_gpu_init_fails() {
        let state = GuiState::new(RenderParams {
            init_gpu: true,
            ..RenderParams::default()
        });
        let job =
            PreparedRenderJob::prepare(JobMode::Render, state.params.clone(), state.scene.clone())
                .expect("render job should prepare");

        let (tx, rx) = mpsc::channel();
        super::run_render_job_with_initializer(job, tx, || {
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

    #[test]
    fn path_tracing_control_visibility_is_backend_gated() {
        assert!(supports_path_tracing_controls("ray.montecarlo"));
        assert!(supports_path_tracing_controls("ray.spectral"));
        assert!(!supports_path_tracing_controls("ray.normal"));
        assert!(!supports_path_tracing_controls("raster.simple"));
    }

    #[test]
    fn worker_job_propagates_path_tracing_settings_to_request() {
        let params = RenderParams {
            backend_id: "ray.spectral".to_string(),
            width: 8,
            height: 8,
            samples_per_pixel: 1,
            max_bounces: 2,
            rr_start_depth: 7,
            rr_min_survival: 0.22,
            indirect_clamp: 2.75,
            direct_lighting: false,
            ..RenderParams::default()
        };
        let state = GuiState::new(params.clone());
        let job = PreparedRenderJob::prepare(JobMode::Render, params, state.scene.clone()).unwrap();

        let (tx, rx) = mpsc::channel();
        super::run_render_job(job, tx);

        let begin_request = loop {
            match rx
                .recv_timeout(Duration::from_secs(2))
                .expect("expected event")
            {
                WorkerMessage::Begin { request, .. } => break request,
                WorkerMessage::Scanline { .. } => {}
                WorkerMessage::End { .. } => panic!("expected begin event before end"),
                WorkerMessage::Error(message) => panic!("unexpected render error: {message}"),
            }
        };

        assert_eq!(begin_request.path_tracing.rr_start_depth, 7);
        assert!((begin_request.path_tracing.rr_min_survival - 0.22).abs() < f32::EPSILON);
        assert!((begin_request.path_tracing.indirect_clamp - 2.75).abs() < f32::EPSILON);
        assert!(!begin_request.path_tracing.direct_lighting);
    }

    #[test]
    fn render_params_camera_config_preserves_explicit_focal_distance() {
        let mut params = RenderParams::default();
        params.camera_focal_distance = Some(1.75);
        let config = params.camera_config();
        assert_eq!(config.focal_distance, Some(1.75));
    }

    #[test]
    fn gui_state_can_add_and_remove_selected_object() {
        let mut state = GuiState::new(RenderParams::default());
        let original_count = state.scene.objects.len();

        let added_id = state.add_object(default_sphere_kind());

        assert_eq!(state.selected, Some(SceneSelection::Object(added_id)));
        assert_eq!(state.scene.objects.len(), original_count + 1);

        state.remove_selected();

        assert_eq!(state.scene.objects.len(), original_count);
    }

    #[test]
    fn gui_state_can_add_and_remove_selected_light() {
        let mut state = GuiState::new(RenderParams::default());
        let original_count = state.scene.lights.len();

        let added_id = state.add_light(default_directional_light_kind());

        assert_eq!(state.selected, Some(SceneSelection::Light(added_id)));
        assert_eq!(state.scene.lights.len(), original_count + 1);

        state.remove_selected();

        assert_eq!(state.scene.lights.len(), original_count);
    }

    #[test]
    fn preview_job_forces_raster_backend() {
        let state = GuiState::new(RenderParams {
            backend_id: "ray.spectral".to_string(),
            ..RenderParams::default()
        });

        let job =
            PreparedRenderJob::prepare(JobMode::Preview, state.params.clone(), state.scene.clone())
                .expect("preview job should prepare");

        assert_eq!(job.backend_id.as_str(), "raster.simple");
    }

    #[test]
    fn render_job_uses_selected_backend() {
        let state = GuiState::new(RenderParams {
            backend_id: "ray.spectral".to_string(),
            ..RenderParams::default()
        });

        let job =
            PreparedRenderJob::prepare(JobMode::Render, state.params.clone(), state.scene.clone())
                .expect("render job should prepare");

        assert_eq!(job.backend_id.as_str(), "ray.spectral");
    }

    #[test]
    fn prepare_job_rejects_invalid_scene() {
        let mut state = GuiState::new(RenderParams::default());
        state.scene.objects.clear();
        let added_id = state.add_object(AuthoredObjectKind::Sphere {
            center: Point3::new(0.0, 0.0, 0.0),
            radius: 0.0,
        });
        assert_eq!(state.selected, Some(SceneSelection::Object(added_id)));

        let err =
            PreparedRenderJob::prepare(JobMode::Render, state.params.clone(), state.scene.clone())
                .err()
                .expect("invalid scene should fail job preparation");

        assert!(err.contains("radius"));
    }

    #[test]
    fn preview_and_render_statuses_are_distinct() {
        let mut state = GuiState::new(RenderParams::default());
        state.mark_job_requested(JobMode::Preview, "raster.simple".to_string());
        assert!(matches!(
            state.status,
            RenderStatus::Running {
                mode: JobMode::Preview,
                ..
            }
        ));
        assert!(state.is_busy());

        state.apply_worker_event(WorkerMessage::End {
            mode: JobMode::Preview,
            stats: RenderStats {
                backend_id: BackendId::new("raster.simple"),
                frame_index: 0,
                elapsed: Duration::from_millis(12),
                samples_per_pixel: 1,
            },
        });

        assert!(matches!(
            state.status,
            RenderStatus::Done(RenderDoneStats {
                mode: JobMode::Preview,
                ..
            })
        ));

        state.mark_job_requested(JobMode::Render, "ray.normal".to_string());
        assert!(matches!(
            state.status,
            RenderStatus::Running {
                mode: JobMode::Render,
                ..
            }
        ));
    }
}
