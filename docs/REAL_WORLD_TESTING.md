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
- current compatibility target: `39/39` files pass under `--require-all-pass`

The manifest stores, for every dataset:

- pinned source repo and commit
- raw download URL
- file size and `sha256`
- provenance note
- expected parser outcome for the current Parallax parser

## Compatibility Gate

This suite is meant to tell the truth about the current parser, not flatter it.

The manifest currently has no expected parser failures. Every authentic file is expected to parse cleanly, and CI runs the suite with `--require-all-pass`.

If a new authentic file exposes a known gap, prefer fixing the parser immediately. If an expected failure is unavoidable for a short period, document the failure in the manifest and open a tracking issue in the same change.

The long-term direction is to grow this corpus past `100` authentic files while preserving a zero-regression gate.

## Quick Start

Build the CLI once:

```bash
cargo build -p flowjoish-cli
```

Hydrate the local cache from local source checkouts and run the full compatibility gate:

```bash
python3 scripts/real_world_fcs_suite.py --require-all-pass
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

Run the same parser gate used by CI:

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
