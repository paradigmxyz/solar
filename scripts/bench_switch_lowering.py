#!/usr/bin/env -S uv run
"""Benchmark forced switch lowerings on synthetic and codegen corpora."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, Iterable, Sequence


DEFAULT_PIN = "01209d2b8ac81645b92e3ef801b5bcdfd61bfd69"
DEFAULT_METHODS = ("auto", "linear", "binary", "buckets", "dense")
SYNTHETIC_FIXTURE_VERSION = 3


def run(command: Sequence[str], cwd: Path | None = None) -> str:
    result = subprocess.run(command, cwd=cwd, check=True, capture_output=True, text=True)
    return result.stdout.strip()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_benchmark_module(root: Path) -> Any:
    sys.path.insert(0, str(root))
    spec = importlib.util.spec_from_file_location("switch_solar_bench", root / "solar_bench.py")
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load solar_bench.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def verify_corpus_pin(root: Path, pin: str) -> dict[str, str]:
    if run(["git", "cat-file", "-t", pin], root) != "commit":
        raise RuntimeError(f"benchmark pin {pin} is unavailable")

    expected = {}
    for line in run(["git", "ls-tree", pin], root).splitlines():
        mode, kind, remainder = line.split(maxsplit=2)
        revision, path = remainder.split("\t", maxsplit=1)
        if mode == "160000" and kind == "commit":
            expected[path] = revision

    actual = {}
    for path, revision in expected.items():
        checkout = root / path
        if not checkout.is_dir():
            raise RuntimeError(f"benchmark submodule is missing: {path}")
        actual[path] = run(["git", "rev-parse", "HEAD"], checkout)
        if actual[path] != revision:
            raise RuntimeError(
                f"benchmark submodule {path} is at {actual[path]}, expected {revision}"
            )
    return actual


def switch_values(count: int) -> list[int]:
    values = []
    value = 0x243F6A88
    while len(values) < count:
        value = (value * 1664525 + 1013904223) & 0xFFFFFFFF
        if value not in values:
            values.append(value)
    return values


def selector_case(bench: Any, count: int) -> Any:
    # The common cold SLOAD lifts dispatch above the transaction calldata floor.
    functions = "\n".join(
        f"    function f{index:02}() external view returns (uint256) {{ return burnSlot + {index}; }}"
        for index in range(count)
    )
    source = f"""// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Selector{count} {{
    uint256 private burnSlot;

{functions}

    fallback() external {{}}
}}
"""
    calls = [(f"f{index:02}()", ()) for index in range(count)]
    calls.append(("missing()", ()))
    return bench.TestCase(
        test_id=f"selector-{count}",
        description=f"{count}-entry external selector switch",
        source_code=source,
        contract_name=f"Selector{count}",
        test_calls=calls,
    )


def value_switch_case(
    bench: Any,
    name: str,
    values: Sequence[int],
    misses: Sequence[int],
) -> Any:
    # The common cold SLOAD lifts dispatch above the transaction calldata floor.
    cases = "\n".join(
        f"            case 0x{value:x} {{ result := {index + 1} }}"
        for index, value in enumerate(values)
    )
    source = f"""// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract {name} {{
    uint256 private burnSlot;

    function select(uint256 value) external view returns (uint256 result) {{
        assembly {{
            switch value
{cases}
            default {{ result := 0 }}
        }}
        result += burnSlot;
    }}
}}
"""
    calls = [(f"entry-{index:02}", "select(uint256)", (str(value),)) for index, value in enumerate(values)]
    calls.extend(
        (f"miss-{index:02}", "select(uint256)", (str(value),))
        for index, value in enumerate(misses)
    )
    return bench.TestCase(
        test_id=name.lower(),
        description=f"{len(values)}-entry value switch",
        source_code=source,
        contract_name=name,
        test_calls=[(signature, args) for _, signature, args in calls],
    ), [label for label, _, _ in calls]


def synthetic_cases(bench: Any) -> tuple[list[Any], dict[str, list[str]]]:
    cases = []
    labels = {}
    for count in (4, 5, 6, 7, 8, 16, 32, 64):
        case = selector_case(bench, count)
        cases.append(case)
        labels[case.test_id] = [f"entry-{index:02}" for index in range(count)] + ["miss-00"]

    for count in (4, 5, 6, 7, 8, 16, 32, 64):
        values = switch_values(count)
        case, case_labels = value_switch_case(
            bench,
            f"Sparse{count}",
            values,
            (0, 0xFFFFFFFF),
        )
        cases.append(case)
        labels[case.test_id] = case_labels

    for count in (4, 5, 6, 7, 8, 16, 24, 32, 64):
        low = 10
        values = list(range(low, low + count))
        case, case_labels = value_switch_case(
            bench,
            f"Dense{count}",
            values,
            (low - 1, low + count),
        )
        cases.append(case)
        labels[case.test_id] = case_labels

    for span in (16, 32, 64):
        low = 100
        values = [value for value in range(low, low + span) if (value - low) % 5 != 2]
        holes = [value for value in range(low, low + span) if (value - low) % 5 == 2]
        case, case_labels = value_switch_case(
            bench,
            f"Holey{span}",
            values,
            (low - 1, holes[0], low + span),
        )
        cases.append(case)
        labels[case.test_id] = case_labels
    return cases, labels


def compiler_specs(bench: Any, solar: Path, methods: Sequence[str], optimization: str) -> list[Any]:
    return [
        bench.CompilerSpec(
            method,
            f"solar {method}",
            solar,
            "solar",
            (f"-O{optimization}", f"-Zswitch-lowering={method}"),
        )
        for method in methods
    ]


def start_anvil() -> subprocess.Popen[bytes]:
    process = subprocess.Popen(
        ["anvil", "--port", "8545", "--steps-tracing"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    time.sleep(2)
    return process


def stop_anvil(process: subprocess.Popen[bytes]) -> None:
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()


def write_results(path: Path, metadata: dict[str, Any], results: Sequence[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps({"metadata": metadata, "results": results}, indent=2) + "\n")


def run_cases(
    bench: Any,
    cases: Iterable[Any],
    specs: Sequence[Any],
    output: Path,
    metadata: dict[str, Any],
    include_gas: bool,
    gas_profile: str,
    labels: dict[str, list[str]] | None = None,
) -> list[dict[str, Any]]:
    previous = {}
    if output.is_file():
        payload = json.loads(output.read_text())
        previous_metadata = payload.get("metadata") or {}
        cache_keys = (
            "solar_sha256",
            "benchmark_pin",
            "methods",
            "scope",
            "optimization",
            "fixture_version",
        )
        if all(previous_metadata.get(key) == metadata.get(key) for key in cache_keys):
            previous = {result["test_id"]: result for result in payload["results"]}

    results = []
    cases = list(cases)
    for index, case in enumerate(cases, 1):
        expected_calls = None
        if include_gas:
            expected_calls = (
                len(labels[case.test_id])
                if labels is not None
                else sum(call.repeat for call in bench.gas_calls(case, gas_profile))
            )
        if case.test_id in previous and result_is_complete(
            previous[case.test_id], specs, include_gas, expected_calls
        ):
            print(f"[{index}/{len(cases)}] {case.test_id} (cached)", flush=True)
            results.append(previous[case.test_id])
            continue

        print(f"[{index}/{len(cases)}] {case.test_id}", flush=True)
        if include_gas:
            result = None
            for spec in specs:
                for attempt in range(1, 4):
                    anvil = start_anvil()
                    try:
                        partial = bench.run_test_case(
                            case,
                            (spec,),
                            True,
                            gas_profile,
                            bench.DEFAULT_RPC_URL,
                            bench.DEFAULT_PRIVATE_KEY,
                            True,
                        )
                    finally:
                        stop_anvil(anvil)
                    if result_is_complete(partial, (spec,), True, expected_calls):
                        break
                    print(
                        f"[{case.test_id}/{spec.compiler_id}] retrying failed gas call "
                        f"({attempt}/3)",
                        flush=True,
                    )
                if result is None:
                    result = partial
                else:
                    result["compilers"].update(partial["compilers"])
            bench.compare_runtime_results(result, specs)
            if not result_is_complete(result, specs, True, expected_calls):
                raise RuntimeError(f"{case.test_id} has incomplete gas results after retries")
            if result.get("runtime_status") not in ("ok", "skipped"):
                raise RuntimeError(
                    f"{case.test_id} runtime comparison is {result.get('runtime_status')}"
                )
        else:
            result = bench.run_test_case(
                case,
                specs,
                False,
                gas_profile,
                bench.DEFAULT_RPC_URL,
                bench.DEFAULT_PRIVATE_KEY,
                True,
            )

        if labels is not None:
            expected = labels[case.test_id]
            for compiler in result["compilers"].values():
                gas_results = compiler.get("gas_results") or []
                for gas_result, label in zip(gas_results, expected):
                    gas_result["label"] = label
        results.append(result)
        write_results(output, metadata, results)
    return results


def gas_failed(result: dict[str, Any]) -> bool:
    return any(
        compiler.get("status") == "ok"
        and (
            compiler.get("deploy_status") == "failed"
            or compiler.get("gas_status") == "failed"
        )
        for compiler in result["compilers"].values()
    )


def result_is_complete(
    result: dict[str, Any],
    specs: Sequence[Any],
    include_gas: bool,
    expected_calls: int | None,
) -> bool:
    compilers = result.get("compilers") or {}
    if any(spec.compiler_id not in compilers for spec in specs):
        return False
    if result.get("runtime_status") in ("failed", "mismatch"):
        return False
    if not include_gas:
        return True
    for compiler in compilers.values():
        if compiler.get("status") != "ok":
            continue
        if compiler.get("deploy_status") != "ok" or compiler.get("gas_status") != "ok":
            return False
        gas_results = compiler.get("gas_results") or []
        if expected_calls is not None and (
            len(gas_results) != expected_calls
            or any(item.get("gas") is None for item in gas_results)
        ):
            return False
    return True


def successful_intersection(results: Sequence[dict[str, Any]], methods: Sequence[str]) -> list[dict[str, Any]]:
    return [
        result
        for result in results
        if all(result["compilers"].get(method, {}).get("status") == "ok" for method in methods)
    ]


def percent_delta(value: int, baseline: int) -> str:
    if baseline == 0:
        return "n/a"
    return f"{(value - baseline) * 100 / baseline:+.2f}%"


def scope_rows(
    name: str,
    optimization: str,
    results: Sequence[dict[str, Any]],
    methods: Sequence[str],
) -> list[list[str]]:
    common = successful_intersection(results, methods)
    totals = {}
    for method in methods:
        compilers = [result["compilers"][method] for result in common]
        gas_values = [
            int(gas_result["gas"])
            for compiler in compilers
            for gas_result in compiler.get("gas_results") or []
            if gas_result.get("gas") is not None
        ]
        totals[method] = {
            "deploy": sum(int(compiler.get("bytecode_size") or 0) for compiler in compilers),
            "runtime": sum(int(compiler.get("runtime_size") or 0) for compiler in compilers),
            "gas": sum(gas_values),
            "calls": len(gas_values),
        }
    auto = totals["auto"]
    return [
        [
            name,
            optimization,
            method,
            str(len(common)),
            str(total["calls"]),
            str(total["deploy"]),
            percent_delta(total["deploy"], auto["deploy"]),
            str(total["runtime"]),
            percent_delta(total["runtime"], auto["runtime"]),
            str(total["gas"]) if total["calls"] else "n/a",
            percent_delta(total["gas"], auto["gas"]) if total["calls"] else "n/a",
        ]
        for method, total in totals.items()
    ]


def markdown_table(headers: Sequence[str], rows: Sequence[Sequence[str]]) -> str:
    lines = [
        "| " + " | ".join(headers) + " |",
        "| " + " | ".join("---" for _ in headers) + " |",
    ]
    lines.extend("| " + " | ".join(row) + " |" for row in rows)
    return "\n".join(lines)


def render_markdown(output_dir: Path, methods: Sequence[str]) -> str:
    aggregate_rows = []
    synthetic_rows = []
    corpus_rows = []
    for path in sorted(output_dir.glob("*.json")):
        payload = json.loads(path.read_text())
        metadata = payload["metadata"]
        results = payload["results"]
        aggregate_rows.extend(
            scope_rows(metadata["scope"], metadata["optimization"], results, methods)
        )
        if metadata["scope"].startswith("ci-"):
            for result in successful_intersection(results, methods):
                for method in methods:
                    compiler = result["compilers"][method]
                    gas = [
                        str(item["gas"])
                        for item in compiler.get("gas_results") or []
                        if item.get("gas") is not None
                    ]
                    corpus_rows.append(
                        [
                            metadata["scope"],
                            metadata["optimization"],
                            str(result["test_id"]),
                            method,
                            str(compiler["bytecode_size"]),
                            str(compiler["runtime_size"]),
                            str(len(gas)),
                            str(sum(map(int, gas))) if gas else "n/a",
                            ", ".join(gas) if gas else "n/a",
                        ]
                    )
        if metadata["scope"] != "synthetic":
            continue
        for result in successful_intersection(results, methods):
            for method in methods:
                compiler = result["compilers"][method]
                gas_results = compiler.get("gas_results") or []
                entries = [
                    int(item["gas"])
                    for item in gas_results
                    if str(item["label"]).startswith("entry-") and item.get("gas") is not None
                ]
                misses = [
                    int(item["gas"])
                    for item in gas_results
                    if str(item["label"]).startswith("miss-") and item.get("gas") is not None
                ]
                synthetic_rows.append(
                    [
                        str(result["test_id"]),
                        method,
                        str(compiler["bytecode_size"]),
                        str(compiler["runtime_size"]),
                        str(len(entries)),
                        str(sum(entries)),
                        f"{min(entries)}–{max(entries)}" if entries else "n/a",
                        ", ".join(map(str, entries)) if entries else "n/a",
                        ", ".join(map(str, misses)) if misses else "n/a",
                    ]
                )

    text = "## Aggregate results\n\n"
    text += markdown_table(
        (
            "Scope",
            "Opt",
            "Method",
            "Cases",
            "Calls",
            "Deploy B",
            "Δ auto",
            "Runtime B",
            "Δ auto",
            "Gas",
            "Δ auto",
        ),
        aggregate_rows,
    )
    if synthetic_rows:
        text += "\n\n## Synthetic switch results\n\n"
        text += markdown_table(
            (
                "Fixture",
                "Method",
                "Deploy B",
                "Runtime B",
                "Entries",
                "Entry gas",
                "Entry range",
                "Entry gas (ordered)",
                "Miss gas (ordered)",
            ),
            synthetic_rows,
        )
    if corpus_rows:
        text += "\n\n## CI corpus results\n\n"
        text += markdown_table(
            (
                "Scope",
                "Opt",
                "Fixture",
                "Method",
                "Deploy B",
                "Runtime B",
                "Calls",
                "Gas",
                "Call gas (ordered)",
            ),
            corpus_rows,
        )
    return text + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--solar", type=Path, default=Path("target/debug/solar"))
    parser.add_argument(
        "--benchmark-repo",
        type=Path,
        default=Path.home() / "github/danipopes/solidity-compiler-benchmarks",
    )
    parser.add_argument("--pin", default=DEFAULT_PIN)
    parser.add_argument("--ui-corpus", type=Path, default=Path("tests/ui/codegen"))
    parser.add_argument("--output-dir", type=Path, default=Path("target/codegen-bench/switch"))
    parser.add_argument("--methods", nargs="+", choices=DEFAULT_METHODS, default=DEFAULT_METHODS)
    parser.add_argument(
        "--scope",
        nargs="+",
        choices=("synthetic", "ui-gas", "ui-size", "ci-size-gas", "ci-size", "ci-gas"),
        default=("synthetic", "ui-gas", "ui-size", "ci-size-gas", "ci-size", "ci-gas"),
    )
    args = parser.parse_args()
    if "auto" not in args.methods:
        parser.error("--methods must include auto")

    solar = args.solar.resolve()
    benchmark_repo = args.benchmark_repo.resolve()
    output_dir = args.output_dir.resolve()
    ui_corpus = args.ui_corpus.resolve()
    if not solar.is_file():
        parser.error(f"solar binary not found: {solar}")
    if not benchmark_repo.is_dir():
        parser.error(f"benchmark repository not found: {benchmark_repo}")
    if not ui_corpus.is_dir():
        parser.error(f"UI corpus not found: {ui_corpus}")
    if any(scope in args.scope for scope in ("synthetic", "ci-gas")):
        for tool in ("anvil", "cast"):
            if shutil.which(tool) is None:
                parser.error(f"{tool} is required for gas benchmarks")

    corpus_revisions = verify_corpus_pin(benchmark_repo, args.pin)
    bench = load_benchmark_module(benchmark_repo)
    metadata_base = {
        "solar": str(solar),
        "solar_sha256": sha256(solar),
        "solar_revision": run(["git", "rev-parse", "HEAD"]),
        "benchmark_pin": args.pin,
        "benchmark_revision": run(["git", "rev-parse", "HEAD"], benchmark_repo),
        "benchmark_script_sha256": sha256(benchmark_repo / "solar_bench.py"),
        "corpus_revisions": corpus_revisions,
        "methods": list(args.methods),
    }

    if "synthetic" in args.scope:
        cases, labels = synthetic_cases(bench)
        metadata = {
            **metadata_base,
            "scope": "synthetic",
            "optimization": "gas",
            "fixture_version": SYNTHETIC_FIXTURE_VERSION,
        }
        run_cases(
            bench,
            cases,
            compiler_specs(bench, solar, args.methods, "gas"),
            output_dir / "synthetic.json",
            metadata,
            True,
            "smoke",
            labels,
        )

    for scope, optimization in (("ui-gas", "gas"), ("ui-size", "size")):
        if scope in args.scope:
            metadata = {**metadata_base, "scope": scope, "optimization": optimization}
            run_cases(
                bench,
                bench.load_corpus_cases(ui_corpus),
                compiler_specs(bench, solar, args.methods, optimization),
                output_dir / f"{scope}.json",
                metadata,
                False,
                "smoke",
            )

    ci_cases = [*bench.TEST_CASES, *bench.REPO_TEST_CASES]
    for scope, optimization in (("ci-size-gas", "gas"), ("ci-size", "size")):
        if scope in args.scope:
            metadata = {**metadata_base, "scope": scope, "optimization": optimization}
            run_cases(
                bench,
                ci_cases,
                compiler_specs(bench, solar, args.methods, optimization),
                output_dir / f"{scope}.json",
                metadata,
                False,
                "hot",
            )

    if "ci-gas" in args.scope:
        metadata = {**metadata_base, "scope": "ci-gas", "optimization": "gas"}
        run_cases(
            bench,
            ci_cases,
            compiler_specs(bench, solar, args.methods, "gas"),
            output_dir / "ci-gas.json",
            metadata,
            True,
            "hot",
        )

    report = render_markdown(output_dir, args.methods)
    (output_dir / "report.md").write_text(report)
    print(report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
