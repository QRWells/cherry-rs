use std::sync::Arc;

use cherry_backend_raster::register_backends_with_threads as register_raster_backends_with_threads;
use cherry_backend_ray::register_backends_with_exposure_and_threads as register_ray_backends_with_exposure_and_threads;
use cherry_core::{
    Bsdf, Camera, Color, Cuboid, GltfMrBsdf, PointSpectralLight, SceneProvider, SceneSnapshot,
};
use cherry_render::BackendRegistry;
use nalgebra::{Point3, Vector3};

pub const DEFAULT_SPECTRAL_EXPOSURE: f32 = 0.2;

#[derive(Debug, Clone, Copy)]
pub struct RuntimeRenderConfig {
    pub exposure: f32,
    pub cpu_threads: Option<usize>,
}

impl Default for RuntimeRenderConfig {
    fn default() -> Self {
        Self {
            exposure: DEFAULT_SPECTRAL_EXPOSURE,
            cpu_threads: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CameraConfig {
    pub look_from: Point3<f32>,
    pub look_at: Point3<f32>,
    pub view_up: Vector3<f32>,
    pub fov_degrees: f32,
    pub aperture: f32,
    pub focal_distance: Option<f32>,
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            look_from: Point3::new(0.0, 0.0, 2.6),
            look_at: Point3::new(0.0, -0.1, -0.25),
            view_up: Vector3::new(0.0, 1.0, 0.0),
            fov_degrees: 38.0,
            aperture: 0.0,
            focal_distance: None,
        }
    }
}

impl CameraConfig {
    pub fn resolved_focal_distance(&self) -> f32 {
        self.focal_distance
            .unwrap_or_else(|| (self.look_from - self.look_at).norm())
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self
            .look_from
            .coords
            .iter()
            .all(|component| component.is_finite())
        {
            return Err("camera look_from must contain finite values".to_string());
        }
        if !self
            .look_at
            .coords
            .iter()
            .all(|component| component.is_finite())
        {
            return Err("camera look_at must contain finite values".to_string());
        }
        if !self.view_up.iter().all(|component| component.is_finite()) {
            return Err("camera view_up must contain finite values".to_string());
        }

        if !self.fov_degrees.is_finite() || self.fov_degrees <= 0.0 || self.fov_degrees >= 179.0 {
            return Err("camera fov must be finite and in (0, 179) degrees".to_string());
        }

        if !self.aperture.is_finite() || self.aperture < 0.0 {
            return Err("camera aperture must be finite and >= 0".to_string());
        }

        let view_direction = self.look_at - self.look_from;
        if view_direction.norm_squared() <= 1e-12 {
            return Err("camera look_from and look_at must not be identical".to_string());
        }

        if self.view_up.norm_squared() <= 1e-12 {
            return Err("camera view_up must be non-zero".to_string());
        }

        if self.view_up.cross(&view_direction).norm_squared() <= 1e-12 {
            return Err("camera view_up must not be parallel to viewing direction".to_string());
        }

        if let Some(distance) = self.focal_distance
            && (!distance.is_finite() || distance <= 0.0)
        {
            return Err("camera focal_distance must be finite and > 0".to_string());
        }

        let resolved = self.resolved_focal_distance();
        if !resolved.is_finite() || resolved <= 0.0 {
            return Err("camera focal_distance must resolve to a finite value > 0".to_string());
        }

        Ok(())
    }

    pub fn to_camera(&self, aspect_ratio: f32) -> Result<Camera, String> {
        self.validate()?;
        if !aspect_ratio.is_finite() || aspect_ratio <= 0.0 {
            return Err("camera aspect ratio must be finite and > 0".to_string());
        }

        Ok(Camera::new(
            self.look_from,
            self.look_at,
            self.view_up,
            self.fov_degrees,
            aspect_ratio,
            self.aperture,
            self.resolved_focal_distance(),
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuInitInfo {
    pub adapter_name: String,
    pub backend: String,
    pub device_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuInitError {
    AdapterUnavailable,
    RequestAdapter(String),
    RequestDevice(String),
}

impl std::fmt::Display for GpuInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AdapterUnavailable => write!(f, "no compatible GPU adapter found"),
            Self::RequestAdapter(message) => {
                write!(f, "failed to request GPU adapter: {message}")
            }
            Self::RequestDevice(message) => {
                write!(f, "failed to request GPU device: {message}")
            }
        }
    }
}

impl std::error::Error for GpuInitError {}

trait AdapterRequestExt {
    fn into_adapter_result(self) -> Result<wgpu::Adapter, GpuInitError>;
}

impl AdapterRequestExt for Option<wgpu::Adapter> {
    fn into_adapter_result(self) -> Result<wgpu::Adapter, GpuInitError> {
        self.ok_or(GpuInitError::AdapterUnavailable)
    }
}

impl<E> AdapterRequestExt for Result<wgpu::Adapter, E>
where
    E: std::fmt::Display,
{
    fn into_adapter_result(self) -> Result<wgpu::Adapter, GpuInitError> {
        self.map_err(|error| GpuInitError::RequestAdapter(error.to_string()))
    }
}

pub struct AnimatedSceneProvider {
    camera_config: CameraConfig,
    aspect_ratio: f32,
    white_material: Arc<dyn Bsdf>,
    red_material: Arc<dyn Bsdf>,
    green_material: Arc<dyn Bsdf>,
    metal_material: Arc<dyn Bsdf>,
    glass_material: Arc<dyn Bsdf>,
}

impl AnimatedSceneProvider {
    pub fn new(aspect_ratio: f32) -> Self {
        Self::with_camera_config(aspect_ratio, CameraConfig::default())
            .expect("default camera config must be valid")
    }

    pub fn with_camera_config(
        aspect_ratio: f32,
        camera_config: CameraConfig,
    ) -> Result<Self, String> {
        camera_config.to_camera(aspect_ratio)?;

        Ok(Self {
            camera_config,
            aspect_ratio,
            white_material: Arc::new(GltfMrBsdf::opaque(Color::new(0.73, 0.73, 0.73), 0.0, 0.55)),
            red_material: Arc::new(GltfMrBsdf::opaque(Color::new(0.63, 0.07, 0.06), 0.0, 0.6)),
            green_material: Arc::new(GltfMrBsdf::opaque(Color::new(0.14, 0.45, 0.09), 0.0, 0.6)),
            metal_material: Arc::new(GltfMrBsdf::new(
                Color::new(0.82, 0.82, 0.8),
                1.0,
                0.2,
                Color::new(0.0, 0.0, 0.0),
                0.0,
                1.5,
            )),
            glass_material: Arc::new(GltfMrBsdf::transmissive(
                Color::new(0.95, 0.97, 1.0),
                0.08,
                1.0,
                1.5,
            )),
        })
    }

    fn camera(&self) -> Camera {
        self.camera_config
            .to_camera(self.aspect_ratio)
            .expect("camera config should be validated at provider construction")
    }
}

impl SceneProvider for AnimatedSceneProvider {
    fn snapshot(&self, _time: f32) -> SceneSnapshot {
        let mut scene =
            SceneSnapshot::new(self.camera()).with_background(Color::new(0.0, 0.0, 0.0));

        scene.add_primitive(Arc::new(Cuboid::new(
            Point3::new(-1.0, -1.0, -1.0),
            Point3::new(1.0, -0.98, 1.0),
            Arc::clone(&self.white_material),
        )));
        scene.add_primitive(Arc::new(Cuboid::new(
            Point3::new(-1.0, 0.98, -1.0),
            Point3::new(1.0, 1.0, 1.0),
            Arc::clone(&self.white_material),
        )));
        scene.add_primitive(Arc::new(Cuboid::new(
            Point3::new(-1.0, -1.0, -1.0),
            Point3::new(-0.98, 1.0, 1.0),
            Arc::clone(&self.red_material),
        )));
        scene.add_primitive(Arc::new(Cuboid::new(
            Point3::new(0.98, -1.0, -1.0),
            Point3::new(1.0, 1.0, 1.0),
            Arc::clone(&self.green_material),
        )));
        scene.add_primitive(Arc::new(Cuboid::new(
            Point3::new(-1.0, -1.0, -1.0),
            Point3::new(1.0, 1.0, -0.98),
            Arc::clone(&self.white_material),
        )));
        scene.add_primitive(Arc::new(Cuboid::new(
            Point3::new(-0.65, -1.0, -0.35),
            Point3::new(-0.1, -0.2, 0.3),
            Arc::clone(&self.metal_material),
        )));
        scene.add_primitive(Arc::new(Cuboid::new(
            Point3::new(0.2, -1.0, -0.7),
            Point3::new(0.7, 0.55, -0.1),
            Arc::clone(&self.glass_material),
        )));

        scene.add_light(Arc::new(PointSpectralLight::from_rgb(
            Point3::new(0.0, 0.85, 0.0),
            Color::new(1.2, 1.2, 1.2),
        )));

        scene
    }
}

pub fn build_animated_scene_provider(aspect_ratio: f32) -> AnimatedSceneProvider {
    AnimatedSceneProvider::new(aspect_ratio)
}

pub fn build_animated_scene_provider_with_camera(
    aspect_ratio: f32,
    camera_config: CameraConfig,
) -> Result<AnimatedSceneProvider, String> {
    AnimatedSceneProvider::with_camera_config(aspect_ratio, camera_config)
}

pub fn build_registry(exposure: f32) -> BackendRegistry {
    build_registry_with_config(RuntimeRenderConfig {
        exposure,
        cpu_threads: None,
    })
}

pub fn build_registry_with_config(config: RuntimeRenderConfig) -> BackendRegistry {
    let mut registry = BackendRegistry::new();
    register_raster_backends_with_threads(&mut registry, config.cpu_threads);
    register_ray_backends_with_exposure_and_threads(
        &mut registry,
        config.exposure,
        config.cpu_threads,
    );
    registry
}

pub fn initialize_gpu() -> Result<GpuInitInfo, GpuInitError> {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .into_adapter_result()?;
    let info = adapter.get_info();

    let descriptor = wgpu::DeviceDescriptor {
        label: Some("cherry-gpu-init-device"),
        ..Default::default()
    };
    let _ = pollster::block_on(adapter.request_device(&descriptor, None))
        .map_err(|error| GpuInitError::RequestDevice(error.to_string()))?;

    Ok(GpuInitInfo {
        adapter_name: info.name,
        backend: format!("{:?}", info.backend),
        device_type: format!("{:?}", info.device_type),
    })
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
    use cherry_core::{Color, SceneProvider};
    use nalgebra::{Point3, Vector2, Vector3};

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
    fn runtime_registry_with_config_contains_expected_backends() {
        let registry = super::build_registry_with_config(super::RuntimeRenderConfig {
            exposure: 0.35,
            cpu_threads: Some(2),
        });
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
    fn animated_scene_provider_snapshot_is_static_cornell_box() {
        let provider = super::build_animated_scene_provider(16.0 / 9.0);
        let scene_t0 = provider.snapshot(0.0);
        let scene_t1 = provider.snapshot(2.5);

        assert_eq!(scene_t0.primitives.len(), 7);
        assert_eq!(scene_t1.primitives.len(), 7);
        assert_eq!(scene_t0.lights.len(), 1);
        assert_eq!(scene_t1.lights.len(), 1);
        assert_eq!(scene_t0.background, Color::new(0.0, 0.0, 0.0));
        assert_eq!(scene_t1.background, Color::new(0.0, 0.0, 0.0));

        let ray_t0 = scene_t0.camera.generate_ray(Vector2::new(0.5, 0.5));
        let ray_t1 = scene_t1.camera.generate_ray(Vector2::new(0.5, 0.5));

        let hit_t0 = scene_t0
            .intersect(&ray_t0)
            .expect("scene at t0 has a center hit");
        let hit_t1 = scene_t1
            .intersect(&ray_t1)
            .expect("scene at t1 has a center hit");

        assert!(
            (hit_t0.distance - hit_t1.distance).abs() <= 1e-5,
            "expected static Cornell default scene to produce identical center hit distances"
        );
        assert!(hit_t0.distance > 0.0);
    }

    #[test]
    fn default_camera_config_matches_current_scene_defaults() {
        let config = super::CameraConfig::default();
        assert_eq!(config.look_from, Point3::new(0.0, 0.0, 2.6));
        assert_eq!(config.look_at, Point3::new(0.0, -0.1, -0.25));
        assert_eq!(config.view_up, Vector3::new(0.0, 1.0, 0.0));
        assert!((config.fov_degrees - 38.0).abs() < f32::EPSILON);
        assert!((config.aperture - 0.0).abs() < f32::EPSILON);
        assert!(config.focal_distance.is_none());
    }

    #[test]
    fn camera_config_auto_focal_distance_uses_look_from_to_look_at_distance() {
        let config = super::CameraConfig {
            look_from: Point3::new(0.0, 0.0, 3.0),
            look_at: Point3::new(0.0, 0.0, 0.0),
            view_up: Vector3::new(0.0, 1.0, 0.0),
            fov_degrees: 45.0,
            aperture: 0.1,
            focal_distance: None,
        };

        let camera = config
            .to_camera(1.0)
            .expect("camera config should be valid");
        assert!((camera.focal_distance() - 3.0).abs() < 1e-5);
    }

    #[test]
    fn build_animated_scene_provider_with_camera_preserves_explicit_focal_distance() {
        let config = super::CameraConfig {
            focal_distance: Some(1.75),
            ..super::CameraConfig::default()
        };
        let provider = super::build_animated_scene_provider_with_camera(16.0 / 9.0, config)
            .expect("camera configuration should be valid");
        let scene = provider.snapshot(0.0);

        assert!((scene.camera.focal_distance() - 1.75).abs() < 1e-5);
    }
}
