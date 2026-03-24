use std::sync::Arc;

use crate::{
    camera::Camera, color::Color, intersection::Hit, light::Light, primitive::Primitive, ray::Ray,
};

#[derive(Clone)]
pub struct SceneSnapshot {
    pub camera: Camera,
    pub primitives: Vec<Arc<dyn Primitive>>,
    pub lights: Vec<Arc<dyn Light>>,
    pub background: Color,
}

impl SceneSnapshot {
    pub fn new(camera: Camera) -> Self {
        Self {
            camera,
            primitives: Vec::new(),
            lights: Vec::new(),
            background: Color::new(0.0, 0.0, 0.0),
        }
    }

    pub fn with_background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    pub fn add_primitive(&mut self, primitive: Arc<dyn Primitive>) {
        self.primitives.push(primitive);
    }

    pub fn add_light(&mut self, light: Arc<dyn Light>) {
        self.lights.push(light);
    }

    pub fn intersect(&self, ray: &Ray) -> Option<Hit> {
        self.primitives
            .iter()
            .filter_map(|primitive| primitive.intersect(ray))
            .min_by(|a, b| a.distance.total_cmp(&b.distance))
    }
}

pub trait SceneProvider: Send + Sync {
    fn snapshot(&self, time: f32) -> SceneSnapshot;
}

pub struct StaticSceneProvider {
    scene: SceneSnapshot,
}

impl StaticSceneProvider {
    pub fn new(scene: SceneSnapshot) -> Self {
        Self { scene }
    }
}

impl SceneProvider for StaticSceneProvider {
    fn snapshot(&self, _time: f32) -> SceneSnapshot {
        self.scene.clone()
    }
}
