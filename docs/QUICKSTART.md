# Parallax Quick Start

This quick start gets you from clone to a running Parallax desktop session as quickly as possible.

## Prerequisites

- Rust toolchain with `cargo`
- CMake 3.24+
- Qt 5 or Qt 6 with `Core`, `Gui`, `Qml`, `Quick`, `QuickControls2`, and `Widgets`
- `qmake` on your `PATH`, or a known Qt installation prefix

## 1. Verify the workspace

Run the test suite first:

```bash
cargo test
```

This validates the shared Rust engine, the FCS parser, the backend stub, and the desktop bridge.

## 2. Configure the desktop app

If `qmake` is available on your `PATH`, the desktop CMake project will try to discover Qt automatically:

```bash
cmake -S apps/desktop-qt -B build/desktop-qt
```

If CMake cannot find Qt, point it at the Qt prefix directly:

```bash
cmake -S apps/desktop-qt -B build/desktop-qt -DCMAKE_PREFIX_PATH="$(qmake -query QT_INSTALL_PREFIX)"
```

## 3. Build the desktop

```bash
cmake --build build/desktop-qt
```

The desktop executable will be written to:

```text
build/desktop-qt/flowjoish-desktop
```

## 4. Launch Parallax

```bash
./build/desktop-qt/flowjoish-desktop
```

When the window opens, Parallax loads a built-in demo sample so you can exercise gating immediately.

To work with real data:

- click `Import FCS Files`
- choose one or more `.fcs` files
- use the sample list in the left rail to switch between imported samples
- if the sample includes a parsed spillover matrix, use `Apply Parsed Compensation`
- use the channel transform controls to switch between `Linear`, `Signed Log10`, `Asinh (150)`, `Biexponential`, and `Logicle`
- use the `Auto`, `Focus`, `Zoom In`, and `Zoom Out` controls above each plot to adjust plot extents through replayable view actions
- use `Save Workspace As` to persist the current local session
- use `Load Workspace` to reopen a saved session later, as long as the referenced source files are still available

## 5. Optional backend and CLI checks

Describe the local backend capability surface:

```bash
cargo run -p flowjoish-backend -- describe
```

Run the backend server:

```bash
cargo run -p flowjoish-backend -- serve 127.0.0.1:8787
```

Run the replay demo:

```bash
cargo run -p flowjoish-cli -- demo-replay
```

Inspect an FCS file from the CLI:

```bash
cargo run -p flowjoish-cli -- inspect-fcs /path/to/file.fcs
```

## What You Should See

- A Parallax desktop window with two scatter plots
- A sample list with the active sample highlighted
- A populations list with `All Events`
- A command log panel
- Analysis settings for compensation and transforms
- Plot view controls for focus and zoom
- Rectangle and polygon gating tools
- Undo, redo, and reset controls

For a guided first session, continue to the [Tutorial](TUTORIAL.md).
