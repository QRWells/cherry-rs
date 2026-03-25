use std::sync::Arc;

use crate::{
    camera::Camera,
    color::Color,
    intersection::Hit,
    light::Light,
    primitive::Primitive,
    ray::Ray,
    spectral::{SampledSpectrum, SpectralCurve, rgb_to_emission_at_nm},
};

#[derive(Clone)]
pub struct SceneSnapshot {
    pub camera: Camera,
    pub primitives: Vec<Arc<dyn Primitive>>,
    pub lights: Vec<Arc<dyn Light>>,
    pub background: Color,
    pub spectral_background: Option<SampledSpectrum>,
}

impl SceneSnapshot {
    pub fn new(camera: Camera) -> Self {
        Self {
            camera,
            primitives: Vec::new(),
            lights: Vec::new(),
            background: Color::new(0.0, 0.0, 0.0),
            spectral_background: None,
        }
    }

    pub fn with_background(mut self, color: Color) -> Self {
        self.background = color;
        self.spectral_background = None;
        self
    }

    pub fn with_spectral_background(mut self, background: SampledSpectrum) -> Self {
        self.background = background.to_linear_srgb();
        self.spectral_background = Some(background);
        self
    }

    pub fn add_primitive(&mut self, primitive: Arc<dyn Primitive>) {
        self.primitives.push(primitive);
    }

    pub fn add_light(&mut self, light: Arc<dyn Light>) {
        self.lights.push(light);
    }

    pub fn background_at_nm(&self, wavelength_nm: f32) -> f32 {
        self.spectral_background
            .as_ref()
            .map(|background| background.sample(wavelength_nm))
            .unwrap_or_else(|| rgb_to_emission_at_nm(self.background, wavelength_nm))
    }

    pub fn intersect(&self, ray: &Ray) -> Option<Hit> {
        self.primitives
            .iter()
            .filter_map(|primitive| primitive.intersect(ray))
            .min_by(|a, b| a.distance.total_cmp(&b.distance))
    }
}

pub trait SceneProvider: Send + Sync {
    fn snapshot(&self, time: f32) -> SceneSnapshot;
}

pub struct StaticSceneProvider {
    scene: SceneSnapshot,
}

impl StaticSceneProvider {
    pub fn new(scene: SceneSnapshot) -> Self {
        Self { scene }
    }
}

impl SceneProvider for StaticSceneProvider {
    fn snapshot(&self, _time: f32) -> SceneSnapshot {
        self.scene.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::SceneSnapshot;
    use crate::{Camera, Color, SampledSpectrum, rgb_to_emission_spectrum};
    use nalgebra::{Point3, Vector3};

    fn camera() -> Camera {
        Camera::new(
            Point3::new(0.0, 0.0, 3.0),
            Point3::new(0.0, 0.0, 0.0),
            Vector3::y_axis().into_inner(),
            60.0,
            1.0,
            0.0,
            1.0,
        )
    }

    #[test]
    fn rgb_background_sampling_is_non_negative() {
        let scene = SceneSnapshot::new(camera()).with_background(Color::new(0.1, 0.2, 0.3));
        assert!(scene.background_at_nm(550.0) >= 0.0);
    }

    #[test]
    fn spectral_background_sampling_uses_curve() {
        let spectrum = SampledSpectrum::constant(0.25);
        let scene = SceneSnapshot::new(camera()).with_spectral_background(spectrum.clone());
        assert!((scene.background_at_nm(500.0) - 0.25).abs() < 1e-6);
        assert_eq!(scene.spectral_background, Some(spectrum));
    }

    #[test]
    fn setting_rgb_background_clears_spectral_background() {
        let scene = SceneSnapshot::new(camera())
            .with_spectral_background(rgb_to_emission_spectrum(Color::new(0.3, 0.3, 0.3)))
            .with_background(Color::new(0.05, 0.05, 0.05));
        assert!(scene.spectral_background.is_none());
    }
}
