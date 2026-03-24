use nalgebra::{Point3, Vector3};

#[derive(Debug, Clone, Copy)]
pub struct Ray {
    pub origin: Point3<f32>,
    pub dir: Vector3<f32>,
}

impl Ray {
    pub fn new(origin: Point3<f32>, dir: Vector3<f32>) -> Self {
        Self { origin, dir }
    }

    pub fn at(&self, distance: f32) -> Point3<f32> {
        self.origin + self.dir * distance
    }

    pub fn reciprocal_dir(&self) -> Vector3<f32> {
        Vector3::new(1.0 / self.dir.x, 1.0 / self.dir.y, 1.0 / self.dir.z)
    }
}
