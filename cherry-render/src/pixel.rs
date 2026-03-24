use cherry_core::Color;
use image::Rgb;

#[inline]
pub fn color_to_rgb8(color: Color) -> Rgb<u8> {
    Rgb([
        (color.x.clamp(0.0, 1.0) * 255.0) as u8,
        (color.y.clamp(0.0, 1.0) * 255.0) as u8,
        (color.z.clamp(0.0, 1.0) * 255.0) as u8,
    ])
}
