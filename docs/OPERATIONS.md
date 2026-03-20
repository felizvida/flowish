# Operations Guide

## Validation Commands

Run these before shipping:

```bash
cargo test
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
- desktop build succeeds
- docs are updated
- changelog is current
- release notes are written
- tag is created from the intended commit

## Recovery Notes

- Undo and redo in the desktop operate on explicit command state
- Resetting the desktop session clears the command log and derived populations
- The current desktop uses a built-in demo dataset, so there is no user file import state to recover in this release
