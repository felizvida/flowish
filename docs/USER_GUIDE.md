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

- Sample import and sample switching
- Stats export for the active sample
- Batch template application and grouped stats export
- Derived metric configuration and derived-metric export
- Command presets for compatible samples
- Tool selection for rectangle or polygon gating
- Undo, redo, and reset controls
- The population list
- The selected population stats panel
- The command log
- Bridge feedback and error reporting

### Main analysis area

The main area contains multiple linked plot panels. In the bundled demo sample, that means:

- `FSC-A vs SSC-A`
- `CD3 vs CD4`
- a histogram panel for the first non-structural analysis channel

For imported samples, the exact plot set depends on the available channels. Parallax prefers two scatter projections when possible, then adds a histogram for a meaningful non-structural channel.

The selected population controls which events are highlighted across every plot.

## Samples And Session State

Parallax can operate on the bundled demo sample or on one or many imported `.fcs` files.

How it works today:

- Click `Import FCS Files` to load one or more files from disk
- Each imported file becomes a sample inside the same local Rust session
- The sample list lets you switch active samples without restarting the desktop
- Each sample keeps its own command log, undo state, and derived populations
- `Save Workspace As` writes a workspace document that records sample sources, active sample, per-sample command logs, and redo state
- `Load Workspace` rebuilds the session from those sample sources and replays the saved command history

The active sample card shows:

- display name
- source path
- event count
- channel count

## Analysis Settings

Parallax now includes replayed per-sample analysis settings ahead of gating and plotting.

Available controls today:

- `Apply Parsed Compensation` when the imported sample includes a compensation matrix
- per-channel transforms for `Linear`, `Signed Log10`, `Asinh (150)`, `Biexponential`, and `Logicle`

How they behave:

- compensation and transforms are replayed in Rust before every gate replay and plot refresh
- the current settings are persisted in saved workspaces
- the analysis history panel records those explicit actions separately from gate commands
- undo and redo still operate on gate commands only in the current desktop

The current `Biexponential` and `Logicle` options are fixed desktop presets rather than fully tunable reference-matched implementations.

## Plot View Controls

Each plot panel now includes explicit replayable view controls:

- `Auto` resets the plot to full-data extents
- `Focus` reframes the active projection around the currently selected population
- `Zoom In` and `Zoom Out` scale the current plot extents around the plot center

How they behave:

- plot-view actions are saved with the workspace and replayed after analysis settings and gates
- if a focused population disappears because a gate is undone, the plot falls back to auto extents instead of breaking the session
- gate undo and redo do not remove plot-view actions in the current desktop

## Histogram View

Parallax now includes a native histogram panel alongside its scatter projections.

How it behaves today:

- histograms are computed in Rust from the same replayed sample state as gating and scatter plots
- the active population highlights its own per-bin counts on top of the full distribution
- the histogram responds to `Auto`, `Focus`, `Zoom In`, and `Zoom Out` like the scatter plots
- histogram panels are read-only today and do not accept gate drawing gestures

Parallax currently chooses the histogram channel automatically, preferring a non-time, non-structural analysis channel such as a fluorescence marker when available.

## Populations and Parenting

Parallax always treats the currently selected population as the parent for the next gate you create.

- If `All Events` is selected, the new gate becomes a root population
- If a child population is selected, the new gate becomes a child of that population

After a successful gate creation, Parallax automatically selects the newly created population so you can continue refining the hierarchy.

## Population Stats

Parallax now computes a stats summary for `All Events` and every replayed population in the active sample.

What you can inspect today:

- matched-event count
- frequency of all events
- frequency of the selected population's parent
- per-channel mean
- per-channel median

How it behaves:

- stats are computed in Rust from the same processed sample used for gating and plotting
- the left-rail stats panel follows the currently selected population
- stats update immediately when gates, transforms, or compensation settings change
- `Export Stats CSV` writes the active sample's full population stats table to disk

## Derived Metrics

Parallax now includes a small replayable formula layer for the currently selected population.

What you can configure today:

- `Positive Fraction`: fraction of matched events with one channel at or above a threshold
- `Mean Ratio`: mean of one channel divided by the mean of another channel

How it behaves:

- the active derived metric is stored in the Rust session and saved with the workspace
- metric evaluation uses the same processed sample state as gating, transforms, compensation, and stats
- the selected population comparison shows the per-sample metric value and delta versus the active sample
- the cohort summary shows the cohort-level mean of that metric and delta versus the active cohort
- if a sample is missing the selected population or the configured channel, Parallax reports that explicitly instead of fabricating a value
- `Export Derived Metric CSV` writes the selected population's per-sample derived-metric table to disk

Current limit:

- derived metrics are limited to the two built-in formulas above; there is no free-form expression editor yet

## Batch Workflows

Parallax now includes an early batch workflow for loaded multi-sample sessions.

What it can do today:

- apply the active sample's gate command log as a template to the other loaded samples
- assign a persisted cohort label to each loaded sample
- evaluate one shared derived metric across that selected population in every loaded sample
- compare the currently selected population across every loaded sample
- aggregate that comparison by cohort label into grouped summaries
- export only that selected population comparison as CSV
- export only that selected population's derived-metric table as CSV
- export only the grouped cohort summary as CSV
- export grouped population stats across all loaded samples as CSV

How it behaves:

- batch template application validates the full gate log against every target sample before changing anything
- applying the template replaces gate history on the other loaded samples and clears their redo/view state
- each target sample keeps its own analysis settings such as transforms and parsed compensation
- cohort labels are saved in the workspace and travel with the local session metadata
- the cross-sample comparison panel uses the active sample as the baseline and reports per-sample deltas for frequency of all events and of parent
- the comparison panel also reports the active derived metric for each sample when it can be evaluated
- the cohort summary panel groups those per-sample rows by cohort label, reports group-level mean frequency plus mean derived metric value, and shows how many comparable samples actually contributed to that metric
- the active sample's cohort acts as the cohort-level baseline for delta reporting
- if a loaded sample does not yet contain the selected population in its own gate history, the comparison panel marks it as missing instead of fabricating values
- `Export Selected Comparison CSV` writes only the selected population comparison across the loaded samples
- `Export Derived Metric CSV` writes only the selected population's derived-metric table across the loaded samples
- `Export Cohort Summary CSV` writes only the grouped cohort summary across the loaded samples
- `Export Batch Stats CSV` writes one grouped table spanning every currently loaded sample

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
- `Reset Session`: clears command history and derived populations for the current session while keeping the loaded sample set

These operations act on explicit command history, not on ad hoc widget state.

## Preset Gates

The desktop includes two preset commands:

- `Add Lymphocyte Gate`
- `Add CD3/CD4 Gate`

They are useful for smoke testing or for comparing your manual gating results against a known reference workflow. Presets are only enabled when the active sample has the channels they require.

## Demo Sample And Real Files

The desktop still opens into a small embedded demo sample, but it no longer stops there. You can replace the demo session by importing one or many real `.fcs` files from disk.

Current implication:

- The desktop now exercises the same deterministic engine against imported files, not only the demo sample
- Workspace save/load now exists, but it depends on the original source files still being present on disk
- There is still no bundled workspace format with cached derived data
- compensation override editing, custom free-form formulas, richer grouped-comparison views, density views, and richer transform tuning are still ahead

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

- Gate editing handles
- Plot pan/zoom
- Manual plot-range entry fields
- Density plots
- Figure and report export
- Cloud sync

Those features are planned, but the current product center is still fast, explicit, reproducible analysis interactions.
