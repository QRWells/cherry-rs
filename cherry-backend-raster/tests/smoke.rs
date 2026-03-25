use std::sync::Arc;

use cherry_backend_raster::RasterBackend;
use cherry_core::{Camera, Color, Cuboid, FrameRequest, GltfMrBsdf, SceneSnapshot, Sphere};
use cherry_render::{NoopFrameSink, RenderBackend};
use nalgebra::{Point3, Vector3};

fn test_camera() -> Camera {
    Camera::new(
        Point3::new(0.0, 0.0, 4.0),
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

#[test]
fn raster_backend_renders_simple_scene() {
    let material = Arc::new(GltfMrBsdf::opaque(Color::new(0.7, 0.2, 0.2), 0.0, 0.5));
    let sphere = Arc::new(Sphere::new(Point3::new(0.0, 0.0, 0.0), 0.9, material));

    let mut scene = SceneSnapshot::new(test_camera()).with_background(Color::new(0.02, 0.02, 0.02));
    scene.add_primitive(sphere);

    let backend = RasterBackend::new();
    let request = FrameRequest {
        width: 32,
        height: 32,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 1,
        max_bounces: 1,
    };

    let mut sink = NoopFrameSink;
    let result = backend.render_frame(&scene, &request, &mut sink);

    assert_eq!(result.image.width(), 32);
    assert_eq!(result.image.height(), 32);
    assert_ne!(result.image.get_pixel(16, 16), &image::Rgb([5, 5, 5]));
}

#[test]
fn raster_backend_handles_empty_scene() {
    let scene = SceneSnapshot::new(test_camera()).with_background(Color::new(0.1, 0.15, 0.2));

    let backend = RasterBackend::new();
    let request = FrameRequest {
        width: 16,
        height: 16,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 1,
        max_bounces: 1,
    };

    let mut sink = NoopFrameSink;
    let result = backend.render_frame(&scene, &request, &mut sink);

    assert_eq!(result.image.get_pixel(0, 0), &image::Rgb([25, 38, 51]));
}

#[test]
fn raster_backend_preserves_material_channel_on_x_face() {
    let mut scene = SceneSnapshot::new(x_face_camera()).with_background(Color::new(0.0, 0.0, 0.0));
    scene.add_primitive(Arc::new(Cuboid::new(
        Point3::new(-1.0, -1.0, -1.0),
        Point3::new(1.0, 1.0, 1.0),
        Arc::new(GltfMrBsdf::opaque(Color::new(0.0, 1.0, 0.0), 0.0, 0.45)),
    )));

    let backend = RasterBackend::new();
    let request = FrameRequest {
        width: 32,
        height: 32,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 1,
        max_bounces: 1,
    };

    let mut sink = NoopFrameSink;
    let result = backend.render_frame(&scene, &request, &mut sink);
    let center = result.image.get_pixel(16, 16).0;

    assert!(
        center[1] > center[0] && center[1] > center[2],
        "expected green channel dominance on x-face material, got {:?}",
        center
    );
}
