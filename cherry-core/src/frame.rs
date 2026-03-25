#[derive(Debug, Clone)]
pub struct PathTracingConfig {
    pub rr_start_depth: u32,
    pub rr_min_survival: f32,
    pub indirect_clamp: f32,
    pub direct_lighting: bool,
}

impl PathTracingConfig {
    pub fn indirect_clamp_enabled(&self) -> bool {
        self.indirect_clamp > 0.0
    }
}

impl Default for PathTracingConfig {
    fn default() -> Self {
        Self {
            rr_start_depth: 3,
            rr_min_survival: 0.05,
            indirect_clamp: 10.0,
            direct_lighting: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FrameRequest {
    pub width: u32,
    pub height: u32,
    pub frame_index: u32,
    pub time: f32,
    pub samples_per_pixel: u32,
    pub max_bounces: u32,
    pub path_tracing: PathTracingConfig,
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
            path_tracing: PathTracingConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameRequest, PathTracingConfig};

    #[test]
    fn path_tracing_defaults_match_balanced_profile() {
        let config = PathTracingConfig::default();
        assert_eq!(config.rr_start_depth, 3);
        assert!((config.rr_min_survival - 0.05).abs() < f32::EPSILON);
        assert!((config.indirect_clamp - 10.0).abs() < f32::EPSILON);
        assert!(config.direct_lighting);
        assert!(config.indirect_clamp_enabled());
    }

    #[test]
    fn indirect_clamp_is_disabled_at_zero_or_negative() {
        let disabled = PathTracingConfig {
            indirect_clamp: 0.0,
            ..PathTracingConfig::default()
        };
        assert!(!disabled.indirect_clamp_enabled());

        let disabled_negative = PathTracingConfig {
            indirect_clamp: -1.0,
            ..PathTracingConfig::default()
        };
        assert!(!disabled_negative.indirect_clamp_enabled());
    }

    #[test]
    fn frame_request_default_uses_default_path_tracing_config() {
        let request = FrameRequest::default();
        assert_eq!(
            request.path_tracing.rr_start_depth,
            PathTracingConfig::default().rr_start_depth
        );
        assert!(
            (request.path_tracing.rr_min_survival - PathTracingConfig::default().rr_min_survival)
                .abs()
                < f32::EPSILON
        );
        assert!(
            (request.path_tracing.indirect_clamp - PathTracingConfig::default().indirect_clamp)
                .abs()
                < f32::EPSILON
        );
        assert_eq!(
            request.path_tracing.direct_lighting,
            PathTracingConfig::default().direct_lighting
        );
    }
}
