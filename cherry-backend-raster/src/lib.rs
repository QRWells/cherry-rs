use std::{sync::Arc, time::Instant};

use cherry_core::{Color, FrameRequest, SceneSnapshot};
use cherry_render::{
    BackendCapabilities, BackendId, BackendMetadata, BackendRegistry, RenderBackend, RenderStats,
    TypedScanline,
};
use nalgebra::Vector2;

pub const RASTER_BACKEND_ID: &str = "raster.simple";

pub struct RasterBackend;

impl RasterBackend {
    pub fn new() -> Self {
        Self
    }

    fn shade_pixel(scene: &SceneSnapshot, uv: Vector2<f32>) -> Color {
        let ray = scene.camera.generate_ray(uv);
        match scene.intersect(&ray) {
            Some(hit) => hit.normal.abs().component_mul(&hit.material.albedo()),
            None => scene.background,
        }
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

        for y in 0..request.height {
            let mut pixels = Vec::with_capacity(request.width as usize);
            for x in 0..request.width {
                let uv = Vector2::new(
                    (x as f32 + 0.5) / request.width as f32,
                    (y as f32 + 0.5) / request.height as f32,
                );
                let color = Self::shade_pixel(scene, uv);
                pixels.push(color);
            }
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
    registry.register_factory(
        BackendId::new(RASTER_BACKEND_ID),
        Arc::new(|| Box::new(RasterBackend::new())),
    );
}
