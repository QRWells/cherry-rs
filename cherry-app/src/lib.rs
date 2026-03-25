use std::sync::Arc;

use cherry_backend_raster::register_backends as register_raster_backends;
use cherry_backend_ray::register_backends_with_exposure as register_ray_backends_with_exposure;
use cherry_core::{
    Camera, Color, Cuboid, DirectionalSpectralLight, Lambertian, PointSpectralLight, SceneProvider,
    SceneSnapshot, Sphere,
};
use cherry_render::BackendRegistry;
use nalgebra::{Point3, Vector3};

pub struct AnimatedSceneProvider {
    camera: Camera,
    sphere_material: Arc<dyn cherry_core::Material>,
    box_material: Arc<dyn cherry_core::Material>,
}

impl AnimatedSceneProvider {
    pub fn new(aspect_ratio: f32) -> Self {
        let camera = Camera::new(
            Point3::new(0.0, 0.7, 5.0),
            Point3::new(0.0, 0.0, 0.0),
            Vector3::y_axis().into_inner(),
            45.0,
            aspect_ratio,
            0.0,
            1.0,
        );

        Self {
            camera,
            sphere_material: Arc::new(Lambertian::new(Color::new(0.9, 0.3, 0.3))),
            box_material: Arc::new(Lambertian::new(Color::new(0.2, 0.5, 0.9))),
        }
    }
}

impl SceneProvider for AnimatedSceneProvider {
    fn snapshot(&self, time: f32) -> SceneSnapshot {
        let mut scene =
            SceneSnapshot::new(self.camera.clone()).with_background(Color::new(0.05, 0.07, 0.1));

        scene.add_primitive(Arc::new(Sphere::new(
            Point3::new(time.sin() * 0.6, 0.0, 0.0),
            0.8,
            Arc::clone(&self.sphere_material),
        )));

        scene.add_primitive(Arc::new(Cuboid::new(
            Point3::new(-1.3, -1.2, -1.2),
            Point3::new(1.3, -0.9, 1.2),
            Arc::clone(&self.box_material),
        )));

        scene.add_light(Arc::new(PointSpectralLight::from_rgb(
            Point3::new(1.5, 2.0, 1.0),
            Color::new(6.0, 5.5, 5.0),
        )));
        scene.add_light(Arc::new(DirectionalSpectralLight::from_rgb(
            Vector3::new(-1.0, -1.0, -0.4),
            Color::new(0.35, 0.4, 0.5),
        )));

        scene
    }
}

pub fn build_animated_scene_provider(aspect_ratio: f32) -> AnimatedSceneProvider {
    AnimatedSceneProvider::new(aspect_ratio)
}

pub fn build_registry(exposure: f32) -> BackendRegistry {
    let mut registry = BackendRegistry::new();
    register_raster_backends(&mut registry);
    register_ray_backends_with_exposure(&mut registry, exposure);
    registry
}

pub fn output_filename(backend_id: &str, frame_index: Option<u32>) -> String {
    let sanitized = backend_id.replace('.', "-");
    match frame_index {
        Some(index) => format!("{}-{:04}.png", sanitized, index),
        None => format!("{}.png", sanitized),
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use cherry_core::SceneProvider;
    use nalgebra::Vector2;

    use super::output_filename;

    #[test]
    fn filename_for_single_frame_is_deterministic() {
        assert_eq!(output_filename("ray.normal", None), "ray-normal.png");
    }

    #[test]
    fn filename_for_sequence_is_indexed() {
        assert_eq!(
            output_filename("raster.simple", Some(0)),
            "raster-simple-0000.png"
        );
        assert_eq!(
            output_filename("raster.simple", Some(12)),
            "raster-simple-0012.png"
        );
    }

    #[test]
    fn runtime_registry_contains_expected_backends() {
        let registry = super::build_registry(1.0);
        let ids = registry
            .list_ids()
            .into_iter()
            .map(|id| id.as_str().to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            ids,
            vec![
                "raster.simple".to_string(),
                "ray.montecarlo".to_string(),
                "ray.normal".to_string(),
                "ray.spectral".to_string(),
            ]
        );
    }

    #[test]
    fn animated_scene_provider_snapshot_changes_with_time() {
        let provider = super::build_animated_scene_provider(16.0 / 9.0);
        let scene_t0 = provider.snapshot(0.0);
        let scene_t1 = provider.snapshot(FRAC_PI_2);

        assert_eq!(scene_t0.primitives.len(), 2);
        assert_eq!(scene_t1.primitives.len(), 2);
        assert_eq!(scene_t0.lights.len(), 2);
        assert_eq!(scene_t1.lights.len(), 2);

        let ray_t0 = scene_t0.camera.generate_ray(Vector2::new(0.5, 0.5));
        let ray_t1 = scene_t1.camera.generate_ray(Vector2::new(0.5, 0.5));

        let hit_t0 = scene_t0
            .intersect(&ray_t0)
            .expect("scene at t0 has a center hit");
        let hit_t1 = scene_t1
            .intersect(&ray_t1)
            .expect("scene at t1 has a center hit");

        assert!(
            (hit_t0.distance - hit_t1.distance).abs() > 1e-4,
            "expected animated scene to produce different center hit distances"
        );
    }
}
