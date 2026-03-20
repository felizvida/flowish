# Contributing to Parallax

Thanks for contributing. Parallax is still early, so the highest-value contributions are the ones that make the core sharper: deterministic analysis behavior, faster interaction, clearer documentation, and safer release tooling.

## Principles

- Keep one shared engine as the source of truth
- Prefer explicit commands over hidden UI state
- Preserve local-first behavior
- Treat performance regressions as real regressions
- Make analysis behavior reproducible and testable

## Development Setup

Run the full test suite:

```bash
cargo test
```

Build the desktop:

```bash
cmake -S apps/desktop-qt -B build/desktop-qt
cmake --build build/desktop-qt
```

Launch the desktop:

```bash
./build/desktop-qt/flowjoish-desktop
```

## Pull Request Expectations

- Keep changes focused and easy to review
- Add or update tests when behavior changes
- Update documentation when the user-facing workflow changes
- Do not move scientific logic into QML or C++
- Call out any reproducibility or performance tradeoffs in the PR description

## Good First Contribution Areas

- Gating UX polish
- Plot interaction improvements
- Workspace persistence
- FCS ingestion hardening
- Documentation clarity
- CI reliability

## Before Opening A PR

- Run `cargo test`
- Rebuild the desktop if you touched the Qt application
- Sanity-check the command-log behavior if you changed desktop interactions
- Make sure README and docs still reflect the actual product behavior
