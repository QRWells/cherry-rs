mod backend;
mod orchestrator;
mod pixel;
mod registry;
mod sink;

pub use backend::{
    BackendCapabilities, BackendId, BackendMetadata, ErasedRenderBackend, PixelRadiance,
    RenderBackend, RenderResult, RenderStats, TypedRenderResult, TypedScanline,
};
pub use orchestrator::{RenderError, SequenceSpec, render_frame, render_sequence};
pub use pixel::color_to_rgb8;
pub use registry::{BackendFactory, BackendRegistry};
pub use sink::{
    FrameEvent, FrameSink, NoopFrameSink, SPECTRAL_BIN_COUNT, SPECTRAL_BIN_END_NM,
    SPECTRAL_BIN_START_NM, SPECTRAL_BIN_STEP_NM, SpectralBins,
};
