# Parallax

[![CI](https://github.com/felizvida/flowish/actions/workflows/ci.yml/badge.svg)](https://github.com/felizvida/flowish/actions/workflows/ci.yml)
[![Release](https://github.com/felizvida/flowish/actions/workflows/release.yml/badge.svg)](https://github.com/felizvida/flowish/actions/workflows/release.yml)
[![Latest Release](https://img.shields.io/github/v/release/felizvida/flowish?display_name=tag)](https://github.com/felizvida/flowish/releases)

![Parallax banner](docs/assets/parallax-banner.svg)

Parallax is a local-first cytometry workstation built around one shared Rust engine, an explicit command log, and a native Qt/QML desktop. It is designed for teams who care about speed, deterministic results, and a clean handoff between interactive desktop work and reproducible execution.

Today, Parallax is an early but real workstation shell. You can launch the desktop, start from the bundled demo sample or import one or many `.fcs` files, switch between samples inside one local session, assign cohort labels to samples, author rectangle and polygon gates directly on linked scatter plots, inspect a native histogram view for channel distributions, review per-population counts, frequencies, means, and medians, define replayable derived metrics such as positive fractions and mean ratios, compare the selected population across loaded samples, inspect grouped cohort summaries, export active-sample, selected-population, derived-metric, cohort-summary, or batch stats to CSV, apply replayable compensation and transform settings, adjust plot views through explicit view actions, inspect the command log, and undo or redo gate actions through the same replayable state model.

## Why Parallax

- Fast, trustworthy cytometry analysis
- One Rust engine shared across desktop and backend surfaces
- Explicit command-log replay instead of hidden state
- Local-first behavior by default
- A native desktop shell meant to feel precise, not web-like

## Documentation

- [Quick Start](docs/QUICKSTART.md)
- [User Guide](docs/USER_GUIDE.md)
- [Tutorial](docs/TUTORIAL.md)
- [Must-Have Feature Matrix](docs/MUST_HAVE_FEATURE_MATRIX.md)
- [Real-World Testing](docs/REAL_WORLD_TESTING.md)
- [Deployment Guide](docs/DEPLOYMENT.md)
- [Operations Guide](docs/OPERATIONS.md)
- [Release Notes](docs/releases/v0.3.0.md)
- [Architecture Decision Record](docs/architecture/adr-0001-rust-qt-rust-backend.md)

## Current Capabilities

- Deterministic gating and replay in a shared Rust core
- FCS parsing crate for ingestion and metadata inspection
- Qt/QML desktop with live rectangle and polygon gate authoring
- Desktop FCS import and multi-sample switching in one local session
- Workspace save/load that reopens sessions from sample sources plus replayable command history
- Parsed FCS compensation toggle plus per-channel linear, signed-log10, asinh, biexponential, and logicle transform presets
- Native histogram panel with population-aware highlighting for imported or demo samples
- Population stats panel with matched-event counts, parent/all frequencies, and per-channel mean/median summaries
- CSV export for active-sample population stats
- Active-sample gate-template application across the other loaded samples
- Batch population-stats CSV export across all loaded samples
- Derived metric controls for replayable positive-fraction and mean-ratio formulas on the selected population
- Cross-sample comparison panel for the selected population, with deltas versus the active sample plus derived-metric values and filtered comparison/derived-metric CSV export
- Persisted sample group labels plus cohort summary cards, cohort-level derived-metric means, and cohort-summary CSV export
- Replayable plot-view controls for auto extents, focus-on-population, and zoom in/out
- Command log with undo and redo
- Rust backend stub for parity-focused service surfaces
- CLI tools for FCS inspection and replay demos

## Current Limits

- Multi-factor cohort labels, group template tools, and richer cohort-review layouts are not implemented yet
- Workspace persistence is source-path based today; bundled raw-data snapshots and derived caches are not implemented yet
- Derived metrics are limited to positive fraction and mean ratio today; custom formula editors and spreadsheet-style expressions are not implemented yet
- Compensation override editing, density plots, and reference-matched transform tuning are not implemented yet
- Gate editing handles, pan/zoom, and figure/report export are not implemented yet
- Cloud sync, jobs, and AI assistance are future phases

## Repository Layout

- `crates/flowjoish-core`: deterministic core, command log, gating kernel
- `crates/flowjoish-fcs`: FCS ingestion and metadata parsing on the shared core
- `crates/flowjoish-cli`: CLI for FCS inspection and replay demos
- `crates/flowjoish-desktop-bridge`: Rust FFI bridge for the desktop shell
- `crates/flowjoish-backend`: Rust backend stub for local/cloud parity pressure
- `apps/desktop-qt`: native Qt/QML desktop application

## Build And Run

Run the full test suite:

```bash
cargo test
```

Configure and build the desktop:

```bash
cmake -S apps/desktop-qt -B build/desktop-qt
cmake --build build/desktop-qt
./build/desktop-qt/flowjoish-desktop
```

Describe the backend surface:

```bash
cargo run -p flowjoish-backend -- describe
```

Hydrate and run the authentic public FCS suite:

```bash
python3 scripts/real_world_fcs_suite.py
```

Reuse local source checkouts when you already have them:

```bash
python3 scripts/real_world_fcs_suite.py \
  --source-root fcsparser=/tmp/fcsparser \
  --source-root flowio=/tmp/flowio \
  --source-root flowcal=/tmp/flowcal
```

## Community

- [Contributing Guide](CONTRIBUTING.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)
- [Security Policy](SECURITY.md)
- [Support](SUPPORT.md)

## License

Parallax is released under the Apache License 2.0. See [LICENSE](LICENSE).

## Internal Naming

The repository and crate identifiers still use the `flowjoish-*` naming scheme while the product brand is Parallax. That keeps the codebase stable while we shape the public-facing product and avoids unnecessary churn before packaging and distribution harden.
