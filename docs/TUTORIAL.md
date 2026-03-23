# Parallax Tutorial

This tutorial walks through a complete first session in Parallax using the bundled demo sample.

By the end, you will:

- create a root rectangle gate
- create a child polygon gate
- apply a transform preset
- refocus a plot on the active population
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

When Parallax opens, it loads a demo sample with two scatter plots:

- `FSC-A vs SSC-A`
- `CD3 vs CD4`

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

## Step 6. Apply A Transform Preset

In the `Analysis Settings` panel, change the transform for `CD3` or `CD4`.

Good first choices:

- `Asinh (150)` for a softer compression
- `Biexponential` or `Logicle` to preview the broader transform families now available in the desktop

Expected result:

- the scatter plot updates immediately
- the analysis history gains a new transform action
- the workspace will remember the transform if you save it later

## Step 7. Refocus A Plot

Select your child population in the population list, then click `Focus` above one of the plots.

Expected result:

- the plot range tightens around the selected population
- the plot subtitle shows a new view summary
- the workspace will remember this view action

## Step 8. Use Undo and Redo

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

## Step 9. Reset the Session

Click `Reset Session`.

Expected result:

- the command log clears
- derived populations disappear
- the view returns to `All Events`

This gives you a clean slate without restarting the application.

## Step 10. Compare Against the CLI

If you want to see the same replay philosophy outside the desktop, run:

```bash
cargo run -p flowjoish-cli -- demo-replay
```

That command prints:

- the command log as canonical JSON
- the execution hash
- matched-event counts for the replayed populations

## Step 11. Save The Session

Click `Save Workspace As` if you want to persist the current desktop session.

What gets saved:

- the sample list and active sample
- the command log for each sample
- analysis settings such as transforms and parsed compensation
- plot-view actions such as focus and zoom
- redo state for each sample

What is required when you reopen it later:

- the original referenced `.fcs` files must still be available at the saved paths

## What You Learned

You just used the three core ideas Parallax is built on:

- analysis actions are explicit commands
- hierarchy comes from selected-parent context
- results can be replayed deterministically

For a broader reference on the desktop workflow, continue with the [User Guide](USER_GUIDE.md).
