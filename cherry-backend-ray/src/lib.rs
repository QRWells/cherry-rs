mod accel;
mod tracer;

use std::{sync::Arc, time::Instant};

use accel::{Accel, BruteForceAccel};
use cherry_core::{Color, FrameRequest, SceneSnapshot};
use cherry_render::{
    color_to_rgb8, BackendCapabilities, BackendId, BackendMetadata, BackendRegistry, FrameEvent,
    FrameSink, RenderBackend, RenderResult, RenderStats,
};
use nalgebra::Vector2;
use tracer::{MonteCarloTracer, NormalTracer, Tracer};

pub use accel::{Accel as RayAccel, BruteForceAccel as RayBruteForceAccel};

pub const RAY_NORMAL_BACKEND_ID: &str = "ray.normal";
pub const RAY_MONTE_CARLO_BACKEND_ID: &str = "ray.montecarlo";

#[derive(Debug, Clone, Copy)]
pub enum TraceMethod {
    Normal,
    MonteCarlo,
}

pub struct RayBackend {
    metadata: BackendMetadata,
    tracer: Arc<dyn Tracer>,
    accel: Arc<dyn Accel>,
}

impl RayBackend {
    pub fn with_method(method: TraceMethod) -> Self {
        match method {
            TraceMethod::Normal => Self::normal(),
            TraceMethod::MonteCarlo => Self::monte_carlo(),
        }
    }

    pub fn normal() -> Self {
        Self {
            metadata: BackendMetadata {
                id: BackendId::new(RAY_NORMAL_BACKEND_ID),
                display_name: "CPU Ray Backend (Normal)".to_string(),
                capabilities: BackendCapabilities {
                    progressive_updates: true,
                    gpu_ready_interface: true,
                },
            },
            tracer: Arc::new(NormalTracer),
            accel: Arc::new(BruteForceAccel),
        }
    }

    pub fn monte_carlo() -> Self {
        Self {
            metadata: BackendMetadata {
                id: BackendId::new(RAY_MONTE_CARLO_BACKEND_ID),
                display_name: "CPU Ray Backend (Monte Carlo)".to_string(),
                capabilities: BackendCapabilities {
                    progressive_updates: true,
                    gpu_ready_interface: true,
                },
            },
            tracer: Arc::new(MonteCarloTracer),
            accel: Arc::new(BruteForceAccel),
        }
    }

    fn hash_u64(mut state: u64) -> u64 {
        state ^= state >> 30;
        state = state.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        state ^= state >> 27;
        state = state.wrapping_mul(0x94d0_49bb_1331_11eb);
        state ^ (state >> 31)
    }

    fn hash01(seed: u64) -> f32 {
        let value = Self::hash_u64(seed);
        (value as f64 / u64::MAX as f64) as f32
    }

    fn sample_lens(seed: u64) -> Vector2<f32> {
        let theta = 2.0 * std::f32::consts::PI * Self::hash01(seed ^ 0x1234);
        let radius = Self::hash01(seed ^ 0x4321).sqrt();
        Vector2::new(radius * theta.cos(), radius * theta.sin())
    }

    fn seed(frame_index: u32, x: u32, y: u32, sample: u32) -> u64 {
        ((frame_index as u64) << 40)
            ^ ((x as u64) << 24)
            ^ ((y as u64) << 8)
            ^ (sample as u64)
            ^ 0xa5a5_5a5a
    }

    fn trace_pixel(&self, scene: &SceneSnapshot, request: &FrameRequest, x: u32, y: u32) -> Color {
        let mut sum = Color::new(0.0, 0.0, 0.0);
        for sample in 0..request.samples_per_pixel.max(1) {
            let seed = Self::seed(request.frame_index, x, y, sample);
            let jitter_x = Self::hash01(seed ^ 0x55) - 0.5;
            let jitter_y = Self::hash01(seed ^ 0x77) - 0.5;
            let uv = Vector2::new(
                (x as f32 + 0.5 + jitter_x) / request.width as f32,
                (y as f32 + 0.5 + jitter_y) / request.height as f32,
            );
            let ray = scene
                .camera
                .generate_ray_with_lens_sample(uv, Self::sample_lens(seed));
            sum += self
                .tracer
                .trace(scene, self.accel.as_ref(), &ray, request, 0, seed);
        }

        sum / request.samples_per_pixel.max(1) as f32
    }
}

impl RenderBackend for RayBackend {
    fn metadata(&self) -> BackendMetadata {
        self.metadata.clone()
    }

    fn render_frame(
        &self,
        scene: &SceneSnapshot,
        request: &FrameRequest,
        sink: &mut dyn FrameSink,
    ) -> RenderResult {
        let metadata = self.metadata();
        sink.on_event(FrameEvent::Begin {
            backend: metadata.clone(),
            request: request.clone(),
        });

        let start = Instant::now();
        let mut image = image::RgbImage::new(request.width, request.height);

        for y in 0..request.height {
            let mut scanline = Vec::with_capacity(request.width as usize);
            for x in 0..request.width {
                let color = self.trace_pixel(scene, request, x, y);
                scanline.push(color);
                image.put_pixel(x, y, color_to_rgb8(color));
            }
            sink.on_event(FrameEvent::Scanline {
                y,
                pixels: scanline,
            });
        }

        let stats = RenderStats {
            backend_id: metadata.id,
            frame_index: request.frame_index,
            elapsed: start.elapsed(),
            samples_per_pixel: request.samples_per_pixel,
        };
        sink.on_event(FrameEvent::End {
            stats: stats.clone(),
        });

        RenderResult { image, stats }
    }
}

pub fn register_backends(registry: &mut BackendRegistry) {
    registry.register_factory(
        BackendId::new(RAY_NORMAL_BACKEND_ID),
        Arc::new(|| Box::new(RayBackend::normal())),
    );
    registry.register_factory(
        BackendId::new(RAY_MONTE_CARLO_BACKEND_ID),
        Arc::new(|| Box::new(RayBackend::monte_carlo())),
    );
}
