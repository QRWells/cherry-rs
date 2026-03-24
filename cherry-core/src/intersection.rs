use std::sync::Arc;

use nalgebra::{Point3, Vector3};

use crate::material::Material;

#[derive(Clone)]
pub struct Hit {
    pub point: Point3<f32>,
    pub normal: Vector3<f32>,
    pub distance: f32,
    pub material: Arc<dyn Material>,
}
