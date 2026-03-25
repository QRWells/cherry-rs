use std::time::Duration;

use cherry_core::{Color, FrameRequest, SceneSnapshot};

use crate::{FrameEvent, FrameSink, SpectralBins};

use crate::pixel::color_to_rgb8;

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

pub trait PixelRadiance: Clone + Send + Sync + 'static {
    fn to_rgb_color(&self) -> Color;

    fn spectral_bins(&self) -> Option<[f32; crate::SPECTRAL_BIN_COUNT]> {
        None
    }
}

impl PixelRadiance for Color {
    fn to_rgb_color(&self) -> Color {
        *self
    }
}

#[derive(Clone)]
pub struct TypedScanline<P: PixelRadiance> {
    pub y: u32,
    pub pixels: Vec<P>,
}

pub struct TypedRenderResult<P: PixelRadiance> {
    pub scanlines: Vec<TypedScanline<P>>,
    pub stats: RenderStats,
}

pub trait RenderBackend: Send + Sync {
    type Pixel: PixelRadiance;

    fn metadata(&self) -> BackendMetadata;

    fn render_scanlines(
        &self,
        scene: &SceneSnapshot,
        request: &FrameRequest,
        emit_scanline: &mut dyn FnMut(TypedScanline<Self::Pixel>),
    ) -> RenderStats;

    fn render_frame_typed(
        &self,
        scene: &SceneSnapshot,
        request: &FrameRequest,
    ) -> TypedRenderResult<Self::Pixel> {
        let mut scanlines = Vec::with_capacity(request.height as usize);
        let stats = self.render_scanlines(scene, request, &mut |scanline| {
            scanlines.push(scanline);
        });
        TypedRenderResult { scanlines, stats }
    }

    fn render_frame(
        &self,
        scene: &SceneSnapshot,
        request: &FrameRequest,
        sink: &mut dyn FrameSink,
    ) -> RenderResult {
        let metadata = self.metadata();
        sink.on_event(FrameEvent::Begin {
            backend: metadata,
            request: request.clone(),
        });

        let mut image = image::RgbImage::new(request.width, request.height);
        let stats = self.render_scanlines(scene, request, &mut |scanline| {
            let mut rgb_scanline = Vec::with_capacity(scanline.pixels.len());
            let mut spectral_scanline = Vec::with_capacity(scanline.pixels.len());
            let mut has_spectral = false;

            for (x, pixel) in scanline.pixels.into_iter().enumerate() {
                let color = pixel.to_rgb_color();
                rgb_scanline.push(color);
                image.put_pixel(x as u32, scanline.y, color_to_rgb8(color));

                if let Some(bins) = pixel.spectral_bins() {
                    has_spectral = true;
                    spectral_scanline.push(SpectralBins { bins });
                } else {
                    spectral_scanline.push(SpectralBins::zeros());
                }
            }

            sink.on_event(FrameEvent::Scanline {
                y: scanline.y,
                pixels: rgb_scanline,
                spectral: has_spectral.then_some(spectral_scanline),
            });
        });

        sink.on_event(FrameEvent::End {
            stats: stats.clone(),
        });

        RenderResult { image, stats }
    }
}

pub trait ErasedRenderBackend: Send + Sync {
    fn metadata(&self) -> BackendMetadata;

    fn render_frame(
        &self,
        scene: &SceneSnapshot,
        request: &FrameRequest,
        sink: &mut dyn FrameSink,
    ) -> RenderResult;
}

impl<T> ErasedRenderBackend for T
where
    T: RenderBackend,
{
    fn metadata(&self) -> BackendMetadata {
        RenderBackend::metadata(self)
    }

    fn render_frame(
        &self,
        scene: &SceneSnapshot,
        request: &FrameRequest,
        sink: &mut dyn FrameSink,
    ) -> RenderResult {
        RenderBackend::render_frame(self, scene, request, sink)
    }
}
