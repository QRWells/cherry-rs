use cherry_core::{
    Color, Light, Ray, SceneSnapshot, WAVELENGTH_BIN_COUNT, WAVELENGTH_BIN_STEP_NM,
    cie_xyz_from_wavelength, wavelength_for_bin, xyz_to_linear_srgb,
};
use nalgebra::{Point3, Vector3};

const EPSILON: f32 = 1e-6;
const SURFACE_OFFSET: f32 = 1e-3;

pub struct LightSampleRgb {
    pub direction_to_light: Vector3<f32>,
    pub distance: f32,
    pub irradiance: Color,
}

pub fn sample_light_rgb(light: &dyn Light, point: Point3<f32>) -> Option<LightSampleRgb> {
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

    Some(LightSampleRgb {
        direction_to_light: geometry_sample.direction_to_light,
        distance: geometry_sample.distance,
        irradiance: clamp_non_negative(xyz_to_linear_srgb(xyz * WAVELENGTH_BIN_STEP_NM)),
    })
}

pub fn offset_origin(
    point: Point3<f32>,
    normal: Vector3<f32>,
    direction: Vector3<f32>,
) -> Point3<f32> {
    let sign = if normal.dot(&direction) >= 0.0 {
        1.0
    } else {
        -1.0
    };
    point + normal * (sign * SURFACE_OFFSET)
}

pub fn is_shadowed(
    scene: &SceneSnapshot,
    origin: Point3<f32>,
    direction_to_light: Vector3<f32>,
    max_distance: f32,
) -> bool {
    let shadow_ray = Ray::new(origin, direction_to_light);
    let Some(hit) = scene.intersect(&shadow_ray) else {
        return false;
    };

    if max_distance.is_finite() {
        hit.distance < max_distance - SURFACE_OFFSET
    } else {
        true
    }
}

pub fn face_forward(normal: Vector3<f32>, reference: Vector3<f32>) -> Vector3<f32> {
    if normal.dot(&reference) < 0.0 {
        -normal
    } else {
        normal
    }
}

pub fn reflect(vector: Vector3<f32>, normal: Vector3<f32>) -> Vector3<f32> {
    vector - 2.0 * vector.dot(&normal) * normal
}

pub fn refract(
    incident: Vector3<f32>,
    normal: Vector3<f32>,
    eta_i: f32,
    eta_t: f32,
) -> Option<Vector3<f32>> {
    let mut n = normal;
    let mut eta_ratio = eta_i / eta_t;
    let mut cos_i = incident.dot(&n).clamp(-1.0, 1.0);

    if cos_i > 0.0 {
        n = -n;
        eta_ratio = eta_t / eta_i;
        cos_i = incident.dot(&n).clamp(-1.0, 1.0);
    }

    let sin_t2 = eta_ratio * eta_ratio * (1.0 - cos_i * cos_i);
    if sin_t2 > 1.0 {
        return None;
    }

    let cos_t = (1.0 - sin_t2).sqrt();
    Some((eta_ratio * incident - (eta_ratio * cos_i + cos_t) * n).normalize())
}

pub fn fresnel_dielectric(cos_theta_i: f32, ior: f32) -> f32 {
    let mut cos_i = cos_theta_i.clamp(-1.0, 1.0);
    let mut eta_i = 1.0;
    let mut eta_t = ior.max(1.0);

    if cos_i <= 0.0 {
        cos_i = cos_i.abs();
        std::mem::swap(&mut eta_i, &mut eta_t);
    }

    let sin_t = eta_i / eta_t * (1.0 - cos_i * cos_i).max(0.0).sqrt();
    if sin_t >= 1.0 {
        return 1.0;
    }

    let cos_t = (1.0 - sin_t * sin_t).max(0.0).sqrt();
    let rs = ((eta_t * cos_i) - (eta_i * cos_t)) / ((eta_t * cos_i) + (eta_i * cos_t) + EPSILON);
    let rp = ((eta_i * cos_i) - (eta_t * cos_t)) / ((eta_i * cos_i) + (eta_t * cos_t) + EPSILON);
    0.5 * (rs * rs + rp * rp)
}

pub fn clamp_non_negative(color: Color) -> Color {
    color.map(|channel| {
        if channel.is_finite() {
            channel.max(0.0)
        } else {
            0.0
        }
    })
}
