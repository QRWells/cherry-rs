use std::sync::Arc;

use cherry_backend_ray::{RayAccel, RayBackend, SpectralRayBackend, TraceMethod};
use cherry_core::{
    Camera, Color, Cuboid, DirectionalSpectralLight, FrameRequest, GltfMrBsdf, PathTracingConfig,
    PointSpectralLight, SceneSnapshot, Sphere, rgb_to_emission_spectrum,
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

fn x_face_camera() -> Camera {
    Camera::new(
        Point3::new(-3.0, 0.0, 0.0),
        Point3::new(0.0, 0.0, 0.0),
        Vector3::y_axis().into_inner(),
        40.0,
        1.0,
        0.0,
        1.0,
    )
}

fn test_scene() -> SceneSnapshot {
    let red = Arc::new(GltfMrBsdf::opaque(Color::new(0.8, 0.2, 0.2), 0.0, 0.55));
    let blue = Arc::new(GltfMrBsdf::opaque(Color::new(0.2, 0.3, 0.8), 0.0, 0.4));

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
    let spectral_material = Arc::new(GltfMrBsdf::new(
        Color::new(0.85, 0.4, 0.2),
        0.1,
        0.3,
        Color::new(0.0, 0.0, 0.0),
        0.4,
        1.45,
    ));
    let floor_material = Arc::new(GltfMrBsdf::opaque(Color::new(0.5, 0.6, 0.8), 0.0, 0.7));

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

fn green_x_face_scene() -> SceneSnapshot {
    let mut scene = SceneSnapshot::new(x_face_camera()).with_background(Color::new(0.0, 0.0, 0.0));
    scene.add_primitive(Arc::new(Cuboid::new(
        Point3::new(-1.0, -1.0, -1.0),
        Point3::new(1.0, 1.0, 1.0),
        Arc::new(GltfMrBsdf::opaque(Color::new(0.0, 1.0, 0.0), 0.0, 0.5)),
    )));
    scene.add_light(Arc::new(PointSpectralLight::from_rgb(
        Point3::new(-3.0, 1.2, 0.4),
        Color::new(1.0, 1.0, 1.0),
    )));
    scene
}

fn spectral_green_x_face_scene() -> SceneSnapshot {
    let mut scene = SceneSnapshot::new(x_face_camera())
        .with_spectral_background(rgb_to_emission_spectrum(Color::new(0.0, 0.0, 0.0)));
    scene.add_primitive(Arc::new(Cuboid::new(
        Point3::new(-1.0, -1.0, -1.0),
        Point3::new(1.0, 1.0, 1.0),
        Arc::new(GltfMrBsdf::opaque(Color::new(0.0, 1.0, 0.0), 0.0, 0.5)),
    )));
    scene.add_light(Arc::new(PointSpectralLight::from_rgb(
        Point3::new(-3.0, 1.2, 0.4),
        Color::new(1.0, 1.0, 1.0),
    )));
    scene
}

fn high_energy_indirect_scene() -> SceneSnapshot {
    let white = Arc::new(GltfMrBsdf::opaque(Color::new(0.9, 0.9, 0.9), 0.0, 0.25));
    let mut scene = SceneSnapshot::new(test_camera()).with_background(Color::new(6.0, 6.0, 6.0));
    scene.add_primitive(Arc::new(Sphere::new(
        Point3::new(0.0, 0.0, 0.0),
        0.9,
        white,
    )));
    scene
}

fn luminance(color: Color) -> f32 {
    0.2126 * color.x + 0.7152 * color.y + 0.0722 * color.z
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
        path_tracing: Default::default(),
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
        path_tracing: Default::default(),
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

#[test]
fn monte_carlo_backend_preserves_material_channel_on_x_face() {
    let scene = green_x_face_scene();
    let request = FrameRequest {
        width: 32,
        height: 32,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 1,
        max_bounces: 1,
        path_tracing: Default::default(),
    };

    let backend = RayBackend::with_method(TraceMethod::MonteCarlo);
    let mut sink = NoopFrameSink;
    let result = backend.render_frame(&scene, &request, &mut sink);
    let center = result.image.get_pixel(16, 16).0;

    assert!(
        center[1] > center[0] && center[1] > center[2],
        "expected green channel dominance on x-face material, got {:?}",
        center
    );
}

#[test]
fn spectral_backend_preserves_material_channel_on_x_face() {
    let scene = spectral_green_x_face_scene();
    let request = FrameRequest {
        width: 32,
        height: 32,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 128,
        max_bounces: 1,
        path_tracing: Default::default(),
    };

    let backend = SpectralRayBackend::with_exposure(1.0);
    let mut sink = NoopFrameSink;
    let result = backend.render_frame(&scene, &request, &mut sink);
    let center = result.image.get_pixel(16, 16).0;

    assert!(
        center[1] > center[0] && center[1] > center[2],
        "expected green channel dominance in spectral path, got {:?}",
        center
    );
}

#[test]
fn ray_backend_is_deterministic_across_thread_counts() {
    let scene = test_scene();
    let request = FrameRequest {
        width: 32,
        height: 20,
        frame_index: 4,
        time: 0.0,
        samples_per_pixel: 3,
        max_bounces: 3,
        path_tracing: PathTracingConfig {
            rr_start_depth: 1,
            rr_min_survival: 0.2,
            indirect_clamp: 1.5,
            direct_lighting: true,
        },
    };

    for method in [TraceMethod::Normal, TraceMethod::MonteCarlo] {
        let backend_single = RayBackend::with_method_and_threads(method, Some(1));
        let backend_multi = RayBackend::with_method_and_threads(method, Some(4));

        let mut sink = NoopFrameSink;
        let single = backend_single.render_frame(&scene, &request, &mut sink);
        let mut sink = NoopFrameSink;
        let multi = backend_multi.render_frame(&scene, &request, &mut sink);

        assert_eq!(single.image, multi.image);
    }
}

#[test]
fn spectral_backend_is_deterministic_across_thread_counts() {
    let scene = spectral_test_scene();
    let request = FrameRequest {
        width: 24,
        height: 24,
        frame_index: 1,
        time: 0.0,
        samples_per_pixel: 4,
        max_bounces: 3,
        path_tracing: PathTracingConfig {
            rr_start_depth: 1,
            rr_min_survival: 0.2,
            indirect_clamp: 1.5,
            direct_lighting: true,
        },
    };

    let backend_single = SpectralRayBackend::with_exposure_and_threads(0.6, Some(1));
    let backend_multi = SpectralRayBackend::with_exposure_and_threads(0.6, Some(4));

    let mut sink = NoopFrameSink;
    let single = backend_single.render_frame(&scene, &request, &mut sink);
    let mut sink = NoopFrameSink;
    let multi = backend_multi.render_frame(&scene, &request, &mut sink);

    assert_eq!(single.image, multi.image);
}

#[test]
fn monte_carlo_backend_honors_direct_lighting_toggle() {
    let scene = green_x_face_scene();
    let base_request = FrameRequest {
        width: 32,
        height: 32,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 4,
        max_bounces: 3,
        path_tracing: PathTracingConfig {
            direct_lighting: true,
            ..PathTracingConfig::default()
        },
    };
    let no_direct_request = FrameRequest {
        path_tracing: PathTracingConfig {
            direct_lighting: false,
            ..base_request.path_tracing.clone()
        },
        ..base_request.clone()
    };

    let backend = RayBackend::with_method(TraceMethod::MonteCarlo);
    let with_direct = backend.render_frame_typed(&scene, &base_request);
    let without_direct = backend.render_frame_typed(&scene, &no_direct_request);

    let on = luminance(with_direct.scanlines[16].pixels[16]);
    let off = luminance(without_direct.scanlines[16].pixels[16]);
    assert!(
        on > off + 0.01,
        "expected direct-lighting toggle to affect luminance, on={on}, off={off}"
    );
}

#[test]
fn monte_carlo_backend_honors_indirect_clamp() {
    let scene = high_energy_indirect_scene();
    let base = FrameRequest {
        width: 24,
        height: 24,
        frame_index: 2,
        time: 0.0,
        samples_per_pixel: 1,
        max_bounces: 4,
        path_tracing: PathTracingConfig {
            rr_start_depth: 8,
            rr_min_survival: 1.0,
            indirect_clamp: 0.0,
            direct_lighting: false,
        },
    };
    let clamped = FrameRequest {
        path_tracing: PathTracingConfig {
            indirect_clamp: 0.01,
            ..base.path_tracing.clone()
        },
        ..base.clone()
    };

    let backend = RayBackend::with_method(TraceMethod::MonteCarlo);
    let unclamped_result = backend.render_frame_typed(&scene, &base);
    let clamped_result = backend.render_frame_typed(&scene, &clamped);
    let unclamped_luma = luminance(unclamped_result.scanlines[12].pixels[12]);
    let clamped_luma = luminance(clamped_result.scanlines[12].pixels[12]);

    assert!(
        unclamped_luma > clamped_luma,
        "expected clamp to reduce indirect energy, unclamped={unclamped_luma}, clamped={clamped_luma}"
    );
}

#[test]
fn spectral_backend_honors_direct_lighting_toggle() {
    let scene = spectral_green_x_face_scene();
    let base_request = FrameRequest {
        width: 32,
        height: 32,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 32,
        max_bounces: 3,
        path_tracing: PathTracingConfig {
            direct_lighting: true,
            ..PathTracingConfig::default()
        },
    };
    let no_direct_request = FrameRequest {
        path_tracing: PathTracingConfig {
            direct_lighting: false,
            ..base_request.path_tracing.clone()
        },
        ..base_request.clone()
    };

    let backend = SpectralRayBackend::with_exposure(1.0);
    let with_direct = backend.render_frame_typed(&scene, &base_request);
    let without_direct = backend.render_frame_typed(&scene, &no_direct_request);

    let on = luminance(with_direct.scanlines[16].pixels[16].color);
    let off = luminance(without_direct.scanlines[16].pixels[16].color);
    assert!(
        on > off + 0.01,
        "expected spectral direct-lighting toggle to affect luminance, on={on}, off={off}"
    );
}
