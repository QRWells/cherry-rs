use std::sync::Arc;

use nalgebra::Point3;

use crate::{intersection::Hit, material::Bsdf, primitive::Primitive, ray::Ray};

pub struct Sphere {
    pub center: Point3<f32>,
    pub radius: f32,
    pub material: Arc<dyn Bsdf>,
}

impl Sphere {
    pub fn new(center: Point3<f32>, radius: f32, material: Arc<dyn Bsdf>) -> Self {
        Self {
            center,
            radius,
            material,
        }
    }
}

impl Primitive for Sphere {
    fn intersect(&self, ray: &Ray) -> Option<Hit> {
        let l = ray.origin - self.center;
        let a = ray.dir.dot(&ray.dir);
        let b = 2.0 * ray.dir.dot(&l);
        let c = l.dot(&l) - self.radius * self.radius;

        let (t0, t1) = crate::math::solve_quadratic(a, b, c)?;
        let distance = if t0 > 0.001 { t0 } else { t1 };
        if distance <= 0.001 {
            return None;
        }

        let point = ray.at(distance);
        let normal = (point - self.center).normalize();

        Some(Hit {
            point,
            normal,
            distance,
            material: Arc::clone(&self.material),
        })
    }
}
