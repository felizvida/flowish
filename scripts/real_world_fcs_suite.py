#!/usr/bin/env python3
"""
Hydrate and validate Parallax against an authentic public FCS corpus.

The suite is intentionally local-first:
- it reuses a verified cache when available
- it can copy bytes from local source checkouts
- it only downloads from pinned raw GitHub URLs when needed
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = REPO_ROOT / "testdata" / "real_world_fcs_manifest.json"
DEFAULT_CACHE_DIR = REPO_ROOT / "testdata" / ".cache" / "real_world_fcs"
DEFAULT_CLI = REPO_ROOT / "target" / "debug" / "flowjoish-cli"
AUTO_SOURCE_ROOTS = {
    "fcsparser": Path("/tmp/fcsparser"),
    "flowio": Path("/tmp/flowio"),
    "flowcal": Path("/tmp/flowcal"),
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Hydrate and validate the authentic real-world FCS suite."
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        default=DEFAULT_MANIFEST,
        help="Path to the suite manifest JSON.",
    )
    parser.add_argument(
        "--cache-dir",
        type=Path,
        default=DEFAULT_CACHE_DIR,
        help="Directory where collected FCS files are cached.",
    )
    parser.add_argument(
        "--cli",
        type=Path,
        default=DEFAULT_CLI,
        help="Path to the flowjoish CLI binary.",
    )
    parser.add_argument(
        "--source-root",
        action="append",
        default=[],
        metavar="NAME=PATH",
        help="Use a local checkout for a manifest source, for example fcsparser=/tmp/fcsparser.",
    )
    parser.add_argument(
        "--dataset",
        action="append",
        default=[],
        help="Run only the named dataset id. Repeat to select multiple ids.",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=None,
        help="Run only the first N selected datasets.",
    )
    parser.add_argument(
        "--refresh",
        action="store_true",
        help="Re-copy or re-download even when a cached file already exists.",
    )
    parser.add_argument(
        "--no-download",
        action="store_true",
        help="Do not fetch from the network; require cache hits or local source roots.",
    )
    parser.add_argument(
        "--hydrate-only",
        action="store_true",
        help="Collect the corpus into the cache without invoking the parser.",
    )
    parser.add_argument(
        "--require-all-pass",
        action="store_true",
        help="Treat expected parser failures in the manifest as hard suite failures.",
    )
    parser.add_argument(
        "--report",
        type=Path,
        default=None,
        help="Write a JSON report to the given path.",
    )
    return parser.parse_args()


def load_manifest(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def source_roots_from_args(values: list[str]) -> dict[str, Path]:
    roots: dict[str, Path] = {}
    for name, path in AUTO_SOURCE_ROOTS.items():
        if path.exists():
            roots[name] = path
    for value in values:
        if "=" not in value:
            raise SystemExit(f"invalid --source-root '{value}', expected NAME=PATH")
        name, raw_path = value.split("=", 1)
        roots[name] = Path(raw_path).expanduser().resolve()
    return roots


def sanitize_relative_path(relative_path: str) -> Path:
    return Path(urllib.parse.unquote(relative_path))


def sha256_path(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def ensure_cache_file(
    dataset: dict[str, Any],
    cache_dir: Path,
    source_roots: dict[str, Path],
    refresh: bool,
    allow_download: bool,
) -> tuple[Path, str]:
    relative_path = sanitize_relative_path(dataset["relative_path"])
    cache_path = cache_dir / dataset["source"] / relative_path
    expected_hash = dataset["sha256"]

    if cache_path.exists() and not refresh and sha256_path(cache_path) == expected_hash:
        return cache_path, "cache"

    if cache_path.exists():
        cache_path.unlink()

    cache_path.parent.mkdir(parents=True, exist_ok=True)

    source_root = source_roots.get(dataset["source"])
    if source_root is not None:
        local_source_path = source_root / relative_path
        if local_source_path.exists():
            shutil.copy2(local_source_path, cache_path)
            verify_cached_hash(cache_path, expected_hash, dataset["id"])
            return cache_path, "local-copy"

    if not allow_download:
        raise RuntimeError(
            f"{dataset['id']}: cache miss and no local source available; re-run without --no-download"
        )

    download_to_cache(dataset["download_url"], cache_path)
    verify_cached_hash(cache_path, expected_hash, dataset["id"])
    return cache_path, "download"


def download_to_cache(url: str, destination: Path) -> None:
    with tempfile.NamedTemporaryFile(dir=destination.parent, delete=False) as handle:
        temp_path = Path(handle.name)
    try:
        with urllib.request.urlopen(url) as response, temp_path.open("wb") as output:
            shutil.copyfileobj(response, output)
        temp_path.replace(destination)
    finally:
        if temp_path.exists():
            temp_path.unlink()


def verify_cached_hash(path: Path, expected_hash: str, dataset_id: str) -> None:
    actual_hash = sha256_path(path)
    if actual_hash != expected_hash:
        raise RuntimeError(
            f"{dataset_id}: sha256 mismatch, expected {expected_hash} but found {actual_hash}"
        )


def run_cli(cli_path: Path, dataset_path: Path) -> subprocess.CompletedProcess[str]:
    if cli_path.exists():
        result = subprocess.run(
            [str(cli_path), "inspect-fcs-json", str(dataset_path)],
            text=True,
            capture_output=True,
        )
        if not should_fallback_to_cargo(result):
            return result

    return subprocess.run(
        [
            "cargo",
            "run",
            "-q",
            "-p",
            "flowjoish-cli",
            "--",
            "inspect-fcs-json",
            str(dataset_path),
        ],
        cwd=REPO_ROOT,
        text=True,
        capture_output=True,
    )


def should_fallback_to_cargo(result: subprocess.CompletedProcess[str]) -> bool:
    stderr = (result.stderr or "").strip()
    if result.returncode == 0 or not stderr.startswith("usage:"):
        return False
    return "inspect-fcs-json" not in stderr


def compare_success(actual: dict[str, Any], expected: dict[str, Any]) -> list[str]:
    mismatches: list[str] = []
    for key in (
        "version",
        "event_count",
        "parameter_count",
        "data_type",
        "byte_order",
        "metadata_keys",
    ):
        if actual.get(key) != expected.get(key):
            mismatches.append(
                f"{key}: expected {expected.get(key)!r}, found {actual.get(key)!r}"
            )

    expected_dimension = expected.get("compensation_dimension")
    expected_source = expected.get("compensation_source")
    actual_compensation = actual.get("compensation")
    actual_dimension = None
    actual_source = None
    if isinstance(actual_compensation, dict):
        actual_dimension = actual_compensation.get("dimension")
        actual_source = actual_compensation.get("source_key")

    if expected_dimension != actual_dimension:
        mismatches.append(
            f"compensation_dimension: expected {expected_dimension!r}, found {actual_dimension!r}"
        )
    if expected_source != actual_source:
        mismatches.append(
            f"compensation_source: expected {expected_source!r}, found {actual_source!r}"
        )

    return mismatches


def evaluate_dataset(
    cli_path: Path,
    dataset: dict[str, Any],
    cached_path: Path,
    require_all_pass: bool,
) -> tuple[str, list[str], dict[str, Any] | None]:
    proc = run_cli(cli_path, cached_path)
    expected = dataset["expected"]

    if expected["status"] == "pass":
        if proc.returncode != 0:
            stderr = proc.stderr.strip() or "parser exited without stderr"
            return "fail", [f"expected parse success but parser failed: {stderr}"], None
        try:
            actual = json.loads(proc.stdout)
        except json.JSONDecodeError as error:
            return "fail", [f"inspect-fcs-json returned invalid JSON: {error}"], None
        mismatches = compare_success(actual, expected)
        if mismatches:
            return "fail", mismatches, actual
        return "pass", [], actual

    if proc.returncode == 0:
        if require_all_pass:
            return "pass", [], json.loads(proc.stdout)
        return "fail", ["expected parser failure but parsing succeeded"], json.loads(proc.stdout)

    stderr = proc.stderr.strip()
    expected_fragment = expected["error_contains"]
    if expected_fragment not in stderr:
        return (
            "fail",
            [
                "parser failed, but stderr did not contain expected fragment: "
                f"{expected_fragment!r}; stderr was {stderr!r}"
            ],
            None,
        )

    if require_all_pass:
        return "fail", [f"parser still fails on authentic file: {stderr}"], None
    return "xfail", [stderr], None


def dataset_selection(
    manifest: dict[str, Any], dataset_ids: list[str], limit: int | None
) -> list[dict[str, Any]]:
    datasets = manifest["datasets"]
    if dataset_ids:
        selected_ids = set(dataset_ids)
        datasets = [dataset for dataset in datasets if dataset["id"] in selected_ids]
        missing = selected_ids.difference(dataset["id"] for dataset in datasets)
        if missing:
            raise SystemExit(f"unknown dataset ids: {', '.join(sorted(missing))}")
    if limit is not None:
        datasets = datasets[:limit]
    return datasets


def main() -> int:
    args = parse_args()
    manifest = load_manifest(args.manifest)
    source_roots = source_roots_from_args(args.source_root)
    datasets = dataset_selection(manifest, args.dataset, args.limit)

    if not datasets:
        raise SystemExit("no datasets selected")

    args.cache_dir.mkdir(parents=True, exist_ok=True)

    summary = {
        "dataset_count": len(datasets),
        "pass": 0,
        "xfail": 0,
        "fail": 0,
        "hydrated_from_cache": 0,
        "hydrated_from_local_copy": 0,
        "hydrated_from_download": 0,
        "results": [],
    }

    for dataset in datasets:
        try:
            cached_path, hydrated_from = ensure_cache_file(
                dataset=dataset,
                cache_dir=args.cache_dir,
                source_roots=source_roots,
                refresh=args.refresh,
                allow_download=not args.no_download,
            )
        except Exception as error:  # pragma: no cover - surfaced in the report
            summary["fail"] += 1
            result = {
                "id": dataset["id"],
                "status": "fail",
                "hydrated_from": "error",
                "messages": [str(error)],
            }
            summary["results"].append(result)
            print(f"FAIL  {dataset['id']}: {error}")
            continue

        if hydrated_from == "cache":
            summary["hydrated_from_cache"] += 1
        elif hydrated_from == "local-copy":
            summary["hydrated_from_local_copy"] += 1
        elif hydrated_from == "download":
            summary["hydrated_from_download"] += 1

        if args.hydrate_only:
            summary["pass"] += 1
            result = {
                "id": dataset["id"],
                "status": "pass",
                "hydrated_from": hydrated_from,
                "cache_path": str(cached_path),
                "messages": [],
            }
            summary["results"].append(result)
            print(f"PASS  {dataset['id']}: hydrated via {hydrated_from}")
            continue

        status, messages, actual = evaluate_dataset(
            cli_path=args.cli,
            dataset=dataset,
            cached_path=cached_path,
            require_all_pass=args.require_all_pass,
        )
        summary[status] += 1
        result = {
            "id": dataset["id"],
            "status": status,
            "hydrated_from": hydrated_from,
            "cache_path": str(cached_path),
            "messages": messages,
            "actual": actual,
        }
        summary["results"].append(result)

        label = status.upper().ljust(5)
        detail = messages[0] if messages else "matched expected parser output"
        print(f"{label} {dataset['id']}: {detail}")

    if args.report is not None:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(summary, indent=2), encoding="utf-8")

    print(
        "\nSummary: "
        f"{summary['dataset_count']} datasets, "
        f"{summary['pass']} pass, "
        f"{summary['xfail']} expected failures, "
        f"{summary['fail']} failures."
    )
    print(
        "Hydration: "
        f"{summary['hydrated_from_cache']} cache, "
        f"{summary['hydrated_from_local_copy']} local copy, "
        f"{summary['hydrated_from_download']} download."
    )
    return 1 if summary["fail"] else 0


if __name__ == "__main__":
    sys.exit(main())
