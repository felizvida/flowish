# Deployment Guide

Parallax is primarily a desktop product today, but the repository also includes a small Rust backend service that can be used for parity testing and future platform work.

## Local Backend Run

Start the backend:

```bash
cargo run -p flowjoish-backend -- serve 127.0.0.1:8787
```

Check capabilities:

```bash
curl -sf http://127.0.0.1:8787/capabilities
```

Check health:

```bash
curl -sf http://127.0.0.1:8787/health
```

## Release-Oriented Pull Workflow

For a pull-based server workflow:

```bash
cd /srv/flowish
git fetch --tags origin
git checkout main
git pull --ff-only origin main
cargo run -p flowjoish-backend -- serve 127.0.0.1:8787
```

## Rollback

To move back to the previous release tag:

```bash
cd /srv/flowish
git fetch --tags origin
git checkout v0.1.0
cargo run -p flowjoish-backend -- serve 127.0.0.1:8787
```

## Notes

- The desktop application is currently distributed from source builds
- The backend is intentionally minimal and exists to preserve parity pressure with the shared engine
- No container or packaged installer is published in this release
