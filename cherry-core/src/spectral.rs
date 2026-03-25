use nalgebra::Vector3;

use crate::Color;

pub const WAVELENGTH_MIN_NM: f32 = 380.0;
pub const WAVELENGTH_MAX_NM: f32 = 780.0;
pub const WAVELENGTH_BIN_STEP_NM: f32 = 10.0;
pub const WAVELENGTH_BIN_COUNT: usize = 41;
const RGB_EMISSION_NORMALIZATION: f32 = 1.53;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wavelength(f32);

impl Wavelength {
    pub fn new(value_nm: f32) -> Self {
        Self(value_nm.clamp(WAVELENGTH_MIN_NM, WAVELENGTH_MAX_NM))
    }

    pub fn nm(self) -> f32 {
        self.0
    }
}

pub trait SpectralCurve: Send + Sync {
    fn sample(&self, wavelength_nm: f32) -> f32;
}

#[derive(Clone, Debug, PartialEq)]
pub struct SampledSpectrum {
    bins: [f32; WAVELENGTH_BIN_COUNT],
}

impl SampledSpectrum {
    pub fn new(bins: [f32; WAVELENGTH_BIN_COUNT]) -> Self {
        Self { bins }
    }

    pub fn zeros() -> Self {
        Self {
            bins: [0.0; WAVELENGTH_BIN_COUNT],
        }
    }

    pub fn constant(value: f32) -> Self {
        Self {
            bins: [value; WAVELENGTH_BIN_COUNT],
        }
    }

    pub fn from_fn(mut sampler: impl FnMut(f32) -> f32) -> Self {
        let mut bins = [0.0; WAVELENGTH_BIN_COUNT];
        for (index, value) in bins.iter_mut().enumerate() {
            *value = sampler(wavelength_for_bin(index));
        }
        Self { bins }
    }

    pub fn bins(&self) -> &[f32; WAVELENGTH_BIN_COUNT] {
        &self.bins
    }

    pub fn sample_clamped(&self, wavelength_nm: f32) -> f32 {
        let clamped = wavelength_nm.clamp(WAVELENGTH_MIN_NM, WAVELENGTH_MAX_NM);
        let position = (clamped - WAVELENGTH_MIN_NM) / WAVELENGTH_BIN_STEP_NM;
        let lower = position.floor() as usize;
        let upper = position.ceil() as usize;
        if lower == upper {
            return self.bins[lower];
        }

        let t = position - lower as f32;
        self.bins[lower] * (1.0 - t) + self.bins[upper] * t
    }

    pub fn to_xyz(&self) -> Vector3<f32> {
        let mut xyz = Vector3::new(0.0, 0.0, 0.0);
        for (index, value) in self.bins.iter().enumerate() {
            let wavelength = wavelength_for_bin(index);
            xyz += cie_xyz_from_wavelength(wavelength) * *value;
        }
        xyz * WAVELENGTH_BIN_STEP_NM
    }

    pub fn to_linear_srgb(&self) -> Color {
        xyz_to_linear_srgb(self.to_xyz())
    }
}

impl SpectralCurve for SampledSpectrum {
    fn sample(&self, wavelength_nm: f32) -> f32 {
        self.sample_clamped(wavelength_nm)
    }
}

pub fn wavelength_for_bin(index: usize) -> f32 {
    WAVELENGTH_MIN_NM + index as f32 * WAVELENGTH_BIN_STEP_NM
}

pub fn bin_index_for_wavelength(wavelength_nm: f32) -> usize {
    ((wavelength_nm.clamp(WAVELENGTH_MIN_NM, WAVELENGTH_MAX_NM) - WAVELENGTH_MIN_NM)
        / WAVELENGTH_BIN_STEP_NM)
        .round() as usize
}

pub fn cie_xyz_from_wavelength(wavelength_nm: f32) -> Vector3<f32> {
    let wavelength = wavelength_nm.clamp(WAVELENGTH_MIN_NM, WAVELENGTH_MAX_NM);

    let t1 = (wavelength - 442.0) * if wavelength < 442.0 { 0.0624 } else { 0.0374 };
    let t2 = (wavelength - 599.8) * if wavelength < 599.8 { 0.0264 } else { 0.0323 };
    let t3 = (wavelength - 501.1) * if wavelength < 501.1 { 0.0490 } else { 0.0382 };
    let x = 0.362 * (-0.5 * t1 * t1).exp() + 1.056 * (-0.5 * t2 * t2).exp()
        - 0.065 * (-0.5 * t3 * t3).exp();

    let t1 = (wavelength - 568.8) * if wavelength < 568.8 { 0.0213 } else { 0.0247 };
    let t2 = (wavelength - 530.9) * if wavelength < 530.9 { 0.0613 } else { 0.0322 };
    let y = 0.821 * (-0.5 * t1 * t1).exp() + 0.286 * (-0.5 * t2 * t2).exp();

    let t1 = (wavelength - 437.0) * if wavelength < 437.0 { 0.0845 } else { 0.0278 };
    let t2 = (wavelength - 459.0) * if wavelength < 459.0 { 0.0385 } else { 0.0725 };
    let z = 1.217 * (-0.5 * t1 * t1).exp() + 0.681 * (-0.5 * t2 * t2).exp();

    Vector3::new(x.max(0.0), y.max(0.0), z.max(0.0))
}

pub fn xyz_to_linear_srgb(xyz: Vector3<f32>) -> Color {
    let x = xyz.x;
    let y = xyz.y;
    let z = xyz.z;
    Color::new(
        (3.2406 * x - 1.5372 * y - 0.4986 * z).max(0.0),
        (-0.9689 * x + 1.8758 * y + 0.0415 * z).max(0.0),
        (0.0557 * x - 0.2040 * y + 1.0570 * z).max(0.0),
    )
}

pub fn apply_exposure_reinhard(color: Color, exposure: f32) -> Color {
    let scaled = color * exposure.max(0.0);
    Color::new(
        scaled.x / (1.0 + scaled.x),
        scaled.y / (1.0 + scaled.y),
        scaled.z / (1.0 + scaled.z),
    )
}

fn gaussian(wavelength_nm: f32, center: f32, sigma: f32) -> f32 {
    let value = (wavelength_nm - center) / sigma;
    (-0.5 * value * value).exp()
}

fn rgb_gaussian_mix(color: Color, wavelength_nm: f32) -> f32 {
    color.x.max(0.0) * gaussian(wavelength_nm, 610.0, 45.0)
        + color.y.max(0.0) * gaussian(wavelength_nm, 550.0, 35.0)
        + color.z.max(0.0) * gaussian(wavelength_nm, 460.0, 30.0)
}

pub fn rgb_to_emission_at_nm(color: Color, wavelength_nm: f32) -> f32 {
    rgb_gaussian_mix(color, wavelength_nm) / RGB_EMISSION_NORMALIZATION
}

pub fn rgb_to_reflectance_at_nm(color: Color, wavelength_nm: f32) -> f32 {
    rgb_gaussian_mix(color, wavelength_nm).clamp(0.0, 1.0)
}

pub fn rgb_to_emission_spectrum(color: Color) -> SampledSpectrum {
    SampledSpectrum::from_fn(|wavelength| rgb_to_emission_at_nm(color, wavelength))
}

pub fn rgb_to_reflectance_spectrum(color: Color) -> SampledSpectrum {
    SampledSpectrum::from_fn(|wavelength| rgb_to_reflectance_at_nm(color, wavelength))
}

#[cfg(test)]
mod tests {
    use super::{
        SampledSpectrum, WAVELENGTH_BIN_COUNT, WAVELENGTH_MAX_NM, WAVELENGTH_MIN_NM, Wavelength,
        apply_exposure_reinhard, cie_xyz_from_wavelength, rgb_to_emission_at_nm,
        xyz_to_linear_srgb,
    };
    use crate::Color;

    #[test]
    fn wavelength_is_clamped_to_domain() {
        let low = Wavelength::new(100.0);
        let high = Wavelength::new(1000.0);
        assert_eq!(low.nm(), WAVELENGTH_MIN_NM);
        assert_eq!(high.nm(), WAVELENGTH_MAX_NM);
    }

    #[test]
    fn sampled_spectrum_interpolates_between_bins() {
        let mut bins = [0.0; WAVELENGTH_BIN_COUNT];
        bins[0] = 0.0;
        bins[1] = 1.0;
        let spectrum = SampledSpectrum::new(bins);
        assert!((spectrum.sample_clamped(385.0) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn cie_xyz_values_are_non_negative() {
        let xyz = cie_xyz_from_wavelength(550.0);
        assert!(xyz.x >= 0.0 && xyz.y >= 0.0 && xyz.z >= 0.0);
    }

    #[test]
    fn xyz_to_rgb_and_tone_map_are_deterministic() {
        let xyz = nalgebra::Vector3::new(0.5, 0.6, 0.2);
        let rgb = xyz_to_linear_srgb(xyz);
        let mapped = apply_exposure_reinhard(rgb, 1.0);
        assert_eq!(mapped, apply_exposure_reinhard(rgb, 1.0));
    }

    #[test]
    fn tone_mapping_reduces_highlights() {
        let mapped = apply_exposure_reinhard(Color::new(10.0, 2.0, 0.5), 1.0);
        assert!(mapped.x < 1.0);
        assert!(mapped.y < 1.0);
        assert!(mapped.z < 1.0);
    }

    #[test]
    fn white_rgb_emission_is_normalized_in_visible_range() {
        let white = Color::new(1.0, 1.0, 1.0);
        for wavelength in (WAVELENGTH_MIN_NM as usize..=WAVELENGTH_MAX_NM as usize).step_by(5) {
            let emission = rgb_to_emission_at_nm(white, wavelength as f32);
            assert!(
                emission <= 1.0 + 1e-4,
                "expected normalized emission <= 1.0, got {emission} at {wavelength}nm"
            );
        }
    }
}
