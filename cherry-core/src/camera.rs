use nalgebra::{Point3, Vector2, Vector3};

use crate::{math::deg_to_rad, ray::Ray};

#[derive(Debug, Clone)]
pub struct Camera {
    look_from: Point3<f32>,
    look_at: Point3<f32>,
    view_up: Vector3<f32>,
    fov_degrees: f32,
    aspect_ratio: f32,
    aperture: f32,
    focal_distance: f32,
    u: Vector3<f32>,
    v: Vector3<f32>,
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
            look_from,
            look_at,
            view_up,
            fov_degrees: fov,
            aspect_ratio,
            aperture,
            focal_distance,
            u,
            v,
            horizontal,
            vertical,
            top_left,
        }
    }

    pub fn with_aspect_ratio(&self, aspect_ratio: f32) -> Self {
        Self::new(
            self.look_from,
            self.look_at,
            self.view_up,
            self.fov_degrees,
            aspect_ratio,
            self.aperture,
            self.focal_distance,
        )
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
        let origin = self.look_from + offset;
        Ray::new(origin, (target - origin).normalize())
    }

    pub fn look_from(&self) -> Point3<f32> {
        self.look_from
    }

    pub fn look_at(&self) -> Point3<f32> {
        self.look_at
    }

    pub fn view_up(&self) -> Vector3<f32> {
        self.view_up
    }

    pub fn fov_degrees(&self) -> f32 {
        self.fov_degrees
    }

    pub fn aspect_ratio(&self) -> f32 {
        self.aspect_ratio
    }

    pub fn aperture(&self) -> f32 {
        self.aperture
    }

    pub fn focal_distance(&self) -> f32 {
        self.focal_distance
    }
}

#[cfg(test)]
mod tests {
    use super::Camera;
    use nalgebra::{Point3, Vector2, Vector3};

    fn test_camera() -> Camera {
        Camera::new(
            Point3::new(0.0, 0.0, 3.0),
            Point3::new(0.0, 0.0, 0.0),
            Vector3::y_axis().into_inner(),
            60.0,
            16.0 / 9.0,
            0.0,
            3.0,
        )
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-5
    }

    #[test]
    fn reprojecting_camera_changes_edge_rays_for_new_aspect() {
        let wide = test_camera();
        let square = wide.with_aspect_ratio(1.0);

        let wide_center = wide.generate_ray(Vector2::new(0.5, 0.5));
        let square_center = square.generate_ray(Vector2::new(0.5, 0.5));
        assert!(approx(wide_center.dir.x, square_center.dir.x));
        assert!(approx(wide_center.dir.y, square_center.dir.y));
        assert!(approx(wide_center.dir.z, square_center.dir.z));

        let wide_right = wide.generate_ray(Vector2::new(1.0, 0.5));
        let square_right = square.generate_ray(Vector2::new(1.0, 0.5));
        assert!(
            (wide_right.dir.x - square_right.dir.x).abs() > 1e-3,
            "expected edge ray to change when aspect ratio changes"
        );
    }

    #[test]
    fn with_aspect_ratio_preserves_canonical_camera_parameters() {
        let camera = test_camera();
        let reproj = camera.with_aspect_ratio(4.0 / 3.0);

        assert_eq!(camera.look_from(), reproj.look_from());
        assert_eq!(camera.look_at(), reproj.look_at());
        assert_eq!(camera.view_up(), reproj.view_up());
        assert!(approx(camera.fov_degrees(), reproj.fov_degrees()));
        assert!(approx(camera.aperture(), reproj.aperture()));
        assert!(approx(camera.focal_distance(), reproj.focal_distance()));
    }
}
