# Cherry.rs

Dual-backend renderer workspace with pluggable runtime-selected tracing methods.

## Workspace Crates

- `cherry-core`: shared scene/camera/material/primitive types and `SceneProvider`
- `cherry-render`: backend traits, registry, frame sink events, render orchestration
- `cherry-backend-raster`: CPU software raster backend
- `cherry-backend-ray`: CPU ray backends (`ray.normal`, `ray.montecarlo`) with pluggable tracer and `Accel` trait (`BruteForceAccel`)
- `cherry-app`: thin CLI runner over the library APIs

## Run

```bash
cargo run -p cherry-app -- --backend=ray.normal --width=640 --height=360 --frames=1
```

Animation frames (PNG sequence):

```bash
cargo run -p cherry-app -- --backend=raster.simple --frames=24
```

The CLI now shows a live per-frame progress bar while rendering.

Use `--spp` (or `--samples-per-pixel`) to control sampling:

```bash
cargo run -p cherry-app -- --backend=ray.montecarlo --spp=8 --frames=1
```

Inspect CLI options:

```bash
cargo run -p cherry-app -- --help
```

Reserved placeholder subcommands (currently TODO no-ops):

```bash
cargo run -p cherry-app -- benchmark
cargo run -p cherry-app -- scene
```
