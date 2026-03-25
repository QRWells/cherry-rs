use std::f32::consts::PI;

use nalgebra::Vector3;

use crate::{
    Color,
    spectral::{
        SampledSpectrum, rgb_to_emission_at_nm, rgb_to_emission_spectrum,
        rgb_to_reflectance_spectrum,
    },
};

const EPSILON: f32 = 1e-6;
const MIN_ROUGHNESS: f32 = 0.045;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BsdfLobeKind {
    Diffuse,
    SpecularReflection,
    SpecularTransmission,
}

#[derive(Debug, Clone, Copy)]
pub struct BsdfEvalQuery {
    pub normal: Vector3<f32>,
    pub outgoing: Vector3<f32>,
    pub incoming: Vector3<f32>,
}

#[derive(Debug, Clone, Copy)]
pub struct BsdfSampleQuery {
    pub normal: Vector3<f32>,
    pub outgoing: Vector3<f32>,
}

#[derive(Debug, Clone, Copy)]
pub struct BsdfSampleInput {
    pub lobe: f32,
    pub u1: f32,
    pub u2: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct BsdfSampleRgb {
    pub incoming: Vector3<f32>,
    pub value: Color,
    pub pdf: f32,
    pub lobe: BsdfLobeKind,
}

#[derive(Debug, Clone, Copy)]
pub struct BsdfSampleSpectral {
    pub incoming: Vector3<f32>,
    pub value: f32,
    pub pdf: f32,
    pub lobe: BsdfLobeKind,
}

/// Core material scattering contract used by all render backends.
///
/// `outgoing` and `incoming` directions are expected to point away from the
/// shaded point in world space.
pub trait Bsdf: Send + Sync {
    fn preview_base_color(&self) -> Color;

    fn emissive_rgb(&self) -> Color {
        Color::new(0.0, 0.0, 0.0)
    }

    fn emissive_at_nm(&self, wavelength_nm: f32) -> f32 {
        rgb_to_emission_at_nm(self.emissive_rgb(), wavelength_nm)
    }

    fn eval(&self, query: &BsdfEvalQuery) -> Color;

    fn pdf(&self, query: &BsdfEvalQuery) -> f32;

    fn sample(&self, query: &BsdfSampleQuery, sample: BsdfSampleInput) -> Option<BsdfSampleRgb>;

    fn eval_spectral(&self, query: &BsdfEvalQuery, wavelength_nm: f32) -> f32;

    fn pdf_spectral(&self, query: &BsdfEvalQuery, wavelength_nm: f32) -> f32;

    fn sample_spectral(
        &self,
        query: &BsdfSampleQuery,
        sample: BsdfSampleInput,
        wavelength_nm: f32,
    ) -> Option<BsdfSampleSpectral>;
}

pub struct GltfMrBsdf {
    base_color: Color,
    metallic: f32,
    roughness: f32,
    emissive: Color,
    transmission: f32,
    ior: f32,
    base_color_spectrum: SampledSpectrum,
    emissive_spectrum: SampledSpectrum,
}

impl GltfMrBsdf {
    pub fn new(
        base_color: Color,
        metallic: f32,
        roughness: f32,
        emissive: Color,
        transmission: f32,
        ior: f32,
    ) -> Self {
        let base_color = clamp_color01(base_color);
        let metallic = clamp01(metallic);
        let roughness = clamp01(roughness).max(MIN_ROUGHNESS);
        let transmission = clamp01(transmission);
        let ior = ior.max(1.0);
        let emissive = emissive.map(|channel| channel.max(0.0));

        Self {
            base_color,
            metallic,
            roughness,
            emissive,
            transmission,
            ior,
            base_color_spectrum: rgb_to_reflectance_spectrum(base_color),
            emissive_spectrum: rgb_to_emission_spectrum(emissive),
        }
    }

    pub fn opaque(base_color: Color, metallic: f32, roughness: f32) -> Self {
        Self::new(
            base_color,
            metallic,
            roughness,
            Color::new(0.0, 0.0, 0.0),
            0.0,
            1.5,
        )
    }

    pub fn transmissive(base_color: Color, roughness: f32, transmission: f32, ior: f32) -> Self {
        Self::new(
            base_color,
            0.0,
            roughness,
            Color::new(0.0, 0.0, 0.0),
            transmission,
            ior,
        )
    }

    pub fn base_color(&self) -> Color {
        self.base_color
    }

    pub fn metallic(&self) -> f32 {
        self.metallic
    }

    pub fn roughness(&self) -> f32 {
        self.roughness
    }

    pub fn transmission(&self) -> f32 {
        self.transmission
    }

    pub fn ior(&self) -> f32 {
        self.ior
    }

    fn alpha(&self) -> f32 {
        (self.roughness * self.roughness).max(1e-4)
    }

    fn dielectric_f0(&self) -> f32 {
        let ratio = (self.ior - 1.0) / (self.ior + 1.0);
        ratio * ratio
    }

    fn f0_color(&self) -> Color {
        let dielectric = Color::new(
            self.dielectric_f0(),
            self.dielectric_f0(),
            self.dielectric_f0(),
        );
        dielectric * (1.0 - self.metallic) + self.base_color * self.metallic
    }

    fn f0_spectral(&self, wavelength_nm: f32) -> f32 {
        let base = self
            .base_color_spectrum
            .sample_clamped(wavelength_nm)
            .clamp(0.0, 1.0);
        self.dielectric_f0() * (1.0 - self.metallic) + base * self.metallic
    }

    fn transmission_tint_rgb(&self) -> Color {
        Color::new(
            self.base_color.x.max(0.02),
            self.base_color.y.max(0.02),
            self.base_color.z.max(0.02),
        )
    }

    fn transmission_tint_spectral(&self, wavelength_nm: f32) -> f32 {
        self.base_color_spectrum
            .sample_clamped(wavelength_nm)
            .clamp(0.02, 1.0)
    }

    fn lobe_weights(&self, cos_outgoing: f32) -> (f32, f32, f32) {
        let fresnel = fresnel_dielectric(cos_outgoing.abs(), self.ior);
        let raw_diffuse = (1.0 - self.metallic) * (1.0 - self.transmission);
        let raw_specular = self.metallic + (1.0 - self.metallic) * fresnel;
        let raw_transmission = (1.0 - self.metallic) * self.transmission * (1.0 - fresnel);

        let sum = raw_diffuse + raw_specular + raw_transmission;
        if sum <= EPSILON {
            return (0.0, 1.0, 0.0);
        }

        (
            raw_diffuse / sum,
            raw_specular / sum,
            raw_transmission / sum,
        )
    }

    fn eval_reflection(&self, n: Vector3<f32>, wo: Vector3<f32>, wi: Vector3<f32>) -> Color {
        let cos_o = n.dot(&wo).abs();
        let cos_i = n.dot(&wi).abs();
        if cos_o <= EPSILON || cos_i <= EPSILON {
            return Color::new(0.0, 0.0, 0.0);
        }

        let mut value = self.base_color * ((1.0 - self.metallic) * (1.0 - self.transmission) / PI);

        let wh = wo + wi;
        if wh.norm_squared() > EPSILON {
            let wh = wh.normalize();
            let d = ggx_distribution(n, wh, self.alpha());
            let g = smith_geometry(n, wo, wi, self.alpha());
            let f = fresnel_schlick_color(wi.dot(&wh).abs(), self.f0_color());
            let spec_scale = d * g / (4.0 * cos_o * cos_i + EPSILON);
            value += f * spec_scale;
        }

        clamp_color_non_negative(value)
    }

    fn eval_reflection_spectral(
        &self,
        n: Vector3<f32>,
        wo: Vector3<f32>,
        wi: Vector3<f32>,
        wavelength_nm: f32,
    ) -> f32 {
        let cos_o = n.dot(&wo).abs();
        let cos_i = n.dot(&wi).abs();
        if cos_o <= EPSILON || cos_i <= EPSILON {
            return 0.0;
        }

        let base = self
            .base_color_spectrum
            .sample_clamped(wavelength_nm)
            .clamp(0.0, 1.0);
        let mut value = base * ((1.0 - self.metallic) * (1.0 - self.transmission) / PI);

        let wh = wo + wi;
        if wh.norm_squared() > EPSILON {
            let wh = wh.normalize();
            let d = ggx_distribution(n, wh, self.alpha());
            let g = smith_geometry(n, wo, wi, self.alpha());
            let f = fresnel_schlick(wi.dot(&wh).abs(), self.f0_spectral(wavelength_nm));
            value += d * g * f / (4.0 * cos_o * cos_i + EPSILON);
        }

        value.max(0.0)
    }

    fn eval_transmission(&self, n: Vector3<f32>, wo: Vector3<f32>, wi: Vector3<f32>) -> Color {
        if self.transmission <= EPSILON || self.metallic >= 1.0 - EPSILON {
            return Color::new(0.0, 0.0, 0.0);
        }

        let cos_o = n.dot(&wo);
        let cos_i = n.dot(&wi);
        if same_hemisphere(cos_o, cos_i) {
            return Color::new(0.0, 0.0, 0.0);
        }

        let eta = if cos_o > 0.0 {
            self.ior
        } else {
            1.0 / self.ior
        };
        let mut wh = wo + wi * eta;
        if wh.norm_squared() <= EPSILON {
            return Color::new(0.0, 0.0, 0.0);
        }
        wh = wh.normalize();
        if wh.dot(&n) < 0.0 {
            wh = -wh;
        }

        let dot_wo_wh = wo.dot(&wh);
        let dot_wi_wh = wi.dot(&wh);
        if dot_wo_wh * dot_wi_wh > 0.0 {
            return Color::new(0.0, 0.0, 0.0);
        }

        let cos_o_abs = cos_o.abs();
        let cos_i_abs = cos_i.abs();
        if cos_o_abs <= EPSILON || cos_i_abs <= EPSILON {
            return Color::new(0.0, 0.0, 0.0);
        }

        let d = ggx_distribution(n, wh, self.alpha());
        let g = smith_geometry(n, wo, wi, self.alpha());
        let f = fresnel_dielectric(dot_wo_wh.abs(), self.ior);
        let sqrt_denom = dot_wo_wh + eta * dot_wi_wh;
        if sqrt_denom.abs() <= EPSILON {
            return Color::new(0.0, 0.0, 0.0);
        }

        let scale = (1.0 - self.metallic)
            * self.transmission
            * (1.0 - f)
            * d
            * g
            * eta
            * eta
            * dot_wi_wh.abs()
            * dot_wo_wh.abs()
            / (cos_i_abs * cos_o_abs * sqrt_denom * sqrt_denom + EPSILON);

        clamp_color_non_negative(self.transmission_tint_rgb() * scale)
    }

    fn eval_transmission_spectral(
        &self,
        n: Vector3<f32>,
        wo: Vector3<f32>,
        wi: Vector3<f32>,
        wavelength_nm: f32,
    ) -> f32 {
        if self.transmission <= EPSILON || self.metallic >= 1.0 - EPSILON {
            return 0.0;
        }

        let cos_o = n.dot(&wo);
        let cos_i = n.dot(&wi);
        if same_hemisphere(cos_o, cos_i) {
            return 0.0;
        }

        let eta = if cos_o > 0.0 {
            self.ior
        } else {
            1.0 / self.ior
        };
        let mut wh = wo + wi * eta;
        if wh.norm_squared() <= EPSILON {
            return 0.0;
        }
        wh = wh.normalize();
        if wh.dot(&n) < 0.0 {
            wh = -wh;
        }

        let dot_wo_wh = wo.dot(&wh);
        let dot_wi_wh = wi.dot(&wh);
        if dot_wo_wh * dot_wi_wh > 0.0 {
            return 0.0;
        }

        let cos_o_abs = cos_o.abs();
        let cos_i_abs = cos_i.abs();
        if cos_o_abs <= EPSILON || cos_i_abs <= EPSILON {
            return 0.0;
        }

        let d = ggx_distribution(n, wh, self.alpha());
        let g = smith_geometry(n, wo, wi, self.alpha());
        let f = fresnel_dielectric(dot_wo_wh.abs(), self.ior);
        let sqrt_denom = dot_wo_wh + eta * dot_wi_wh;
        if sqrt_denom.abs() <= EPSILON {
            return 0.0;
        }

        let tint = self.transmission_tint_spectral(wavelength_nm);
        (1.0 - self.metallic)
            * self.transmission
            * (1.0 - f)
            * d
            * g
            * eta
            * eta
            * dot_wi_wh.abs()
            * dot_wo_wh.abs()
            * tint
            / (cos_i_abs * cos_o_abs * sqrt_denom * sqrt_denom + EPSILON)
    }

    fn reflection_pdf(&self, n: Vector3<f32>, wo: Vector3<f32>, wi: Vector3<f32>) -> f32 {
        if !same_hemisphere(n.dot(&wo), n.dot(&wi)) {
            return 0.0;
        }

        let (w_diffuse, w_specular, _) = self.lobe_weights(n.dot(&wo));
        let diffuse_pdf = cosine_hemisphere_pdf(n, wi);
        let specular_pdf = ggx_reflection_pdf(n, wo, wi, self.alpha());
        w_diffuse * diffuse_pdf + w_specular * specular_pdf
    }

    fn transmission_pdf(&self, n: Vector3<f32>, wo: Vector3<f32>, wi: Vector3<f32>) -> f32 {
        if same_hemisphere(n.dot(&wo), n.dot(&wi)) {
            return 0.0;
        }

        let (_, _, w_transmission) = self.lobe_weights(n.dot(&wo));
        w_transmission * ggx_transmission_pdf(n, wo, wi, self.alpha(), self.ior)
    }

    fn sample_diffuse(
        &self,
        n: Vector3<f32>,
        sample: BsdfSampleInput,
    ) -> Option<(Vector3<f32>, BsdfLobeKind)> {
        let incoming = sample_cosine_hemisphere(n, sample.u1, sample.u2);
        if incoming.norm_squared() <= EPSILON {
            return None;
        }
        Some((incoming.normalize(), BsdfLobeKind::Diffuse))
    }

    fn sample_reflection(
        &self,
        n: Vector3<f32>,
        wo: Vector3<f32>,
        sample: BsdfSampleInput,
    ) -> Option<(Vector3<f32>, BsdfLobeKind)> {
        let half_vector = sample_ggx_half_vector(n, self.alpha(), sample.u1, sample.u2);
        let incoming = reflect(-wo, half_vector).normalize();
        if !same_hemisphere(n.dot(&wo), n.dot(&incoming)) {
            return None;
        }
        Some((incoming, BsdfLobeKind::SpecularReflection))
    }

    fn sample_transmission(
        &self,
        n: Vector3<f32>,
        wo: Vector3<f32>,
        sample: BsdfSampleInput,
    ) -> Option<(Vector3<f32>, BsdfLobeKind)> {
        if self.transmission <= EPSILON || self.metallic >= 1.0 - EPSILON {
            return None;
        }

        let half_vector = sample_ggx_half_vector(n, self.alpha(), sample.u1, sample.u2);
        let (eta_i, eta_t) = if n.dot(&wo) > 0.0 {
            (1.0, self.ior)
        } else {
            (self.ior, 1.0)
        };

        let incident = -wo;
        match refract_direction(incident, half_vector, eta_i, eta_t) {
            Some(direction) => Some((direction.normalize(), BsdfLobeKind::SpecularTransmission)),
            None => {
                let reflected = reflect(-wo, half_vector).normalize();
                Some((reflected, BsdfLobeKind::SpecularReflection))
            }
        }
    }
}

impl Bsdf for GltfMrBsdf {
    fn preview_base_color(&self) -> Color {
        self.base_color
    }

    fn emissive_rgb(&self) -> Color {
        self.emissive
    }

    fn emissive_at_nm(&self, wavelength_nm: f32) -> f32 {
        self.emissive_spectrum
            .sample_clamped(wavelength_nm)
            .max(0.0)
    }

    fn eval(&self, query: &BsdfEvalQuery) -> Color {
        let Some(n) = safe_normalize(query.normal) else {
            return Color::new(0.0, 0.0, 0.0);
        };
        let Some(wo) = safe_normalize(query.outgoing) else {
            return Color::new(0.0, 0.0, 0.0);
        };
        let Some(wi) = safe_normalize(query.incoming) else {
            return Color::new(0.0, 0.0, 0.0);
        };

        if same_hemisphere(n.dot(&wo), n.dot(&wi)) {
            self.eval_reflection(n, wo, wi)
        } else {
            self.eval_transmission(n, wo, wi)
        }
    }

    fn pdf(&self, query: &BsdfEvalQuery) -> f32 {
        let Some(n) = safe_normalize(query.normal) else {
            return 0.0;
        };
        let Some(wo) = safe_normalize(query.outgoing) else {
            return 0.0;
        };
        let Some(wi) = safe_normalize(query.incoming) else {
            return 0.0;
        };

        self.reflection_pdf(n, wo, wi) + self.transmission_pdf(n, wo, wi)
    }

    fn sample(&self, query: &BsdfSampleQuery, sample: BsdfSampleInput) -> Option<BsdfSampleRgb> {
        let n = face_forward(safe_normalize(query.normal)?, query.outgoing);
        let wo = safe_normalize(query.outgoing)?;

        let (w_diffuse, w_specular, _) = self.lobe_weights(n.dot(&wo));
        let lobe = clamp01(sample.lobe);

        let sampled = if lobe < w_diffuse {
            self.sample_diffuse(n, sample)
        } else if lobe < w_diffuse + w_specular {
            self.sample_reflection(n, wo, sample)
        } else {
            self.sample_transmission(n, wo, sample)
        };

        let (incoming, lobe_kind) = sampled?;
        let eval_query = BsdfEvalQuery {
            normal: n,
            outgoing: wo,
            incoming,
        };
        let value = self.eval(&eval_query);
        let pdf = self.pdf(&eval_query);
        if pdf <= EPSILON {
            return None;
        }

        Some(BsdfSampleRgb {
            incoming,
            value,
            pdf,
            lobe: lobe_kind,
        })
    }

    fn eval_spectral(&self, query: &BsdfEvalQuery, wavelength_nm: f32) -> f32 {
        let Some(n) = safe_normalize(query.normal) else {
            return 0.0;
        };
        let Some(wo) = safe_normalize(query.outgoing) else {
            return 0.0;
        };
        let Some(wi) = safe_normalize(query.incoming) else {
            return 0.0;
        };

        if same_hemisphere(n.dot(&wo), n.dot(&wi)) {
            self.eval_reflection_spectral(n, wo, wi, wavelength_nm)
        } else {
            self.eval_transmission_spectral(n, wo, wi, wavelength_nm)
        }
    }

    fn pdf_spectral(&self, query: &BsdfEvalQuery, _wavelength_nm: f32) -> f32 {
        self.pdf(query)
    }

    fn sample_spectral(
        &self,
        query: &BsdfSampleQuery,
        sample: BsdfSampleInput,
        wavelength_nm: f32,
    ) -> Option<BsdfSampleSpectral> {
        let sampled = self.sample(query, sample)?;
        let eval_query = BsdfEvalQuery {
            normal: query.normal,
            outgoing: query.outgoing,
            incoming: sampled.incoming,
        };
        let value = self.eval_spectral(&eval_query, wavelength_nm);
        let pdf = self.pdf_spectral(&eval_query, wavelength_nm);
        if pdf <= EPSILON {
            return None;
        }

        Some(BsdfSampleSpectral {
            incoming: sampled.incoming,
            value,
            pdf,
            lobe: sampled.lobe,
        })
    }
}

fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn clamp_color01(color: Color) -> Color {
    color.map(clamp01)
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

fn safe_normalize(vector: Vector3<f32>) -> Option<Vector3<f32>> {
    if vector.norm_squared() <= EPSILON {
        return None;
    }
    Some(vector.normalize())
}

fn same_hemisphere(a: f32, b: f32) -> bool {
    a * b > 0.0
}

fn face_forward(normal: Vector3<f32>, reference: Vector3<f32>) -> Vector3<f32> {
    if normal.dot(&reference) < 0.0 {
        -normal
    } else {
        normal
    }
}

fn reflect(vector: Vector3<f32>, normal: Vector3<f32>) -> Vector3<f32> {
    vector - 2.0 * vector.dot(&normal) * normal
}

fn refract_direction(
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
    Some(eta_ratio * incident - (eta_ratio * cos_i + cos_t) * n)
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

fn sample_cosine_hemisphere(normal: Vector3<f32>, u1: f32, u2: f32) -> Vector3<f32> {
    let u1 = clamp01(u1);
    let u2 = clamp01(u2);
    let r = u1.sqrt();
    let phi = 2.0 * PI * u2;
    let x = r * phi.cos();
    let y = r * phi.sin();
    let z = (1.0 - u1).sqrt();

    let (tangent, bitangent) = orthonormal_basis(normal);
    (tangent * x + bitangent * y + normal * z).normalize()
}

fn cosine_hemisphere_pdf(normal: Vector3<f32>, direction: Vector3<f32>) -> f32 {
    let cosine = normal.dot(&direction).abs();
    cosine / PI
}

fn sample_ggx_half_vector(normal: Vector3<f32>, alpha: f32, u1: f32, u2: f32) -> Vector3<f32> {
    let alpha2 = alpha * alpha;
    let u1 = clamp01(u1).max(1e-6);
    let u2 = clamp01(u2);

    let phi = 2.0 * PI * u2;
    let tan_theta2 = alpha2 * u1 / (1.0 - u1 + EPSILON);
    let cos_theta = 1.0 / (1.0 + tan_theta2).sqrt();
    let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();

    let local = Vector3::new(sin_theta * phi.cos(), sin_theta * phi.sin(), cos_theta);
    let (tangent, bitangent) = orthonormal_basis(normal);
    (tangent * local.x + bitangent * local.y + normal * local.z).normalize()
}

fn ggx_distribution(normal: Vector3<f32>, half_vector: Vector3<f32>, alpha: f32) -> f32 {
    let cos_theta = normal.dot(&half_vector).abs();
    if cos_theta <= EPSILON {
        return 0.0;
    }

    let alpha2 = alpha * alpha;
    let cos2 = cos_theta * cos_theta;
    let denom = cos2 * (alpha2 - 1.0) + 1.0;
    alpha2 / (PI * denom * denom + EPSILON)
}

fn smith_g1(cos_theta: f32, alpha: f32) -> f32 {
    let cos_theta = cos_theta.abs();
    if cos_theta <= EPSILON {
        return 0.0;
    }
    let alpha2 = alpha * alpha;
    let tan2 = (1.0 - cos_theta * cos_theta).max(0.0) / (cos_theta * cos_theta + EPSILON);
    2.0 / (1.0 + (1.0 + alpha2 * tan2).sqrt())
}

fn smith_geometry(normal: Vector3<f32>, wo: Vector3<f32>, wi: Vector3<f32>, alpha: f32) -> f32 {
    smith_g1(normal.dot(&wo), alpha) * smith_g1(normal.dot(&wi), alpha)
}

fn fresnel_schlick(cos_theta: f32, f0: f32) -> f32 {
    let t = (1.0 - cos_theta.clamp(0.0, 1.0)).powi(5);
    f0 + (1.0 - f0) * t
}

fn fresnel_schlick_color(cos_theta: f32, f0: Color) -> Color {
    let t = (1.0 - cos_theta.clamp(0.0, 1.0)).powi(5);
    f0 + (Color::new(1.0, 1.0, 1.0) - f0) * t
}

fn fresnel_dielectric(cos_theta_i: f32, ior: f32) -> f32 {
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
    let rs = ((eta_t * cos_i) - (eta_i * cos_t)) / ((eta_t * cos_i) + (eta_i * cos_t));
    let rp = ((eta_i * cos_i) - (eta_t * cos_t)) / ((eta_i * cos_i) + (eta_t * cos_t));
    0.5 * (rs * rs + rp * rp)
}

fn ggx_reflection_pdf(normal: Vector3<f32>, wo: Vector3<f32>, wi: Vector3<f32>, alpha: f32) -> f32 {
    if !same_hemisphere(normal.dot(&wo), normal.dot(&wi)) {
        return 0.0;
    }

    let half_vector = wo + wi;
    if half_vector.norm_squared() <= EPSILON {
        return 0.0;
    }
    let half_vector = half_vector.normalize();

    let d = ggx_distribution(normal, half_vector, alpha);
    let dwh = d * normal.dot(&half_vector).abs();
    dwh / (4.0 * wo.dot(&half_vector).abs() + EPSILON)
}

fn ggx_transmission_pdf(
    normal: Vector3<f32>,
    wo: Vector3<f32>,
    wi: Vector3<f32>,
    alpha: f32,
    ior: f32,
) -> f32 {
    if same_hemisphere(normal.dot(&wo), normal.dot(&wi)) {
        return 0.0;
    }

    let eta = if normal.dot(&wo) > 0.0 {
        ior
    } else {
        1.0 / ior
    };

    let mut half_vector = wo + wi * eta;
    if half_vector.norm_squared() <= EPSILON {
        return 0.0;
    }
    half_vector = half_vector.normalize();
    if half_vector.dot(&normal) < 0.0 {
        half_vector = -half_vector;
    }

    let dot_wo_wh = wo.dot(&half_vector);
    let dot_wi_wh = wi.dot(&half_vector);
    let sqrt_denom = dot_wo_wh + eta * dot_wi_wh;
    if sqrt_denom.abs() <= EPSILON {
        return 0.0;
    }

    let d = ggx_distribution(normal, half_vector, alpha);
    let dwh = d * half_vector.dot(&normal).abs();
    let dwh_dwi = eta * eta * dot_wi_wh.abs() / (sqrt_denom * sqrt_denom + EPSILON);
    dwh * dwh_dwi
}

#[cfg(test)]
mod tests {
    use super::{
        Bsdf, BsdfEvalQuery, BsdfSampleInput, BsdfSampleQuery, GltfMrBsdf, fresnel_dielectric,
        refract_direction,
    };
    use crate::Color;
    use nalgebra::Vector3;

    fn v(x: f32, y: f32, z: f32) -> Vector3<f32> {
        Vector3::new(x, y, z).normalize()
    }

    #[test]
    fn constructor_clamps_parameters() {
        let bsdf = GltfMrBsdf::new(
            Color::new(2.0, -1.0, 0.5),
            -2.0,
            0.0,
            Color::new(-1.0, 2.0, 3.0),
            2.0,
            0.2,
        );

        assert_eq!(bsdf.base_color(), Color::new(1.0, 0.0, 0.5));
        assert!((bsdf.metallic() - 0.0).abs() < 1e-6);
        assert!(bsdf.roughness() >= 0.045);
        assert!((bsdf.transmission() - 1.0).abs() < 1e-6);
        assert!((bsdf.ior() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn bsdf_eval_is_finite_and_non_negative() {
        let bsdf = GltfMrBsdf::new(
            Color::new(0.8, 0.6, 0.2),
            0.2,
            0.35,
            Color::new(0.0, 0.0, 0.0),
            0.3,
            1.45,
        );

        let query = BsdfEvalQuery {
            normal: Vector3::new(0.0, 1.0, 0.0),
            outgoing: v(0.0, 1.0, 0.0),
            incoming: v(0.3, 0.9, 0.1),
        };
        let value = bsdf.eval(&query);

        assert!(value.x.is_finite() && value.y.is_finite() && value.z.is_finite());
        assert!(value.x >= 0.0 && value.y >= 0.0 && value.z >= 0.0);
    }

    #[test]
    fn fresnel_grows_toward_grazing() {
        let normal_incidence = fresnel_dielectric(1.0, 1.5);
        let grazing = fresnel_dielectric(0.05, 1.5);

        assert!(grazing > normal_incidence);
        assert!((0.0..=1.0).contains(&normal_incidence));
        assert!((0.0..=1.0).contains(&grazing));
    }

    #[test]
    fn sample_pdf_matches_pdf_query() {
        let bsdf = GltfMrBsdf::new(
            Color::new(0.7, 0.7, 0.7),
            0.0,
            0.25,
            Color::new(0.0, 0.0, 0.0),
            0.2,
            1.5,
        );

        let query = BsdfSampleQuery {
            normal: Vector3::new(0.0, 1.0, 0.0),
            outgoing: v(0.1, 0.99, 0.05),
        };
        let sample = bsdf
            .sample(
                &query,
                BsdfSampleInput {
                    lobe: 0.63,
                    u1: 0.21,
                    u2: 0.78,
                },
            )
            .expect("expected a valid sample");

        let eval_query = BsdfEvalQuery {
            normal: query.normal,
            outgoing: query.outgoing,
            incoming: sample.incoming,
        };

        let pdf = bsdf.pdf(&eval_query);
        assert!(pdf > 0.0);
        assert!((sample.pdf - pdf).abs() <= 1e-5);
    }

    #[test]
    fn refract_reports_total_internal_reflection() {
        let incident = Vector3::new(0.95, -0.31, 0.0).normalize();
        let normal = Vector3::new(0.0, 1.0, 0.0);

        let refracted = refract_direction(incident, normal, 1.5, 1.0);
        assert!(refracted.is_none());
    }

    #[test]
    fn spectral_eval_is_non_negative_at_grazing() {
        let bsdf = GltfMrBsdf::transmissive(Color::new(0.9, 0.95, 1.0), 0.2, 1.0, 1.52);
        let query = BsdfEvalQuery {
            normal: Vector3::new(0.0, 1.0, 0.0),
            outgoing: v(0.98, 0.2, 0.0),
            incoming: v(0.0, -1.0, 0.0),
        };

        let value = bsdf.eval_spectral(&query, 550.0);
        assert!(value.is_finite());
        assert!(value >= 0.0);
    }
}
