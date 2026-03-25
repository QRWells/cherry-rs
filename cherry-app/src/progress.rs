use std::time::Duration;

use cherry_render::{FrameEvent, FrameSink};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

pub struct CliProgressSink {
    frame_index: u32,
    total_frames: u32,
    progress_bar: ProgressBar,
}

impl CliProgressSink {
    pub fn new(frame_index: u32, total_frames: u32) -> Self {
        Self::with_draw_target(frame_index, total_frames, ProgressDrawTarget::stderr())
    }

    fn with_draw_target(
        frame_index: u32,
        total_frames: u32,
        draw_target: ProgressDrawTarget,
    ) -> Self {
        let progress_bar = ProgressBar::with_draw_target(Some(0), draw_target);
        progress_bar.set_style(Self::progress_style());

        Self {
            frame_index,
            total_frames,
            progress_bar,
        }
    }

    fn progress_style() -> ProgressStyle {
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} {msg}",
        )
        .expect("progress template is valid")
    }

    fn frame_label(&self) -> String {
        let total = self.total_frames.max(1);
        let current = (self.frame_index + 1).min(total);
        format!("frame {current}/{total}")
    }

    fn begin_message(&self, backend_id: &str) -> String {
        format!("{backend_id} {}", self.frame_label())
    }

    fn end_message(&self, backend_id: &str, elapsed: Duration) -> String {
        format!(
            "{backend_id} {} done in {:.2?}",
            self.frame_label(),
            elapsed
        )
    }

    #[cfg(test)]
    pub(crate) fn for_test(frame_index: u32, total_frames: u32) -> Self {
        Self::with_draw_target(frame_index, total_frames, ProgressDrawTarget::hidden())
    }

    #[cfg(test)]
    pub(crate) fn position(&self) -> u64 {
        self.progress_bar.position()
    }

    #[cfg(test)]
    pub(crate) fn length(&self) -> Option<u64> {
        self.progress_bar.length()
    }

    #[cfg(test)]
    pub(crate) fn is_finished(&self) -> bool {
        self.progress_bar.is_finished()
    }
}

impl FrameSink for CliProgressSink {
    fn on_event(&mut self, event: FrameEvent) {
        match event {
            FrameEvent::Begin { backend, request } => {
                self.progress_bar.set_length(request.height as u64);
                self.progress_bar.set_position(0);
                self.progress_bar
                    .set_message(self.begin_message(backend.id.as_str()));
                self.progress_bar
                    .enable_steady_tick(Duration::from_millis(80));
            }
            FrameEvent::Scanline { .. } => {
                let current = self.progress_bar.position();
                let max = self.progress_bar.length().unwrap_or(u64::MAX);
                if current < max {
                    self.progress_bar.inc(1);
                }
            }
            FrameEvent::End { stats } => {
                self.progress_bar.finish_with_message(
                    self.end_message(stats.backend_id.as_str(), stats.elapsed),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use cherry_core::{Color, FrameRequest};
    use cherry_render::{
        BackendCapabilities, BackendId, BackendMetadata, FrameEvent, FrameSink, RenderStats,
    };

    use super::CliProgressSink;

    #[test]
    fn begin_event_initializes_progress_bar() {
        let mut sink = CliProgressSink::for_test(0, 1);

        sink.on_event(begin_event(10, 0));

        assert_eq!(sink.length(), Some(10));
        assert_eq!(sink.position(), 0);
    }

    #[test]
    fn scanline_event_increments_progress() {
        let mut sink = CliProgressSink::for_test(0, 1);
        sink.on_event(begin_event(3, 0));

        sink.on_event(FrameEvent::Scanline {
            y: 0,
            pixels: vec![Color::new(0.0, 0.0, 0.0)],
            spectral: None,
        });
        sink.on_event(FrameEvent::Scanline {
            y: 1,
            pixels: vec![Color::new(0.0, 0.0, 0.0)],
            spectral: None,
        });

        assert_eq!(sink.position(), 2);
    }

    #[test]
    fn end_event_finishes_progress_bar() {
        let mut sink = CliProgressSink::for_test(0, 1);
        sink.on_event(begin_event(1, 0));

        sink.on_event(end_event(0));

        assert!(sink.is_finished());
    }

    fn begin_event(height: u32, frame_index: u32) -> FrameEvent {
        FrameEvent::Begin {
            backend: BackendMetadata {
                id: BackendId::new("ray.normal"),
                display_name: "CPU Ray Backend (Normal)".to_string(),
                capabilities: BackendCapabilities {
                    progressive_updates: true,
                    gpu_ready_interface: true,
                },
            },
            request: FrameRequest {
                width: 8,
                height,
                frame_index,
                time: 0.0,
                samples_per_pixel: 1,
                max_bounces: 3,
            },
        }
    }

    fn end_event(frame_index: u32) -> FrameEvent {
        FrameEvent::End {
            stats: RenderStats {
                backend_id: BackendId::new("ray.normal"),
                frame_index,
                elapsed: Duration::from_millis(42),
                samples_per_pixel: 1,
            },
        }
    }
}
