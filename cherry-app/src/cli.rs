use std::path::PathBuf;

use clap::{ArgAction, CommandFactory, Parser, Subcommand, error::ErrorKind};
use nalgebra::{Point3, Vector3};

#[derive(Debug, Clone, Parser)]
#[command(
    name = "cherry-app",
    about = "Cherry renderer CLI runner",
    long_about = None
)]
pub struct Cli {
    #[arg(long, default_value = "ray.normal")]
    pub backend: String,

    #[arg(long, default_value_t = 320, value_parser = clap::value_parser!(u32).range(1..))]
    pub width: u32,

    #[arg(long, default_value_t = 180, value_parser = clap::value_parser!(u32).range(1..))]
    pub height: u32,

    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..))]
    pub frames: u32,

    #[arg(
        long = "spp",
        visible_alias = "samples-per-pixel",
        default_value_t = 1,
        value_parser = clap::value_parser!(u32).range(1..)
    )]
    pub samples_per_pixel: u32,

    #[arg(
        long,
        default_value_t = 3,
        value_parser = clap::value_parser!(u32).range(1..)
    )]
    pub max_bounces: u32,

    #[arg(long, default_value_t = 3)]
    pub rr_start_depth: u32,

    #[arg(
        long,
        default_value_t = 0.05,
        value_parser = parse_rr_min_survival
    )]
    pub rr_min_survival: f32,

    #[arg(long, default_value_t = 10.0, value_parser = clap::value_parser!(f32))]
    pub indirect_clamp: f32,

    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    pub direct_lighting: bool,

    #[arg(
        long,
        default_value_t = cherry_app::DEFAULT_SPECTRAL_EXPOSURE,
        value_parser = clap::value_parser!(f32)
    )]
    pub exposure: f32,

    #[arg(long, value_parser = parse_cpu_threads)]
    pub cpu_threads: Option<usize>,

    #[arg(long)]
    pub init_gpu: bool,

    #[arg(long, default_value = "output")]
    pub output_dir: PathBuf,

    #[arg(long = "camera-look-from-x", default_value_t = 0.0, value_parser = parse_finite_f32)]
    pub camera_look_from_x: f32,
    #[arg(long = "camera-look-from-y", default_value_t = 0.0, value_parser = parse_finite_f32)]
    pub camera_look_from_y: f32,
    #[arg(long = "camera-look-from-z", default_value_t = 2.6, value_parser = parse_finite_f32)]
    pub camera_look_from_z: f32,

    #[arg(long = "camera-look-at-x", default_value_t = 0.0, value_parser = parse_finite_f32)]
    pub camera_look_at_x: f32,
    #[arg(long = "camera-look-at-y", default_value_t = -0.1, value_parser = parse_finite_f32)]
    pub camera_look_at_y: f32,
    #[arg(long = "camera-look-at-z", default_value_t = -0.25, value_parser = parse_finite_f32)]
    pub camera_look_at_z: f32,

    #[arg(long = "camera-view-up-x", default_value_t = 0.0, value_parser = parse_finite_f32)]
    pub camera_view_up_x: f32,
    #[arg(long = "camera-view-up-y", default_value_t = 1.0, value_parser = parse_finite_f32)]
    pub camera_view_up_y: f32,
    #[arg(long = "camera-view-up-z", default_value_t = 0.0, value_parser = parse_finite_f32)]
    pub camera_view_up_z: f32,

    #[arg(long = "camera-fov", default_value_t = 38.0, value_parser = parse_finite_f32)]
    pub camera_fov: f32,
    #[arg(long = "camera-aperture", default_value_t = 0.0, value_parser = parse_non_negative_f32)]
    pub camera_aperture: f32,
    #[arg(long = "camera-focal-distance", value_parser = parse_positive_f32)]
    pub camera_focal_distance: Option<f32>,

    #[command(subcommand)]
    pub command: Option<FutureCommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
pub enum FutureCommand {
    #[command(about = "TODO: benchmark and profiling workflows")]
    Benchmark,
    #[command(about = "TODO: scene authoring and management workflows")]
    Scene,
}

impl FutureCommand {
    pub fn todo_message(self) -> &'static str {
        match self {
            Self::Benchmark => "TODO: benchmark workflow is not implemented yet.",
            Self::Scene => "TODO: scene workflow is not implemented yet.",
        }
    }
}

impl Cli {
    pub fn camera_config(&self) -> Result<cherry_app::CameraConfig, clap::Error> {
        let config = cherry_app::CameraConfig {
            look_from: Point3::new(
                self.camera_look_from_x,
                self.camera_look_from_y,
                self.camera_look_from_z,
            ),
            look_at: Point3::new(
                self.camera_look_at_x,
                self.camera_look_at_y,
                self.camera_look_at_z,
            ),
            view_up: Vector3::new(
                self.camera_view_up_x,
                self.camera_view_up_y,
                self.camera_view_up_z,
            ),
            fov_degrees: self.camera_fov,
            aperture: self.camera_aperture,
            focal_distance: self.camera_focal_distance,
        };

        if let Err(message) = config.validate() {
            return Err(Cli::command().error(
                ErrorKind::ValueValidation,
                format!("invalid camera configuration: {message}"),
            ));
        }

        Ok(config)
    }
}

pub fn validate_backend(
    backend: &str,
    available_backend_ids: &[String],
) -> Result<(), clap::Error> {
    if available_backend_ids.iter().any(|id| id == backend) {
        return Ok(());
    }

    let mut message = format!("invalid value '{backend}' for '--backend'");
    if available_backend_ids.is_empty() {
        message.push_str("\n\nNo backends are currently registered.");
    } else {
        message.push_str("\n\nValid backend ids:");
        for backend_id in available_backend_ids {
            message.push_str("\n  - ");
            message.push_str(backend_id);
        }
    }

    Err(Cli::command().error(ErrorKind::InvalidValue, message))
}

fn parse_cpu_threads(raw: &str) -> Result<usize, String> {
    let parsed = raw
        .parse::<usize>()
        .map_err(|_| format!("invalid CPU thread count '{raw}'"))?;
    if parsed == 0 {
        return Err("CPU thread count must be at least 1".to_string());
    }
    Ok(parsed)
}

fn parse_rr_min_survival(raw: &str) -> Result<f32, String> {
    let parsed = raw
        .parse::<f32>()
        .map_err(|_| format!("invalid rr_min_survival '{raw}'"))?;
    if !(0.0..=1.0).contains(&parsed) {
        return Err("rr_min_survival must be between 0.0 and 1.0".to_string());
    }
    Ok(parsed)
}

fn parse_finite_f32(raw: &str) -> Result<f32, String> {
    let parsed = raw
        .parse::<f32>()
        .map_err(|_| format!("invalid floating-point value '{raw}'"))?;
    if !parsed.is_finite() {
        return Err(format!("value '{raw}' must be finite"));
    }
    Ok(parsed)
}

fn parse_non_negative_f32(raw: &str) -> Result<f32, String> {
    let parsed = parse_finite_f32(raw)?;
    if parsed < 0.0 {
        return Err(format!("value '{raw}' must be >= 0"));
    }
    Ok(parsed)
}

fn parse_positive_f32(raw: &str) -> Result<f32, String> {
    let parsed = parse_finite_f32(raw)?;
    if parsed <= 0.0 {
        return Err(format!("value '{raw}' must be > 0"));
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, FutureCommand, validate_backend};

    #[test]
    fn defaults_match_previous_behavior() {
        let cli = Cli::parse_from(["cherry-app"]);
        assert_eq!(cli.backend, "ray.normal");
        assert_eq!(cli.width, 320);
        assert_eq!(cli.height, 180);
        assert_eq!(cli.frames, 1);
        assert_eq!(cli.samples_per_pixel, 1);
        assert_eq!(cli.max_bounces, 3);
        assert_eq!(cli.rr_start_depth, 3);
        assert!((cli.rr_min_survival - 0.05).abs() < f32::EPSILON);
        assert!((cli.indirect_clamp - 10.0).abs() < f32::EPSILON);
        assert!(cli.direct_lighting);
        assert_eq!(cli.exposure, 0.2);
        assert!(cli.cpu_threads.is_none());
        assert!(!cli.init_gpu);
        assert_eq!(cli.output_dir.to_string_lossy(), "output");
        assert!((cli.camera_look_from_x - 0.0).abs() < f32::EPSILON);
        assert!((cli.camera_look_from_y - 0.0).abs() < f32::EPSILON);
        assert!((cli.camera_look_from_z - 2.6).abs() < f32::EPSILON);
        assert!((cli.camera_look_at_x - 0.0).abs() < f32::EPSILON);
        assert!((cli.camera_look_at_y + 0.1).abs() < f32::EPSILON);
        assert!((cli.camera_look_at_z + 0.25).abs() < f32::EPSILON);
        assert!((cli.camera_view_up_x - 0.0).abs() < f32::EPSILON);
        assert!((cli.camera_view_up_y - 1.0).abs() < f32::EPSILON);
        assert!((cli.camera_view_up_z - 0.0).abs() < f32::EPSILON);
        assert!((cli.camera_fov - 38.0).abs() < f32::EPSILON);
        assert!((cli.camera_aperture - 0.0).abs() < f32::EPSILON);
        assert!(cli.camera_focal_distance.is_none());
        assert!(cli.command.is_none());
    }

    #[test]
    fn spp_and_long_alias_both_parse() {
        let short = Cli::parse_from(["cherry-app", "--spp=8"]);
        let long = Cli::parse_from(["cherry-app", "--samples-per-pixel=6"]);
        assert_eq!(short.samples_per_pixel, 8);
        assert_eq!(long.samples_per_pixel, 6);
    }

    #[test]
    fn exposure_parses() {
        let cli = Cli::parse_from(["cherry-app", "--exposure=1.75"]);
        assert!((cli.exposure - 1.75).abs() < f32::EPSILON);
    }

    #[test]
    fn path_tracing_flags_parse() {
        let cli = Cli::parse_from([
            "cherry-app",
            "--rr-start-depth=6",
            "--rr-min-survival=0.2",
            "--indirect-clamp=2.5",
            "--direct-lighting=false",
        ]);
        assert_eq!(cli.rr_start_depth, 6);
        assert!((cli.rr_min_survival - 0.2).abs() < f32::EPSILON);
        assert!((cli.indirect_clamp - 2.5).abs() < f32::EPSILON);
        assert!(!cli.direct_lighting);
    }

    #[test]
    fn cpu_threads_and_gpu_init_parse() {
        let cli = Cli::parse_from(["cherry-app", "--cpu-threads=6", "--init-gpu"]);
        assert_eq!(cli.cpu_threads, Some(6));
        assert!(cli.init_gpu);
    }

    #[test]
    fn camera_controls_parse() {
        let cli = Cli::parse_from([
            "cherry-app",
            "--camera-look-from-x=1.0",
            "--camera-look-from-y=2.0",
            "--camera-look-from-z=3.0",
            "--camera-look-at-x=0.5",
            "--camera-look-at-y=0.0",
            "--camera-look-at-z=-0.5",
            "--camera-view-up-x=0.0",
            "--camera-view-up-y=1.0",
            "--camera-view-up-z=0.1",
            "--camera-fov=50.0",
            "--camera-aperture=0.2",
            "--camera-focal-distance=2.5",
        ]);

        assert!((cli.camera_look_from_x - 1.0).abs() < f32::EPSILON);
        assert!((cli.camera_look_from_y - 2.0).abs() < f32::EPSILON);
        assert!((cli.camera_look_from_z - 3.0).abs() < f32::EPSILON);
        assert!((cli.camera_look_at_x - 0.5).abs() < f32::EPSILON);
        assert!((cli.camera_look_at_y - 0.0).abs() < f32::EPSILON);
        assert!((cli.camera_look_at_z + 0.5).abs() < f32::EPSILON);
        assert!((cli.camera_view_up_z - 0.1).abs() < f32::EPSILON);
        assert!((cli.camera_fov - 50.0).abs() < f32::EPSILON);
        assert!((cli.camera_aperture - 0.2).abs() < f32::EPSILON);
        assert_eq!(cli.camera_focal_distance, Some(2.5));
    }

    #[test]
    fn unknown_flags_are_rejected() {
        let err = Cli::try_parse_from(["cherry-app", "--does-not-exist"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn malformed_numbers_are_rejected() {
        let err = Cli::try_parse_from(["cherry-app", "--width=nope"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
    }

    #[test]
    fn zero_values_are_rejected() {
        let err = Cli::try_parse_from(["cherry-app", "--height=0"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
    }

    #[test]
    fn rr_min_survival_out_of_range_is_rejected() {
        let err = Cli::try_parse_from(["cherry-app", "--rr-min-survival=1.2"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
    }

    #[test]
    fn zero_cpu_threads_is_rejected() {
        let err = Cli::try_parse_from(["cherry-app", "--cpu-threads=0"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
    }

    #[test]
    fn placeholder_subcommands_parse() {
        let benchmark = Cli::parse_from(["cherry-app", "benchmark"]);
        let scene = Cli::parse_from(["cherry-app", "scene"]);

        assert!(matches!(benchmark.command, Some(FutureCommand::Benchmark)));
        assert!(matches!(scene.command, Some(FutureCommand::Scene)));
    }

    #[test]
    fn backend_validation_reports_available_ids() {
        let err = validate_backend(
            "ray.unknown",
            &["raster.simple".to_string(), "ray.normal".to_string()],
        )
        .unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidValue);

        let rendered = err.to_string();
        assert!(rendered.contains("ray.unknown"));
        assert!(rendered.contains("raster.simple"));
        assert!(rendered.contains("ray.normal"));
    }

    #[test]
    fn camera_validation_rejects_degenerate_view_basis() {
        let cli = Cli::parse_from([
            "cherry-app",
            "--camera-look-from-x=0.0",
            "--camera-look-from-y=0.0",
            "--camera-look-from-z=0.0",
            "--camera-look-at-x=0.0",
            "--camera-look-at-y=0.0",
            "--camera-look-at-z=0.0",
        ]);

        let err = cli.camera_config().unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
        assert!(err.to_string().contains("camera"));
    }
}
