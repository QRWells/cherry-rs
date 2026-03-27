use cherry_core::{BsdfEvalQuery, Color, FrameRequest, Ray, SceneSnapshot};
use nalgebra::{Vector2, Vector3};

use crate::{
    config::RasterBackendConfig,
    lighting::{
        clamp_non_negative, face_forward, fresnel_dielectric, is_shadowed, offset_origin, reflect,
        refract, sample_light_rgb,
    },
};

pub struct RasterPipeline<'a> {
    scene: &'a SceneSnapshot,
    request: &'a FrameRequest,
    _config: RasterBackendConfig,
}

impl<'a> RasterPipeline<'a> {
    pub fn new(
        scene: &'a SceneSnapshot,
        request: &'a FrameRequest,
        config: RasterBackendConfig,
    ) -> Self {
        Self {
            scene,
            request,
            _config: config,
        }
    }

    pub fn shade_pixel(&self, uv: Vector2<f32>) -> Color {
        let ray = self.scene.camera.generate_ray(uv);
        self.trace_ray(&ray, 0)
    }

    fn trace_ray(&self, ray: &Ray, depth: u32) -> Color {
        let Some(hit) = self.scene.intersect(ray) else {
            return self.scene.background;
        };

        let Some(raw_normal) = safe_normalize(hit.normal) else {
            return hit.material.preview_base_color();
        };
        let Some(outgoing) = safe_normalize(-ray.dir) else {
            return hit.material.preview_base_color();
        };
        let normal = face_forward(raw_normal, outgoing);
        let origin = offset_origin(hit.point, normal, outgoing);

        let mut color = hit.material.emissive_rgb();

        if self.request.path_tracing.direct_lighting {
            for light in &self.scene.lights {
                let Some(sample) = sample_light_rgb(light.as_ref(), origin) else {
                    continue;
                };
                let incoming = sample.direction_to_light.normalize();
                let cosine = normal.dot(&incoming).max(0.0);
                if cosine <= 0.0 {
                    continue;
                }
                if is_shadowed(self.scene, origin, incoming, sample.distance) {
                    continue;
                }

                let eval = hit.material.eval(&BsdfEvalQuery {
                    normal,
                    outgoing,
                    incoming,
                });
                color += eval.component_mul(&sample.irradiance) * cosine;
            }
        }

        let preview = hit.material.preview_material();
        if depth + 1 < self.max_depth() && preview.transmission > 0.0 {
            let incident = ray.dir.normalize();
            let entering = raw_normal.dot(&incident) < 0.0;
            let (eta_i, eta_t) = if entering {
                (1.0, preview.ior.max(1.0))
            } else {
                (preview.ior.max(1.0), 1.0)
            };

            let fresnel = fresnel_dielectric(outgoing.dot(&normal).abs(), preview.ior);
            let reflected_dir = reflect(incident, normal).normalize();
            let reflected_origin = offset_origin(hit.point, normal, reflected_dir);
            let reflected = self.trace_ray(&Ray::new(reflected_origin, reflected_dir), depth + 1);

            let transmission_tint = preview.base_color.map(|channel| channel.clamp(0.02, 1.0));
            let refracted = refract(incident, normal, eta_i, eta_t)
                .map(|direction| {
                    let origin = offset_origin(hit.point, normal, direction);
                    self.trace_ray(&Ray::new(origin, direction), depth + 1)
                        .component_mul(&transmission_tint)
                })
                .unwrap_or_else(|| Color::new(0.0, 0.0, 0.0));

            let reflection_weight = fresnel;
            let transmission_weight = if refracted == Color::new(0.0, 0.0, 0.0) {
                1.0 - fresnel
            } else {
                (1.0 - fresnel) * preview.transmission
            };
            color += reflected * reflection_weight + refracted * transmission_weight;
        }

        clamp_non_negative(color)
    }

    fn max_depth(&self) -> u32 {
        self.request.max_bounces.max(1).min(4)
    }
}

fn safe_normalize(vector: Vector3<f32>) -> Option<Vector3<f32>> {
    if vector.norm_squared() <= 1e-6 {
        return None;
    }
    Some(vector.normalize())
}
