use cherry_core::{Color, FrameRequest};

use crate::{BackendMetadata, RenderStats};

#[derive(Clone)]
pub enum FrameEvent {
    Begin {
        backend: BackendMetadata,
        request: FrameRequest,
    },
    Scanline {
        y: u32,
        pixels: Vec<Color>,
    },
    End {
        stats: RenderStats,
    },
}

pub trait FrameSink: Send {
    fn on_event(&mut self, event: FrameEvent);
}

pub struct NoopFrameSink;

impl FrameSink for NoopFrameSink {
    fn on_event(&mut self, _event: FrameEvent) {}
}
