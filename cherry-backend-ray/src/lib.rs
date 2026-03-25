mod accel;
mod tracer;

use std::{sync::Arc, time::Instant};

use accel::{Accel, BruteForceAccel};
use cherry_core::{
    BsdfEvalQuery, BsdfSampleInput, BsdfSampleQuery, Color, FrameRequest, Ray, SceneSnapshot,
    WAVELENGTH_MAX_NM, WAVELENGTH_MIN_NM, apply_exposure_reinhard, cie_xyz_from_wavelength,
    xyz_to_linear_srgb,
};
use cherry_render::{
    BackendCapabilities, BackendId, BackendMetadata, BackendRegistry, PixelRadiance, RenderBackend,
    RenderStats, SPECTRAL_BIN_COUNT, TypedScanline,
};
use nalgebra::{Point3, Vector2, Vector3};
use rayon::prelude::*;
use tracer::{MonteCarloTracer, NormalTracer, Tracer};

pub use accel::{Accel as RayAccel, BruteForceAccel as RayBruteForceAccel};

pub const RAY_NORMAL_BACKEND_ID: &str = "ray.normal";
pub const RAY_MONTE_CARLO_BACKEND_ID: &str = "ray.montecarlo";
pub const RAY_SPECTRAL_BACKEND_ID: &str = "ray.spectral";
const RAY_EPSILON: f32 = 1e-6;

#[derive(Debug, Clone, Copy)]
pub enum TraceMethod {
    Normal,
    MonteCarlo,
}

pub struct RayBackend {
    metadata: BackendMetadata,
    tracer: Arc<dyn Tracer>,
    accel: Arc<dyn Accel>,
    cpu_threads: Option<usize>,
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
    cpu_threads: Option<usize>,
}

impl SpectralRayBackend {
    pub fn new() -> Self {
        Self::with_exposure_and_threads(1.0, None)
    }

    pub fn with_exposure(exposure: f32) -> Self {
        Self::with_exposure_and_threads(exposure, None)
    }

    pub fn with_exposure_and_threads(exposure: f32, cpu_threads: Option<usize>) -> Self {
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
            cpu_threads,
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
                    .map(|x| self.trace_pixel(scene, request, x, y))
                    .collect::<Vec<_>>()
            };
            let pixels = match &configured_pool {
                Some(pool) => pool.install(compute_scanline),
                None => compute_scanline(),
            };
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
        Self::with_method_and_threads(method, None)
    }

    pub fn with_method_and_threads(method: TraceMethod, cpu_threads: Option<usize>) -> Self {
        match method {
            TraceMethod::Normal => Self::normal_with_threads(cpu_threads),
            TraceMethod::MonteCarlo => Self::monte_carlo_with_threads(cpu_threads),
        }
    }

    pub fn normal() -> Self {
        Self::normal_with_threads(None)
    }

    pub fn normal_with_threads(cpu_threads: Option<usize>) -> Self {
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
            cpu_threads,
        }
    }

    pub fn monte_carlo() -> Self {
        Self::monte_carlo_with_threads(None)
    }

    pub fn monte_carlo_with_threads(cpu_threads: Option<usize>) -> Self {
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
            cpu_threads,
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
                    .map(|x| self.trace_pixel(scene, request, x, y))
                    .collect::<Vec<_>>()
            };
            let pixels = match &configured_pool {
                Some(pool) => pool.install(compute_scanline),
                None => compute_scanline(),
            };
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
    register_backends_with_exposure_and_threads(registry, 1.0, None);
}

pub fn register_backends_with_exposure(registry: &mut BackendRegistry, exposure: f32) {
    register_backends_with_exposure_and_threads(registry, exposure, None);
}

pub fn register_backends_with_exposure_and_threads(
    registry: &mut BackendRegistry,
    exposure: f32,
    cpu_threads: Option<usize>,
) {
    registry.register_factory(
        BackendId::new(RAY_NORMAL_BACKEND_ID),
        Arc::new(move || Box::new(RayBackend::normal_with_threads(cpu_threads))),
    );
    registry.register_factory(
        BackendId::new(RAY_MONTE_CARLO_BACKEND_ID),
        Arc::new(move || Box::new(RayBackend::monte_carlo_with_threads(cpu_threads))),
    );
    registry.register_factory(
        BackendId::new(RAY_SPECTRAL_BACKEND_ID),
        Arc::new(move || {
            Box::new(SpectralRayBackend::with_exposure_and_threads(
                exposure,
                cpu_threads,
            ))
        }),
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

fn bsdf_sample_input(seed: u64, depth: u32) -> BsdfSampleInput {
    BsdfSampleInput {
        lobe: hash01(seed ^ ((depth as u64 + 1) * 0x1a2b_3c4d)),
        u1: hash01(seed ^ ((depth as u64 + 1) * 0x5566_7788)),
        u2: hash01(seed ^ ((depth as u64 + 1) * 0x99aa_bbcc)),
    }
}

fn spectral_bin_for_wavelength(wavelength_nm: f32) -> usize {
    let step = (WAVELENGTH_MAX_NM - WAVELENGTH_MIN_NM) / (SPECTRAL_BIN_COUNT as f32 - 1.0);
    let index = ((wavelength_nm - WAVELENGTH_MIN_NM) / step).round() as i32;
    index.clamp(0, SPECTRAL_BIN_COUNT as i32 - 1) as usize
}

fn offset_origin(point: Point3<f32>, normal: Vector3<f32>, direction: Vector3<f32>) -> Point3<f32> {
    let sign = if normal.dot(&direction) >= 0.0 {
        1.0
    } else {
        -1.0
    };
    point + normal * (sign * 1e-3)
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

    let normal = hit.normal.normalize();
    let outgoing = (-ray.dir).normalize();
    let hit_origin = offset_origin(hit.point, normal, outgoing);
    let emission = hit.material.emissive_at_nm(wavelength_nm);

    let mut direct = 0.0;
    for light in &scene.lights {
        let Some(spectral_light) = light.as_spectral() else {
            continue;
        };

        let Some(sample) = spectral_light.sample_irradiance_at(hit_origin, wavelength_nm) else {
            continue;
        };

        let incoming = sample.direction_to_light.normalize();
        let cosine = normal.dot(&incoming).max(0.0);
        if cosine <= 0.0 {
            continue;
        }

        let shadowed = is_shadowed(accel, scene, hit_origin, incoming, sample.distance);
        if shadowed {
            continue;
        }

        let eval = hit.material.eval_spectral(
            &BsdfEvalQuery {
                normal,
                outgoing,
                incoming,
            },
            wavelength_nm,
        );
        direct += sample.irradiance_at_nm * eval * cosine;
    }

    if depth + 1 >= request.max_bounces.max(1) {
        return (emission + direct).max(0.0);
    }

    let sample_query = BsdfSampleQuery { normal, outgoing };
    let sample_input = bsdf_sample_input(seed, depth);
    let Some(sampled) = hit
        .material
        .sample_spectral(&sample_query, sample_input, wavelength_nm)
    else {
        return (emission + direct).max(0.0);
    };

    if sampled.pdf <= RAY_EPSILON {
        return (emission + direct).max(0.0);
    }

    let cosine = normal.dot(&sampled.incoming).abs();
    if cosine <= RAY_EPSILON {
        return (emission + direct).max(0.0);
    }

    let bounce_origin = offset_origin(hit.point, normal, sampled.incoming);
    let bounce_ray = Ray::new(bounce_origin, sampled.incoming);
    let indirect = trace_spectral_wavelength(
        scene,
        accel,
        &bounce_ray,
        request,
        depth + 1,
        seed ^ 0x9e37_79b9,
        wavelength_nm,
    );

    (emission + direct + sampled.value * indirect * cosine / sampled.pdf).max(0.0)
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
