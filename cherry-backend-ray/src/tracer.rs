use cherry_core::{
    Bsdf, BsdfEvalQuery, BsdfSampleInput, BsdfSampleQuery, Color, FrameRequest, Ray, SceneSnapshot,
    WAVELENGTH_BIN_COUNT, WAVELENGTH_BIN_STEP_NM, cie_xyz_from_wavelength, wavelength_for_bin,
    xyz_to_linear_srgb,
};
use nalgebra::{Point3, Vector3};

use crate::accel::Accel;

const EPSILON: f32 = 1e-6;

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
            Some(hit) => hit
                .normal
                .abs()
                .component_mul(&hit.material.preview_base_color()),
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

    fn sample_input(seed: u64, depth: u32) -> BsdfSampleInput {
        BsdfSampleInput {
            lobe: Self::hash01(seed ^ ((depth as u64 + 1) * 0x1a2b_3c4d)),
            u1: Self::hash01(seed ^ ((depth as u64 + 1) * 0x5566_7788)),
            u2: Self::hash01(seed ^ ((depth as u64 + 1) * 0x99aa_bbcc)),
        }
    }

    fn russian_roulette_scale(
        request: &FrameRequest,
        depth: u32,
        seed: u64,
        continuation: f32,
    ) -> Option<f32> {
        let next_depth = depth + 1;
        if next_depth < request.path_tracing.rr_start_depth {
            return Some(1.0);
        }

        let min_survival = request.path_tracing.rr_min_survival.clamp(0.0, 1.0);
        let survival = continuation.clamp(0.0, 1.0).max(min_survival);
        if survival <= EPSILON {
            return None;
        }

        if survival >= 1.0 {
            return Some(1.0);
        }

        let rr_sample = Self::hash01(seed ^ ((next_depth as u64 + 1) * 0xd1b5_4a32));
        if rr_sample > survival {
            None
        } else {
            Some(1.0 / survival)
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

        let normal = hit.normal.normalize();
        let outgoing = (-ray.dir).normalize();
        let origin = offset_origin(hit.point, normal, outgoing);

        let bsdf = hit.material;
        let emission = bsdf.emissive_rgb();
        let direct = if request.path_tracing.direct_lighting {
            estimate_direct_lighting(scene, accel, origin, normal, outgoing, bsdf.as_ref())
        } else {
            Color::new(0.0, 0.0, 0.0)
        };

        if depth + 1 >= request.max_bounces.max(1) {
            return clamp_color_non_negative(emission + direct);
        }

        let sample_query = BsdfSampleQuery { normal, outgoing };
        let sample_input = Self::sample_input(seed, depth);
        let Some(sampled) = bsdf.sample(&sample_query, sample_input) else {
            return clamp_color_non_negative(emission + direct);
        };

        if sampled.pdf <= EPSILON {
            return clamp_color_non_negative(emission + direct);
        }

        let cosine = normal.dot(&sampled.incoming).abs();
        if cosine <= EPSILON {
            return clamp_color_non_negative(emission + direct);
        }

        let bounce_origin = offset_origin(hit.point, normal, sampled.incoming);
        let bounce_ray = Ray::new(bounce_origin, sampled.incoming);

        let bounce_factor = sampled.value * (cosine / sampled.pdf);
        let continuation = bounce_factor
            .x
            .max(bounce_factor.y)
            .max(bounce_factor.z)
            .max(0.0);
        let Some(rr_scale) = Self::russian_roulette_scale(request, depth, seed, continuation)
        else {
            return clamp_color_non_negative(emission + direct);
        };

        let indirect = self.trace(
            scene,
            accel,
            &bounce_ray,
            request,
            depth + 1,
            seed ^ 0x9e37_79b9,
        );

        let mut throughput = bounce_factor.component_mul(&indirect) * rr_scale;
        if request.path_tracing.indirect_clamp_enabled() {
            throughput = clamp_color_max(throughput, request.path_tracing.indirect_clamp);
        }

        clamp_color_non_negative(emission + direct + throughput)
    }
}

fn estimate_direct_lighting(
    scene: &SceneSnapshot,
    accel: &dyn Accel,
    origin: Point3<f32>,
    normal: Vector3<f32>,
    outgoing: Vector3<f32>,
    bsdf: &dyn Bsdf,
) -> Color {
    let mut direct = Color::new(0.0, 0.0, 0.0);

    for light in &scene.lights {
        let Some((direction_to_light, distance, irradiance_rgb)) =
            sample_light_rgb(light.as_ref(), origin)
        else {
            continue;
        };

        let cosine = normal.dot(&direction_to_light).max(0.0);
        if cosine <= 0.0 {
            continue;
        }

        if is_shadowed(accel, scene, origin, direction_to_light, distance) {
            continue;
        }

        let eval = bsdf.eval(&BsdfEvalQuery {
            normal,
            outgoing,
            incoming: direction_to_light,
        });

        direct += eval.component_mul(&irradiance_rgb) * cosine;
    }

    direct
}

fn sample_light_rgb(
    light: &dyn cherry_core::Light,
    point: Point3<f32>,
) -> Option<(Vector3<f32>, f32, Color)> {
    let spectral_light = light.as_spectral()?;

    let geometry_sample = spectral_light.sample_irradiance_at(point, 550.0)?;
    let mut xyz = Vector3::new(0.0, 0.0, 0.0);

    for index in 0..WAVELENGTH_BIN_COUNT {
        let wavelength = wavelength_for_bin(index);
        let Some(sample) = spectral_light.sample_irradiance_at(point, wavelength) else {
            continue;
        };
        xyz += cie_xyz_from_wavelength(wavelength) * sample.irradiance_at_nm;
    }

    let rgb = xyz_to_linear_srgb(xyz * WAVELENGTH_BIN_STEP_NM);
    Some((
        geometry_sample.direction_to_light,
        geometry_sample.distance,
        clamp_color_non_negative(rgb),
    ))
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

fn offset_origin(point: Point3<f32>, normal: Vector3<f32>, direction: Vector3<f32>) -> Point3<f32> {
    let sign = if normal.dot(&direction) >= 0.0 {
        1.0
    } else {
        -1.0
    };
    point + normal * (sign * 1e-3)
}

fn clamp_color_non_negative(color: Color) -> Color {
    color.map(|channel| {
        if channel.is_finite() {
            channel.max(0.0)
        } else {
            0.0
        }
    })
}

fn clamp_color_max(color: Color, max_value: f32) -> Color {
    color.map(|channel| {
        if channel.is_finite() {
            channel.clamp(0.0, max_value)
        } else {
            0.0
        }
    })
}
