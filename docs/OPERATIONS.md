# Operations Guide

## Validation Commands

Run these before shipping:

```bash
cargo test
python3 -m unittest discover -s tests -p 'test_*.py'
cmake --build build/desktop-qt
```

## Desktop Troubleshooting

If the desktop fails to configure:

- verify `qmake` is on your `PATH`
- pass `-DCMAKE_PREFIX_PATH="$(qmake -query QT_INSTALL_PREFIX)"` to CMake

If the desktop fails to link:

- rebuild the Rust bridge with `cargo build -p flowjoish-desktop-bridge`
- rerun `cmake --build build/desktop-qt`

## Backend Troubleshooting

If the backend fails to start:

- confirm the port is free
- run `cargo run -p flowjoish-backend -- describe`
- inspect the bind address you passed to `serve`

## Release Checklist

- `cargo test` passes
- Python regression tests pass
- desktop build succeeds
- desktop startup smoke check succeeds
- docs are updated
- changelog is current
- release notes are written
- tag is created from the intended commit

## Recovery Notes

- Undo and redo in the desktop operate on explicit command state
- Plot-view actions and transform presets are persisted in workspaces and replay after load
- Resetting the desktop session clears per-sample command logs and derived populations
- Saved workspaces can restore imported samples and command history, but only if the original source files are still available at the recorded paths
