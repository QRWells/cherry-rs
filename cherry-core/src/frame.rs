#[derive(Debug, Clone)]
pub struct FrameRequest {
    pub width: u32,
    pub height: u32,
    pub frame_index: u32,
    pub time: f32,
    pub samples_per_pixel: u32,
    pub max_bounces: u32,
}

impl FrameRequest {
    pub fn with_frame(&self, frame_index: u32, time: f32) -> Self {
        let mut next = self.clone();
        next.frame_index = frame_index;
        next.time = time;
        next
    }
}

impl Default for FrameRequest {
    fn default() -> Self {
        Self {
            width: 640,
            height: 360,
            frame_index: 0,
            time: 0.0,
            samples_per_pixel: 1,
            max_bounces: 4,
        }
    }
}
