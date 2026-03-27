# Cherry.rs

Dual-backend renderer workspace with pluggable runtime-selected tracing methods, including
modular BSDF-based PBR shading and a spectral ray-tracing path.

## Workspace Crates

- `cherry-core`: shared scene/camera/BSDF/primitive types, spectral contracts/utilities, and `SceneProvider` (includes glTF-style metallic-roughness + transmission `GltfMrBsdf`)
- `cherry-render`: typed backend traits, runtime-erased registry, frame sink events, render orchestration
- `cherry-backend-raster`: CPU software raster backend
- `cherry-backend-ray`: CPU ray backends (`ray.normal`, `ray.montecarlo` path tracing, `ray.spectral`) with BSDF-driven path tracing and pluggable `Accel` trait (`BruteForceAccel`)
- `cherry-app`: thin CLI runner over the library APIs
- `cherry-gui`: native desktop GUI preview app (`eframe` + `egui`) for realtime progressive frame display

## CI/CD

- CI workflow (`.github/workflows/ci.yml`) runs on push and pull request with:
  - `cargo fmt --all -- --check`
  - `cargo check --workspace`
  - `cargo test --workspace`
- Release workflow (`.github/workflows/release.yml`) builds `cherry-app` and `cherry-gui` on Linux, macOS, and Windows.
- Push a `v*` tag (example: `v0.1.0`) to automatically publish a GitHub Release with packaged binaries.

## Run

```bash
cargo run -p cherry-app -- --backend=ray.normal --width=640 --height=360 --frames=1
```

Camera framing now always follows the render resolution (`width / height`) for each frame.

Animation frames (PNG sequence):

```bash
cargo run -p cherry-app -- --backend=raster.simple --frames=24
```

The CLI now shows a live per-frame progress bar while rendering.

Launch GUI preview app with a scene editor, draft raster preview, and manual final render:

```bash
cargo run -p cherry-gui
```

GUI workflow notes:

- `Preview` always uses `raster.simple` for fast draft feedback.
- `Render` uses the selected backend from the render controls panel.
- The scene panel supports adding/removing cuboids, spheres, point lights, and directional lights, plus editing `GltfMrBsdf` material inputs.
- Raster preview is intentionally lightweight: it shows geometry, base color, and background quickly, but does not yet reflect authored lighting, shadows, or higher-fidelity material behavior.

Use `--spp` (or `--samples-per-pixel`) to control path tracing sampling:

```bash
cargo run -p cherry-app -- --backend=ray.montecarlo --spp=8 --frames=1
```

Use `--cpu-threads` to control CPU multi-core worker count (omit for auto/default):

```bash
cargo run -p cherry-app -- --backend=ray.montecarlo --spp=8 --cpu-threads=8 --frames=1
```

`ray.montecarlo` is the path tracing backend ID (kept for compatibility). Path tracing controls:

```bash
cargo run -p cherry-app -- --backend=ray.montecarlo --spp=8 --rr-start-depth=3 --rr-min-survival=0.05 --indirect-clamp=10 --direct-lighting=true --frames=1
```

Use `--exposure` to control display mapping (Reinhard) for both `ray.montecarlo` and `ray.spectral`.
Example with the spectral backend (hero wavelength sampling, CIE XYZ mapping):

```bash
cargo run -p cherry-app -- --backend=ray.spectral --spp=8 --exposure=1.25 --frames=1
```

Use `--init-gpu` to run `wgpu` adapter/device initialization at render start:

```bash
cargo run -p cherry-app -- --backend=ray.normal --init-gpu --frames=1
```

Use camera controls (pose + optics) from CLI:

```bash
cargo run -p cherry-app -- --backend=ray.normal --width=640 --height=400 --camera-look-from-z=3.2 --camera-look-at-y=-0.05 --camera-fov=45 --camera-aperture=0.08 --camera-focal-distance=3.2 --frames=1
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
