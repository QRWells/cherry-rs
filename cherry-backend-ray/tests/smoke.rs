use std::sync::Arc;

use cherry_backend_ray::{RayAccel, RayBackend, TraceMethod};
use cherry_core::{Camera, Color, Cuboid, FrameRequest, Lambertian, SceneSnapshot, Sphere};
use cherry_render::{NoopFrameSink, RenderBackend};
use nalgebra::{Point3, Vector2, Vector3};

fn test_camera() -> Camera {
    Camera::new(
        Point3::new(0.0, 0.5, 5.0),
        Point3::new(0.0, 0.0, 0.0),
        Vector3::y_axis().into_inner(),
        45.0,
        1.0,
        0.0,
        1.0,
    )
}

fn test_scene() -> SceneSnapshot {
    let red = Arc::new(Lambertian::new(Color::new(0.8, 0.2, 0.2)));
    let blue = Arc::new(Lambertian::new(Color::new(0.2, 0.3, 0.8)));

    let mut scene = SceneSnapshot::new(test_camera()).with_background(Color::new(0.03, 0.04, 0.05));
    scene.add_primitive(Arc::new(Sphere::new(Point3::new(-0.6, 0.0, 0.0), 0.8, red)));
    scene.add_primitive(Arc::new(Cuboid::new(
        Point3::new(0.2, -0.8, -0.8),
        Point3::new(1.2, 0.2, 0.2),
        blue,
    )));
    scene
}

#[test]
fn brute_force_accel_hits_scene() {
    let scene = test_scene();
    let ray = scene.camera.generate_ray(Vector2::new(0.5, 0.5));
    let accel = cherry_backend_ray::RayBruteForceAccel;

    assert!(accel.intersect(&ray, &scene).is_some());
}

#[test]
fn ray_backend_normal_and_monte_carlo_render() {
    let scene = test_scene();
    let request = FrameRequest {
        width: 32,
        height: 32,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 2,
        max_bounces: 3,
    };

    for method in [TraceMethod::Normal, TraceMethod::MonteCarlo] {
        let backend = RayBackend::with_method(method);
        let mut sink = NoopFrameSink;
        let result = backend.render_frame(&scene, &request, &mut sink);

        assert_eq!(result.image.width(), 32);
        assert_eq!(result.image.height(), 32);
        assert_ne!(result.image.get_pixel(16, 16), &image::Rgb([0, 0, 0]));
    }
}
