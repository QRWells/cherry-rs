mod backend;
mod orchestrator;
mod pixel;
mod registry;
mod sink;

pub use backend::{
    BackendCapabilities, BackendId, BackendMetadata, RenderBackend, RenderResult, RenderStats,
};
pub use orchestrator::{render_frame, render_sequence, RenderError, SequenceSpec};
pub use pixel::color_to_rgb8;
pub use registry::{BackendFactory, BackendRegistry};
pub use sink::{FrameEvent, FrameSink, NoopFrameSink};
