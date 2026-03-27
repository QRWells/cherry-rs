use std::sync::Arc;

use cherry_core::{
    Bsdf, Camera, Color, Cuboid, DirectionalSpectralLight, GltfMrBsdf, Light, PointSpectralLight,
    Primitive, SceneSnapshot, Sphere,
};
use nalgebra::{Point3, Vector3};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneSelection {
    Object(u64),
    Light(u64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuthoredScene {
    pub background: Color,
    pub objects: Vec<AuthoredObject>,
    pub lights: Vec<AuthoredLight>,
    next_id: u64,
}

impl Default for AuthoredScene {
    fn default() -> Self {
        let white = AuthoredMaterial::opaque(Color::new(0.73, 0.73, 0.73), 0.0, 0.55);
        let red = AuthoredMaterial::opaque(Color::new(0.63, 0.07, 0.06), 0.0, 0.6);
        let green = AuthoredMaterial::opaque(Color::new(0.14, 0.45, 0.09), 0.0, 0.6);
        let metal = AuthoredMaterial::new(
            Color::new(0.82, 0.82, 0.8),
            1.0,
            0.2,
            Color::new(0.0, 0.0, 0.0),
            0.0,
            1.5,
        );
        let glass = AuthoredMaterial::transmissive(Color::new(0.95, 0.97, 1.0), 0.08, 1.0, 1.5);

        let mut scene = Self {
            background: Color::new(0.0, 0.0, 0.0),
            objects: Vec::new(),
            lights: Vec::new(),
            next_id: 1,
        };

        scene.add_object(
            "Floor".to_string(),
            AuthoredObjectKind::Cuboid {
                min: Point3::new(-1.0, -1.0, -1.0),
                max: Point3::new(1.0, -0.98, 1.0),
            },
            white.clone(),
        );
        scene.add_object(
            "Ceiling".to_string(),
            AuthoredObjectKind::Cuboid {
                min: Point3::new(-1.0, 0.98, -1.0),
                max: Point3::new(1.0, 1.0, 1.0),
            },
            white.clone(),
        );
        scene.add_object(
            "Left Wall".to_string(),
            AuthoredObjectKind::Cuboid {
                min: Point3::new(-1.0, -1.0, -1.0),
                max: Point3::new(-0.98, 1.0, 1.0),
            },
            red,
        );
        scene.add_object(
            "Right Wall".to_string(),
            AuthoredObjectKind::Cuboid {
                min: Point3::new(0.98, -1.0, -1.0),
                max: Point3::new(1.0, 1.0, 1.0),
            },
            green,
        );
        scene.add_object(
            "Back Wall".to_string(),
            AuthoredObjectKind::Cuboid {
                min: Point3::new(-1.0, -1.0, -1.0),
                max: Point3::new(1.0, 1.0, -0.98),
            },
            white.clone(),
        );
        scene.add_object(
            "Metal Block".to_string(),
            AuthoredObjectKind::Cuboid {
                min: Point3::new(-0.65, -1.0, -0.35),
                max: Point3::new(-0.1, -0.2, 0.3),
            },
            metal,
        );
        scene.add_object(
            "Glass Block".to_string(),
            AuthoredObjectKind::Cuboid {
                min: Point3::new(0.2, -1.0, -0.7),
                max: Point3::new(0.7, 0.55, -0.1),
            },
            glass,
        );
        scene.add_light(
            "Ceiling Light".to_string(),
            AuthoredLightKind::Point {
                position: Point3::new(0.0, 0.85, 0.0),
                intensity: Color::new(1.2, 1.2, 1.2),
            },
        );

        scene
    }
}

impl AuthoredScene {
    pub fn add_object(
        &mut self,
        name: String,
        kind: AuthoredObjectKind,
        material: AuthoredMaterial,
    ) -> u64 {
        let id = self.allocate_id();
        self.objects.push(AuthoredObject {
            id,
            name,
            kind,
            material,
        });
        id
    }

    pub fn add_light(&mut self, name: String, kind: AuthoredLightKind) -> u64 {
        let id = self.allocate_id();
        self.lights.push(AuthoredLight { id, name, kind });
        id
    }

    pub fn remove_object(&mut self, id: u64) -> bool {
        if let Some(index) = self.objects.iter().position(|object| object.id == id) {
            self.objects.remove(index);
            true
        } else {
            false
        }
    }

    pub fn remove_light(&mut self, id: u64) -> bool {
        if let Some(index) = self.lights.iter().position(|light| light.id == id) {
            self.lights.remove(index);
            true
        } else {
            false
        }
    }

    pub fn object_mut(&mut self, id: u64) -> Option<&mut AuthoredObject> {
        self.objects.iter_mut().find(|object| object.id == id)
    }

    pub fn light_mut(&mut self, id: u64) -> Option<&mut AuthoredLight> {
        self.lights.iter_mut().find(|light| light.id == id)
    }

    pub fn to_snapshot(&self, camera: Camera) -> Result<SceneSnapshot, String> {
        validate_color("scene background", &self.background)?;

        let mut scene = SceneSnapshot::new(camera).with_background(self.background);
        for object in &self.objects {
            scene.add_primitive(object.to_primitive()?);
        }
        for light in &self.lights {
            scene.add_light(light.to_light()?);
        }
        Ok(scene)
    }

    fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuthoredObject {
    pub id: u64,
    pub name: String,
    pub kind: AuthoredObjectKind,
    pub material: AuthoredMaterial,
}

impl AuthoredObject {
    pub fn kind_label(&self) -> &'static str {
        self.kind.label()
    }

    fn to_primitive(&self) -> Result<Arc<dyn Primitive>, String> {
        let material = self.material.to_bsdf()?;
        match self.kind {
            AuthoredObjectKind::Cuboid { min, max } => {
                validate_point3("cuboid min", &min)?;
                validate_point3("cuboid max", &max)?;
                if !(min.x < max.x && min.y < max.y && min.z < max.z) {
                    return Err(format!(
                        "object '{}' has invalid cuboid min/max bounds",
                        self.name
                    ));
                }
                let primitive: Arc<dyn Primitive> = Arc::new(Cuboid::new(min, max, material));
                Ok(primitive)
            }
            AuthoredObjectKind::Sphere { center, radius } => {
                validate_point3("sphere center", &center)?;
                if !radius.is_finite() || radius <= 0.0 {
                    return Err(format!(
                        "object '{}' sphere radius must be finite and > 0",
                        self.name
                    ));
                }
                let primitive: Arc<dyn Primitive> = Arc::new(Sphere::new(center, radius, material));
                Ok(primitive)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AuthoredObjectKind {
    Cuboid { min: Point3<f32>, max: Point3<f32> },
    Sphere { center: Point3<f32>, radius: f32 },
}

impl AuthoredObjectKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Cuboid { .. } => "Cuboid",
            Self::Sphere { .. } => "Sphere",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuthoredLight {
    pub id: u64,
    pub name: String,
    pub kind: AuthoredLightKind,
}

impl AuthoredLight {
    pub fn kind_label(&self) -> &'static str {
        self.kind.label()
    }

    fn to_light(&self) -> Result<Arc<dyn Light>, String> {
        match self.kind {
            AuthoredLightKind::Point {
                position,
                intensity,
            } => {
                validate_point3("point light position", &position)?;
                validate_color("point light intensity", &intensity)?;
                let light: Arc<dyn Light> =
                    Arc::new(PointSpectralLight::from_rgb(position, intensity));
                Ok(light)
            }
            AuthoredLightKind::Directional {
                direction,
                intensity,
            } => {
                validate_vector3("directional light direction", &direction)?;
                validate_color("directional light intensity", &intensity)?;
                if direction.norm_squared() <= 1e-12 {
                    return Err(format!("light '{}' direction must be non-zero", self.name));
                }
                let light: Arc<dyn Light> =
                    Arc::new(DirectionalSpectralLight::from_rgb(direction, intensity));
                Ok(light)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AuthoredLightKind {
    Point {
        position: Point3<f32>,
        intensity: Color,
    },
    Directional {
        direction: Vector3<f32>,
        intensity: Color,
    },
}

impl AuthoredLightKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Point { .. } => "Point Light",
            Self::Directional { .. } => "Directional Light",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuthoredMaterial {
    pub base_color: Color,
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: Color,
    pub transmission: f32,
    pub ior: f32,
}

impl AuthoredMaterial {
    pub fn new(
        base_color: Color,
        metallic: f32,
        roughness: f32,
        emissive: Color,
        transmission: f32,
        ior: f32,
    ) -> Self {
        Self {
            base_color,
            metallic,
            roughness,
            emissive,
            transmission,
            ior,
        }
    }

    pub fn opaque(base_color: Color, metallic: f32, roughness: f32) -> Self {
        Self::new(
            base_color,
            metallic,
            roughness,
            Color::new(0.0, 0.0, 0.0),
            0.0,
            1.5,
        )
    }

    pub fn transmissive(base_color: Color, roughness: f32, transmission: f32, ior: f32) -> Self {
        Self::new(
            base_color,
            0.0,
            roughness,
            Color::new(0.0, 0.0, 0.0),
            transmission,
            ior,
        )
    }

    fn to_bsdf(&self) -> Result<Arc<dyn Bsdf>, String> {
        self.validate()?;
        let material: Arc<dyn Bsdf> = Arc::new(GltfMrBsdf::new(
            self.base_color,
            self.metallic,
            self.roughness,
            self.emissive,
            self.transmission,
            self.ior,
        ));
        Ok(material)
    }

    fn validate(&self) -> Result<(), String> {
        validate_color("material base_color", &self.base_color)?;
        validate_color("material emissive", &self.emissive)?;
        validate_scalar("material metallic", self.metallic)?;
        validate_scalar("material roughness", self.roughness)?;
        validate_scalar("material transmission", self.transmission)?;
        if !self.ior.is_finite() || self.ior <= 0.0 {
            return Err("material ior must be finite and > 0".to_string());
        }
        Ok(())
    }
}

fn validate_scalar(label: &str, value: f32) -> Result<(), String> {
    if !value.is_finite() {
        Err(format!("{label} must be finite"))
    } else {
        Ok(())
    }
}

fn validate_point3(label: &str, point: &Point3<f32>) -> Result<(), String> {
    for component in point.coords.iter() {
        if !component.is_finite() {
            return Err(format!("{label} must contain finite values"));
        }
    }
    Ok(())
}

fn validate_vector3(label: &str, vector: &Vector3<f32>) -> Result<(), String> {
    for component in vector.iter() {
        if !component.is_finite() {
            return Err(format!("{label} must contain finite values"));
        }
    }
    Ok(())
}

fn validate_color(label: &str, color: &Color) -> Result<(), String> {
    for component in color.iter() {
        if !component.is_finite() {
            return Err(format!("{label} must contain finite values"));
        }
        if *component < 0.0 {
            return Err(format!("{label} must be non-negative"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use cherry_app::CameraConfig;

    use super::{AuthoredLightKind, AuthoredMaterial, AuthoredObjectKind, AuthoredScene};
    use cherry_core::Color;
    use nalgebra::{Point3, Vector3};

    #[test]
    fn default_scene_matches_current_cornell_defaults() {
        let scene = AuthoredScene::default();

        assert_eq!(scene.objects.len(), 7);
        assert_eq!(scene.lights.len(), 1);
        assert_eq!(scene.background, Color::new(0.0, 0.0, 0.0));
    }

    #[test]
    fn authored_scene_converts_into_snapshot() {
        let scene = AuthoredScene::default();
        let camera = CameraConfig::default()
            .to_camera(16.0 / 9.0)
            .expect("default camera should be valid");

        let snapshot = scene
            .to_snapshot(camera)
            .expect("default authored scene should convert");

        assert_eq!(snapshot.primitives.len(), 7);
        assert_eq!(snapshot.lights.len(), 1);
        assert_eq!(snapshot.background, Color::new(0.0, 0.0, 0.0));
    }

    #[test]
    fn invalid_scene_is_rejected_before_render() {
        let mut scene = AuthoredScene::default();
        scene.objects.clear();
        scene.add_object(
            "Broken Sphere".to_string(),
            AuthoredObjectKind::Sphere {
                center: Point3::new(0.0, 0.0, 0.0),
                radius: 0.0,
            },
            AuthoredMaterial::opaque(Color::new(0.5, 0.5, 0.5), 0.0, 0.5),
        );

        let camera = CameraConfig::default()
            .to_camera(1.0)
            .expect("default camera should be valid");
        let err = scene
            .to_snapshot(camera)
            .err()
            .expect("invalid sphere should fail conversion");

        assert!(err.contains("radius"));
    }

    #[test]
    fn directional_light_requires_non_zero_direction() {
        let mut scene = AuthoredScene::default();
        scene.lights.clear();
        scene.add_light(
            "Broken Sun".to_string(),
            AuthoredLightKind::Directional {
                direction: Vector3::new(0.0, 0.0, 0.0),
                intensity: Color::new(1.0, 1.0, 1.0),
            },
        );

        let camera = CameraConfig::default()
            .to_camera(1.0)
            .expect("default camera should be valid");
        let err = scene
            .to_snapshot(camera)
            .err()
            .expect("zero direction light should fail conversion");

        assert!(err.contains("direction"));
    }
}
