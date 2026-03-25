# AGENTS.md

This file defines maintenance rules for `cherry-rs` so future changes preserve architecture and behavior.

## 1) Workspace Contracts

- Keep the repository as a Cargo workspace rooted at `Cargo.toml`.
- Preserve crate roles:
  - `cherry-core`: scene/math/domain contracts only (including spectral domain primitives/utilities).
  - `cherry-render`: backend-agnostic render orchestration, typed backend contracts, and runtime-erased registry.
  - `cherry-backend-raster`: raster backend implementations.
  - `cherry-backend-ray`: ray backend implementations and accel/tracer strategy implementations.
  - `cherry-app`: thin CLI runner and shared app-level runtime setup (`build_registry`, `build_animated_scene_provider`, output naming).
  - `cherry-gui`: thin native GUI runner for interactive preview (`eframe` + `egui`) built on shared app-level runtime setup.
- Do not reintroduce a monolithic root `src/` application.

## 2) Dependency Direction (Must Stay Acyclic)

- `cherry-core` must not depend on render/backends/app crates.
- `cherry-render` may depend on `cherry-core`, but not on backend crates.
- Backend crates may depend on `cherry-core` and `cherry-render`.
- `cherry-app` may depend on all crates.
- `cherry-gui` may depend on `cherry-app`, `cherry-core`, and `cherry-render`.
- No crate may depend on `cherry-gui`.

## 3) Public API Invariants

- `FrameRequest` remains the core frame unit for still and animation rendering.
- Time-varying scenes are provided through `SceneProvider::snapshot(time) -> SceneSnapshot`.
- Backend selection stays runtime-pluggable via `BackendRegistry` + `BackendId`.
- Progressive output remains event-driven through `FrameSink`.
- Backend event order must be: `Begin` -> zero or more `Scanline` events -> `End`.
- `RenderBackend` remains typed (`type Pixel: PixelRadiance`) with `render_frame_typed(...)`; the default `render_frame(...)` bridge must continue emitting `FrameEvent`s and producing `RenderResult`.
- `BackendRegistry` remains runtime-erased (`ErasedRenderBackend` factory storage), while backend implementations stay typed.
- `FrameEvent::Scanline` keeps RGB pixels and optional spectral payload (`Option<Vec<SpectralBins>>`).
- Trait-object scene/material ownership in public APIs should stay `Arc`-based, not borrowed lifetimes.

## 4) Backend Extension Rules

- New backends must implement `RenderBackend` and provide a `register_backends(...)` helper (or an additive variant like `register_backends_with_exposure(...)` when needed).
- Backend IDs should be stable, lowercase, and dot-scoped (example: `ray.normal`, `raster.simple`).
- Backend crates must include smoke tests that render a minimal scene without panics.
- Ray backends should continue using pluggable tracing strategy objects and the `Accel` abstraction.
- If a backend produces spectral data, expose it through `PixelRadiance::spectral_bins()` so scanline events can carry optional spectral bins.

## 5) Testing and Quality Gates

Before merge, run and pass:

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo test --workspace`

When changing contracts:

- Update/add tests in `cherry-render/tests` for orchestration/registry behavior.
- Update/add backend smoke tests in each backend crate.
- Update/add `cherry-app` tests when shared runtime setup behavior changes.
- Update/add `cherry-gui` tests when GUI state transitions, worker event handling, or preview buffering changes.
- Keep tests deterministic (fixed seeds or deterministic sampling).

## 6) App/Runner Scope

- Keep `cherry-app` thin. It should orchestrate/configure and host shared app-level setup helpers, not own renderer internals.
- Keep `cherry-gui` thin. It should orchestrate UI + event plumbing, not duplicate renderer internals.
- GUI v1 invariants:
  - Single-frame preview mode (no sequence playback yet).
  - Manual render trigger (`Render` button), not auto-start.
  - Single active render at a time; controls disabled while rendering.
  - Progressive preview updates are driven by `FrameSink` scanline events.
  - Preview-only output (no export/save pipeline required in v1).
- Output naming for animation frames should remain deterministic and index-based.
- Video encoding is out of scope for renderer core; treat it as a separate layer/tool.

## 7) Change Management

- If you break a public contract, update:
  - crate-level docs/comments where the contract is defined,
  - `README.md`,
  - this `AGENTS.md` if governance rules changed.
- Prefer additive evolution over disruptive refactors unless there is a clear architectural reason.
