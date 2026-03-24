use std::sync::Arc;

mod cli;
mod progress;

use cherry_app::output_filename;
use cherry_backend_raster::register_backends as register_raster_backends;
use cherry_backend_ray::register_backends as register_ray_backends;
use cherry_core::{
    Camera, Color, Cuboid, FrameRequest, Lambertian, SceneProvider, SceneSnapshot, Sphere,
};
use cherry_render::{BackendId, BackendRegistry, SequenceSpec, render_frame, render_sequence};
use clap::Parser;
use cli::{Cli, validate_backend};
use nalgebra::{Point3, Vector3};
use progress::CliProgressSink;

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
    let cli = Cli::parse();

    if let Some(command) = cli.command {
        println!("{}", command.todo_message());
        return Ok(());
    }

    let registry = build_registry();
    let available_backends = registry
        .list_ids()
        .into_iter()
        .map(|id| id.as_str().to_string())
        .collect::<Vec<_>>();
    if let Err(error) = validate_backend(&cli.backend, &available_backends) {
        error.exit();
    }

    std::fs::create_dir_all(&cli.output_dir)?;

    let provider = AnimatedSceneProvider::new(cli.width as f32 / cli.height as f32);

    let backend_id = BackendId::new(cli.backend.clone());
    let request = FrameRequest {
        width: cli.width,
        height: cli.height,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: cli.samples_per_pixel,
        max_bounces: cli.max_bounces,
    };

    if cli.frames <= 1 {
        let mut sink = CliProgressSink::new(0, 1);
        let result = render_frame(&registry, &provider, &backend_id, &request, &mut sink)?;
        let output = cli
            .output_dir
            .join(output_filename(backend_id.as_str(), None));
        result.image.save(&output)?;
        println!("Rendered {}", output.display());
        return Ok(());
    }

    let sequence = SequenceSpec {
        frame_count: cli.frames,
        start_time: 0.0,
        frame_time_step: 1.0 / 24.0,
        template: request,
    };

    let total_frames = cli.frames;
    let results = render_sequence(
        &registry,
        &provider,
        &backend_id,
        &sequence,
        |frame, _request| Box::new(CliProgressSink::new(frame, total_frames)),
    )?;

    for result in results {
        let output = cli.output_dir.join(output_filename(
            backend_id.as_str(),
            Some(result.stats.frame_index),
        ));
        result.image.save(&output)?;
        println!("Rendered {}", output.display());
    }

    Ok(())
}
