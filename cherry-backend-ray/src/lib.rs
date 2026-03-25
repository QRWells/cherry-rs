mod accel;
mod tracer;

use std::{f32::consts::PI, sync::Arc, time::Instant};

use accel::{Accel, BruteForceAccel};
use cherry_core::{
    Color, FrameRequest, Ray, SceneSnapshot, WAVELENGTH_MAX_NM, WAVELENGTH_MIN_NM,
    apply_exposure_reinhard, cie_xyz_from_wavelength, rgb_to_reflectance_at_nm, xyz_to_linear_srgb,
};
use cherry_render::{
    BackendCapabilities, BackendId, BackendMetadata, BackendRegistry, PixelRadiance, RenderBackend,
    RenderStats, SPECTRAL_BIN_COUNT, TypedScanline,
};
use nalgebra::{Point3, Vector2, Vector3};
use tracer::{MonteCarloTracer, NormalTracer, Tracer};

pub use accel::{Accel as RayAccel, BruteForceAccel as RayBruteForceAccel};

pub const RAY_NORMAL_BACKEND_ID: &str = "ray.normal";
pub const RAY_MONTE_CARLO_BACKEND_ID: &str = "ray.montecarlo";
pub const RAY_SPECTRAL_BACKEND_ID: &str = "ray.spectral";

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

#[derive(Clone, Debug, PartialEq)]
pub struct SpectralPixel {
    pub color: Color,
    pub bins: [f32; SPECTRAL_BIN_COUNT],
}

impl PixelRadiance for SpectralPixel {
    fn to_rgb_color(&self) -> Color {
        self.color
    }

    fn spectral_bins(&self) -> Option<[f32; SPECTRAL_BIN_COUNT]> {
        Some(self.bins)
    }
}

pub struct SpectralRayBackend {
    metadata: BackendMetadata,
    accel: Arc<dyn Accel>,
    exposure: f32,
}

impl SpectralRayBackend {
    pub fn new() -> Self {
        Self::with_exposure(1.0)
    }

    pub fn with_exposure(exposure: f32) -> Self {
        Self {
            metadata: BackendMetadata {
                id: BackendId::new(RAY_SPECTRAL_BACKEND_ID),
                display_name: "CPU Ray Backend (Spectral)".to_string(),
                capabilities: BackendCapabilities {
                    progressive_updates: true,
                    gpu_ready_interface: true,
                },
            },
            accel: Arc::new(BruteForceAccel),
            exposure,
        }
    }

    pub fn exposure(&self) -> f32 {
        self.exposure
    }

    fn trace_pixel(
        &self,
        scene: &SceneSnapshot,
        request: &FrameRequest,
        x: u32,
        y: u32,
    ) -> SpectralPixel {
        let spp = request.samples_per_pixel.max(1);
        let mut xyz_sum = Vector3::new(0.0, 0.0, 0.0);
        let mut bins = [0.0; SPECTRAL_BIN_COUNT];

        for sample in 0..spp {
            let seed = seed(request.frame_index, x, y, sample);
            let jitter_x = hash01(seed ^ 0x55) - 0.5;
            let jitter_y = hash01(seed ^ 0x77) - 0.5;
            let uv = Vector2::new(
                (x as f32 + 0.5 + jitter_x) / request.width as f32,
                (y as f32 + 0.5 + jitter_y) / request.height as f32,
            );
            let ray = scene
                .camera
                .generate_ray_with_lens_sample(uv, sample_lens(seed));

            let wavelength = sample_wavelength(seed ^ 0x8899);
            let spectral_radiance = trace_spectral_wavelength(
                scene,
                self.accel.as_ref(),
                &ray,
                request,
                0,
                seed,
                wavelength,
            );

            let wavelength_measure = WAVELENGTH_MAX_NM - WAVELENGTH_MIN_NM;
            xyz_sum +=
                cie_xyz_from_wavelength(wavelength) * (spectral_radiance * wavelength_measure);

            let bin_index = spectral_bin_for_wavelength(wavelength);
            bins[bin_index] += spectral_radiance;
        }

        for bin in &mut bins {
            *bin /= spp as f32;
        }

        let xyz = xyz_sum / spp as f32;
        let linear_rgb = xyz_to_linear_srgb(xyz);
        let display_rgb = apply_exposure_reinhard(linear_rgb, self.exposure);

        SpectralPixel {
            color: display_rgb,
            bins,
        }
    }
}

impl Default for SpectralRayBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderBackend for SpectralRayBackend {
    type Pixel = SpectralPixel;

    fn metadata(&self) -> BackendMetadata {
        self.metadata.clone()
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
                pixels.push(self.trace_pixel(scene, request, x, y));
            }
            emit_scanline(TypedScanline { y, pixels });
        }

        RenderStats {
            backend_id: self.metadata.id.clone(),
            frame_index: request.frame_index,
            elapsed: start.elapsed(),
            samples_per_pixel: request.samples_per_pixel,
        }
    }
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

    fn trace_pixel(&self, scene: &SceneSnapshot, request: &FrameRequest, x: u32, y: u32) -> Color {
        let mut sum = Color::new(0.0, 0.0, 0.0);
        for sample in 0..request.samples_per_pixel.max(1) {
            let seed = seed(request.frame_index, x, y, sample);
            let jitter_x = hash01(seed ^ 0x55) - 0.5;
            let jitter_y = hash01(seed ^ 0x77) - 0.5;
            let uv = Vector2::new(
                (x as f32 + 0.5 + jitter_x) / request.width as f32,
                (y as f32 + 0.5 + jitter_y) / request.height as f32,
            );
            let ray = scene
                .camera
                .generate_ray_with_lens_sample(uv, sample_lens(seed));
            sum += self
                .tracer
                .trace(scene, self.accel.as_ref(), &ray, request, 0, seed);
        }

        sum / request.samples_per_pixel.max(1) as f32
    }
}

impl RenderBackend for RayBackend {
    type Pixel = Color;

    fn metadata(&self) -> BackendMetadata {
        self.metadata.clone()
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
                let color = self.trace_pixel(scene, request, x, y);
                pixels.push(color);
            }
            emit_scanline(TypedScanline { y, pixels });
        }

        RenderStats {
            backend_id: self.metadata.id.clone(),
            frame_index: request.frame_index,
            elapsed: start.elapsed(),
            samples_per_pixel: request.samples_per_pixel,
        }
    }
}

pub fn register_backends(registry: &mut BackendRegistry) {
    register_backends_with_exposure(registry, 1.0);
}

pub fn register_backends_with_exposure(registry: &mut BackendRegistry, exposure: f32) {
    registry.register_factory(
        BackendId::new(RAY_NORMAL_BACKEND_ID),
        Arc::new(|| Box::new(RayBackend::normal())),
    );
    registry.register_factory(
        BackendId::new(RAY_MONTE_CARLO_BACKEND_ID),
        Arc::new(|| Box::new(RayBackend::monte_carlo())),
    );
    registry.register_factory(
        BackendId::new(RAY_SPECTRAL_BACKEND_ID),
        Arc::new(move || Box::new(SpectralRayBackend::with_exposure(exposure))),
    );
}

fn hash_u64(mut state: u64) -> u64 {
    state ^= state >> 30;
    state = state.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    state ^= state >> 27;
    state = state.wrapping_mul(0x94d0_49bb_1331_11eb);
    state ^ (state >> 31)
}

fn hash01(seed: u64) -> f32 {
    let value = hash_u64(seed);
    (value as f64 / u64::MAX as f64) as f32
}

fn sample_lens(seed: u64) -> Vector2<f32> {
    let theta = 2.0 * std::f32::consts::PI * hash01(seed ^ 0x1234);
    let radius = hash01(seed ^ 0x4321).sqrt();
    Vector2::new(radius * theta.cos(), radius * theta.sin())
}

fn seed(frame_index: u32, x: u32, y: u32, sample: u32) -> u64 {
    ((frame_index as u64) << 40)
        ^ ((x as u64) << 24)
        ^ ((y as u64) << 8)
        ^ (sample as u64)
        ^ 0xa5a5_5a5a
}

fn sample_wavelength(seed: u64) -> f32 {
    WAVELENGTH_MIN_NM + (WAVELENGTH_MAX_NM - WAVELENGTH_MIN_NM) * hash01(seed ^ 0x3141_5926)
}

fn spectral_bin_for_wavelength(wavelength_nm: f32) -> usize {
    let step = (WAVELENGTH_MAX_NM - WAVELENGTH_MIN_NM) / (SPECTRAL_BIN_COUNT as f32 - 1.0);
    let index = ((wavelength_nm - WAVELENGTH_MIN_NM) / step).round() as i32;
    index.clamp(0, SPECTRAL_BIN_COUNT as i32 - 1) as usize
}

fn trace_spectral_wavelength(
    scene: &SceneSnapshot,
    accel: &dyn Accel,
    ray: &Ray,
    request: &FrameRequest,
    depth: u32,
    seed: u64,
    wavelength_nm: f32,
) -> f32 {
    if depth >= request.max_bounces.max(1) {
        return 0.0;
    }

    let hit = match accel.intersect(ray, scene) {
        Some(hit) => hit,
        None => return scene.background_at_nm(wavelength_nm),
    };

    let hit_origin = hit.point + hit.normal * 1e-3;
    let reflectance = hit
        .material
        .as_spectral()
        .map(|material| material.reflectance_at_nm(wavelength_nm))
        .unwrap_or_else(|| rgb_to_reflectance_at_nm(hit.material.albedo(), wavelength_nm))
        .clamp(0.0, 1.0);

    let mut direct = 0.0;
    for light in &scene.lights {
        let Some(spectral_light) = light.as_spectral() else {
            continue;
        };

        let Some(sample) = spectral_light.sample_irradiance_at(hit_origin, wavelength_nm) else {
            continue;
        };

        let cosine = hit.normal.dot(&sample.direction_to_light).max(0.0);
        if cosine <= 0.0 {
            continue;
        }

        let shadowed = is_shadowed(
            accel,
            scene,
            hit_origin,
            sample.direction_to_light,
            sample.distance,
        );
        if shadowed {
            continue;
        }

        direct += sample.irradiance_at_nm * reflectance * cosine / PI;
    }

    if depth + 1 >= request.max_bounces.max(1) {
        return direct;
    }

    let bounce_direction = sample_cosine_weighted_hemisphere(hit.normal, seed ^ depth as u64);
    let bounce_ray = Ray::new(hit_origin, bounce_direction);
    let indirect = trace_spectral_wavelength(
        scene,
        accel,
        &bounce_ray,
        request,
        depth + 1,
        seed ^ 0x9e37_79b9,
        wavelength_nm,
    );

    direct + reflectance * indirect
}

fn sample_cosine_weighted_hemisphere(normal: Vector3<f32>, seed: u64) -> Vector3<f32> {
    let u1 = hash01(seed ^ 0x55aa).max(1e-6);
    let u2 = hash01(seed ^ 0xaa55);

    let r = u1.sqrt();
    let theta = 2.0 * PI * u2;
    let x = r * theta.cos();
    let y = r * theta.sin();
    let z = (1.0 - u1).sqrt();

    let (tangent, bitangent) = orthonormal_basis(normal);
    (tangent * x + bitangent * y + normal * z).normalize()
}

fn orthonormal_basis(normal: Vector3<f32>) -> (Vector3<f32>, Vector3<f32>) {
    let helper = if normal.z.abs() < 0.999 {
        Vector3::new(0.0, 0.0, 1.0)
    } else {
        Vector3::new(0.0, 1.0, 0.0)
    };
    let tangent = normal.cross(&helper).normalize();
    let bitangent = normal.cross(&tangent).normalize();
    (tangent, bitangent)
}

fn is_shadowed(
    accel: &dyn Accel,
    scene: &SceneSnapshot,
    origin: Point3<f32>,
    direction_to_light: Vector3<f32>,
    max_distance: f32,
) -> bool {
    let shadow_ray = Ray::new(origin, direction_to_light);
    let Some(hit) = accel.intersect(&shadow_ray, scene) else {
        return false;
    };

    if max_distance.is_finite() {
        hit.distance < max_distance - 1e-3
    } else {
        true
    }
}
