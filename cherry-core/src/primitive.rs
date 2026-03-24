mod cuboid;
mod sphere;

use crate::{intersection::Hit, ray::Ray};

pub use cuboid::Cuboid;
pub use sphere::Sphere;

pub trait Primitive: Send + Sync {
    fn intersect(&self, ray: &Ray) -> Option<Hit>;
}
