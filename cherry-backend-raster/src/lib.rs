mod config;
mod lighting;
mod pipeline;

use std::{sync::Arc, time::Instant};

use cherry_core::{Color, FrameRequest, SceneSnapshot, apply_exposure_reinhard};
use cherry_render::{
    BackendCapabilities, BackendId, BackendMetadata, BackendRegistry, RenderBackend, RenderStats,
    TypedScanline,
};
use nalgebra::Vector2;
use pipeline::RasterPipeline;
use rayon::prelude::*;

pub use config::RasterBackendConfig;

pub const RASTER_BACKEND_ID: &str = "raster.simple";

pub struct RasterBackend {
    config: RasterBackendConfig,
}

impl RasterBackend {
    pub fn new() -> Self {
        Self::with_config(RasterBackendConfig::default())
    }

    pub fn with_threads(cpu_threads: Option<usize>) -> Self {
        Self::with_config(RasterBackendConfig {
            cpu_threads,
            ..RasterBackendConfig::default()
        })
    }

    pub fn with_config(config: RasterBackendConfig) -> Self {
        Self { config }
    }
}

impl Default for RasterBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderBackend for RasterBackend {
    type Pixel = Color;

    fn metadata(&self) -> BackendMetadata {
        BackendMetadata {
            id: BackendId::new(RASTER_BACKEND_ID),
            display_name: "Software Raster Backend".to_string(),
            capabilities: BackendCapabilities {
                progressive_updates: true,
                gpu_ready_interface: true,
            },
        }
    }

    fn render_scanlines(
        &self,
        scene: &SceneSnapshot,
        request: &FrameRequest,
        emit_scanline: &mut dyn FnMut(TypedScanline<Self::Pixel>),
    ) -> RenderStats {
        let start = Instant::now();
        let configured_pool = self.config.cpu_threads.and_then(|threads| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .ok()
        });
        let pipeline = RasterPipeline::new(scene, request, self.config);

        for y in 0..request.height {
            let compute_scanline = || {
                (0..request.width)
                    .into_par_iter()
                    .map(|x| {
                        let uv = Vector2::new(
                            (x as f32 + 0.5) / request.width as f32,
                            (y as f32 + 0.5) / request.height as f32,
                        );
                        apply_exposure_reinhard(pipeline.shade_pixel(uv), self.config.exposure)
                    })
                    .collect::<Vec<_>>()
            };

            let pixels = match &configured_pool {
                Some(pool) => pool.install(compute_scanline),
                None => compute_scanline(),
            };
            emit_scanline(TypedScanline { y, pixels });
        }

        RenderStats {
            backend_id: BackendId::new(RASTER_BACKEND_ID),
            frame_index: request.frame_index,
            elapsed: start.elapsed(),
            samples_per_pixel: request.samples_per_pixel,
        }
    }
}

pub fn register_backends(registry: &mut BackendRegistry) {
    register_backends_with_config(registry, RasterBackendConfig::default());
}

pub fn register_backends_with_threads(registry: &mut BackendRegistry, cpu_threads: Option<usize>) {
    register_backends_with_config(
        registry,
        RasterBackendConfig {
            cpu_threads,
            ..RasterBackendConfig::default()
        },
    );
}

pub fn register_backends_with_config(registry: &mut BackendRegistry, config: RasterBackendConfig) {
    registry.register_factory(
        BackendId::new(RASTER_BACKEND_ID),
        Arc::new(move || Box::new(RasterBackend::with_config(config))),
    );
}
