use nalgebra::{Point3, Vector3};

use crate::{
    Color,
    spectral::{SampledSpectrum, SpectralCurve, rgb_to_emission_spectrum},
};

#[derive(Debug, Clone, Copy)]
pub struct SpectralLightSample {
    pub direction_to_light: Vector3<f32>,
    pub distance: f32,
    pub irradiance_at_nm: f32,
}

pub trait SpectralLight: Send + Sync {
    fn sample_irradiance_at(
        &self,
        point: Point3<f32>,
        wavelength_nm: f32,
    ) -> Option<SpectralLightSample>;
}

pub trait Light: Send + Sync {
    fn as_spectral(&self) -> Option<&dyn SpectralLight> {
        None
    }
}

pub struct PointSpectralLight {
    position: Point3<f32>,
    intensity: SampledSpectrum,
}

impl PointSpectralLight {
    pub fn new(position: Point3<f32>, intensity: SampledSpectrum) -> Self {
        Self {
            position,
            intensity,
        }
    }

    pub fn from_rgb(position: Point3<f32>, intensity: Color) -> Self {
        Self::new(position, rgb_to_emission_spectrum(intensity))
    }
}

impl SpectralLight for PointSpectralLight {
    fn sample_irradiance_at(
        &self,
        point: Point3<f32>,
        wavelength_nm: f32,
    ) -> Option<SpectralLightSample> {
        let to_light = self.position - point;
        let distance_sq = to_light.norm_squared();
        if distance_sq <= 1e-6 {
            return None;
        }

        let distance = distance_sq.sqrt();
        Some(SpectralLightSample {
            direction_to_light: to_light / distance,
            distance,
            irradiance_at_nm: self.intensity.sample(wavelength_nm).max(0.0) / distance_sq,
        })
    }
}

impl Light for PointSpectralLight {
    fn as_spectral(&self) -> Option<&dyn SpectralLight> {
        Some(self)
    }
}

pub struct DirectionalSpectralLight {
    direction_to_light: Vector3<f32>,
    irradiance: SampledSpectrum,
}

impl DirectionalSpectralLight {
    pub fn new(direction_to_light: Vector3<f32>, irradiance: SampledSpectrum) -> Self {
        Self {
            direction_to_light: direction_to_light.normalize(),
            irradiance,
        }
    }

    pub fn from_rgb(direction_to_light: Vector3<f32>, irradiance: Color) -> Self {
        Self::new(direction_to_light, rgb_to_emission_spectrum(irradiance))
    }
}

impl SpectralLight for DirectionalSpectralLight {
    fn sample_irradiance_at(
        &self,
        _point: Point3<f32>,
        wavelength_nm: f32,
    ) -> Option<SpectralLightSample> {
        Some(SpectralLightSample {
            direction_to_light: self.direction_to_light,
            distance: f32::INFINITY,
            irradiance_at_nm: self.irradiance.sample(wavelength_nm).max(0.0),
        })
    }
}

impl Light for DirectionalSpectralLight {
    fn as_spectral(&self) -> Option<&dyn SpectralLight> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::{DirectionalSpectralLight, PointSpectralLight, SpectralLight};
    use crate::{Color, rgb_to_emission_spectrum};
    use nalgebra::{Point3, Vector3};

    #[test]
    fn point_light_falls_off_with_distance() {
        let light = PointSpectralLight::new(
            Point3::new(0.0, 1.0, 0.0),
            rgb_to_emission_spectrum(Color::new(2.0, 2.0, 2.0)),
        );
        let near = light
            .sample_irradiance_at(Point3::new(0.0, 0.0, 0.0), 550.0)
            .unwrap();
        let far = light
            .sample_irradiance_at(Point3::new(0.0, -3.0, 0.0), 550.0)
            .unwrap();
        assert!(near.irradiance_at_nm > far.irradiance_at_nm);
    }

    #[test]
    fn directional_light_uses_infinite_distance() {
        let light = DirectionalSpectralLight::from_rgb(
            Vector3::new(0.0, -1.0, 0.0),
            Color::new(0.8, 0.8, 0.8),
        );
        let sample = light
            .sample_irradiance_at(Point3::new(0.0, 0.0, 0.0), 550.0)
            .unwrap();
        assert!(sample.distance.is_infinite());
    }
}
