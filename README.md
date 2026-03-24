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
