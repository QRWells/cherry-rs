use cherry_core::{Color, FrameRequest};

use crate::{BackendMetadata, RenderStats};

pub const SPECTRAL_BIN_START_NM: u16 = 380;
pub const SPECTRAL_BIN_END_NM: u16 = 780;
pub const SPECTRAL_BIN_STEP_NM: u16 = 10;
pub const SPECTRAL_BIN_COUNT: usize =
    ((SPECTRAL_BIN_END_NM - SPECTRAL_BIN_START_NM) / SPECTRAL_BIN_STEP_NM + 1) as usize;

#[derive(Clone, Debug, PartialEq)]
pub struct SpectralBins {
    pub bins: [f32; SPECTRAL_BIN_COUNT],
}

impl SpectralBins {
    pub fn zeros() -> Self {
        Self {
            bins: [0.0; SPECTRAL_BIN_COUNT],
        }
    }
}

#[derive(Clone)]
pub enum FrameEvent {
    Begin {
        backend: BackendMetadata,
        request: FrameRequest,
    },
    Scanline {
        y: u32,
        pixels: Vec<Color>,
        spectral: Option<Vec<SpectralBins>>,
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
