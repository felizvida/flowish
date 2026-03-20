# Parallax Quick Start

This quick start gets you from clone to a running Parallax desktop session as quickly as possible.

## Prerequisites

- Rust toolchain with `cargo`
- CMake 3.24+
- Qt 5 or Qt 6 with `Core`, `Gui`, `Qml`, `Quick`, and `QuickControls2`
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

When the window opens, Parallax loads a built-in demo dataset so you can exercise gating immediately.

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
- A populations list with `All Events`
- A command log panel
- Rectangle and polygon gating tools
- Undo, redo, and reset controls

For a guided first session, continue to the [Tutorial](TUTORIAL.md).
