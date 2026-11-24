"""Reproducible pass-pair sampling from the pinned real-contract corpus.

Capture is explicitly unchecked. Selection includes rejected and unsupported
regions, and verification totals count only the selected checks. Preserve source,
compiler, semantics and input fingerprints, output identity, timings and reports.
"""

import argparse
import hashlib
import importlib.util
import json
import os
import re
import subprocess
import sys
import time
from collections import defaultdict
from itertools import zip_longest
from pathlib import Path

from . import audit
from .check import run

ROOT = Path(__file__).resolve().parents[2]
TV = ROOT / "solar-tv"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def stable(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def inputs(case):
    # Reuse the benchmark's import/remapping resolver and pinned archives.
    name = "solar_tv_runtime_inputs"
    spec = importlib.util.spec_from_file_location(
        name, ROOT / "benches/runtime/common.py"
    )
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    archive = ROOT / case["archive"]
    if digest(archive.read_bytes()) != case["sha256"]:
        raise ValueError(f"archive fingerprint changed: {archive}")
    project = module.load_project(archive)
    return {
        "language": "Solidity",
        "sources": module.project_slice(project, case["source"]),
        "settings": {
            "optimizer": {"enabled": True, "runs": 200},
            "evmVersion": "cancun",
            "viaIR": True,
            "metadata": {"appendCBOR": False, "bytecodeHash": "none"},
            "remappings": project["settings"].get("remappings", []),
            "outputSelection": {
                case["source"]: {
                    case["contract"]: [
                        "abi",
                        "evm.bytecode.object",
                        "evm.deployedBytecode.object",
                    ]
                }
            },
        },
    }


def captures(folder):
    rows = []
    for stream in sorted((folder / "captures").glob("*/regions.jsonl")):
        pair = stream.parent
        for line in stream.read_text().splitlines():
            record = json.loads(line)
            if record.get("version") != 1 or record.get("status") != "captured":
                raise ValueError("invalid capture record")
            layer = record["layer"]
            paths = [pair / f"{side}.{layer}" for side in ("before", "after")]
            key = {**record, "sources": [digest(path.read_bytes()) for path in paths]}
            rows.append({**record, "id": digest(stable(key)), "paths": paths})
    return rows


def select(rows, limit, seed):
    groups = defaultdict(lambda: defaultdict(list))
    for row in rows:
        groups[row["layer"]][row["pass"]].append(row)
    queues = []
    for layer in sorted(groups):
        queues.append(
            [
                sorted(group, key=lambda row: digest(f"{seed}:{row['id']}".encode()))
                for _, group in sorted(
                    groups[layer].items(),
                    key=lambda item: digest(f"{seed}:{item[0]}".encode()),
                )
            ]
        )
    # Alternate layers and passes before taking a second item from any group.
    interleaved = [group for batch in zip_longest(*queues) for group in batch if group]
    ordered = [row for batch in zip_longest(*interleaved) for row in batch if row]
    return ordered[:limit]


def pass_coverage(rows, checks):
    passes = {}
    for row in rows:
        key = (row["layer"], row["pass"], row["target"])
        record = passes.setdefault(
            key,
            {
                "layer": key[0],
                "pass": key[1],
                "target": key[2],
                "captured": 0,
                "unselected": 0,
                "statuses": dict.fromkeys(audit.STATUSES, 0),
            },
        )
        record["captured"] += 1
        record["unselected"] += 1
    for check in checks:
        record = passes[check["layer"], check["pass"], check["target"]]
        record["unselected"] -= 1
        record["statuses"][check["status"]] += 1
    return [
        {
            **record,
            "unverified": record["captured"] - record["statuses"]["verified"],
        }
        for _, record in sorted(passes.items())
    ]


def compile_input(solar, source, directory, *, capture=False, timeout=120):
    args = [solar, "--standard-json", "-j1"]
    if capture:
        args += [f"-Zsolar-tv={directory}", "-Zsolar-tv-capture"]
    start = time.monotonic()
    result = subprocess.run(
        args,
        input=json.dumps(source),
        text=True,
        capture_output=True,
        timeout=timeout,
        check=False,
    )
    (directory / ("capture.stderr" if capture else "baseline.stderr")).write_text(
        result.stderr
    )
    value = json.loads(result.stdout)
    errors = [
        error for error in value.get("errors", []) if error.get("severity") == "error"
    ]
    if result.returncode or errors:
        raise ValueError(f"compiler failed: {errors or result.stderr}")
    contracts = value.get("contracts", {})
    if not contracts:
        raise ValueError("compiler produced no contract outputs")
    (directory / ("capture.json" if capture else "baseline.json")).write_text(
        json.dumps(value)
    )
    return digest(stable(contracts)), time.monotonic() - start


def final_call_case(case, row):
    region = row["region"]
    return (
        bool(case.get("final_call"))
        and row["module"] == case["contract"]
        and row["layer"] == "mir"
        and row["pass"] == "lower-builtins"
        and row["target"] == "cancun"
        and not row["rejection"]
        and region
        and region["kind"] == "function"
        and region["name"] == case["final_call"]
    )


def audit_command(solar, row, target, timeout):
    replay = Path(solar).with_name(
        "solar-tv-replay" + (".exe" if os.name == "nt" else "")
    )
    return [
        str(TV / "solar-tv"),
        row["layer"],
        *row["paths"],
        "--solar",
        solar,
        "--replay",
        str(replay),
        "--timeout",
        str(timeout),
        "--evm-version",
        row["target"],
        "--report",
        target,
    ]


def checked_status(process, record):
    status = record.get("status")
    if status not in audit.STATUSES:
        raise ValueError("checker returned an unknown status")
    expected = 0 if status == "verified" else 1 if status == "counterexample" else 2
    if record.get("version") != 1 or process.returncode != expected:
        raise ValueError("checker failed outside its status protocol")
    return status


def check_entry(solar, row, target, timeout):
    log = target.with_suffix(".log")
    try:
        process = run(
            [*audit_command(solar, row, target, timeout), "--entry-trace"],
            ROOT,
            timeout * 8,
        )
    except subprocess.TimeoutExpired as error:
        log.write_text(str(error))
        record = {
            "version": 1,
            "status": "timeout",
            "diagnostic": "entry checker wall-time budget",
        }
        audit.write(target, record)
        return record
    log.write_text(process.stdout + process.stderr)
    record = json.loads(target.read_text())
    checked_status(process, record)
    return record


def covers_region(record, region):
    if record["status"] == "counterexample":
        return True
    if (
        record["status"] != "verified"
        or record.get("region", {}).get("kind") != "entry"
    ):
        return False
    label = int(region["name"].removeprefix("bb"))
    sides = record.get("import_correspondence", [])
    return len(sides) == 2 and all(
        side.get("physical_entry") and label in side.get("block_labels", ())
        for side in sides
    )


def evaluate_case(case, solar, folder, limit, seed, timeout, *, passes=()):
    folder.mkdir()
    source = inputs(case)
    (folder / "input.json").write_text(json.dumps(source))
    baseline, baseline_seconds = compile_input(solar, source, folder)
    captured, capture_seconds = compile_input(solar, source, folder, capture=True)
    if baseline != captured:
        raise ValueError("capture changed compiler output")
    rows = captures(folder)
    if not rows:
        raise ValueError("no changed pass pairs captured")
    eligible = [row for row in rows if not passes or row["pass"] in passes]
    chosen = select(eligible, limit, seed)
    results = []
    dispatch_reports = {}
    entry_reports = {}
    for index, row in enumerate(chosen):
        target = folder / f"check-{index:03}.json"
        start = time.monotonic()
        region = row["region"]
        prefix = (
            row["layer"] == "evmir"
            and region
            and region["kind"] == "window"
            and row["rejection"] == "incoming EVM stack height is unknown"
        )
        universal_region = (
            row["layer"] == "evmir"
            and region
            and region["kind"] == "region"
            and row["rejection"] == "incoming EVM stack height is unknown"
        )
        dispatch = row["layer"] == "mir" and row["pass"] == "lower-dispatch"
        debug_reverts = dispatch and row.get("debug_reverts", False)
        dispatch_key = (*row["paths"], row["target"], debug_reverts)
        prune = row["layer"] == "mir" and (
            dispatch
            or row["rejection"]
            in {
                "function was removed or its identity changed",
                "shared MIR declarations or phase changed; no function-local claim",
            }
        )
        entry = None
        if (
            row["layer"] == "evmir"
            and row["pass"]
            in {"block-layout", "loop-layout", "compact-pushes", "cfg-simplify-early"}
            and region
        ):
            key = (*row["paths"], row["target"])
            if key not in entry_reports:
                entry_path = folder / f"entry-{index:03}.json"
                entry_reports[key] = (
                    entry_path,
                    check_entry(solar, row, entry_path, timeout),
                )
            candidate = entry_reports[key]
            if covers_region(candidate[1], region):
                entry = candidate
        if entry is not None:
            target, record = entry
        elif dispatch and dispatch_key in dispatch_reports:
            target, record = dispatch_reports[dispatch_key]
        elif row["rejection"] and not (prefix or universal_region or prune):
            record = {
                "version": 1,
                "status": "unsupported",
                "diagnostic": row["rejection"],
            }
            audit.write(target, record)
        else:
            command = audit_command(solar, row, target, timeout)
            if prune:
                command.append("--prune")
                if debug_reverts:
                    command.append("--debug-reverts")
            elif region := row["region"]:
                command += [
                    "--prefix" if prefix else f"--{region['kind']}",
                    region["name"],
                ]
                if region["stack_height"] is not None:
                    command += ["--stack-height", str(region["stack_height"])]
            exact_call = final_call_case(case, row)
            if exact_call:
                bundle = folder / f"check-{index:03}-call"
                command = [
                    str(TV / "solar-tv"),
                    "call-budget",
                    "--solar",
                    solar,
                    "--output",
                    bundle,
                    "--timeout",
                    str(timeout),
                    "--mir-pair",
                    *row["paths"],
                ]
            try:
                process = run(command, ROOT, timeout * 8)
                (folder / f"check-{index:03}.log").write_text(
                    process.stdout + process.stderr
                )
                if exact_call:
                    if process.returncode == 0:
                        artifact = json.loads((bundle / "result.json").read_text())
                        if artifact["compiler_output_sha256"] != baseline:
                            raise ValueError(
                                "final CALL proof used a different compiler output"
                            )
                        record = json.loads((bundle / "mir.json").read_text())
                    else:
                        record = {
                            "version": 1,
                            "status": "unsupported",
                            "diagnostic": process.stderr,
                        }
                    audit.write(target, record)
                record = json.loads(target.read_text())
                checked_status(process, record)
            except subprocess.TimeoutExpired as error:
                (folder / f"check-{index:03}.log").write_text(str(error))
                record = {
                    "version": 1,
                    "status": "timeout",
                    "diagnostic": "checker wall-time budget",
                }
                audit.write(target, record)
        if dispatch:
            dispatch_reports[dispatch_key] = (target, record)
        results.append(
            {
                "id": row["id"],
                "layer": row["layer"],
                "pass": row["pass"],
                "target": row["target"],
                "status": record["status"],
                "capture_rejection": row["rejection"],
                "proof_mode": record.get("region", {}).get("proof", "original"),
                "seconds": time.monotonic() - start,
                "report": str(target.relative_to(folder.parent)),
            }
        )
    return {
        "case": case["name"],
        "archive_sha256": case["sha256"],
        "input_sha256": digest(stable(source)),
        "output_sha256": baseline,
        "output_identical": True,
        "baseline_seconds": baseline_seconds,
        "capture_seconds": capture_seconds,
        "captured": len(rows),
        "eligible": len(eligible),
        "selected": len(chosen),
        "unselected": len(rows) - len(chosen),
        "checks": results,
        "pass_coverage": pass_coverage(rows, results),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--solar", type=Path, default=ROOT / "target/debug/solar")
    parser.add_argument("--manifest", type=Path, default=TV / "evaluation.json")
    parser.add_argument("--output", type=Path, required=True)
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument(
        "--limit",
        type=int,
        default=24,
        help="checks per contract, including unsupported regions",
    )
    selection.add_argument(
        "--all",
        dest="limit",
        action="store_const",
        const=None,
        help="check every captured region for each selected contract",
    )
    parser.add_argument(
        "--require-verified",
        action="store_true",
        help="with --all, fail if any eligible region remains unverified",
    )
    parser.add_argument("--seed", default="0")
    parser.add_argument(
        "--timeout", type=int, default=15, help="seconds per prover invocation"
    )
    parser.add_argument("--case", action="append", default=[])
    parser.add_argument(
        "--pass",
        dest="passes",
        action="append",
        default=[],
        help="select a pass (repeatable)",
    )
    args = parser.parse_args()
    if os.environ.get("SOLAR_TV_GAS"):
        parser.error("unset SOLAR_TV_GAS for the pinned sufficient-gas evaluation")
    if (
        args.limit is not None and not 1 <= args.limit <= 128
    ) or not 1 <= args.timeout <= 120:
        parser.error("limit must be 1..128 and timeout 1..120")
    if args.require_verified and args.limit is not None:
        parser.error("--require-verified needs --all")
    if args.output.exists():
        parser.error("output must be a fresh directory")
    args.output = args.output.resolve()
    manifest = json.loads(args.manifest.read_text())
    if manifest.get("version") != 1:
        parser.error("unsupported evaluation manifest")
    cases = [
        case for case in manifest["cases"] if not args.case or case["name"] in args.case
    ]
    if not cases or set(args.case) - {case["name"] for case in cases}:
        parser.error("unknown or empty case selection")
    if len({case["name"] for case in cases}) != len(cases) or any(
        not re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}", case["name"])
        for case in cases
    ):
        parser.error("case names must be unique path-safe identifiers")
    solar = str(args.solar.resolve())
    args.output.mkdir(parents=True)
    report = {
        "version": 1,
        "compiler_sha256": digest(Path(solar).read_bytes()),
        "replay_sha256": digest(
            Path(solar)
            .with_name("solar-tv-replay" + (".exe" if os.name == "nt" else ""))
            .read_bytes()
        ),
        "compiler_version": subprocess.check_output(
            [solar, "--version"], text=True
        ).strip(),
        "semantics": audit.semantics(),
        "manifest_sha256": digest(args.manifest.read_bytes()),
        "seed": args.seed,
        "limit": args.limit,
        "timeout": args.timeout,
        "selected_passes": sorted(set(args.passes)),
        "cases": [],
        "expected_cases": len(cases),
        "sampling": (
            "all eligible captured regions, including rejected and unsupported regions"
            if args.limit is None
            else "round-robin across layers and passes, stable hash order within groups; includes rejected and unsupported regions"
        ),
        "errors": [],
    }
    for case in cases:
        try:
            report["cases"].append(
                evaluate_case(
                    case,
                    solar,
                    args.output / case["name"],
                    args.limit,
                    args.seed,
                    args.timeout,
                    passes=args.passes,
                )
            )
        except (OSError, ValueError, subprocess.TimeoutExpired) as error:
            report["errors"].append({"case": case["name"], "error": str(error)})
        audit.write(args.output / "results.json", report)
    missing = set(args.passes) - {
        row["pass"] for case in report["cases"] for row in case["pass_coverage"]
    }
    if missing:
        report["errors"].append(
            {"error": "no captures for passes: " + ", ".join(sorted(missing))}
        )
    counts = {status: 0 for status in audit.STATUSES}
    for case in report["cases"]:
        for check in case["checks"]:
            counts[check["status"]] += 1
    report["totals"] = counts
    report["semantics_unchanged"] = report["semantics"] == audit.semantics()
    audit.write(args.output / "results.json", report)
    print(
        json.dumps(
            {
                "cases": len(report["cases"]),
                "totals": counts,
                "errors": report["errors"],
            },
            indent=2,
        )
    )
    return int(
        bool(
            report["errors"]
            or counts["counterexample"]
            or counts["error"]
            or not report["semantics_unchanged"]
            or (
                args.require_verified
                and any(
                    case["eligible"]
                    != sum(c["status"] == "verified" for c in case["checks"])
                    for case in report["cases"]
                )
            )
        )
    )
