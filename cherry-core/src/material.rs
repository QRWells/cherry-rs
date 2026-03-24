use nalgebra::Vector3;

use crate::Color;

pub trait Material: Send + Sync {
    fn albedo(&self) -> Color;
}

pub struct Lambertian {
    pub color: Color,
}

impl Lambertian {
    pub fn new(color: Vector3<f32>) -> Self {
        Self { color }
    }
}

impl Material for Lambertian {
    fn albedo(&self) -> Color {
        self.color
    }
}
