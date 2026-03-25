use std::sync::Arc;

use cherry_backend_ray::{RayAccel, RayBackend, SpectralRayBackend, TraceMethod};
use cherry_core::{
    Camera, Color, Cuboid, DirectionalSpectralLight, FrameRequest, Lambertian, PointSpectralLight,
    SceneSnapshot, SpectralLambertian, Sphere, rgb_to_emission_spectrum,
};
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

fn spectral_test_scene() -> SceneSnapshot {
    let spectral_material = Arc::new(SpectralLambertian::from_rgb(Color::new(0.85, 0.4, 0.2)));
    let floor_material = Arc::new(SpectralLambertian::from_rgb(Color::new(0.5, 0.6, 0.8)));

    let mut scene = SceneSnapshot::new(test_camera())
        .with_spectral_background(rgb_to_emission_spectrum(Color::new(0.02, 0.03, 0.04)));

    scene.add_primitive(Arc::new(Sphere::new(
        Point3::new(0.0, 0.0, 0.0),
        0.9,
        spectral_material,
    )));
    scene.add_primitive(Arc::new(Cuboid::new(
        Point3::new(-1.5, -1.2, -1.5),
        Point3::new(1.5, -0.9, 1.5),
        floor_material,
    )));

    scene.add_light(Arc::new(PointSpectralLight::from_rgb(
        Point3::new(1.8, 2.0, 1.4),
        Color::new(7.0, 6.5, 6.0),
    )));
    scene.add_light(Arc::new(DirectionalSpectralLight::from_rgb(
        Vector3::new(-1.0, -1.0, -0.5),
        Color::new(0.4, 0.5, 0.6),
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

#[test]
fn spectral_backend_renders_and_is_deterministic() {
    let scene = spectral_test_scene();
    let request = FrameRequest {
        width: 24,
        height: 24,
        frame_index: 3,
        time: 0.0,
        samples_per_pixel: 3,
        max_bounces: 3,
    };

    let backend = SpectralRayBackend::with_exposure(1.0);

    let mut sink = NoopFrameSink;
    let result = backend.render_frame(&scene, &request, &mut sink);
    assert_ne!(result.image.get_pixel(12, 12), &image::Rgb([0, 0, 0]));

    let typed_a = backend.render_frame_typed(&scene, &request);
    let typed_b = backend.render_frame_typed(&scene, &request);

    assert_eq!(
        typed_a.scanlines[10].pixels[10],
        typed_b.scanlines[10].pixels[10]
    );
}
