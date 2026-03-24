use nalgebra::{Point3, Vector2, Vector3};

use crate::{math::deg_to_rad, ray::Ray};

#[derive(Debug, Clone)]
pub struct Camera {
    aperture: f32,
    focal_distance: f32,
    u: Vector3<f32>,
    v: Vector3<f32>,
    position: Point3<f32>,
    horizontal: Vector3<f32>,
    vertical: Vector3<f32>,
    top_left: Point3<f32>,
}

impl Camera {
    pub fn new(
        look_from: Point3<f32>,
        look_at: Point3<f32>,
        view_up: Vector3<f32>,
        fov: f32,
        aspect_ratio: f32,
        aperture: f32,
        focal_distance: f32,
    ) -> Self {
        let theta = deg_to_rad(fov);
        let h = (theta / 2.0).tan();
        let viewport_height = 2.0 * h;
        let viewport_width = viewport_height * aspect_ratio;

        let w = (look_from - look_at).normalize();
        let u = view_up.cross(&w).normalize();
        let v = w.cross(&u);

        let horizontal = viewport_width * u * focal_distance;
        let vertical = -viewport_height * v * focal_distance;
        let top_left = look_from - horizontal / 2.0 - vertical / 2.0 - w * focal_distance;

        Self {
            aperture,
            focal_distance,
            u,
            v,
            position: look_from,
            horizontal,
            vertical,
            top_left,
        }
    }

    pub fn generate_ray(&self, uv: Vector2<f32>) -> Ray {
        self.generate_ray_with_lens_sample(uv, Vector2::new(0.0, 0.0))
    }

    pub fn generate_ray_with_lens_sample(
        &self,
        uv: Vector2<f32>,
        lens_sample: Vector2<f32>,
    ) -> Ray {
        let lens_radius = self.aperture / 2.0;
        let offset = self.u * lens_sample.x * lens_radius + self.v * lens_sample.y * lens_radius;
        let target = self.top_left + self.horizontal * uv.x + self.vertical * uv.y;
        let origin = self.position + offset;
        Ray::new(origin, (target - origin).normalize())
    }

    pub fn focal_distance(&self) -> f32 {
        self.focal_distance
    }
}
