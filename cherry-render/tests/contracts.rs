use std::sync::{Arc, Mutex};
use std::time::Duration;

use cherry_core::{Camera, Color, FrameRequest, SceneProvider, SceneSnapshot};
use cherry_render::{
    BackendCapabilities, BackendId, BackendMetadata, BackendRegistry, FrameEvent, FrameSink,
    NoopFrameSink, PixelRadiance, RenderBackend, RenderStats, SPECTRAL_BIN_COUNT, SequenceSpec,
    TypedRenderResult, TypedScanline, render_frame, render_sequence,
};
use nalgebra::{Point3, Vector3};

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

    fn render_frame_typed(
        &self,
        _scene: &SceneSnapshot,
        request: &FrameRequest,
    ) -> TypedRenderResult<Self::Pixel> {
        let mut scanlines = Vec::with_capacity(request.height as usize);
        for y in 0..request.height {
            scanlines.push(TypedScanline {
                y,
                pixels: vec![Color::new(0.25, 0.5, 0.75); request.width as usize],
            });
        }

        let stats = RenderStats {
            backend_id: BackendId::new("mock"),
            frame_index: request.frame_index,
            elapsed: Duration::from_millis(1),
            samples_per_pixel: request.samples_per_pixel,
        };

        TypedRenderResult { scanlines, stats }
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

    fn render_frame_typed(
        &self,
        _scene: &SceneSnapshot,
        request: &FrameRequest,
    ) -> TypedRenderResult<Self::Pixel> {
        let mut scanlines = Vec::with_capacity(request.height as usize);
        for y in 0..request.height {
            let pixel = MockSpectralPixel {
                color: Color::new(0.2, 0.3, 0.4),
                bins: [0.1; SPECTRAL_BIN_COUNT],
            };
            scanlines.push(TypedScanline {
                y,
                pixels: vec![pixel; request.width as usize],
            });
        }

        let stats = RenderStats {
            backend_id: BackendId::new("mock.spectral"),
            frame_index: request.frame_index,
            elapsed: Duration::from_millis(1),
            samples_per_pixel: request.samples_per_pixel,
        };

        TypedRenderResult { scanlines, stats }
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
