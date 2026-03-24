use cherry_core::{Hit, Ray, SceneSnapshot};

pub trait Accel: Send + Sync {
    fn intersect(&self, ray: &Ray, scene: &SceneSnapshot) -> Option<Hit>;
}

pub struct BruteForceAccel;

impl Accel for BruteForceAccel {
    fn intersect(&self, ray: &Ray, scene: &SceneSnapshot) -> Option<Hit> {
        scene.intersect(ray)
    }
}
