use std::time::Duration;

use cherry_core::{FrameRequest, SceneSnapshot};

use crate::FrameSink;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BackendId(String);

impl BackendId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for BackendId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone)]
pub struct BackendCapabilities {
    pub progressive_updates: bool,
    pub gpu_ready_interface: bool,
}

#[derive(Debug, Clone)]
pub struct BackendMetadata {
    pub id: BackendId,
    pub display_name: String,
    pub capabilities: BackendCapabilities,
}

#[derive(Debug, Clone)]
pub struct RenderStats {
    pub backend_id: BackendId,
    pub frame_index: u32,
    pub elapsed: Duration,
    pub samples_per_pixel: u32,
}

pub struct RenderResult {
    pub image: image::RgbImage,
    pub stats: RenderStats,
}

pub trait RenderBackend: Send + Sync {
    fn metadata(&self) -> BackendMetadata;

    fn render_frame(
        &self,
        scene: &SceneSnapshot,
        request: &FrameRequest,
        sink: &mut dyn FrameSink,
    ) -> RenderResult;
}
