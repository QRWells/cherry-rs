# AGENTS.md

This file defines maintenance rules for `cherry-rs` so future changes preserve architecture and behavior.

## 1) Workspace Contracts

- Keep the repository as a Cargo workspace rooted at `Cargo.toml`.
- Preserve crate roles:
  - `cherry-core`: scene/math/domain contracts only.
  - `cherry-render`: backend-agnostic render orchestration and registry.
  - `cherry-backend-raster`: raster backend implementations.
  - `cherry-backend-ray`: ray backend implementations and accel/tracer strategy implementations.
  - `cherry-app`: thin runner/CLI only.
- Do not reintroduce a monolithic root `src/` application.

## 2) Dependency Direction (Must Stay Acyclic)

- `cherry-core` must not depend on render/backends/app crates.
- `cherry-render` may depend on `cherry-core`, but not on backend crates.
- Backend crates may depend on `cherry-core` and `cherry-render`.
- `cherry-app` may depend on all crates.

## 3) Public API Invariants

- `FrameRequest` remains the core frame unit for still and animation rendering.
- Time-varying scenes are provided through `SceneProvider::snapshot(time) -> SceneSnapshot`.
- Backend selection stays runtime-pluggable via `BackendRegistry` + `BackendId`.
- Progressive output remains event-driven through `FrameSink`.
- Backend event order must be: `Begin` -> zero or more progress events -> `End`.
- Trait-object scene/material ownership in public APIs should stay `Arc`-based, not borrowed lifetimes.

## 4) Backend Extension Rules

- New backends must implement `RenderBackend` and provide a `register_backends(...)` helper.
- Backend IDs should be stable, lowercase, and dot-scoped (example: `ray.normal`, `raster.simple`).
- Backend crates must include smoke tests that render a minimal scene without panics.
- Ray backends should continue using pluggable tracing strategy objects and the `Accel` abstraction.

## 5) Testing and Quality Gates

Before merge, run and pass:

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo test --workspace`

When changing contracts:

- Update/add tests in `cherry-render/tests` for orchestration/registry behavior.
- Update/add backend smoke tests in each backend crate.
- Keep tests deterministic (fixed seeds or deterministic sampling).

## 6) App/Runner Scope

- Keep `cherry-app` thin. It should orchestrate and configure, not own renderer internals.
- Output naming for animation frames should remain deterministic and index-based.
- Video encoding is out of scope for renderer core; treat it as a separate layer/tool.

## 7) Change Management

- If you break a public contract, update:
  - crate-level docs/comments where the contract is defined,
  - `README.md`,
  - this `AGENTS.md` if governance rules changed.
- Prefer additive evolution over disruptive refactors unless there is a clear architectural reason.
