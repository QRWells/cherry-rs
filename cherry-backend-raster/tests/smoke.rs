use std::sync::Arc;

use cherry_backend_raster::{RasterBackend, RasterBackendConfig};
use cherry_core::{
    Camera, Color, Cuboid, FrameRequest, GltfMrBsdf, PointSpectralLight, Primitive, SceneSnapshot,
    Sphere, apply_exposure_reinhard,
};
use cherry_render::{NoopFrameSink, RenderBackend, color_to_rgb8};
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

fn grazing_camera() -> Camera {
    Camera::new(
        Point3::new(1.8, 0.15, 3.4),
        Point3::new(0.0, 0.0, 0.0),
        Vector3::y_axis().into_inner(),
        36.0,
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
        path_tracing: Default::default(),
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
        path_tracing: Default::default(),
    };

    let mut sink = NoopFrameSink;
    let result = backend.render_frame(&scene, &request, &mut sink);

    assert_eq!(
        result.image.get_pixel(0, 0),
        &color_to_rgb8(apply_exposure_reinhard(Color::new(0.1, 0.15, 0.2), 1.0))
    );
}

#[test]
fn raster_backend_preserves_material_channel_on_x_face() {
    let mut scene = SceneSnapshot::new(x_face_camera()).with_background(Color::new(0.0, 0.0, 0.0));
    scene.add_primitive(Arc::new(Cuboid::new(
        Point3::new(-1.0, -1.0, -1.0),
        Point3::new(1.0, 1.0, 1.0),
        Arc::new(GltfMrBsdf::opaque(Color::new(0.0, 1.0, 0.0), 0.0, 0.45)),
    )));
    scene.add_light(Arc::new(PointSpectralLight::from_rgb(
        Point3::new(-2.5, 1.0, 1.5),
        Color::new(8.0, 8.0, 8.0),
    )));

    let backend = RasterBackend::new();
    let request = FrameRequest {
        width: 32,
        height: 32,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 1,
        max_bounces: 1,
        path_tracing: Default::default(),
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

#[test]
fn raster_backend_is_deterministic_across_thread_counts() {
    let material = Arc::new(GltfMrBsdf::opaque(Color::new(0.6, 0.35, 0.2), 0.0, 0.5));
    let sphere = Arc::new(Sphere::new(Point3::new(0.0, 0.0, 0.0), 0.9, material));

    let mut scene = SceneSnapshot::new(test_camera()).with_background(Color::new(0.02, 0.02, 0.02));
    scene.add_primitive(sphere);

    let request = FrameRequest {
        width: 40,
        height: 24,
        frame_index: 2,
        time: 0.0,
        samples_per_pixel: 1,
        max_bounces: 1,
        path_tracing: Default::default(),
    };

    let mut sink = NoopFrameSink;
    let single = RasterBackend::with_threads(Some(1)).render_frame(&scene, &request, &mut sink);
    let mut sink = NoopFrameSink;
    let multi = RasterBackend::with_threads(Some(4)).render_frame(&scene, &request, &mut sink);

    assert_eq!(single.image, multi.image);
}

#[test]
fn raster_backend_uses_authored_lights_for_direct_shading() {
    let material = Arc::new(GltfMrBsdf::opaque(Color::new(0.75, 0.25, 0.2), 0.0, 0.35));
    let sphere: Arc<dyn Primitive> =
        Arc::new(Sphere::new(Point3::new(0.0, 0.0, 0.0), 0.9, material));

    let mut unlit = SceneSnapshot::new(test_camera()).with_background(Color::new(0.0, 0.0, 0.0));
    unlit.add_primitive(Arc::clone(&sphere));

    let mut lit = unlit.clone();
    lit.add_light(Arc::new(PointSpectralLight::from_rgb(
        Point3::new(0.0, 1.7, 2.0),
        Color::new(12.0, 12.0, 12.0),
    )));

    let request = FrameRequest {
        width: 32,
        height: 32,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 1,
        max_bounces: 1,
        path_tracing: Default::default(),
    };

    let mut sink = NoopFrameSink;
    let unlit_result = RasterBackend::new().render_frame(&unlit, &request, &mut sink);
    let mut sink = NoopFrameSink;
    let lit_result = RasterBackend::new().render_frame(&lit, &request, &mut sink);

    let unlit_center = unlit_result.image.get_pixel(16, 16).0;
    let lit_center = lit_result.image.get_pixel(16, 16).0;

    assert!(
        lit_center[0] > unlit_center[0]
            || lit_center[1] > unlit_center[1]
            || lit_center[2] > unlit_center[2],
        "expected authored light to brighten the center pixel, unlit={:?}, lit={:?}",
        unlit_center,
        lit_center
    );
}

#[test]
fn raster_backend_renders_emissive_surfaces_without_scene_lights() {
    let emissive = Arc::new(GltfMrBsdf::new(
        Color::new(0.0, 0.0, 0.0),
        0.0,
        0.3,
        Color::new(2.0, 0.8, 0.1),
        0.0,
        1.5,
    ));
    let sphere = Arc::new(Sphere::new(Point3::new(0.0, 0.0, 0.0), 0.9, emissive));

    let mut scene = SceneSnapshot::new(test_camera()).with_background(Color::new(0.0, 0.0, 0.0));
    scene.add_primitive(sphere);

    let request = FrameRequest {
        width: 32,
        height: 32,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 1,
        max_bounces: 1,
        path_tracing: Default::default(),
    };

    let mut sink = NoopFrameSink;
    let result = RasterBackend::new().render_frame(&scene, &request, &mut sink);
    let center = result.image.get_pixel(16, 16).0;

    assert!(
        center[0] > 0 || center[1] > 0 || center[2] > 0,
        "expected emissive surface to contribute visible radiance, got {:?}",
        center
    );
}

#[test]
fn raster_backend_darkens_shadowed_pixels() {
    let wall_material = Arc::new(GltfMrBsdf::opaque(Color::new(0.75, 0.75, 0.75), 0.0, 0.45));
    let wall: Arc<dyn Primitive> = Arc::new(Cuboid::new(
        Point3::new(-1.1, -1.1, -1.05),
        Point3::new(1.1, 1.1, -0.95),
        wall_material,
    ));

    let mut lit = SceneSnapshot::new(test_camera()).with_background(Color::new(0.0, 0.0, 0.0));
    lit.add_primitive(Arc::clone(&wall));
    lit.add_light(Arc::new(PointSpectralLight::from_rgb(
        Point3::new(0.0, 1.8, 2.0),
        Color::new(14.0, 14.0, 14.0),
    )));

    let mut shadowed = lit.clone();
    shadowed.add_primitive(Arc::new(Cuboid::new(
        Point3::new(-0.2, 0.45, 0.25),
        Point3::new(0.2, 1.05, 0.75),
        Arc::new(GltfMrBsdf::opaque(Color::new(0.1, 0.1, 0.1), 0.0, 0.5)),
    )));

    let request = FrameRequest {
        width: 48,
        height: 48,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 1,
        max_bounces: 1,
        path_tracing: Default::default(),
    };

    let mut sink = NoopFrameSink;
    let lit_result = RasterBackend::new().render_frame(&lit, &request, &mut sink);
    let mut sink = NoopFrameSink;
    let shadowed_result = RasterBackend::new().render_frame(&shadowed, &request, &mut sink);

    let lit_center = lit_result.image.get_pixel(24, 24).0;
    let shadowed_center = shadowed_result.image.get_pixel(24, 24).0;

    let lit_luma = lit_center[0] as u32 + lit_center[1] as u32 + lit_center[2] as u32;
    let shadowed_luma =
        shadowed_center[0] as u32 + shadowed_center[1] as u32 + shadowed_center[2] as u32;

    assert!(
        shadowed_luma < lit_luma,
        "expected blocker to cast a shadow, lit={:?}, shadowed={:?}",
        lit_center,
        shadowed_center
    );
}

#[test]
fn raster_backend_transmissive_preview_reveals_geometry_behind_surface() {
    let glass = Arc::new(GltfMrBsdf::transmissive(
        Color::new(0.98, 0.99, 1.0),
        0.08,
        1.0,
        1.5,
    ));
    let back_wall = Arc::new(GltfMrBsdf::new(
        Color::new(0.85, 0.1, 0.1),
        0.0,
        0.45,
        Color::new(1.5, 0.15, 0.15),
        0.0,
        1.5,
    ));

    let mut scene = SceneSnapshot::new(test_camera()).with_background(Color::new(0.0, 0.0, 0.0));
    scene.add_primitive(Arc::new(Sphere::new(
        Point3::new(0.0, 0.0, 0.0),
        0.8,
        glass,
    )));
    scene.add_primitive(Arc::new(Cuboid::new(
        Point3::new(-1.2, -1.2, -1.05),
        Point3::new(1.2, 1.2, -0.95),
        back_wall,
    )));
    scene.add_light(Arc::new(PointSpectralLight::from_rgb(
        Point3::new(0.0, 1.8, 2.0),
        Color::new(14.0, 14.0, 14.0),
    )));

    let request = FrameRequest {
        width: 48,
        height: 48,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 1,
        max_bounces: 4,
        path_tracing: Default::default(),
    };

    let mut sink = NoopFrameSink;
    let result = RasterBackend::new().render_frame(&scene, &request, &mut sink);
    let center = result.image.get_pixel(24, 24).0;

    assert!(
        center[0] > center[1] && center[0] > center[2],
        "expected transmissive preview to reveal red wall behind glass, got {:?}",
        center
    );
}

#[test]
fn raster_backend_applies_configured_exposure_tone_mapping() {
    let emissive = Arc::new(GltfMrBsdf::new(
        Color::new(0.0, 0.0, 0.0),
        0.0,
        0.2,
        Color::new(8.0, 4.0, 2.0),
        0.0,
        1.5,
    ));
    let sphere = Arc::new(Sphere::new(Point3::new(0.0, 0.0, 0.0), 0.9, emissive));

    let mut scene = SceneSnapshot::new(test_camera()).with_background(Color::new(0.0, 0.0, 0.0));
    scene.add_primitive(sphere);

    let request = FrameRequest {
        width: 32,
        height: 32,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: 1,
        max_bounces: 1,
        path_tracing: Default::default(),
    };

    let mut sink = NoopFrameSink;
    let default_result = RasterBackend::with_config(RasterBackendConfig {
        cpu_threads: None,
        exposure: 1.0,
    })
    .render_frame(&scene, &request, &mut sink);
    let mut sink = NoopFrameSink;
    let darker_result = RasterBackend::with_config(RasterBackendConfig {
        cpu_threads: None,
        exposure: 0.2,
    })
    .render_frame(&scene, &request, &mut sink);

    let default_center = default_result.image.get_pixel(16, 16).0;
    let darker_center = darker_result.image.get_pixel(16, 16).0;

    let default_luma =
        default_center[0] as u32 + default_center[1] as u32 + default_center[2] as u32;
    let darker_luma = darker_center[0] as u32 + darker_center[1] as u32 + darker_center[2] as u32;

    assert!(
        darker_luma < default_luma,
        "expected lower raster exposure to darken the tone-mapped result, default={:?}, darker={:?}",
        default_center,
        darker_center
    );
}

#[test]
fn raster_backend_grazing_transmission_is_deterministic_across_thread_counts() {
    let glass = Arc::new(GltfMrBsdf::transmissive(
        Color::new(0.9, 0.95, 1.0),
        0.12,
        1.0,
        1.52,
    ));
    let wall = Arc::new(GltfMrBsdf::new(
        Color::new(0.2, 0.25, 0.9),
        0.0,
        0.5,
        Color::new(0.4, 0.5, 1.6),
        0.0,
        1.5,
    ));

    let mut scene = SceneSnapshot::new(grazing_camera()).with_background(Color::new(0.0, 0.0, 0.0));
    scene.add_primitive(Arc::new(Sphere::new(
        Point3::new(0.0, 0.0, 0.0),
        0.9,
        glass,
    )));
    scene.add_primitive(Arc::new(Cuboid::new(
        Point3::new(-1.5, -1.2, -1.05),
        Point3::new(1.5, 1.2, -0.95),
        wall,
    )));

    let request = FrameRequest {
        width: 40,
        height: 40,
        frame_index: 3,
        time: 0.0,
        samples_per_pixel: 1,
        max_bounces: 4,
        path_tracing: Default::default(),
    };

    let mut sink = NoopFrameSink;
    let single = RasterBackend::with_threads(Some(1)).render_frame(&scene, &request, &mut sink);
    let mut sink = NoopFrameSink;
    let multi = RasterBackend::with_threads(Some(4)).render_frame(&scene, &request, &mut sink);

    assert_eq!(single.image, multi.image);
}
