use nalgebra::Vector3;

pub type Color = Vector3<f32>;

#[inline]
pub fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}
