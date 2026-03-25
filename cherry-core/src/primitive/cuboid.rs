use std::sync::Arc;

use nalgebra::{Point3, Vector3};

use crate::{intersection::Hit, material::Bsdf, primitive::Primitive, ray::Ray};

pub struct Cuboid {
    pub min: Point3<f32>,
    pub max: Point3<f32>,
    pub material: Arc<dyn Bsdf>,
}

impl Cuboid {
    pub fn new(min: Point3<f32>, max: Point3<f32>, material: Arc<dyn Bsdf>) -> Self {
        Self { min, max, material }
    }
}

impl Primitive for Cuboid {
    fn intersect(&self, ray: &Ray) -> Option<Hit> {
        let inv = ray.reciprocal_dir();
        let t_min = (self.min - ray.origin).component_mul(&inv);
        let t_max = (self.max - ray.origin).component_mul(&inv);

        let t0 = Vector3::new(
            t_min.x.min(t_max.x),
            t_min.y.min(t_max.y),
            t_min.z.min(t_max.z),
        );
        let t1 = Vector3::new(
            t_min.x.max(t_max.x),
            t_min.y.max(t_max.y),
            t_min.z.max(t_max.z),
        );

        let t_enter = t0.x.max(t0.y).max(t0.z);
        let t_exit = t1.x.min(t1.y).min(t1.z);
        if t_enter >= t_exit || t_exit <= 0.001 {
            return None;
        }

        let distance = if t_enter > 0.001 { t_enter } else { t_exit };
        if distance <= 0.001 {
            return None;
        }

        let point = ray.at(distance);
        let eps = 1e-3;
        let normal = if (point.x - self.min.x).abs() < eps {
            Vector3::new(-1.0, 0.0, 0.0)
        } else if (point.x - self.max.x).abs() < eps {
            Vector3::new(1.0, 0.0, 0.0)
        } else if (point.y - self.min.y).abs() < eps {
            Vector3::new(0.0, -1.0, 0.0)
        } else if (point.y - self.max.y).abs() < eps {
            Vector3::new(0.0, 1.0, 0.0)
        } else if (point.z - self.min.z).abs() < eps {
            Vector3::new(0.0, 0.0, -1.0)
        } else if (point.z - self.max.z).abs() < eps {
            Vector3::new(0.0, 0.0, 1.0)
        } else {
            return None;
        };

        Some(Hit {
            point,
            normal,
            distance,
            material: Arc::clone(&self.material),
        })
    }
}
