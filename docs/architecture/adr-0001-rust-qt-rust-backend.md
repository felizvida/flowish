# ADR 0001: Rust Engine + Qt/QML Desktop + Rust Backend

## Status

Accepted on March 20, 2026.

## Context

The product goal is not generic analytics software. It is fast, trustworthy, reproducible cytometry analysis that can scale from local workstations to cloud execution without changing scientific results.

The non-negotiables are:

- One engine everywhere
- Local-first workflows
- Replayable command logs
- No silent AI actions
- Performance as a core feature

We evaluated three implementation directions:

1. Rust + Qt/QML + Rust backend
2. Rust + Tauri/web UI + Rust backend
3. Rust + egui or iced + Rust backend

## Decision

We will use:

- Rust for the scientific core and backend services
- Qt/QML for the desktop shell and interaction layer
- A thin Rust FFI bridge between QML-facing desktop code and the shared engine

## Why

- Qt/QML gives the strongest path to a professional desktop UX for dense, interaction-heavy analysis workflows.
- Rust keeps the analysis engine shared between desktop and cloud, reducing drift risk.
- A Rust backend preserves local/cloud parity and avoids splitting core logic across languages.
- The Qt layer remains a shell, not a second engine.

## Consequences

### Positive

- Better long-term desktop interaction model for gating, plots, and multi-pane workspaces
- Clear separation between UI shell and deterministic engine
- Easier golden testing across local and service execution paths

### Negative

- Qt integration adds build and packaging complexity
- FFI boundaries must stay disciplined
- Desktop hiring is more specialized than a web-only stack

## Guardrails

- No scientific logic in QML or C++
- No alternate backend implementation in another language
- Every user action must resolve to a serializable Rust command
- Desktop must still work with the backend unavailable
