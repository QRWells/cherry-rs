#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RasterBackendConfig {
    pub cpu_threads: Option<usize>,
    pub exposure: f32,
}

impl Default for RasterBackendConfig {
    fn default() -> Self {
        Self {
            cpu_threads: None,
            exposure: 1.0,
        }
    }
}
