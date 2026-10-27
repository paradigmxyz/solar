#!/usr/bin/env python3
"""Runs solsymdiff over every deployable contract of a set of sources."""

from __future__ import annotations

import argparse
import collections
import concurrent.futures
import gzip
import json
import pathlib
import re
import subprocess
import sys
import time
from typing import Any

SOLSYMDIFF = pathlib.Path(__file__).resolve().parents[1] / "bin" / "solsymdiff"
# Project sources that are not deployed: tests, scripts, and test libraries.
PROJECT_EXCLUDE = r"(^|/)(tests?|scripts?|forge-std|ds-test)/|\.[ts]\.sol$"
PROJECT_OUTPUTS = [
    "abi",
    "evm.bytecode.linkReferences",
    "evm.bytecode.object",
    "evm.deployedBytecode.immutableReferences",
    "evm.deployedBytecode.linkReferences",
    "evm.deployedBytecode.object",
    "evm.methodIdentifiers",
    "storageLayout",
]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Compare every deployable contract of the given sources with "
            "solsymdiff --all-functions."
        )
    )
    parser.add_argument("sources", nargs="+", type=pathlib.Path)
    parser.add_argument("--out", type=pathlib.Path, required=True)
    parser.add_argument("--solc", default="solc")
    parser.add_argument("--solar", default="target/debug/solar")
    parser.add_argument(
        "--exclude",
        default=PROJECT_EXCLUDE,
        help="source units of Standard JSON projects whose contracts are skipped",
    )
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument(
        "--timeout",
        type=float,
        default=600.0,
        help="seconds per contract",
    )
    argv = sys.argv[1:] if argv is None else argv
    # Arguments after `--` are passed to every solsymdiff run.
    split = argv.index("--") if "--" in argv else len(argv)
    args = parser.parse_args(argv[:split])
    extra = argv[split + 1 :]
    args.out.mkdir(parents=True, exist_ok=True)

    jobs = []
    for source in args.sources:
        if source.name.endswith((".json", ".json.gz")):
            found = _project_contracts(source.resolve(), args)
        else:
            found = [
                (source.resolve(), contract, [])
                for contract in _deployable_contracts(source.resolve(), args.solc)
            ]
        if not found:
            # A source that stops compiling must not silently leave the campaign.
            print(f"error: {source} has no deployable contract", file=sys.stderr)
            return 2
        jobs.extend(found)
    print(f"{len(jobs)} contracts", file=sys.stderr)
    results = []
    with (
        (args.out / "results.jsonl").open("w") as output,
        concurrent.futures.ThreadPoolExecutor(args.jobs) as pool,
    ):
        futures = [
            pool.submit(_run, source, contract, args, [*attempts, *extra])
            for source, contract, attempts in jobs
        ]
        for future in concurrent.futures.as_completed(futures):
            result = future.result()
            results.append(result)
            # Written as results arrive, so a run cut short keeps them.
            output.write(json.dumps(result, sort_keys=True) + "\n")
            output.flush()
            print(
                f"{result['status']:<18} {result['contract']} ({result['seconds']}s)",
                file=sys.stderr,
            )
    results.sort(key=lambda result: (result["source"], result["contract"]))
    summary = _summary(results)
    (args.out / "summary.md").write_text(summary)
    print(summary)
    return 1 if any(result["status"] == "mismatch" for result in results) else 0


def _deployable_contracts(source: pathlib.Path, solc: str) -> list[str]:
    request = {
        "language": "Solidity",
        "sources": {source.name: {"urls": [source.name]}},
        "settings": {
            "outputSelection": {"*": {"*": ["evm.bytecode.object"]}},
        },
    }
    # Relative imports resolve against the source's directory.
    result = subprocess.run(
        [solc, "--standard-json", "--base-path", str(source.parent)],
        input=json.dumps(request),
        capture_output=True,
        text=True,
        check=False,
    )
    try:
        output = json.loads(result.stdout)
    except json.JSONDecodeError:
        return []
    contracts = output.get("contracts", {}).get(source.name, {})
    return sorted(
        name
        for name, artifact in contracts.items()
        if artifact.get("evm", {}).get("bytecode", {}).get("object")
    )


def _project_contracts(
    archive: pathlib.Path, args: argparse.Namespace
) -> list[tuple[pathlib.PurePosixPath, str, list[str]]]:
    """Compiles a Standard JSON project once with each compiler.

    Returns every deployable contract outside `--exclude` with the saved
    attempts that solsymdiff compares, so a project keeps its own settings and
    is not recompiled per contract.
    """
    data = archive.read_bytes()
    request = json.loads(gzip.decompress(data) if archive.suffix == ".gz" else data)
    exclude = re.compile(args.exclude)
    units = [unit for unit in request["sources"] if not exclude.search(unit)]
    settings = request.setdefault("settings", {})
    # Saved attempts must name the EVM version; this is current Solc's default.
    settings.setdefault("evmVersion", "osaka")
    settings["metadata"] = {"bytecodeHash": "none"}
    settings["outputSelection"] = {
        unit: {"*": PROJECT_OUTPUTS, "": ["ast"]} for unit in units
    }
    serialized = json.dumps(request)
    name = archive.name.removesuffix(".gz").removesuffix(".json")
    attempts = {}
    stdout = {}
    for label, compiler in (("solc", args.solc), ("solar", args.solar)):
        directory = (args.out / "projects" / name / label).resolve()
        directory.mkdir(parents=True, exist_ok=True)
        process = subprocess.run(
            [compiler, "--standard-json"],
            input=serialized,
            capture_output=True,
            text=True,
            check=False,
        )
        (directory / "input.json").write_text(serialized)
        (directory / "stdout.txt").write_text(process.stdout)
        (directory / "result.json").write_text(
            json.dumps({"returncode": process.returncode})
        )
        attempts[label] = ["--" + label + "-attempt", str(directory)]
        stdout[label] = process.stdout
    try:
        output = json.loads(stdout["solc"])
    except json.JSONDecodeError:
        return []
    for error in output.get("errors", []):
        if error.get("severity") == "error":
            print(error.get("formattedMessage", error), file=sys.stderr)
    # Libraries have bytecode but are not called directly.
    libraries = {
        (unit, node.get("name"))
        for unit in units
        for node in output.get("sources", {})
        .get(unit, {})
        .get("ast", {})
        .get("nodes", [])
        if node.get("nodeType") == "ContractDefinition"
        and node.get("contractKind") == "library"
    }
    return [
        (
            pathlib.PurePosixPath(unit),
            contract,
            [*attempts["solc"], *attempts["solar"]],
        )
        for unit in units
        for contract, artifact in sorted(
            output.get("contracts", {}).get(unit, {}).items()
        )
        if artifact.get("evm", {}).get("bytecode", {}).get("object")
        and (unit, contract) not in libraries
    ]


def _run(
    source: pathlib.PurePath,
    contract: str,
    args: argparse.Namespace,
    extra: list[str],
) -> dict[str, Any]:
    command = [
        sys.executable,
        str(SOLSYMDIFF),
        "--source",
        str(source),
        "--contract",
        contract,
        "--all-functions",
        "--solc",
        args.solc,
        "--solar",
        args.solar,
        "--timeout",
        str(args.timeout),
        "--output-root",
        str(args.out / "runs"),
        *extra,
    ]
    start = time.monotonic()
    process = subprocess.run(command, capture_output=True, text=True, check=False)
    result: dict[str, Any] = {
        # A project's source unit is relative and keeps its path.
        "source": source.name if source.is_absolute() else source.as_posix(),
        "contract": contract,
        "seconds": round(time.monotonic() - start, 1),
    }
    try:
        report = json.loads(process.stdout)
    except json.JSONDecodeError:
        return {
            **result,
            "status": "incomplete",
            "reason": (process.stderr or process.stdout).strip()[-500:],
            "checks": [],
        }
    checks = report.get("checks")
    if checks is None and "status" in report and "bounds" in report:
        checks = [{"label": label, **report} for label in report.get("checked", [])]
    return {
        **result,
        "status": report["status"],
        "reason": report.get("reason"),
        "differs_in": report.get("differs_in"),
        "checks": [
            {
                key: check[key]
                for key in ("label", "status", "reason", "differs_in", "counterexample")
                if key in check
            }
            for check in checks or []
        ],
        "skipped": report.get("skipped", []),
        "limitations": report.get("limitations", []),
    }


def _summary(results: list[dict[str, Any]]) -> str:
    contracts = collections.Counter(result["status"] for result in results)
    checks = collections.Counter(
        check["status"] for result in results for check in result["checks"]
    )
    reasons = collections.Counter()
    for result in results:
        if not result["checks"] and result["status"] == "incomplete":
            reasons[_reason(result.get("reason"))] += 1
        for check in result["checks"]:
            if check["status"] == "incomplete":
                reasons[_reason(check.get("reason"))] += 1
    lines = [
        "# solsymdiff campaign",
        "",
        "| | bounded agreement | mismatch | incomplete |",
        "| --- | --- | --- | --- |",
        _row("contracts", contracts),
        _row("call sequences", checks),
        "",
    ]
    mismatches = [
        (result, check)
        for result in results
        for check in result["checks"]
        if check["status"] == "mismatch"
    ]
    # An ABI disagreement is a mismatch of the whole contract, without checks.
    mismatches += [
        (result, {"label": "external interface", "differs_in": "abi"})
        for result in results
        if result["status"] == "mismatch" and result.get("differs_in") == "abi"
    ]
    if mismatches:
        lines += ["## Mismatches", ""]
        for result, check in mismatches:
            calls = check.get("counterexample", {}).get("calls", [])
            calldata = " -> ".join(call["calldata"] for call in calls)
            lines.append(
                f"- `{result['contract']}` `{check['label']}` differs in "
                f"{check.get('differs_in', 'an assertion')}: `{calldata}`"
            )
        lines.append("")
    skipped = [
        (result["contract"], item["signature"])
        for result in results
        for item in result.get("skipped", [])
    ]
    if skipped:
        lines += [f"## Skipped functions ({len(skipped)})", ""]
        lines += [f"- `{contract}` `{signature}`" for contract, signature in skipped]
        lines.append("")
    limitations = collections.Counter(
        limitation for result in results for limitation in result.get("limitations", [])
    )
    if limitations:
        lines += ["## Not compared", "", "| contracts | what |", "| --- | --- |"]
        lines += [f"| {count} | {what} |" for what, count in limitations.most_common()]
        lines.append("")
    if reasons:
        lines += ["## Incomplete reasons", "", "| count | reason |", "| --- | --- |"]
        lines += [
            f"| {count} | {reason} |" for reason, count in reasons.most_common(20)
        ]
        lines.append("")
    return "\n".join(lines)


def _row(name: str, counts: collections.Counter[str]) -> str:
    statuses = ("bounded_agreement", "mismatch", "incomplete")
    return (
        f"| {name} | " + " | ".join(str(counts[status]) for status in statuses) + " |"
    )


def _reason(reason: Any) -> str:
    text = re.sub(r"0x[0-9a-fA-F]+|\d+", "N", str(reason)).replace("|", "/")
    return text.splitlines()[0][:120] if text else "unknown"


if __name__ == "__main__":
    raise SystemExit(main())
