use std::{env, path::PathBuf, sync::Arc};

use cherry_app::output_filename;
use cherry_backend_raster::register_backends as register_raster_backends;
use cherry_backend_ray::register_backends as register_ray_backends;
use cherry_core::{
    Camera, Color, Cuboid, FrameRequest, Lambertian, SceneProvider, SceneSnapshot, Sphere,
};
use cherry_render::{
    render_frame, render_sequence, BackendId, BackendRegistry, NoopFrameSink, SequenceSpec,
};
use nalgebra::{Point3, Vector3};

struct AppConfig {
    backend: String,
    width: u32,
    height: u32,
    frames: u32,
    samples_per_pixel: u32,
    max_bounces: u32,
    output_dir: PathBuf,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            backend: "ray.normal".to_string(),
            width: 320,
            height: 180,
            frames: 1,
            samples_per_pixel: 1,
            max_bounces: 3,
            output_dir: PathBuf::from("output"),
        }
    }
}

impl AppConfig {
    fn from_args() -> Self {
        let mut config = Self::default();

        for arg in env::args().skip(1) {
            if let Some(value) = arg.strip_prefix("--backend=") {
                config.backend = value.to_string();
            } else if let Some(value) = arg.strip_prefix("--width=") {
                config.width = value.parse().unwrap_or(config.width);
            } else if let Some(value) = arg.strip_prefix("--height=") {
                config.height = value.parse().unwrap_or(config.height);
            } else if let Some(value) = arg.strip_prefix("--frames=") {
                config.frames = value.parse().unwrap_or(config.frames);
            } else if let Some(value) = arg.strip_prefix("--spp=") {
                config.samples_per_pixel = value.parse().unwrap_or(config.samples_per_pixel);
            } else if let Some(value) = arg.strip_prefix("--max-bounces=") {
                config.max_bounces = value.parse().unwrap_or(config.max_bounces);
            } else if let Some(value) = arg.strip_prefix("--output-dir=") {
                config.output_dir = PathBuf::from(value);
            }
        }

        config
    }
}

struct AnimatedSceneProvider {
    camera: Camera,
    sphere_material: Arc<dyn cherry_core::Material>,
    box_material: Arc<dyn cherry_core::Material>,
}

impl AnimatedSceneProvider {
    fn new(aspect_ratio: f32) -> Self {
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

        scene
    }
}

fn build_registry() -> BackendRegistry {
    let mut registry = BackendRegistry::new();
    register_raster_backends(&mut registry);
    register_ray_backends(&mut registry);
    registry
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::from_args();
    std::fs::create_dir_all(&config.output_dir)?;

    let registry = build_registry();
    let provider = AnimatedSceneProvider::new(config.width as f32 / config.height as f32);

    let backend_id = BackendId::new(config.backend.clone());
    let request = FrameRequest {
        width: config.width,
        height: config.height,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: config.samples_per_pixel,
        max_bounces: config.max_bounces,
    };

    if config.frames <= 1 {
        let mut sink = NoopFrameSink;
        let result = render_frame(&registry, &provider, &backend_id, &request, &mut sink)?;
        let output = config
            .output_dir
            .join(output_filename(backend_id.as_str(), None));
        result.image.save(&output)?;
        println!("Rendered {}", output.display());
        return Ok(());
    }

    let sequence = SequenceSpec {
        frame_count: config.frames,
        start_time: 0.0,
        frame_time_step: 1.0 / 24.0,
        template: request,
    };

    let results = render_sequence(
        &registry,
        &provider,
        &backend_id,
        &sequence,
        |_frame, _request| Box::new(NoopFrameSink),
    )?;

    for result in results {
        let output = config.output_dir.join(output_filename(
            backend_id.as_str(),
            Some(result.stats.frame_index),
        ));
        result.image.save(&output)?;
        println!("Rendered {}", output.display());
    }

    Ok(())
}
