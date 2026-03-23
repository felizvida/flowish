# Real-World Testing

Parallax ships with an authentic public FCS corpus and a cache-aware runner so we can test the parser against real instrument output instead of synthetic fixtures alone.

The suite lives in:

- `testdata/real_world_fcs_manifest.json`
- `scripts/real_world_fcs_suite.py`

## What Is In The Corpus

- `39` authentic public FCS files
- `3` pinned upstream sources: `fcsparser`, `FlowIO`, and `FlowCal`
- mixed vendors and exporters: BD, Cytek, Guava, Miltenyi, Attune-derived files, and FlowCal's documented E. coli plus calibration-bead tutorial experiment
- mixed formats and payloads: `FCS2.0`, `FCS3.0`, `FCS3.1`, integer and float data, big and little endian, files with and without compensation matrices

The manifest stores, for every dataset:

- pinned source repo and commit
- raw download URL
- file size and `sha256`
- provenance note
- expected parser outcome for the current Parallax parser

## Why Some Files Are Expected To Fail

This suite is meant to tell the truth about the current parser, not flatter it.

Today the manifest contains `10` authentic files that Parallax still fails to parse cleanly. Those expected failures are tracked on purpose so we can:

- prevent silent regressions on files that already work
- keep pressure on the parser gaps that matter in real labs
- flip failures to passes deliberately as the parser improves

Use `--require-all-pass` when you want the suite to fail on those known gaps.

## Quick Start

Build the CLI once:

```bash
cargo build -p flowjoish-cli
```

Hydrate the local cache from local source checkouts and run the full suite:

```bash
python3 scripts/real_world_fcs_suite.py
```

Prefer local source checkouts when you already have them:

```bash
python3 scripts/real_world_fcs_suite.py \
  --source-root fcsparser=/tmp/fcsparser \
  --source-root flowio=/tmp/flowio \
  --source-root flowcal=/tmp/flowcal
```

Run only one dataset:

```bash
python3 scripts/real_world_fcs_suite.py --dataset flowio-b01-kc-a-w-91-us
```

Treat every authentic parser failure as a hard failure:

```bash
python3 scripts/real_world_fcs_suite.py --require-all-pass
```

Write a JSON report:

```bash
python3 scripts/real_world_fcs_suite.py --report testdata/.cache/real_world_fcs/report.json
```

## Cache Behavior

The suite is local-first.

- If a verified cache file already exists, it is reused.
- If a local source checkout is available, the file is copied into the cache.
- If neither exists, the runner downloads the pinned raw GitHub URL.

Cached binaries live under `testdata/.cache/real_world_fcs` and are gitignored.

## Updating The Corpus

When you add a new authentic source:

1. audit provenance first
2. pin a commit, not a branch
3. record `sha256` and expected parser behavior
4. prefer authentic upstream files over hand-made fixtures

If parser behavior changes intentionally, update the manifest expectations in the same change so the suite stays honest.
