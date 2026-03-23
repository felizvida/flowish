# Must-Have Feature Matrix

This document maps the user testimonies gathered during discovery to concrete product requirements for Parallax.

The goal is simple: separate what is truly required for a viable FlowJo alternative from what is merely nice to have.

## Discovery Signals

The testimony set is remarkably consistent. Researchers rely on FlowJo for:

- daily gating and review of flow cytometry data
- compensation review and correction
- dot-density, histogram, and other plot views with adjustable scaling
- batch application of gating strategies and templates across many samples
- quantitative statistics, formulas, and publication-ready exports
- saved, reproducible analysis workflows that can be reopened later
- working away from acquisition instruments so cytometers are freed for other users

The strongest use cases in the testimony set include:

- immunophenotyping and immune-cell subset analysis
- cell cycle, proliferation, apoptosis, and DNA-damage assays
- intracellular staining and inflammatory marker analysis
- CRISPR editing, screening, and engineered-cell workflows
- mitochondrial function, viral infection rate, and membrane-protein quantification
- longitudinal and clinical-trial style batch analysis

## Current Baseline

Parallax already has a real analytical foundation:

- deterministic Rust command log and replay
- rectangle and polygon gates
- population hierarchy with parent-child gating
- undo and redo on explicit command state
- FCS parsing in Rust and CLI inspection
- authentic real-world parser regression suite

That foundation is valuable, but it is not yet enough for real lab adoption.

## Feature Matrix

| Capability | Evidence From Testimonies | Why It Is Must-Have | Current Status In Parallax | Priority | Recommended Epic |
| --- | --- | --- | --- | --- | --- |
| Desktop FCS import and sample browser | SePil Lee, Chad Williamson, Adriana Golding, Sitanshu Sarangi all describe opening acquired files and analyzing them away from the instrument | Without direct file import, the desktop cannot replace FlowJo in daily work | Partial. Desktop users can now import `.fcs` files and switch samples, but folder import, saved sessions, and downstream batch workflows are still missing | P0 | EPIC A |
| Multi-sample workspace and session management | Jack Yanovski, Deliang Zhang, Chad Williamson, Sitanshu Sarangi describe workflows across many experiments and time points | Real labs do not analyze one sample at a time in isolation | Partial. One local session can now hold multiple imported samples with per-sample command history, but there is no persisted workspace or cross-sample batch workflow yet | P0 | EPIC A |
| Compensation review, edit, and application | SePil Lee explicitly calls out detecting improper compensation and correcting it immediately; Jack Yanovski names compensation as a core function | Compensation is foundational to trustworthy flow analysis | Partial. The desktop can now inspect parsed compensation availability and explicitly apply the parsed matrix, but custom override editing and compensation QC tooling are still missing | P0 | EPIC B |
| Plot system beyond scatter plots | SePil Lee, Keita Saeki, Deliang Zhang, Marcela Teatin Latancia all depend on richer visual analysis | A Flow workstation needs density, histogram, and flexible visualization, not just scatter | Partial. Two scatter plots exist today; histogram and density views do not | P0 | EPIC C |
| Axis scaling and transforms | SePil Lee mentions customizable axis scales; many assays require logicle or biexponential transforms for interpretation | Marker intensity analysis depends on the right transform model | Partial. The desktop now supports replayed `Linear`, `Signed Log10`, `Asinh`, `Biexponential`, and `Logicle` transform presets, plus explicit `Auto`, `Focus`, and zoom plot-view controls, but it still lacks histogram/density views, manual axis entry, and fully tunable reference-matched transform implementations | P0 | EPIC B |
| Saved workspaces | Marcela Teatin Latancia, Jack Yanovski, Deliang Zhang, Chad Williamson all emphasize reproducibility and reuse | Reopening analysis state is table stakes for real use | Partial. The desktop can now save and reopen source-path-based workspaces with per-sample command history, but there is no bundled workspace format, derived cache, or portability layer yet | P0 | EPIC D |
| Gating templates and batch application | Marcela Teatin Latancia, Jack Yanovski, Deliang Zhang, Sitanshu Sarangi all describe repeated analysis across many files | This is one of the clearest speed and reproducibility advantages FlowJo has today | Not implemented | P0 | EPIC D |
| Statistics engine | Marcela Teatin Latancia, Keita Saeki, Anup Dey, Tina Maio need quantitative outputs, not only plots | Researchers need counts, frequencies, central tendency, and assay-specific summaries | Not implemented | P1 | EPIC E |
| Custom formulas and derived metrics | SePil Lee explicitly mentions customized formulas; Tina Maio describes converting signal into quantitative metrics | Labs need derived values, not only raw gated counts | Not implemented | P1 | EPIC E |
| Export to CSV and Excel | SePil Lee and others need direct table export for downstream analysis and reporting | No export means no practical downstream workflow | Not implemented | P1 | EPIC F |
| Publication-quality figure export | Jack Waite, Deliang Zhang, Anup Dey, Chad Williamson, Guillaume Gaud all mention figure generation for meetings and manuscripts | Publication-grade output is a central reason people use FlowJo | Not implemented | P1 | EPIC F |
| Comparison workflows across samples and conditions | Jack Waite, Jack Yanovski, Antony Cougnoux, Guillaume Gaud, Tina Maio all compare treated vs control, time points, or longitudinal cohorts | A single-sample tool is insufficient for modern biology workflows | Not implemented | P1 | EPIC G |
| Gate editing, quadrant gates, and navigation polish | Daily users need to refine gates precisely and quickly across projections | Initial gate creation alone is not enough for expert analysis | Partial. Rectangle and polygon creation exist, but no gate handles, quadrant gate, or pan/zoom | P1 | EPIC H |
| Parser compatibility with real instrument output | Deliang Zhang cites broad file compatibility; Pedro Pereira Da Rocha points out lack of viable alternatives | If authentic cytometer files fail to load, trust collapses immediately | Partial. The authentic suite has 39 files with 10 known expected failures | P0 | EPIC I |
| Reproducibility and audit trail at workspace level | Marcela Teatin Latancia, Jack Yanovski, Deliang Zhang, Chad Williamson all emphasize consistency and reproducibility | The command log is a strong start, but users need saved and exportable analysis lineage | Partial. Command replay exists, but workspace persistence and report export do not | P1 | EPIC D |

## Evidence-Led Prioritization

The testimonies point to a very clear order of operations.

### P0: Required before the product is a credible FlowJo replacement

- desktop FCS import
- multi-sample workspace model
- compensation workflow
- transforms and axis scaling
- histogram and density plots
- workspace save/load
- batch templates and template application
- parser compatibility improvement on authentic public files

### P1: Required soon after P0 for real lab adoption

- statistics engine
- custom formulas
- CSV and Excel export
- publication-quality figure export
- comparison workflows
- gate editing, quadrant gating, and pan/zoom

### P2: Important, but not the current proof of value

- cloud sync
- background jobs
- AI copilot
- retrieval and semantic workspace search

## Recommended Epic Map

### EPIC A — Desktop Ingestion And Multi-Sample Sessions

Deliver:

- import one or many FCS files from disk
- sample list and sample switching
- session model for multi-sample analysis

Acceptance:

- open a folder of FCS files in the desktop
- switch between samples without restarting
- preserve selection, gating tree, and sample context in one session

### EPIC B — Compensation And Transforms

Deliver:

- inspect parsed compensation matrices
- apply or override compensation
- linear, logicle, and biexponential transforms
- axis scaling controls

Acceptance:

- compensation changes update plots and populations deterministically
- transformed axes remain stable across sessions and replays

### EPIC C — Real Plot System

Deliver:

- scatter
- dot density
- histogram
- density view

Acceptance:

- users can switch plot type per panel
- axis settings and transforms are visible and adjustable
- interaction remains smooth on large event counts

### EPIC D — Workspace Persistence And Batch Templates

Deliver:

- save/load workspace
- reusable gating templates
- apply template across groups of samples

Acceptance:

- a saved workspace reopens identically
- template application across N samples is deterministic
- command-log lineage remains preserved

### EPIC E — Stats And Formulas

Deliver:

- count
- frequency
- mean and median
- positive fractions
- custom formulas over gated populations

Acceptance:

- results are reproducible and exportable
- formulas recalculate correctly after gating changes

### EPIC F — Exports

Deliver:

- CSV and Excel table export
- figure export for plots and layouts
- PDF-ready outputs

Acceptance:

- exported numbers match on-screen analysis
- exported figures are publication-ready without manual recreation elsewhere

### EPIC G — Sample Comparison

Deliver:

- compare samples side by side
- compare grouped conditions and time points
- summary tables across groups

Acceptance:

- users can analyze control vs treatment and longitudinal cohorts directly in Parallax

### EPIC H — Expert Gating UX

Deliver:

- editable gates
- quadrant gates
- pan and zoom
- gate handles and refinement tools

Acceptance:

- users can iteratively refine a gate without recreating it from scratch

### EPIC I — Real-World Parser Hardening

Deliver:

- close known parser gaps exposed by the authentic test suite
- expand authentic-file coverage as new sources are added

Acceptance:

- convert current expected failures to passing cases deliberately
- maintain zero unexpected regressions in the authentic suite

## Immediate Recommendation

If we want to move the product meaningfully toward the testimony-defined target, the next engineering sequence should be:

1. EPIC A: desktop import plus multi-sample sessions
2. EPIC B: compensation plus transforms
3. EPIC C: histogram and density plots
4. EPIC D: workspace save/load and batch templates
5. EPIC I: parser hardening in parallel with the above

That sequence keeps the product focused on the real promise users care about:

- fast
- trustworthy
- reproducible
- practical for daily flow cytometry work

## What This Matrix Explicitly Does Not Prioritize Yet

The testimonies do not justify spending early effort on:

- AI-first auto-gating
- fancy dashboards
- plugin marketplace work
- real-time multi-user editing
- browser-first UI at the expense of desktop performance

Those may matter later, but they are not the core job users are hiring the product to do.
