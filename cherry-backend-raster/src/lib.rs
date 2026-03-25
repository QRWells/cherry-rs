use std::{sync::Arc, time::Instant};

use cherry_core::{Color, FrameRequest, SceneSnapshot};
use cherry_render::{
    BackendCapabilities, BackendId, BackendMetadata, BackendRegistry, RenderBackend, RenderStats,
    TypedScanline,
};
use nalgebra::{Vector2, Vector3};
use rayon::prelude::*;

pub const RASTER_BACKEND_ID: &str = "raster.simple";
const PREVIEW_AMBIENT: f32 = 0.2;

pub struct RasterBackend {
    cpu_threads: Option<usize>,
}

impl RasterBackend {
    pub fn new() -> Self {
        Self::with_threads(None)
    }

    pub fn with_threads(cpu_threads: Option<usize>) -> Self {
        Self { cpu_threads }
    }

    fn shade_pixel(scene: &SceneSnapshot, uv: Vector2<f32>) -> Color {
        let ray = scene.camera.generate_ray(uv);
        match scene.intersect(&ray) {
            Some(hit) => preview_diffuse(hit.normal, hit.material.preview_base_color()),
            None => scene.background,
        }
    }
}

fn preview_diffuse(normal: Vector3<f32>, albedo: Color) -> Color {
    let key_light_dir = Vector3::new(-0.4, 0.8, 0.45).normalize();
    let diffuse = normal.dot(&key_light_dir).max(0.0);
    let shade = PREVIEW_AMBIENT + (1.0 - PREVIEW_AMBIENT) * diffuse;
    albedo * shade
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
        let configured_pool = self.cpu_threads.and_then(|threads| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .ok()
        });

        for y in 0..request.height {
            let compute_scanline = || {
                (0..request.width)
                    .into_par_iter()
                    .map(|x| {
                        let uv = Vector2::new(
                            (x as f32 + 0.5) / request.width as f32,
                            (y as f32 + 0.5) / request.height as f32,
                        );
                        Self::shade_pixel(scene, uv)
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
    register_backends_with_threads(registry, None);
}

pub fn register_backends_with_threads(registry: &mut BackendRegistry, cpu_threads: Option<usize>) {
    registry.register_factory(
        BackendId::new(RASTER_BACKEND_ID),
        Arc::new(move || Box::new(RasterBackend::with_threads(cpu_threads))),
    );
}
