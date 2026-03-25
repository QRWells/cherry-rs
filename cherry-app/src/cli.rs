use std::path::PathBuf;

use clap::{ArgAction, CommandFactory, Parser, Subcommand, error::ErrorKind};

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
}
