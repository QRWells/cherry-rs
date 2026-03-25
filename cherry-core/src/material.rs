use nalgebra::Vector3;

use crate::{
    Color,
    spectral::{SampledSpectrum, SpectralCurve, rgb_to_reflectance_spectrum},
};

pub trait SpectralMaterial: Send + Sync {
    fn reflectance_at_nm(&self, wavelength_nm: f32) -> f32;

    fn reflectance_curve(&self) -> Option<&SampledSpectrum> {
        None
    }
}

pub trait Material: Send + Sync {
    fn albedo(&self) -> Color;

    fn as_spectral(&self) -> Option<&dyn SpectralMaterial> {
        None
    }
}

pub struct Lambertian {
    pub color: Color,
}

impl Lambertian {
    pub fn new(color: Vector3<f32>) -> Self {
        Self { color }
    }
}

impl Material for Lambertian {
    fn albedo(&self) -> Color {
        self.color
    }
}

pub struct SpectralLambertian {
    spectrum: SampledSpectrum,
    preview_color: Color,
}

impl SpectralLambertian {
    pub fn new(spectrum: SampledSpectrum) -> Self {
        let preview_color = spectrum.to_linear_srgb();
        Self {
            spectrum,
            preview_color,
        }
    }

    pub fn from_rgb(color: Color) -> Self {
        Self::new(rgb_to_reflectance_spectrum(color))
    }

    pub fn spectrum(&self) -> &SampledSpectrum {
        &self.spectrum
    }
}

impl SpectralMaterial for SpectralLambertian {
    fn reflectance_at_nm(&self, wavelength_nm: f32) -> f32 {
        self.spectrum.sample(wavelength_nm).clamp(0.0, 1.0)
    }

    fn reflectance_curve(&self) -> Option<&SampledSpectrum> {
        Some(&self.spectrum)
    }
}

impl Material for SpectralLambertian {
    fn albedo(&self) -> Color {
        self.preview_color
    }

    fn as_spectral(&self) -> Option<&dyn SpectralMaterial> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::{Lambertian, Material, SpectralLambertian, SpectralMaterial};
    use crate::Color;

    #[test]
    fn legacy_lambertian_has_no_spectral_interface() {
        let material = Lambertian::new(Color::new(0.5, 0.2, 0.1));
        assert!(material.as_spectral().is_none());
    }

    #[test]
    fn spectral_lambertian_exposes_reflectance() {
        let material = SpectralLambertian::from_rgb(Color::new(0.8, 0.4, 0.2));
        let reflectance = material.reflectance_at_nm(550.0);
        assert!((0.0..=1.0).contains(&reflectance));
        assert!(material.as_spectral().is_some());
    }
}
