mod cli;
mod progress;

use cherry_app::{
    RuntimeRenderConfig, build_animated_scene_provider, build_registry_with_config, initialize_gpu,
    output_filename,
};
use cherry_core::FrameRequest;
use cherry_render::{BackendId, SequenceSpec, render_frame, render_sequence};
use clap::Parser;
use cli::{Cli, validate_backend};
use progress::CliProgressSink;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if let Some(command) = cli.command {
        println!("{}", command.todo_message());
        return Ok(());
    }

    let registry = build_registry_with_config(RuntimeRenderConfig {
        exposure: cli.exposure,
        cpu_threads: cli.cpu_threads,
    });
    let available_backends = registry
        .list_ids()
        .into_iter()
        .map(|id| id.as_str().to_string())
        .collect::<Vec<_>>();
    if let Err(error) = validate_backend(&cli.backend, &available_backends) {
        error.exit();
    }

    std::fs::create_dir_all(&cli.output_dir)?;

    let provider = build_animated_scene_provider(cli.width as f32 / cli.height as f32);

    if cli.init_gpu {
        let info = initialize_gpu()?;
        println!(
            "Initialized GPU adapter '{}' ({}, {})",
            info.adapter_name, info.backend, info.device_type
        );
    }

    let backend_id = BackendId::new(cli.backend.clone());
    let request = FrameRequest {
        width: cli.width,
        height: cli.height,
        frame_index: 0,
        time: 0.0,
        samples_per_pixel: cli.samples_per_pixel,
        max_bounces: cli.max_bounces,
    };

    if cli.frames <= 1 {
        let mut sink = CliProgressSink::new(0, 1);
        let result = render_frame(&registry, &provider, &backend_id, &request, &mut sink)?;
        let output = cli
            .output_dir
            .join(output_filename(backend_id.as_str(), None));
        result.image.save(&output)?;
        println!("Rendered {}", output.display());
        return Ok(());
    }

    let sequence = SequenceSpec {
        frame_count: cli.frames,
        start_time: 0.0,
        frame_time_step: 1.0 / 24.0,
        template: request,
    };

    let total_frames = cli.frames;
    let results = render_sequence(
        &registry,
        &provider,
        &backend_id,
        &sequence,
        |frame, _request| Box::new(CliProgressSink::new(frame, total_frames)),
    )?;

    for result in results {
        let output = cli.output_dir.join(output_filename(
            backend_id.as_str(),
            Some(result.stats.frame_index),
        ));
        result.image.save(&output)?;
        println!("Rendered {}", output.display());
    }

    Ok(())
}
