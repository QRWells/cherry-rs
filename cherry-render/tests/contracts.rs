use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use cherry_core::{Camera, Color, FrameRequest, SceneProvider, SceneSnapshot};
use cherry_render::{
    BackendCapabilities, BackendId, BackendMetadata, BackendRegistry, FrameEvent, FrameSink,
    NoopFrameSink, PixelRadiance, RenderBackend, RenderStats, SPECTRAL_BIN_COUNT, SequenceSpec,
    TypedScanline, render_frame, render_sequence,
};
use nalgebra::{Point3, Vector2, Vector3};

struct MockSceneProvider {
    calls: Arc<Mutex<Vec<f32>>>,
}

impl MockSceneProvider {
    fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl SceneProvider for MockSceneProvider {
    fn snapshot(&self, time: f32) -> SceneSnapshot {
        self.calls.lock().unwrap().push(time);
        SceneSnapshot::new(Camera::new(
            Point3::new(0.0, 0.0, 3.0),
            Point3::new(0.0, 0.0, 0.0),
            Vector3::y_axis().into_inner(),
            60.0,
            16.0 / 9.0,
            0.0,
            1.0,
        ))
        .with_background(Color::new(0.1, 0.1, 0.1))
    }
}

struct MockBackend;

impl RenderBackend for MockBackend {
    type Pixel = Color;

    fn metadata(&self) -> BackendMetadata {
        BackendMetadata {
            id: BackendId::new("mock"),
            display_name: "Mock Backend".to_string(),
            capabilities: BackendCapabilities {
                progressive_updates: true,
                gpu_ready_interface: false,
            },
        }
    }

    fn render_scanlines(
        &self,
        _scene: &SceneSnapshot,
        request: &FrameRequest,
        emit_scanline: &mut dyn FnMut(TypedScanline<Self::Pixel>),
    ) -> RenderStats {
        for y in 0..request.height {
            emit_scanline(TypedScanline {
                y,
                pixels: vec![Color::new(0.25, 0.5, 0.75); request.width as usize],
            });
        }

        RenderStats {
            backend_id: BackendId::new("mock"),
            frame_index: request.frame_index,
            elapsed: Duration::from_millis(1),
            samples_per_pixel: request.samples_per_pixel,
        }
    }
}

struct CameraProbeBackend {
    ray_dir_probe: Arc<Mutex<Option<Vector3<f32>>>>,
}

impl RenderBackend for CameraProbeBackend {
    type Pixel = Color;

    fn metadata(&self) -> BackendMetadata {
        BackendMetadata {
            id: BackendId::new("mock.camera-probe"),
            display_name: "Mock Camera Probe Backend".to_string(),
            capabilities: BackendCapabilities {
                progressive_updates: false,
                gpu_ready_interface: false,
            },
        }
    }

    fn render_scanlines(
        &self,
        scene: &SceneSnapshot,
        request: &FrameRequest,
        emit_scanline: &mut dyn FnMut(TypedScanline<Self::Pixel>),
    ) -> RenderStats {
        let ray = scene.camera.generate_ray(Vector2::new(1.0, 0.5));
        *self.ray_dir_probe.lock().unwrap() = Some(ray.dir);

        for y in 0..request.height {
            emit_scanline(TypedScanline {
                y,
                pixels: vec![Color::new(0.0, 0.0, 0.0); request.width as usize],
            });
        }

        RenderStats {
            backend_id: BackendId::new("mock.camera-probe"),
            frame_index: request.frame_index,
            elapsed: Duration::from_millis(1),
            samples_per_pixel: request.samples_per_pixel,
        }
    }
}

#[derive(Clone)]
struct MockSpectralPixel {
    color: Color,
    bins: [f32; SPECTRAL_BIN_COUNT],
}

impl PixelRadiance for MockSpectralPixel {
    fn to_rgb_color(&self) -> Color {
        self.color
    }

    fn spectral_bins(&self) -> Option<[f32; SPECTRAL_BIN_COUNT]> {
        Some(self.bins)
    }
}

struct MockSpectralBackend;

impl RenderBackend for MockSpectralBackend {
    type Pixel = MockSpectralPixel;

    fn metadata(&self) -> BackendMetadata {
        BackendMetadata {
            id: BackendId::new("mock.spectral"),
            display_name: "Mock Spectral Backend".to_string(),
            capabilities: BackendCapabilities {
                progressive_updates: true,
                gpu_ready_interface: true,
            },
        }
    }

    fn render_scanlines(
        &self,
        _scene: &SceneSnapshot,
        request: &FrameRequest,
        emit_scanline: &mut dyn FnMut(TypedScanline<Self::Pixel>),
    ) -> RenderStats {
        for y in 0..request.height {
            let pixel = MockSpectralPixel {
                color: Color::new(0.2, 0.3, 0.4),
                bins: [0.1; SPECTRAL_BIN_COUNT],
            };
            emit_scanline(TypedScanline {
                y,
                pixels: vec![pixel; request.width as usize],
            });
        }

        RenderStats {
            backend_id: BackendId::new("mock.spectral"),
            frame_index: request.frame_index,
            elapsed: Duration::from_millis(1),
            samples_per_pixel: request.samples_per_pixel,
        }
    }
}

struct CollectingSink {
    events: Vec<String>,
}

impl CollectingSink {
    fn new() -> Self {
        Self { events: Vec::new() }
    }
}

impl FrameSink for CollectingSink {
    fn on_event(&mut self, event: FrameEvent) {
        let label = match event {
            FrameEvent::Begin { .. } => "begin",
            FrameEvent::Scanline { .. } => "scanline",
            FrameEvent::End { .. } => "end",
        };
        self.events.push(label.to_string());
    }
}

fn register_mock_backend(registry: &mut BackendRegistry) {
    registry.register_factory(BackendId::new("mock"), Arc::new(|| Box::new(MockBackend)));
}

fn register_mock_spectral_backend(registry: &mut BackendRegistry) {
    registry.register_factory(
        BackendId::new("mock.spectral"),
        Arc::new(|| Box::new(MockSpectralBackend)),
    );
}

fn register_camera_probe_backend(
    registry: &mut BackendRegistry,
    ray_dir_probe: Arc<Mutex<Option<Vector3<f32>>>>,
) {
    registry.register_factory(
        BackendId::new("mock.camera-probe"),
        Arc::new(move || {
            Box::new(CameraProbeBackend {
                ray_dir_probe: Arc::clone(&ray_dir_probe),
            })
        }),
    );
}

#[test]
fn registry_can_register_and_create_backend() {
    let mut registry = BackendRegistry::new();
    register_mock_backend(&mut registry);

    assert!(registry.contains(&BackendId::new("mock")));
    assert_eq!(registry.list_ids(), vec![BackendId::new("mock")]);
    assert!(registry.create(&BackendId::new("mock")).is_some());
}

#[test]
fn render_frame_produces_image_of_expected_size() {
    let mut registry = BackendRegistry::new();
    register_mock_backend(&mut registry);
    let provider = MockSceneProvider::new();

    let request = FrameRequest {
        width: 8,
        height: 4,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 1,
        max_bounces: 1,
        path_tracing: Default::default(),
    };

    let mut sink = NoopFrameSink;
    let result = render_frame(
        &registry,
        &provider,
        &BackendId::new("mock"),
        &request,
        &mut sink,
    )
    .unwrap();

    assert_eq!(result.image.width(), 8);
    assert_eq!(result.image.height(), 4);
    assert_ne!(result.image.get_pixel(0, 0), &image::Rgb([0, 0, 0]));
}

#[test]
fn frame_sink_receives_events_in_order() {
    let mut registry = BackendRegistry::new();
    register_mock_backend(&mut registry);
    let provider = MockSceneProvider::new();

    let request = FrameRequest {
        width: 4,
        height: 2,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 1,
        max_bounces: 1,
        path_tracing: Default::default(),
    };

    let mut sink = CollectingSink::new();
    let _ = render_frame(
        &registry,
        &provider,
        &BackendId::new("mock"),
        &request,
        &mut sink,
    )
    .unwrap();

    assert_eq!(sink.events.first().unwrap(), "begin");
    assert_eq!(sink.events.last().unwrap(), "end");
    assert_eq!(
        sink.events
            .iter()
            .filter(|event| event.as_str() == "scanline")
            .count(),
        2
    );
}

#[test]
fn render_sequence_calls_snapshot_with_expected_times() {
    let mut registry = BackendRegistry::new();
    register_mock_backend(&mut registry);
    let provider = MockSceneProvider::new();

    let spec = SequenceSpec {
        frame_count: 3,
        start_time: 1.0,
        frame_time_step: 0.25,
        template: FrameRequest {
            width: 4,
            height: 4,
            frame_index: 0,
            time: 0.0,
            samples_per_pixel: 1,
            max_bounces: 1,
            path_tracing: Default::default(),
        },
    };

    let results = render_sequence(
        &registry,
        &provider,
        &BackendId::new("mock"),
        &spec,
        |_frame_index, _request| Box::new(NoopFrameSink),
    )
    .unwrap();

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].stats.frame_index, 0);
    assert_eq!(results[1].stats.frame_index, 1);
    assert_eq!(results[2].stats.frame_index, 2);

    let calls = provider.calls.lock().unwrap().clone();
    assert_eq!(calls, vec![1.0, 1.25, 1.5]);
}

fn approx_vec3(a: Vector3<f32>, b: Vector3<f32>) -> bool {
    (a - b).norm() <= 1e-5
}

#[test]
fn render_frame_reprojects_scene_camera_to_request_aspect_ratio() {
    let mut registry = BackendRegistry::new();
    let ray_dir_probe = Arc::new(Mutex::new(None));
    register_camera_probe_backend(&mut registry, Arc::clone(&ray_dir_probe));
    let provider = MockSceneProvider::new();

    let request = FrameRequest {
        width: 64,
        height: 64,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 1,
        max_bounces: 1,
        path_tracing: Default::default(),
    };

    let mut sink = NoopFrameSink;
    let _ = render_frame(
        &registry,
        &provider,
        &BackendId::new("mock.camera-probe"),
        &request,
        &mut sink,
    )
    .unwrap();

    let probed_ray = ray_dir_probe
        .lock()
        .unwrap()
        .as_ref()
        .copied()
        .expect("expected probe backend to capture a ray");

    let base_camera = Camera::new(
        Point3::new(0.0, 0.0, 3.0),
        Point3::new(0.0, 0.0, 0.0),
        Vector3::y_axis().into_inner(),
        60.0,
        16.0 / 9.0,
        0.0,
        1.0,
    );
    let expected_square = base_camera
        .with_aspect_ratio(request.width as f32 / request.height as f32)
        .generate_ray(Vector2::new(1.0, 0.5))
        .dir;
    let wide_ray = base_camera.generate_ray(Vector2::new(1.0, 0.5)).dir;

    assert!(
        approx_vec3(probed_ray, expected_square),
        "expected orchestrator to reproject camera to request aspect ratio"
    );
    assert!(
        !approx_vec3(probed_ray, wide_ray),
        "expected probed ray to differ from original 16:9 framing"
    );
}

struct SpectralDetectSink {
    saw_spectral_scanline: bool,
}

impl SpectralDetectSink {
    fn new() -> Self {
        Self {
            saw_spectral_scanline: false,
        }
    }
}

impl FrameSink for SpectralDetectSink {
    fn on_event(&mut self, event: FrameEvent) {
        if let FrameEvent::Scanline { spectral, .. } = event {
            self.saw_spectral_scanline |= spectral.is_some();
        }
    }
}

#[test]
fn spectral_backend_scanline_emits_optional_spectral_payload() {
    let mut registry = BackendRegistry::new();
    register_mock_spectral_backend(&mut registry);
    let provider = MockSceneProvider::new();

    let request = FrameRequest {
        width: 3,
        height: 2,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 1,
        max_bounces: 1,
        path_tracing: Default::default(),
    };

    let mut sink = SpectralDetectSink::new();
    let _ = render_frame(
        &registry,
        &provider,
        &BackendId::new("mock.spectral"),
        &request,
        &mut sink,
    )
    .unwrap();

    assert!(sink.saw_spectral_scanline);
}

struct EventSignalSink {
    tx: mpsc::Sender<&'static str>,
}

impl FrameSink for EventSignalSink {
    fn on_event(&mut self, event: FrameEvent) {
        let label = match event {
            FrameEvent::Begin { .. } => "begin",
            FrameEvent::Scanline { .. } => "scanline",
            FrameEvent::End { .. } => "end",
        };
        let _ = self.tx.send(label);
    }
}

struct BlockingBackend {
    ready_tx: mpsc::Sender<()>,
    allow_finish_rx: Arc<Mutex<mpsc::Receiver<()>>>,
}

impl RenderBackend for BlockingBackend {
    type Pixel = Color;

    fn metadata(&self) -> BackendMetadata {
        BackendMetadata {
            id: BackendId::new("mock.blocking"),
            display_name: "Mock Blocking Backend".to_string(),
            capabilities: BackendCapabilities {
                progressive_updates: true,
                gpu_ready_interface: false,
            },
        }
    }

    fn render_scanlines(
        &self,
        _scene: &SceneSnapshot,
        request: &FrameRequest,
        emit_scanline: &mut dyn FnMut(TypedScanline<Self::Pixel>),
    ) -> RenderStats {
        let first = TypedScanline {
            y: 0,
            pixels: vec![Color::new(0.1, 0.2, 0.3); request.width as usize],
        };
        emit_scanline(first);

        let _ = self.ready_tx.send(());
        let _ = self
            .allow_finish_rx
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(1));

        let second = TypedScanline {
            y: 1,
            pixels: vec![Color::new(0.3, 0.2, 0.1); request.width as usize],
        };
        emit_scanline(second);

        RenderStats {
            backend_id: BackendId::new("mock.blocking"),
            frame_index: request.frame_index,
            elapsed: Duration::from_millis(1),
            samples_per_pixel: request.samples_per_pixel,
        }
    }
}

#[test]
fn scanline_event_is_emitted_before_backend_completion() {
    let (ready_tx, ready_rx) = mpsc::channel();
    let (allow_finish_tx, allow_finish_rx) = mpsc::channel();
    let allow_finish_rx = Arc::new(Mutex::new(allow_finish_rx));

    let mut registry = BackendRegistry::new();
    {
        let ready_tx = ready_tx.clone();
        let allow_finish_rx = Arc::clone(&allow_finish_rx);
        registry.register_factory(
            BackendId::new("mock.blocking"),
            Arc::new(move || {
                Box::new(BlockingBackend {
                    ready_tx: ready_tx.clone(),
                    allow_finish_rx: Arc::clone(&allow_finish_rx),
                })
            }),
        );
    }

    let provider = MockSceneProvider::new();
    let request = FrameRequest {
        width: 4,
        height: 2,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 1,
        max_bounces: 1,
        path_tracing: Default::default(),
    };

    let (event_tx, event_rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        let mut sink = EventSignalSink { tx: event_tx };
        let _ = render_frame(
            &registry,
            &provider,
            &BackendId::new("mock.blocking"),
            &request,
            &mut sink,
        );
    });

    ready_rx
        .recv_timeout(Duration::from_millis(250))
        .expect("backend should report first-row work");

    let deadline = Instant::now() + Duration::from_millis(250);
    let mut saw_scanline = false;
    while Instant::now() < deadline {
        match event_rx.recv_timeout(Duration::from_millis(20)) {
            Ok("scanline") => {
                saw_scanline = true;
                break;
            }
            Ok(_) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let _ = allow_finish_tx.send(());
    let _ = handle.join();

    assert!(
        saw_scanline,
        "expected at least one scanline event before backend completion"
    );
}
