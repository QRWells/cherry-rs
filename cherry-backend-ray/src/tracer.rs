use cherry_core::{Color, FrameRequest, Ray, SceneSnapshot};
use nalgebra::Vector3;

use crate::accel::Accel;

pub trait Tracer: Send + Sync {
    fn trace(
        &self,
        scene: &SceneSnapshot,
        accel: &dyn Accel,
        ray: &Ray,
        request: &FrameRequest,
        depth: u32,
        seed: u64,
    ) -> Color;
}

pub struct NormalTracer;

impl Tracer for NormalTracer {
    fn trace(
        &self,
        scene: &SceneSnapshot,
        accel: &dyn Accel,
        ray: &Ray,
        _request: &FrameRequest,
        _depth: u32,
        _seed: u64,
    ) -> Color {
        match accel.intersect(ray, scene) {
            Some(hit) => hit.normal.abs().component_mul(&hit.material.albedo()),
            None => scene.background,
        }
    }
}

pub struct MonteCarloTracer;

impl MonteCarloTracer {
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

    fn sample_hemisphere(normal: Vector3<f32>, seed: u64) -> Vector3<f32> {
        let z = Self::hash01(seed ^ 0x1122).max(1e-4);
        let phi = 2.0 * std::f32::consts::PI * Self::hash01(seed ^ 0x3344);
        let r = (1.0 - z * z).sqrt();
        let local = Vector3::new(r * phi.cos(), r * phi.sin(), z).normalize();
        if local.dot(&normal) < 0.0 {
            -local
        } else {
            local
        }
    }
}

impl Tracer for MonteCarloTracer {
    fn trace(
        &self,
        scene: &SceneSnapshot,
        accel: &dyn Accel,
        ray: &Ray,
        request: &FrameRequest,
        depth: u32,
        seed: u64,
    ) -> Color {
        if depth >= request.max_bounces.max(1) {
            return Color::new(0.0, 0.0, 0.0);
        }

        let hit = match accel.intersect(ray, scene) {
            Some(hit) => hit,
            None => return scene.background,
        };

        let albedo = hit.material.albedo();
        let normal_tint = hit.normal.abs().component_mul(&albedo) * 0.7;

        if depth + 1 >= request.max_bounces.max(1) {
            return normal_tint;
        }

        let bounce_dir = Self::sample_hemisphere(hit.normal, seed ^ depth as u64);
        let bounce_ray = Ray::new(hit.point + hit.normal * 1e-3, bounce_dir);
        let indirect = self.trace(
            scene,
            accel,
            &bounce_ray,
            request,
            depth + 1,
            seed ^ 0x9e37_79b9,
        );

        normal_tint + indirect.component_mul(&albedo) * 0.3
    }
}
