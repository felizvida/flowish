# Parallax Tutorial

This tutorial walks through a complete first session in Parallax using the bundled demo sample.

By the end, you will:

- create a root rectangle gate
- create a child polygon gate
- create a histogram range gate
- refine a selected gate with exact fields or plot handles
- apply a transform preset
- review or override compensation when a real sample provides spillover metadata
- refocus a plot on the active population
- export a high-resolution plot PNG or PDF
- export a one-page plot report PDF
- inspect population stats and export them
- configure and export a derived metric
- inspect the resulting population hierarchy
- use undo and redo
- understand what the command log is tracking

## Before You Start

Build and launch the desktop:

```bash
cmake -S apps/desktop-qt -B build/desktop-qt
cmake --build build/desktop-qt
./build/desktop-qt/flowjoish-desktop
```

When Parallax opens, it loads a demo sample with two scatter plots and a histogram:

- `FSC-A vs SSC-A`
- `CD3 vs CD4`
- a histogram for the first analysis channel

If you want to use real files instead, click `Import FCS Files` and choose one or more `.fcs` files. The rest of the interaction model stays the same, but the exact plots and preset availability will depend on the channels in your imported sample.

## Step 1. Start from All Events

Look at the population list on the left.

You should see:

- `All Events`

Make sure `All Events` is selected before you create the first gate. That ensures the next gate becomes a root population.

## Step 2. Create a Rectangle Gate

1. Select `Rectangle Tool`.
2. In the `FSC-A vs SSC-A` plot, drag a rectangle around the lower-left cluster.
3. Release the mouse button to commit the gate.

For the built-in demo data, a good rectangle is roughly:

- `x: 0 to 35`
- `y: 0 to 35`

Expected result:

- a new population appears in the list
- the command log gains one `rectangle_gate`
- the highlighted event count on the scatter plots drops to `3`

You have just created the same root gate that the preset `Lymphocyte Gate` command uses.

## Step 3. Inspect the Parenting Behavior

Click the new population in the population list.

This matters because Parallax uses the selected population as the parent for your next gate. Any gate you create now will become a child of this rectangle gate.

## Step 4. Create a Polygon Gate

1. Select `Polygon Tool`.
2. Move to the `CD3 vs CD4` plot.
3. Left-click four vertices around the upper-left cluster.
4. Right-click to commit the polygon.

For the built-in demo sample, a good polygon is close to:

- `(0, 7)`
- `(6, 7)`
- `(6, 10)`
- `(0, 10)`

Expected result:

- a new child population appears in the population list
- that new population is internally parented to the population you had selected when you drew it
- the command log gains one `polygon_gate`
- the highlighted event count becomes `2`

## Step 5. Read the Command Log

Look at the command log after the two gates.

You should now see two ordered entries:

1. a rectangle gate
2. a polygon gate

This is the important Parallax idea: your analysis is represented as an ordered, replayable sequence of explicit commands.

## Step 6. Refine A Gate

Select `lymphocytes` in the population list, then refine the gate in either of two ways:

1. Use `Gate Refinement` to tighten one of the rectangle bounds, then click `Append Gate Edit`.
2. Select `Edit Tool`, then drag the selected rectangle's corner, edge, or body directly on the scatter plot.
3. If a polygon population is selected, drag one of its vertices or drag inside the polygon to move the whole gate.

Expected result:

- the command log gains one `update_rectangle_gate`
- the rectangle or polygon overlay moves to the edited geometry
- child populations recompute from the edited parent gate
- undo removes only the refinement command and restores the previous gate geometry

## Step 7. Create A Histogram Range Gate

Drag horizontally across the histogram panel to define a one-channel range. For precise thresholds, type numeric values into the histogram panel's `Exact range` min/max fields and click `Apply Range`. For quick midpoint shortcuts, click `Low Gate` for the visible low half or `High Gate` for the visible high half.

Expected result:

- a new population appears in the list
- the command log gains one `range_gate`
- the histogram highlights the bins that fall in the gated range

For `Low Gate`, Parallax uses the visible minimum through the midpoint. For `High Gate`, it uses the midpoint through the visible maximum. Use `Zoom In`, `Zoom Out`, or `Auto` first if you want to change the visible range before using either shortcut.

After creating a range gate, select it in the population list and choose `Edit Tool`. Drag the range's left handle, right handle, or filled body to append an `update_range_gate` command without re-authoring the gate from scratch.

## Step 8. Apply A Transform Preset

In the `Analysis Settings` panel, change the transform for `CD3` or `CD4`.

Good first choices:

- `Asinh (150)` for a softer compression
- `Biexponential` or `Logicle` to preview the broader transform families now available in the desktop

Expected result:

- the scatter plot updates immediately
- the analysis history gains a new transform action
- the workspace will remember the transform if you save it later

If you are working with an imported file that includes compensation metadata, inspect `Compensation QC` in the same panel. Use `Apply Parsed Compensation` to apply the parsed FCS matrix. If the parsed matrix is wrong for the assay, paste a spillover string such as `2,FITC-A,PE-A,1,0.08,0.02,1` into `Manual Compensation Override` and click `Apply Override`.

Expected result for an override:

- QC marks the matrix source as a manual override
- the analysis history gains a replayable compensation override action
- plots, gates, stats, and derived metrics recompute from the overridden compensated values

## Step 9. Refocus And Pan A Plot

Select your child population in the population list, then click `Focus` above one of the plots.

To inspect nearby events without changing the gate, select `Pan Tool` in the command tools and drag across a scatter or histogram panel. Pan is recorded as a replayable view action, so reopening the workspace preserves the same visual context.

For exact axis control, click `View Fields`, edit the plot's `View` x/y min/max fields, and click `Set View`.

Expected result:

- the plot range tightens around the selected population
- the plot subtitle shows a new view summary
- the workspace will remember this view action
- if you focus the histogram panel instead, its x-range tightens around the selected population's distribution
- if you pan after focusing, the subtitle changes to `Panned view`
- if you use exact bounds, the subtitle changes to `Manual view range`

## Step 10. Export A Plot Figure

Click `Export PNG` or `Export PDF` above one of the plots and choose a destination.

Expected result:

- Parallax captures a high-resolution figure of that plot card
- interaction controls are hidden during the capture
- the export keeps the plot title, axis label, highlighted count, and current gate overlays
- PDF export places the same capture onto a page-oriented PDF for review or sharing

## Step 10b. Export A Plot Report

Click `Export Plot Report PDF` in the left rail and choose a destination.

Expected result:

- Parallax captures the visible plot cards with interaction controls hidden
- the report PDF places the current plot panels together on one landscape page
- the report reflects the current sample, gates, transforms, selected population, and plot-view ranges

## Step 11. Inspect Population Stats

Look at the `Population Stats` panel in the left rail while your child population is selected.

Expected result:

- the matched-event count reflects the selected population
- you can see its percentage of all events and of its parent population
- each channel shows a mean and median for the selected population

If you want a file output, click `Export Stats CSV` and save the active sample's stats table.

## Step 12. Configure A Derived Metric

Use the `Derived Metric` panel in the left rail while your child population is still selected.

Try one of these:

- `Positive Fraction` on `CD3` with a threshold near `2.0`
- `Mean Ratio` with `CD4` as the numerator and `CD3` as the denominator

Expected result:

- the selected population comparison picks up a per-sample derived-metric value
- if you loaded multiple samples, the cohort summary also shows the cohort-level mean for that metric
- `Export Derived Metric CSV` saves the selected population's derived-metric table

## Step 13. Optional Batch Workflow

If you imported more than one compatible sample, keep your current sample selected and click `Apply Template To Other Samples`.

Expected result:

- the current gate command log is copied onto the other loaded samples
- you can type cohort labels such as `Control` and `Treated` into the `Active Sample Group` field as you switch samples
- those samples become immediately ready for the same population stats workflow
- the `Cross-Sample Comparison` panel shows the selected population side by side, marking the active sample as the baseline
- the `Cross-Sample Comparison` panel also reports the active derived metric for each sample
- the `Cohort Summary` panel rolls those rows up by cohort label and compares group means and derived-metric means against the active cohort
- samples that do not yet contain that population are called out explicitly instead of being merged into the baseline
- you can click `Export Selected Comparison CSV` to save just that side-by-side comparison
- you can click `Export Derived Metric CSV` to save just the selected population's derived-metric values
- you can click `Export Cohort Summary CSV` to save the grouped cohort summary
- you can then click `Export Batch Stats CSV` to write one grouped table across all loaded samples

## Step 14. Use Undo and Redo

Click `Undo`.

Expected result:

- the polygon gate disappears
- the selected population falls back if the removed population was active
- the command count drops by one

Then click `Redo`.

Expected result:

- the polygon gate reappears
- the command count returns to two

Note that undo and redo currently apply to gate commands only. Transform and plot-view actions remain explicit session state, but are not yet part of the undo stack.

## Step 15. Reset the Session

Click `Reset Session`.

Expected result:

- the command log clears
- derived populations disappear
- the view returns to `All Events`

This gives you a clean slate without restarting the application.

## Step 16. Compare Against the CLI

If you want to see the same replay philosophy outside the desktop, run:

```bash
cargo run -p flowjoish-cli -- demo-replay
```

That command prints:

- the command log as canonical JSON
- the execution hash
- matched-event counts for the replayed populations

## Step 17. Save The Session

Click `Save Workspace As` if you want to persist the current desktop session.

Use `Save Portable Bundle` instead when you want the analysis, imported FCS files, and integrity metadata to travel together as a `.parallax` directory.

What gets saved:

- the sample list and active sample
- the command log for each sample
- analysis settings such as transforms, parsed compensation, and manual compensation overrides
- the active derived metric definition
- plot-view actions such as focus and zoom
- redo state for each sample

What is required when you reopen it later:

- source-path workspace files require the original referenced `.fcs` files to stay available at the saved paths
- portable bundles can be reopened with `Load Bundle` because copied FCS files live inside the bundle's `samples/` directory and are checked against saved integrity metadata

## What You Learned

You just used the three core ideas Parallax is built on:

- analysis actions are explicit commands
- hierarchy comes from selected-parent context
- results can be replayed deterministically

For a broader reference on the desktop workflow, continue with the [User Guide](USER_GUIDE.md).
