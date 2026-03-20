# Parallax User Guide

This guide explains how Parallax works today and how to use the current desktop workflow effectively.

## Mental Model

Parallax is designed around a small set of strong rules:

- One shared Rust engine computes every analytical result
- Every meaningful analysis action becomes an explicit command
- The desktop always operates locally
- Undo and redo move through command state, not hidden UI state

That means the visual interface is a front end to a deterministic analysis session, not a separate source of truth.

## Desktop Layout

When you launch Parallax, the window is split into two main regions.

### Left rail

The left rail contains:

- Command presets for the built-in demo session
- Tool selection for rectangle or polygon gating
- Undo, redo, and reset controls
- The population list
- The command log
- Bridge feedback and error reporting

### Main analysis area

The main area contains two linked scatter plots:

- `FSC-A vs SSC-A`
- `CD3 vs CD4`

The selected population controls which events are highlighted across both plots.

## Populations and Parenting

Parallax always treats the currently selected population as the parent for the next gate you create.

- If `All Events` is selected, the new gate becomes a root population
- If a child population is selected, the new gate becomes a child of that population

After a successful gate creation, Parallax automatically selects the newly created population so you can continue refining the hierarchy.

## Gating Tools

### Rectangle Tool

Use the rectangle tool when you want an axis-aligned gate.

How it works:

- Select `Rectangle Tool`
- Drag on either scatter plot
- Release to commit the gate

What happens next:

- The rectangle is converted into a `rectangle_gate` command
- The Rust engine replays the command log
- The new population appears in the population list
- Highlighting updates across both plots

### Polygon Tool

Use the polygon tool when a rectangular boundary is too coarse.

How it works:

- Select `Polygon Tool`
- Left-click to place each vertex
- Watch the draft path update as you move the cursor
- Right-click to commit the polygon

If you right-click before placing at least three vertices, Parallax clears the draft instead of creating an invalid gate.

## Command Log

The command log is the analytical history of the session. Each row corresponds to one applied gate command and is replayed through the shared Rust engine.

Use the command log to answer questions like:

- What did I do to get this population?
- In what order were gates applied?
- Does undo actually remove the last analytical action?

## Undo, Redo, and Reset

Parallax supports:

- `Undo`: removes the most recent command from the active log and moves it to redo state
- `Redo`: reapplies the last undone command
- `Reset Session`: clears the current command log and returns to the base demo dataset

These operations act on explicit command history, not on ad hoc widget state.

## Preset Gates

The desktop includes two preset commands for the built-in demo session:

- `Add Lymphocyte Gate`
- `Add CD3/CD4 Gate`

They are useful for smoke testing or for comparing your manual gating results against a known reference workflow.

## Built-In Demo Dataset

The current desktop opens into a small embedded demo sample. This is intentional for the present stage of the project because it keeps the gating and replay workflow testable while file-import UX is still under construction.

Current implication:

- The desktop is best understood as an interaction prototype over a deterministic analysis core
- FCS ingestion exists in Rust and the CLI, but it is not yet surfaced as a desktop import workflow

## CLI and Backend

Parallax also exposes two non-desktop entry points.

### CLI

Useful commands:

- `cargo run -p flowjoish-cli -- demo-replay`
- `cargo run -p flowjoish-cli -- inspect-fcs /path/to/file.fcs`

### Backend

Useful commands:

- `cargo run -p flowjoish-backend -- describe`
- `cargo run -p flowjoish-backend -- serve 127.0.0.1:8787`

The backend exists to preserve local/cloud parity pressure early, not to replace the desktop.

## Known Limits

Parallax does not yet include:

- Desktop file import
- Saved workspaces
- Gate editing handles
- Plot pan/zoom
- Reporting export
- Cloud sync

Those features are planned, but the current product center is still fast, explicit, reproducible analysis interactions.
