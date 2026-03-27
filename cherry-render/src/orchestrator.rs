use cherry_core::{FrameRequest, SceneProvider};

use crate::{BackendId, BackendRegistry, FrameSink, RenderResult};

#[derive(Debug)]
pub enum RenderError {
    BackendNotFound(BackendId),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BackendNotFound(id) => {
                write!(f, "backend '{}' is not registered", id.as_str())
            }
        }
    }
}

impl std::error::Error for RenderError {}

#[derive(Debug, Clone)]
pub struct SequenceSpec {
    pub frame_count: u32,
    pub start_time: f32,
    pub frame_time_step: f32,
    pub template: FrameRequest,
}

impl SequenceSpec {
    pub fn frame_request(&self, frame_index: u32) -> FrameRequest {
        self.template.with_frame(
            frame_index,
            self.start_time + self.frame_time_step * frame_index as f32,
        )
    }
}

pub fn render_frame(
    registry: &BackendRegistry,
    scene_provider: &dyn SceneProvider,
    backend_id: &BackendId,
    request: &FrameRequest,
    sink: &mut dyn FrameSink,
) -> Result<RenderResult, RenderError> {
    let backend = registry
        .create(backend_id)
        .ok_or_else(|| RenderError::BackendNotFound(backend_id.clone()))?;
    let mut scene = scene_provider.snapshot(request.time);
    let aspect_ratio = if request.height == 0 {
        1.0
    } else {
        request.width as f32 / request.height as f32
    };
    scene.camera = scene.camera.with_aspect_ratio(aspect_ratio);
    Ok(backend.render_frame(&scene, request, sink))
}

pub fn render_sequence(
    registry: &BackendRegistry,
    scene_provider: &dyn SceneProvider,
    backend_id: &BackendId,
    spec: &SequenceSpec,
    mut sink_factory: impl FnMut(u32, &FrameRequest) -> Box<dyn FrameSink>,
) -> Result<Vec<RenderResult>, RenderError> {
    let mut results = Vec::with_capacity(spec.frame_count as usize);

    for frame_index in 0..spec.frame_count {
        let request = spec.frame_request(frame_index);
        let mut sink = sink_factory(frame_index, &request);
        let result = render_frame(registry, scene_provider, backend_id, &request, &mut *sink)?;
        results.push(result);
    }

    Ok(results)
}
