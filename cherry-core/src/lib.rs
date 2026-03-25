pub mod camera;
pub mod color;
pub mod frame;
pub mod intersection;
pub mod light;
pub mod material;
pub mod math;
pub mod primitive;
pub mod ray;
pub mod scene;
pub mod spectral;

pub use camera::Camera;
pub use color::Color;
pub use frame::FrameRequest;
pub use intersection::Hit;
pub use light::{DirectionalSpectralLight, Light, PointSpectralLight, SpectralLight};
pub use material::{Lambertian, Material, SpectralLambertian, SpectralMaterial};
pub use primitive::{Cuboid, Primitive, Sphere};
pub use ray::Ray;
pub use scene::{SceneProvider, SceneSnapshot, StaticSceneProvider};
pub use spectral::{
    SampledSpectrum, SpectralCurve, WAVELENGTH_BIN_COUNT, WAVELENGTH_BIN_STEP_NM,
    WAVELENGTH_MAX_NM, WAVELENGTH_MIN_NM, Wavelength, apply_exposure_reinhard,
    cie_xyz_from_wavelength, rgb_to_emission_at_nm, rgb_to_emission_spectrum,
    rgb_to_reflectance_at_nm, rgb_to_reflectance_spectrum, xyz_to_linear_srgb,
};
