#!/usr/bin/env -S uv run
"""Benchmark forced switch lowerings on synthetic and codegen corpora."""

from __future__ import annotations

import argparse
import concurrent.futures
import copy
import hashlib
import importlib.util
import json
import re
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable, Sequence


DEFAULT_PIN = "01209d2b8ac81645b92e3ef801b5bcdfd61bfd69"
DEFAULT_SOLC_VERSION = "0.8.36"
DEFAULT_METHODS = ("auto", "linear", "binary", "buckets", "dense", "perfect")
SYNTHETIC_FIXTURE_VERSION = 4
ANVIL_HARDFORK = "osaka"
EXPECTED_UI_FAILURE_RE = re.compile(r"//~[\^v|?]*\s*(?:ERROR|ICE)(?::|\b)")
GROWTH_SWEEP_CASE_COUNTS = {"synthetic": 52, "ui-gas": 147, "ci-gas": 12}
GROWTH_SWEEP_TEST_IDS_SHA256 = {
    "synthetic": "9361088c4aad17fe1e2ddc5ff57cb48f1c7d4ee6e98d133cbc8d985f42dffa90",
    "ui-gas": "fdcd4acf090d23542816011e7997d5d1b87c120ba6f345b9cf9650ed88e3ee26",
    "ci-gas": "f370cc6074dd09c2d968355a51fc218fce3fa5f4d349d840bf5e36be3bc9ad6d",
}
GROWTH_SWEEP_MIN_BUDGET = 0
GROWTH_SWEEP_MAX_BUDGET = 512
GROWTH_SWEEP_POLICY_BUDGET = 192
GROWTH_SWEEP_PROVENANCE_KEYS = (
    "solar_revision",
    "source_tree",
    "solar_sha256",
    "benchmark_pin",
    "benchmark_tree",
    "benchmark_script_sha256",
    "gas_script_sha256",
    "benchmark_fixtures_sha256",
    "corpus_revisions",
    "ui_corpus_sha256",
    "solc_sha256",
    "solc_version",
    "anvil_version",
    "cast_version",
    "hardfork",
    "driver_sha256",
    "methods",
    "variants",
)


@dataclass(frozen=True)
class UiCase:
    test_id: str
    description: str
    corpus_root: Path
    source: str
    expected_failure: bool


@dataclass(frozen=True)
class SwitchVariant:
    compiler_id: str
    label: str
    flags: tuple[str, ...]


def run(command: Sequence[str], cwd: Path | None = None) -> str:
    result = subprocess.run(command, cwd=cwd, check=True, capture_output=True, text=True)
    return result.stdout.strip()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def tree_sha256(root: Path, paths: Iterable[Path]) -> str:
    digest = hashlib.sha256()
    for path in sorted(paths):
        relative = path.relative_to(root).as_posix().encode()
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        contents = path.read_bytes()
        digest.update(len(contents).to_bytes(8, "big"))
        digest.update(contents)
    return digest.hexdigest()


def compiler_revision(solar: Path) -> str:
    prefix = "Commit SHA:"
    for line in run([str(solar), "--version"]).splitlines():
        if line.startswith(prefix):
            return line.removeprefix(prefix).strip()
    raise RuntimeError("solar --version did not report a commit SHA")


def bytecode_sha256(value: str) -> str:
    return hashlib.sha256(value.encode()).hexdigest()


def json_sha256(value: Any) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def load_benchmark_module(root: Path) -> Any:
    sys.modules.pop("gas_bench", None)
    sys.modules.pop("switch_solar_bench", None)
    sys.path.insert(0, str(root))
    spec = importlib.util.spec_from_file_location("switch_solar_bench", root / "solar_bench.py")
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load solar_bench.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def install_bytecode_hashes(bench: Any) -> None:
    def wrap(compile_case: Any) -> Any:
        def compile_with_hashes(*args: Any, **kwargs: Any) -> dict[str, Any]:
            result = compile_case(*args, **kwargs)
            if result.get("status") == "ok":
                result["bytecode_sha256"] = bytecode_sha256(str(result["bytecode"]))
                result["runtime_sha256"] = bytecode_sha256(
                    str(result.get("runtime_bytecode") or "")
                )
            return result

        return compile_with_hashes

    bench.compile_standard_json = wrap(bench.compile_standard_json)
    bench.compile_repo_case = wrap(bench.compile_repo_case)


def gitlinks(root: Path, revision: str) -> dict[str, str]:
    links = {}
    for line in run(["git", "ls-tree", "-r", revision], root).splitlines():
        mode, kind, remainder = line.split(maxsplit=2)
        object_id, path = remainder.split("\t", maxsplit=1)
        if mode == "160000" and kind == "commit":
            links[path] = object_id
    return links


def verify_submodule(root: Path, revision: str, display: str) -> dict[str, str]:
    if not root.is_dir():
        raise RuntimeError(f"benchmark submodule is missing: {display}")
    actual = run(["git", "rev-parse", "HEAD"], root)
    if actual != revision:
        raise RuntimeError(f"benchmark submodule {display} is at {actual}, expected {revision}")
    if run(["git", "status", "--porcelain", "--untracked-files=normal"], root):
        raise RuntimeError(f"benchmark submodule has local changes: {display}")

    revisions = {display: actual}
    for path, nested_revision in gitlinks(root, revision).items():
        revisions.update(
            verify_submodule(root / path, nested_revision, f"{display}/{path}")
        )
    return revisions


def verify_corpus_pin(root: Path, pin: str) -> dict[str, str]:
    if run(["git", "cat-file", "-t", pin], root) != "commit":
        raise RuntimeError(f"benchmark pin {pin} is unavailable")

    revisions = {}
    for path, revision in gitlinks(root, pin).items():
        revisions.update(verify_submodule(root / path, revision, path))
    return revisions


def materialize_benchmark_pin(
    root: Path, pin: str, destination: Path, corpus_revisions: dict[str, str]
) -> Path:
    archive = destination / "benchmark.tar"
    checkout = destination / "checkout"
    checkout.mkdir()
    subprocess.run(
        ["git", "archive", "--format=tar", "-o", str(archive), pin],
        cwd=root,
        check=True,
    )
    with tarfile.open(archive) as file:
        file.extractall(checkout, filter="data")

    for path in gitlinks(root, pin):
        link = checkout / path
        if link.is_dir():
            link.rmdir()
        link.parent.mkdir(parents=True, exist_ok=True)
        link.symlink_to(root / path, target_is_directory=True)

    if not corpus_revisions:
        raise RuntimeError("benchmark pin contains no corpus submodules")
    return checkout


def use_pinned_benchmark_inputs(bench: Any, root: Path, pinned_root: Path) -> str:
    pinned_fixtures = list((pinned_root / "fixtures").rglob("*"))
    pinned_files = [path for path in pinned_fixtures if path.is_file()]
    runtime_fixture = pinned_root / "fixtures/runtime/RuntimeFixtures.sol"
    for pinned in pinned_files:
        if pinned == runtime_fixture:
            continue
        relative = pinned.relative_to(pinned_root)
        actual = root / relative
        if not actual.is_file() or actual.read_bytes() != pinned.read_bytes():
            raise RuntimeError(f"benchmark input differs from pin: {relative}")

    pinned_compile_runtime_fixture = bench.compile_runtime_fixture

    def compile_runtime_fixture(solc_path: str, contract_name: str) -> Any:
        active_root = bench.ROOT
        bench.ROOT = pinned_root
        try:
            return pinned_compile_runtime_fixture(solc_path, contract_name)
        finally:
            bench.ROOT = active_root

    bench.ROOT = root
    bench.RESULT_ROOT = root / "solar_results"
    bench.compile_runtime_fixture = compile_runtime_fixture
    return tree_sha256(pinned_root, pinned_files)


def require_clean_source(root: Path) -> None:
    status = run(["git", "status", "--porcelain", "--untracked-files=normal"], root)
    if status:
        raise RuntimeError("source checkout has uncommitted or untracked files")


def tool_version(tool: str) -> str:
    return run([tool, "--version"])


def switch_values(count: int) -> list[int]:
    values = []
    value = 0x243F6A88
    while len(values) < count:
        value = (value * 1664525 + 1013904223) & 0xFFFFFFFF
        if value not in values:
            values.append(value)
    return values


def selector_case(bench: Any, count: int) -> tuple[Any, list[Any]]:
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
    case = bench.TestCase(
        test_id=f"selector-{count}",
        description=f"{count}-entry external selector switch",
        source_code=source,
        contract_name=f"Selector{count}",
        test_calls=calls,
    )
    checks = [
        bench.RuntimeCheck(f"entry-{index:02}", f"f{index:02}()(uint256)")
        for index in range(count)
    ]
    checks.append(bench.RuntimeCheck("miss-00", "missing()"))
    return case, checks


def value_switch_case(
    bench: Any,
    name: str,
    values: Sequence[int],
    misses: Sequence[int],
) -> tuple[Any, list[str], list[Any]]:
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
    case = bench.TestCase(
        test_id=name.lower(),
        description=f"{len(values)}-entry value switch",
        source_code=source,
        contract_name=name,
        test_calls=[(signature, args) for _, signature, args in calls],
    )
    checks = [
        bench.RuntimeCheck(label, "select(uint256)(uint256)", args)
        for label, _, args in calls
    ]
    return case, [label for label, _, _ in calls], checks


def synthetic_cases(
    bench: Any,
) -> tuple[list[Any], dict[str, list[str]], dict[str, list[Any]]]:
    cases = []
    labels = {}
    runtime_checks = {}
    for count in (4, 5, 6, 7, 8, 16, 32, 64):
        case, checks = selector_case(bench, count)
        cases.append(case)
        labels[case.test_id] = [f"entry-{index:02}" for index in range(count)] + ["miss-00"]
        runtime_checks[case.test_id] = checks

    for count in (4, 5, 6, 7, 8, 16, 32, 64):
        values = switch_values(count)
        case, case_labels, checks = value_switch_case(
            bench,
            f"Sparse{count}",
            values,
            (0, 0xFFFFFFFF),
        )
        cases.append(case)
        labels[case.test_id] = case_labels
        runtime_checks[case.test_id] = checks

    for count in (4, 5, 6, 7, 8, 16, 24, 32, 64):
        low = 10
        values = list(range(low, low + count))
        case, case_labels, checks = value_switch_case(
            bench,
            f"Dense{count}",
            values,
            (low - 1, low + count),
        )
        cases.append(case)
        labels[case.test_id] = case_labels
        runtime_checks[case.test_id] = checks

    for span in (16, 32, 64):
        low = 100
        values = [value for value in range(low, low + span) if (value - low) % 5 != 2]
        holes = [value for value in range(low, low + span) if (value - low) % 5 == 2]
        case, case_labels, checks = value_switch_case(
            bench,
            f"Holey{span}",
            values,
            (low - 1, holes[0], low + span),
        )
        cases.append(case)
        labels[case.test_id] = case_labels
        runtime_checks[case.test_id] = checks

    for count in (4, 5, 6, 7, 8, 16, 32, 64):
        low = 10_000
        stride = 257
        values = [low + index * stride for index in range(count)]
        case, case_labels, checks = value_switch_case(
            bench,
            f"AffineOdd{count}",
            values,
            (low - 1, low + 1, low + count * stride),
        )
        cases.append(case)
        labels[case.test_id] = case_labels
        runtime_checks[case.test_id] = checks

    for count in (4, 5, 6, 7, 8, 16, 32, 64):
        low = 20_000
        stride = 6
        values = [low + index * stride for index in range(count)]
        case, case_labels, checks = value_switch_case(
            bench,
            f"AffineEven{count}",
            values,
            (low - 1, low + 1, low + count * stride),
        )
        cases.append(case)
        labels[case.test_id] = case_labels
        runtime_checks[case.test_id] = checks

    for count in (4, 5, 6, 7, 8, 16, 32, 64):
        values = [((index + 1) ** 2 << 16) | index for index in range(count)]
        case, case_labels, checks = value_switch_case(
            bench,
            f"BitSlice{count}",
            values,
            (0, values[0] + (1 << 16), values[-1] + 1),
        )
        cases.append(case)
        labels[case.test_id] = case_labels
        runtime_checks[case.test_id] = checks
    return cases, labels, runtime_checks


def install_runtime_checks(bench: Any, checks: dict[str, list[Any]]) -> None:
    original = bench.runtime_checks

    def runtime_checks(case: Any) -> Sequence[Any]:
        return checks.get(case.test_id, original(case))

    bench.runtime_checks = runtime_checks


def compiler_specs(
    bench: Any,
    solar: Path,
    variants: Sequence[SwitchVariant],
    optimization: str,
    wrapper_dir: Path,
) -> list[Any]:
    specs = []
    solar = solar.resolve()
    wrapper_dir = wrapper_dir.resolve()
    wrapper_dir.mkdir(parents=True, exist_ok=True)
    for variant in variants:
        wrapper = wrapper_dir / f"solar-{optimization}-{variant.compiler_id}"
        command = " ".join(
            shlex.quote(value)
            for value in (
                str(solar),
                f"-O{optimization}",
                *variant.flags,
            )
        )
        wrapper.write_text(f"#!/bin/sh\nexec {command} \"$@\"\n")
        wrapper.chmod(0o755)
        specs.append(
            bench.CompilerSpec(
                variant.compiler_id,
                variant.label,
                wrapper,
                "solar",
            )
        )
    return specs


def parse_integer_ranges(value: str) -> list[int]:
    values = []
    for item in value.split(","):
        item = item.strip()
        if not item:
            raise argparse.ArgumentTypeError("integer range contains an empty item")
        if ".." in item:
            start_text, end_text = item.split("..", maxsplit=1)
            try:
                start = int(start_text)
                end = int(end_text.removeprefix("="))
            except ValueError as error:
                raise argparse.ArgumentTypeError(f"invalid integer range: {item}") from error
            if start < 0 or end < start:
                raise argparse.ArgumentTypeError(f"invalid integer range: {item}")
            values.extend(range(start, end + 1))
        else:
            try:
                parsed = int(item)
            except ValueError as error:
                raise argparse.ArgumentTypeError(f"invalid integer: {item}") from error
            if parsed < 0:
                raise argparse.ArgumentTypeError(f"invalid negative integer: {item}")
            values.append(parsed)
    if len(set(values)) != len(values):
        raise argparse.ArgumentTypeError("integer ranges must not contain duplicates")
    return values


def switch_variants(
    methods: Sequence[str],
    growth_budgets: Sequence[int],
) -> list[SwitchVariant]:
    variants = [
        SwitchVariant(method, f"solar {method}", (f"-Zswitch-lowering={method}",))
        for method in methods
    ]
    variants.extend(
        SwitchVariant(
            f"auto-g{budget}",
            f"solar auto growth={budget}",
            (
                "-Zswitch-lowering=auto",
                f"-Zswitch-max-gas-code-growth={budget}",
            ),
        )
        for budget in growth_budgets
    )
    return variants


def load_ui_cases(root: Path) -> list[UiCase]:
    cases = []
    for path in sorted(root.rglob("*.sol")):
        relative = path.relative_to(root)
        if "auxiliary" in relative.parts:
            continue
        source = relative.as_posix()
        contents = path.read_text()
        cases.append(
            UiCase(
                source,
                f"UI codegen file {source}",
                root,
                source,
                bool(EXPECTED_UI_FAILURE_RE.search(contents))
                and "--evm-version" not in contents,
            )
        )
    return cases


def parse_solar_corpus_output(stdout: str) -> tuple[str | None, str | None, int, str]:
    decoder = json.JSONDecoder()
    objects = []
    index = 0
    while index < len(stdout):
        while index < len(stdout) and stdout[index].isspace():
            index += 1
        if index >= len(stdout):
            break
        try:
            value, index = decoder.raw_decode(stdout, index)
        except json.JSONDecodeError as error:
            return None, None, 0, f"invalid JSON output: {error}"
        if isinstance(value, dict):
            objects.append(value)

    output = next((value for value in reversed(objects) if value.get("contracts")), {})
    errors = output.get("errors") or []
    fatal = [
        error.get("formattedMessage") or error.get("message") or str(error)
        for error in errors
        if error.get("severity") == "error"
    ]
    if fatal:
        return None, None, 0, fatal[0][:1000]
    if not output:
        return None, None, 0, "no contracts in JSON output"

    bytecodes = []
    runtimes = []
    for contract in output["contracts"].values():
        bytecode = str(contract.get("bin") or "").strip().removeprefix("0x")
        runtime = str(
            contract.get("bin-runtime") or contract.get("bin_runtime") or ""
        ).strip().removeprefix("0x")
        if bytecode:
            bytecodes.append(bytecode)
            runtimes.append(runtime)
    if not bytecodes:
        return None, None, 0, "no concrete contracts emitted"
    return "".join(bytecodes), "".join(runtimes), len(bytecodes), ""


def run_ui_case(bench: Any, case: UiCase, specs: Sequence[Any]) -> dict[str, Any]:
    compilers = {}
    for spec in specs:
        command = [
            str(spec.path),
            "-Zcodegen",
            "--emit",
            "bin,bin-runtime",
            "--base-path",
            str(case.corpus_root),
            "--color",
            "never",
            case.source,
        ]
        process = bench.run(command, timeout=180, cwd=case.corpus_root)
        compiler = {
            "compiler_id": spec.compiler_id,
            "label": spec.label,
            "status": "failed",
            "bytecode_size": 0,
            "runtime_size": 0,
            "contracts": 0,
            "command": bench.display_command(command),
            "error": "",
        }
        if process.returncode == 0:
            bytecode, runtime, contracts, error = parse_solar_corpus_output(process.stdout)
            if bytecode:
                compiler.update(
                    {
                        "status": "ok",
                        "bytecode_size": len(bytecode) // 2,
                        "runtime_size": len(runtime or "") // 2,
                        "bytecode_sha256": bytecode_sha256(bytecode),
                        "runtime_sha256": bytecode_sha256(runtime or ""),
                        "contracts": contracts,
                    }
                )
            else:
                compiler["error"] = error
        else:
            compiler["error"] = (
                process.stderr or process.stdout or "compiler failed"
            )[:1000]
        compilers[spec.compiler_id] = compiler
    return {
        "test_id": case.test_id,
        "description": case.description,
        "suite": "ui",
        "compilers": compilers,
        "runtime_status": "skipped",
    }


def start_anvil() -> subprocess.Popen[bytes]:
    process = subprocess.Popen(
        [
            "anvil",
            "--port",
            "8545",
            "--hardfork",
            ANVIL_HARDFORK,
            "--steps-tracing",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    time.sleep(2)
    if process.poll() is not None:
        raise RuntimeError("anvil exited before becoming ready")
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


def compile_specs(
    bench: Any,
    case: Any,
    specs: Sequence[Any],
    gas_profile: str,
    jobs: int,
) -> dict[str, Any]:
    def compile_one(spec: Any) -> dict[str, Any]:
        if isinstance(case, UiCase):
            return run_ui_case(bench, case, (spec,))
        return bench.run_test_case(
            case,
            (spec,),
            False,
            gas_profile,
            bench.DEFAULT_RPC_URL,
            bench.DEFAULT_PRIVATE_KEY,
            False,
        )

    if jobs == 1:
        partials = map(compile_one, specs)
    else:
        executor = concurrent.futures.ThreadPoolExecutor(max_workers=jobs)
        partials = executor.map(compile_one, specs)

    result = None
    try:
        for partial in partials:
            if result is None:
                result = {**partial, "compilers": {}}
            result["compilers"].update(partial["compilers"])
    finally:
        if jobs != 1:
            executor.shutdown()
    if result is None:
        raise RuntimeError("cannot compile a case without compiler variants")
    return result


def run_cases(
    bench: Any,
    cases: Iterable[Any],
    specs: Sequence[Any],
    output: Path,
    metadata: dict[str, Any],
    include_gas: bool,
    gas_profile: str,
    labels: dict[str, list[str]] | None = None,
    expected_failures: set[str] | None = None,
    reference_spec: Any | None = None,
    jobs: int = 1,
) -> list[dict[str, Any]]:
    expected_failures = expected_failures or set()
    if include_gas and reference_spec is None:
        raise RuntimeError("gas benchmarks require a reference compiler")

    previous = {}
    if output.is_file():
        payload = json.loads(output.read_text())
        previous_metadata = payload.get("metadata") or {}
        if previous_metadata == metadata:
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
        expected_status = "failed" if case.test_id in expected_failures else "ok"
        if case.test_id in previous and result_is_complete(
            previous[case.test_id],
            specs,
            include_gas,
            expected_calls,
            expected_status,
            include_gas and expected_status == "ok",
        ):
            print(f"[{index}/{len(cases)}] {case.test_id} (cached)", flush=True)
            results.append(previous[case.test_id])
            continue

        print(f"[{index}/{len(cases)}] {case.test_id}", flush=True)
        if include_gas:
            compiled = compile_specs(bench, case, specs, gas_profile, jobs)
            grouped_specs = {}
            for spec in specs:
                compiler = compiled["compilers"][spec.compiler_id]
                key = (
                    compiler.get("status"),
                    compiler.get("bytecode_sha256"),
                    compiler.get("runtime_sha256"),
                )
                grouped_specs.setdefault(key, []).append(spec)

            result = None
            for equivalent_specs in grouped_specs.values():
                spec = equivalent_specs[0]
                for attempt in range(1, 4):
                    anvil = start_anvil()
                    try:
                        partial = bench.run_test_case(
                            case,
                            (spec, reference_spec),
                            True,
                            gas_profile,
                            bench.DEFAULT_RPC_URL,
                            bench.DEFAULT_PRIVATE_KEY,
                            True,
                        )
                        reference = partial["compilers"][reference_spec.compiler_id]
                        reference_checks_sha256 = json_sha256(
                            reference.get("runtime_results") or []
                        )
                        for execution_spec in (spec, reference_spec):
                            compiler = partial["compilers"][execution_spec.compiler_id]
                            compiler["runtime_checks_sha256"] = json_sha256(
                                compiler.get("runtime_results") or []
                            )
                            compiler["reference_checks_sha256"] = (
                                reference_checks_sha256
                            )
                            compiler["reference_runtime_status"] = partial.get(
                                "runtime_status"
                            )
                    finally:
                        stop_anvil(anvil)
                    if result_is_complete(
                        partial,
                        (spec, reference_spec),
                        True,
                        expected_calls,
                        expected_status,
                        expected_status == "ok",
                    ):
                        break
                    print(
                        f"[{case.test_id}/{spec.compiler_id}] retrying failed gas call "
                        f"({attempt}/3)",
                        flush=True,
                    )
                if result is None:
                    result = {**partial, "compilers": {}}
                compiler = partial["compilers"][spec.compiler_id]
                for equivalent_spec in equivalent_specs:
                    equivalent = copy.deepcopy(compiler)
                    equivalent.update(compiled["compilers"][equivalent_spec.compiler_id])
                    result["compilers"][equivalent_spec.compiler_id] = equivalent
                if partial.get("runtime_status") != "ok" and expected_status == "ok":
                    raise RuntimeError(
                        f"{case.test_id}/{spec.compiler_id} did not match the reference compiler"
                    )
            bench.compare_runtime_results(result, specs)
            if not result_is_complete(
                result,
                specs,
                True,
                expected_calls,
                expected_status,
                expected_status == "ok",
            ):
                raise RuntimeError(f"{case.test_id} has incomplete gas results after retries")
        else:
            result = compile_specs(bench, case, specs, gas_profile, jobs)
            if not result_is_complete(
                result, specs, False, None, expected_status, False
            ):
                raise RuntimeError(
                    f"{case.test_id} has unexpected or incomplete compilation results"
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


def result_is_complete(
    result: dict[str, Any],
    specs: Sequence[Any],
    include_gas: bool,
    expected_calls: int | None,
    expected_status: str,
    require_runtime: bool,
) -> bool:
    compilers = result.get("compilers") or {}
    if any(spec.compiler_id not in compilers for spec in specs):
        return False
    selected = [compilers[spec.compiler_id] for spec in specs]
    statuses = {compiler.get("status") for compiler in selected}
    if statuses != {expected_status}:
        return False
    if result.get("runtime_status") in ("failed", "mismatch") or result.get(
        "runtime_mismatches"
    ):
        return False
    if require_runtime and result.get("runtime_status") != "ok":
        return False
    for compiler in selected:
        if compiler.get("status") != "ok":
            continue
        if not isinstance(compiler.get("bytecode_size"), int) or not isinstance(
            compiler.get("runtime_size"), int
        ):
            return False
        if any(
            not isinstance(compiler.get(key), str) or len(compiler[key]) != 64
            for key in ("bytecode_sha256", "runtime_sha256")
        ):
            return False
        if not include_gas:
            continue
        if (
            compiler.get("reference_runtime_status") != "ok"
            or compiler.get("runtime_checks_sha256")
            != compiler.get("reference_checks_sha256")
        ):
            return False
        if compiler.get("deploy_status") != "ok" or compiler.get("gas_status") != "ok":
            return False
        gas_results = compiler.get("gas_results") or []
        if expected_calls is not None and (
            len(gas_results) != expected_calls
            or any(not isinstance(item.get("gas"), int) for item in gas_results)
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


def report_value(value: Any) -> str:
    return str(value).replace("\n", "<br>")


def render_markdown(
    output_dir: Path,
    methods: Sequence[str],
    metadata_base: dict[str, Any],
    scopes: Sequence[str],
    include_details: bool = True,
) -> str:
    aggregate_rows = []
    synthetic_rows = []
    corpus_rows = []
    ui_rows = []
    coverage_rows = []
    payloads = []
    for scope in scopes:
        path = output_dir / f"{scope}.json"
        if not path.is_file():
            raise RuntimeError(f"missing benchmark result: {path}")
        payload = json.loads(path.read_text())
        metadata = payload["metadata"]
        if any(metadata.get(key) != value for key, value in metadata_base.items()):
            raise RuntimeError(f"{path} has mismatched benchmark provenance")
        if metadata.get("scope") != scope:
            raise RuntimeError(f"{path} contains scope {metadata.get('scope')}, expected {scope}")
        payloads.append(payload)

    for payload in payloads:
        metadata = payload["metadata"]
        results = payload["results"]
        aggregate_rows.extend(
            scope_rows(metadata["scope"], metadata["optimization"], results, methods)
        )
        expected_failures = set(metadata.get("expected_failures") or ())
        runtime_statuses = {}
        for result in results:
            status = str(result.get("runtime_status") or "skipped")
            runtime_statuses[status] = runtime_statuses.get(status, 0) + 1
        coverage_rows.append(
            [
                metadata["scope"],
                str(len(results)),
                str(len(successful_intersection(results, methods))),
                ", ".join(sorted(expected_failures)) or "none",
                ", ".join(metadata.get("excluded_cases") or ()) or "none",
                ", ".join(f"{key}: {value}" for key, value in sorted(runtime_statuses.items())),
            ]
        )
        if include_details and metadata["scope"].startswith("ci-"):
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
        if include_details and metadata["scope"].startswith("ui-"):
            for result in successful_intersection(results, methods):
                compilers = result["compilers"]
                auto = compilers["auto"]
                if all(
                    compiler["bytecode_size"] == auto["bytecode_size"]
                    and compiler["runtime_size"] == auto["runtime_size"]
                    and compiler["bytecode_sha256"] == auto["bytecode_sha256"]
                    and compiler["runtime_sha256"] == auto["runtime_sha256"]
                    for compiler in compilers.values()
                ):
                    continue
                row = [
                    metadata["scope"],
                    str(result["test_id"]),
                    f'{auto["bytecode_size"]}/{auto["runtime_size"]}',
                ]
                for method in methods:
                    if method == "auto":
                        continue
                    compiler = compilers[method]
                    differs = (
                        compiler["bytecode_sha256"] != auto["bytecode_sha256"]
                        or compiler["runtime_sha256"] != auto["runtime_sha256"]
                    )
                    same_sizes = (
                        compiler["bytecode_size"] == auto["bytecode_size"]
                        and compiler["runtime_size"] == auto["runtime_size"]
                    )
                    row.append(
                        f'{compiler["bytecode_size"]}/{compiler["runtime_size"]} '
                        f'({percent_delta(compiler["runtime_size"], auto["runtime_size"])})'
                        f'{"; code differs" if differs and same_sizes else ""}'
                    )
                ui_rows.append(row)
        if not include_details or metadata["scope"] != "synthetic":
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

    text = "## Benchmark provenance\n\n"
    text += markdown_table(
        ("Input", "Value"),
        [
            ["Compiler revision", report_value(metadata_base["solar_revision"])],
            ["Source tree", report_value(metadata_base["source_tree"])],
            ["Compiler SHA-256", report_value(metadata_base["solar_sha256"])],
            ["Benchmark pin", report_value(metadata_base["benchmark_pin"])],
            ["Benchmark tree", report_value(metadata_base["benchmark_tree"])],
            [
                "Benchmark runner SHA-256",
                report_value(metadata_base["benchmark_script_sha256"]),
            ],
            ["Gas fixture SHA-256", report_value(metadata_base["gas_script_sha256"])],
            [
                "Benchmark fixtures SHA-256",
                report_value(metadata_base["benchmark_fixtures_sha256"]),
            ],
            ["Driver SHA-256", report_value(metadata_base["driver_sha256"])],
            ["UI corpus SHA-256", report_value(metadata_base["ui_corpus_sha256"])],
            ["solc", report_value(metadata_base["solc_version"])],
            ["Anvil", report_value(metadata_base["anvil_version"])],
            ["Cast", report_value(metadata_base["cast_version"])],
            ["Hardfork", report_value(metadata_base["hardfork"])],
        ],
    )
    text += "\n\n## Coverage and correctness\n\n"
    text += markdown_table(
        (
            "Scope",
            "Measured",
            "Successful",
            "Expected compile failures",
            "Excluded",
            "Runtime checks",
        ),
        coverage_rows,
    )
    if metadata_base.get("compile_only"):
        text += (
            "\n\nThis compile-only sweep records creation and runtime bytecode hashes for "
            "deduplication before execution. Runtime correctness is checked when each "
            "distinct artifact is benchmarked."
        )
    else:
        text += (
            "\n\nEvery synthetic entry and miss has a matching read check against CI's pinned "
            "solc. The CI workloads also run the pinned runtime and cold-path checks. Every "
            "gas row records the method and reference check digests, and every successful "
            "artifact records creation and runtime bytecode hashes."
        )
    text += "\n\n## Aggregate results\n\n"
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
    if ui_rows:
        ui_methods = ("auto", *(method for method in methods if method != "auto"))
        text += "\n\n## Changed UI corpus files\n\n"
        text += "Sizes are deploy/runtime bytes; parenthesized deltas are runtime bytes versus auto.\n\n"
        text += markdown_table(
            ("Scope", "Fixture", *(method.title() for method in ui_methods)),
            ui_rows,
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


def growth_budget(compiler_id: str) -> int | None:
    prefix = "auto-g"
    if not compiler_id.startswith(prefix):
        return None
    try:
        return int(compiler_id.removeprefix(prefix))
    except ValueError:
        return None


def artifact_key(compiler: dict[str, Any]) -> tuple[str, str]:
    return (
        str(compiler["bytecode_sha256"]),
        str(compiler["runtime_sha256"]),
    )


def load_scope(output_dir: Path, scope: str) -> dict[str, Any]:
    path = output_dir / f"{scope}.json"
    if not path.is_file():
        raise RuntimeError(f"missing benchmark result: {path}")
    return json.loads(path.read_text())


def results_by_test_id(payload: dict[str, Any], scope: str) -> dict[str, dict[str, Any]]:
    results = payload.get("results")
    if not isinstance(results, list):
        raise RuntimeError(f"{scope} results are missing")
    by_id = {str(result["test_id"]): result for result in results}
    if len(by_id) != len(results):
        raise RuntimeError(f"{scope} contains duplicate test IDs")
    expected = GROWTH_SWEEP_CASE_COUNTS[scope]
    if len(by_id) != expected:
        raise RuntimeError(f"{scope} contains {len(by_id)} tests, expected {expected}")
    test_ids_sha256 = json_sha256(sorted(by_id))
    if test_ids_sha256 != GROWTH_SWEEP_TEST_IDS_SHA256[scope]:
        raise RuntimeError(f"{scope} test IDs differ from the growth sweep manifest")
    return by_id


def expected_growth_sweep_variants() -> list[dict[str, Any]]:
    return [
        {
            "compiler_id": variant.compiler_id,
            "label": variant.label,
            "flags": list(variant.flags),
        }
        for variant in switch_variants(
            ("auto",),
            range(GROWTH_SWEEP_MIN_BUDGET, GROWTH_SWEEP_MAX_BUDGET + 1),
        )
    ]


def validate_growth_sweep_inputs(
    compile_payloads: dict[str, dict[str, Any]],
    gas_payloads: dict[str, dict[str, Any]],
) -> None:
    canonical = compile_payloads["synthetic"]["metadata"]
    expected_variants = expected_growth_sweep_variants()
    expected_methods = [variant["compiler_id"] for variant in expected_variants]
    expected_method_set = set(expected_methods)
    missing = [
        key
        for key in GROWTH_SWEEP_PROVENANCE_KEYS
        if key not in canonical or canonical[key] is None
    ]
    if missing:
        raise RuntimeError(f"synthetic compile provenance is missing {', '.join(missing)}")
    compile_results = {}
    for scope, payload in compile_payloads.items():
        metadata = payload.get("metadata") or {}
        if metadata.get("scope") != scope or metadata.get("optimization") != "gas":
            raise RuntimeError(f"{scope} has incompatible compile metadata")
        if metadata.get("compile_only") is not True:
            raise RuntimeError(f"{scope} is not a compile-only sweep")
        for key in GROWTH_SWEEP_PROVENANCE_KEYS:
            if key not in metadata or metadata[key] is None:
                raise RuntimeError(f"{scope} compile provenance is missing {key}")
            if metadata[key] != canonical[key]:
                raise RuntimeError(f"{scope} compile provenance differs for {key}")
        if metadata.get("methods") != expected_methods:
            raise RuntimeError(f"{scope} methods differ from the growth sweep manifest")
        if metadata.get("variants") != expected_variants:
            raise RuntimeError(f"{scope} variants differ from the growth sweep manifest")
        compile_results[scope] = results_by_test_id(payload, scope)
        for test_id, result in compile_results[scope].items():
            if list(result.get("compilers") or ()) != expected_methods:
                raise RuntimeError(
                    f"{scope}/{test_id} compilers differ from the growth sweep manifest"
                )

    for scope, payload in gas_payloads.items():
        metadata = payload.get("metadata") or {}
        compile_metadata = compile_payloads[scope]["metadata"]
        if metadata.get("scope") != scope or metadata.get("optimization") != "gas":
            raise RuntimeError(f"{scope} has incompatible gas metadata")
        if metadata.get("compile_only") is not False:
            raise RuntimeError(f"{scope} does not contain gas execution")
        for key in GROWTH_SWEEP_PROVENANCE_KEYS:
            if key not in metadata or metadata[key] is None:
                raise RuntimeError(f"{scope} gas provenance is missing {key}")
            if metadata[key] != compile_metadata[key]:
                raise RuntimeError(f"{scope} gas provenance differs for {key}")
        for key in ("expected_failures", "excluded_cases", "fixture_version"):
            if metadata.get(key) != compile_metadata.get(key):
                raise RuntimeError(f"{scope} coverage metadata differs for {key}")

        gas_results = results_by_test_id(payload, scope)
        for test_id, result in gas_results.items():
            if set(result.get("compilers") or ()) != expected_method_set:
                raise RuntimeError(
                    f"{scope}/{test_id} gas compilers differ from the growth sweep manifest"
                )
        if gas_results.keys() != compile_results[scope].keys():
            raise RuntimeError(f"{scope} compile and gas test IDs differ")
        for test_id, result in gas_results.items():
            compiled = compile_results[scope][test_id]
            descriptor = ("description", "contract_name", "suite", "gas_profile")
            if any(result.get(key) != compiled.get(key) for key in descriptor):
                raise RuntimeError(f"{scope}/{test_id} fixture metadata differs")
            if result.get("runtime_status") != "ok" or result.get("runtime_mismatches"):
                raise RuntimeError(f"{scope}/{test_id} runtime checks failed")
            for compiler_id, compiler in result["compilers"].items():
                if compiler.get("status") != "ok":
                    raise RuntimeError(f"{scope}/{test_id}/{compiler_id} failed")
                if (
                    compiler.get("deploy_status") != "ok"
                    or compiler.get("gas_status") != "ok"
                    or compiler.get("runtime_status") != "ok"
                    or compiler.get("reference_runtime_status") != "ok"
                ):
                    raise RuntimeError(f"{scope}/{test_id}/{compiler_id} checks failed")
                checks = compiler.get("runtime_checks_sha256")
                if not checks or checks != compiler.get("reference_checks_sha256"):
                    raise RuntimeError(f"{scope}/{test_id}/{compiler_id} check digests differ")
                call_gas_results = compiler.get("gas_results") or ()
                if compiler.get("total_gas") != sum(item["gas"] for item in call_gas_results):
                    raise RuntimeError(f"{scope}/{test_id}/{compiler_id} gas total differs")


def render_growth_sweep(
    compile_dir: Path,
    gas_dir: Path,
) -> tuple[str, dict[str, Any]]:
    compile_payloads = {
        scope: load_scope(compile_dir, scope)
        for scope in ("synthetic", "ui-gas", "ci-gas")
    }
    gas_payloads = {
        scope: load_scope(gas_dir, scope)
        for scope in ("synthetic", "ci-gas")
    }
    validate_growth_sweep_inputs(compile_payloads, gas_payloads)
    budgets = sorted(
        budget
        for variant in compile_payloads["synthetic"]["metadata"]["variants"]
        if (budget := growth_budget(variant["compiler_id"])) is not None
    )
    expected_budgets = list(
        range(GROWTH_SWEEP_MIN_BUDGET, GROWTH_SWEEP_MAX_BUDGET + 1)
    )
    if budgets != expected_budgets:
        raise RuntimeError("growth sweep must contain every budget from 0 through 512")

    measured = {}
    for scope, payload in gas_payloads.items():
        for result in payload["results"]:
            for compiler in result["compilers"].values():
                if compiler.get("status") != "ok" or compiler.get("gas_status") != "ok":
                    continue
                key = (scope, result["test_id"], artifact_key(compiler))
                gas = tuple(
                    (item["label"], item["gas"])
                    for item in compiler.get("gas_results") or ()
                )
                previous = measured.setdefault(key, gas)
                if previous != gas:
                    raise RuntimeError(f"inconsistent gas for identical bytecode: {key}")

    rows = []
    fingerprints = {}
    ci_gas_by_budget = {}
    for budget in budgets:
        compiler_id = f"auto-g{budget}"
        totals = {}
        fingerprint = []
        ci_gas_by_test = {}
        for scope, payload in compile_payloads.items():
            deploy = 0
            runtime = 0
            gas = 0
            calls = 0
            for result in payload["results"]:
                compiler = result["compilers"][compiler_id]
                if compiler.get("status") != "ok":
                    raise RuntimeError(f"{scope}/{result['test_id']}/{compiler_id} failed")
                key = artifact_key(compiler)
                fingerprint.append((scope, result["test_id"], *key))
                deploy += compiler["bytecode_size"]
                runtime += compiler["runtime_size"]
                if scope in gas_payloads:
                    measured_gas = measured.get((scope, result["test_id"], key))
                    if measured_gas is None:
                        raise RuntimeError(
                            f"unmeasured bytecode: {scope}/{result['test_id']}/{compiler_id}"
                        )
                    if scope == "ci-gas":
                        ci_gas_by_test[result["test_id"]] = dict(measured_gas)
                    gas += sum(value for _, value in measured_gas)
                    calls += len(measured_gas)
            totals[scope] = {
                "deploy": deploy,
                "runtime": runtime,
                "gas": gas if calls else None,
                "calls": calls,
            }
        rows.append({"budget": budget, "totals": totals})
        fingerprints[budget] = json_sha256(fingerprint)
        ci_gas_by_budget[budget] = ci_gas_by_test

    baseline = rows[-1]
    baseline_ci_gas = ci_gas_by_budget[baseline["budget"]]
    for row in rows:
        max_contract_regression = 0
        max_call_regression = 0
        regressing_contracts = []
        for test_id, calls in ci_gas_by_budget[row["budget"]].items():
            baseline_calls = baseline_ci_gas[test_id]
            if calls.keys() != baseline_calls.keys():
                raise RuntimeError(f"CI call labels differ for {test_id}")
            contract_regression = sum(calls.values()) - sum(baseline_calls.values())
            if contract_regression > 0:
                regressing_contracts.append([test_id, contract_regression])
                max_contract_regression = max(max_contract_regression, contract_regression)
            for label, gas in calls.items():
                max_call_regression = max(
                    max_call_regression, gas - baseline_calls[label]
                )
        row["max_ci_contract_regression"] = max_contract_regression
        row["max_ci_call_regression"] = max_call_regression
        row["ci_regressing_contracts"] = regressing_contracts

    policy = rows[GROWTH_SWEEP_POLICY_BUDGET - GROWTH_SWEEP_MIN_BUDGET]
    if (
        policy["budget"] != GROWTH_SWEEP_POLICY_BUDGET
        or policy["max_ci_contract_regression"] != 0
        or policy["max_ci_call_regression"] != 0
    ):
        raise RuntimeError("growth budget policy regresses a CI contract or call")

    plateaus = []
    for row in rows:
        fingerprint = fingerprints[row["budget"]]
        if not plateaus or plateaus[-1]["fingerprint"] != fingerprint:
            plateaus.append(
                {
                    "first": row["budget"],
                    "last": row["budget"],
                    "fingerprint": fingerprint,
                }
            )
        else:
            plateaus[-1]["last"] = row["budget"]

    text = "## Growth-budget sweep\n\n"
    text += (
        f"Every integer budget from {budgets[0]} through {budgets[-1]} was compiled "
        f"across {len(compile_payloads['synthetic']['results'])} synthetic fixtures, "
        f"{len(compile_payloads['ui-gas']['results'])} UI files, and "
        f"{len(compile_payloads['ci-gas']['results'])} pinned CI contracts. "
        f"The sweep produced {len(set(fingerprints.values()))} distinct whole-corpus "
        "artifact fingerprints. Gas was executed once per distinct creation/runtime "
        "artifact and mapped back to every budget. The sweep checks the fixed, round policy "
        "limit against the saturated baseline; it does not select the compiler default."
    )
    text += "\n\n"
    text += markdown_table(
        (
            "Budget",
            "Synthetic gas",
            "Synthetic runtime B",
            "UI Ogas runtime B",
            "CI hot gas",
            "CI Ogas runtime B",
            "Max CI contract regression",
            "Max CI call regression",
        ),
        [
            [
                str(row["budget"]),
                str(row["totals"]["synthetic"]["gas"]),
                str(row["totals"]["synthetic"]["runtime"]),
                str(row["totals"]["ui-gas"]["runtime"]),
                str(row["totals"]["ci-gas"]["gas"]),
                str(row["totals"]["ci-gas"]["runtime"]),
                str(row["max_ci_contract_regression"]),
                str(row["max_ci_call_regression"]),
            ]
            for row in (policy, baseline)
        ],
    )
    text += "\n\n<details>\n<summary>Every growth budget</summary>\n\n"
    text += markdown_table(
        (
            "Budget",
            "Synthetic gas",
            "Synthetic runtime B",
            "UI Ogas runtime B",
            "CI hot gas",
            "CI Ogas runtime B",
            "Max CI contract regression",
            "Max CI call regression",
        ),
        [
            [
                str(row["budget"]),
                str(row["totals"]["synthetic"]["gas"]),
                str(row["totals"]["synthetic"]["runtime"]),
                str(row["totals"]["ui-gas"]["runtime"]),
                str(row["totals"]["ci-gas"]["gas"]),
                str(row["totals"]["ci-gas"]["runtime"]),
                str(row["max_ci_contract_regression"]),
                str(row["max_ci_call_regression"]),
            ]
            for row in rows
        ],
    )
    text += "\n\n</details>\n"
    summary = {
        "budgets": budgets,
        "policy_budget": policy["budget"],
        "policy": policy,
        "baseline_budget": baseline["budget"],
        "baseline": baseline,
        "fingerprint_plateaus": plateaus,
        "rows": rows,
    }
    return text, summary


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--solar", type=Path, default=Path("target/debug/solar"))
    parser.add_argument(
        "--benchmark-repo",
        type=Path,
        default=Path.home() / "github/danipopes/solidity-compiler-benchmarks",
    )
    parser.add_argument("--pin", default=DEFAULT_PIN)
    parser.add_argument(
        "--solc",
        type=Path,
        default=Path.home()
        / f".local/share/svm/{DEFAULT_SOLC_VERSION}/solc-{DEFAULT_SOLC_VERSION}",
    )
    parser.add_argument("--ui-corpus", type=Path, default=Path("tests/ui/codegen"))
    parser.add_argument("--output-dir", type=Path, default=Path("target/codegen-bench/switch"))
    parser.add_argument("--methods", nargs="+", choices=DEFAULT_METHODS, default=DEFAULT_METHODS)
    parser.add_argument(
        "--auto-growth-budgets",
        type=parse_integer_ranges,
        default=[],
        metavar="RANGES",
        help="add auto variants for comma-separated integers or inclusive START..END ranges",
    )
    parser.add_argument("--jobs", type=int, default=1)
    parser.add_argument("--aggregate-only", action="store_true")
    parser.add_argument("--compile-only", action="store_true")
    parser.add_argument(
        "--analyze-growth-sweep",
        nargs=2,
        type=Path,
        metavar=("COMPILE_DIR", "GAS_DIR"),
    )
    parser.add_argument(
        "--scope",
        nargs="+",
        choices=("synthetic", "ui-gas", "ui-size", "ci-size-gas", "ci-size", "ci-gas"),
        default=("synthetic", "ui-gas", "ui-size", "ci-size-gas", "ci-size", "ci-gas"),
    )
    args = parser.parse_args()
    if args.analyze_growth_sweep is not None:
        compile_dir, gas_dir = map(Path.resolve, args.analyze_growth_sweep)
        report, summary = render_growth_sweep(compile_dir, gas_dir)
        (compile_dir / "growth-report.md").write_text(report)
        (compile_dir / "growth-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        print(report)
        return 0
    if "auto" not in args.methods:
        parser.error("--methods must include auto")
    if len(set(args.methods)) != len(args.methods):
        parser.error("--methods must not contain duplicates")
    if len(set(args.scope)) != len(args.scope):
        parser.error("--scope must not contain duplicates")
    if args.jobs < 1:
        parser.error("--jobs must be positive")
    variants = switch_variants(args.methods, args.auto_growth_budgets)
    methods = [variant.compiler_id for variant in variants]
    if len(set(methods)) != len(methods):
        parser.error("switch variant IDs must not contain duplicates")

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
    for tool in ("anvil", "cast"):
        if shutil.which(tool) is None:
            parser.error(f"{tool} is required")
    solc = args.solc.resolve()
    if not solc.is_file():
        parser.error(f"solc binary not found: {solc}")

    source_root = Path(run(["git", "rev-parse", "--show-toplevel"])).resolve()
    require_clean_source(source_root)
    corpus_revisions = verify_corpus_pin(benchmark_repo, args.pin)
    source_revision = run(["git", "rev-parse", "HEAD"], source_root)
    built_revision = compiler_revision(solar)
    if built_revision != source_revision:
        raise RuntimeError(
            f"solar was built at {built_revision}, but the source checkout is {source_revision}"
        )
    ui_sources = list(ui_corpus.rglob("*.sol"))
    if not ui_sources:
        raise RuntimeError(f"UI corpus contains no Solidity files: {ui_corpus}")

    with tempfile.TemporaryDirectory(prefix="switch-benchmark-pin-") as temporary:
        pinned_root = materialize_benchmark_pin(
            benchmark_repo, args.pin, Path(temporary), corpus_revisions
        )
        bench = load_benchmark_module(pinned_root)
        benchmark_fixtures_sha256 = use_pinned_benchmark_inputs(
            bench, benchmark_repo, pinned_root
        )
        install_bytecode_hashes(bench)
        solc_version, solc_error = bench.binary_version(solc)
        if solc_error:
            raise RuntimeError(f"could not identify solc: {solc_error}")
        if solc_version.split("+", maxsplit=1)[0] != DEFAULT_SOLC_VERSION:
            raise RuntimeError(
                f"solc is {solc_version}, but CI is pinned to {DEFAULT_SOLC_VERSION}"
            )
        reference_spec = bench.CompilerSpec(
            "solc-reference", f"solc {solc_version}", solc, "solc"
        )

        metadata_base = {
            "solar": str(solar),
            "solar_sha256": sha256(solar),
            "solar_revision": built_revision,
            "source_tree": run(["git", "rev-parse", "HEAD^{tree}"], source_root),
            "benchmark_pin": args.pin,
            "benchmark_tree": run(["git", "rev-parse", f"{args.pin}^{{tree}}"], benchmark_repo),
            "benchmark_script_sha256": sha256(pinned_root / "solar_bench.py"),
            "gas_script_sha256": sha256(pinned_root / "gas_bench.py"),
            "benchmark_fixtures_sha256": benchmark_fixtures_sha256,
            "driver_sha256": sha256(Path(__file__).resolve()),
            "corpus_revisions": corpus_revisions,
            "ui_corpus_sha256": tree_sha256(ui_corpus, ui_sources),
            "methods": methods,
            "variants": [
                {
                    "compiler_id": variant.compiler_id,
                    "label": variant.label,
                    "flags": list(variant.flags),
                }
                for variant in variants
            ],
            "solc": str(solc),
            "solc_sha256": sha256(solc),
            "solc_version": tool_version(str(solc)),
            "anvil_version": tool_version("anvil"),
            "cast_version": tool_version("cast"),
            "hardfork": ANVIL_HARDFORK,
            "compile_only": args.compile_only,
        }

        if "synthetic" in args.scope:
            cases, labels, checks = synthetic_cases(bench)
            install_runtime_checks(bench, checks)
            metadata = {
                **metadata_base,
                "scope": "synthetic",
                "optimization": "gas",
                "fixture_version": SYNTHETIC_FIXTURE_VERSION,
                "expected_failures": [],
            }
            run_cases(
                bench,
                cases,
                compiler_specs(
                    bench, solar, variants, "gas", output_dir / "wrappers"
                ),
                output_dir / "synthetic.json",
                metadata,
                not args.compile_only,
                "smoke",
                labels,
                reference_spec=None if args.compile_only else reference_spec,
                jobs=args.jobs,
            )

        all_ui_cases = load_ui_cases(ui_corpus)
        ui_cases = [case for case in all_ui_cases if not case.expected_failure]
        ui_exclusions = sorted(
            case.test_id for case in all_ui_cases if case.expected_failure
        )
        for scope, optimization in (("ui-gas", "gas"), ("ui-size", "size")):
            if scope in args.scope:
                metadata = {
                    **metadata_base,
                    "scope": scope,
                    "optimization": optimization,
                    "expected_failures": [],
                    "excluded_cases": ui_exclusions,
                }
                run_cases(
                    bench,
                    ui_cases,
                    compiler_specs(
                        bench, solar, variants, optimization, output_dir / "wrappers"
                    ),
                    output_dir / f"{scope}.json",
                    metadata,
                    False,
                    "smoke",
                    jobs=args.jobs,
                )

        all_ci_cases = [*bench.TEST_CASES, *bench.REPO_TEST_CASES]
        ci_cases = [
            case
            for case in all_ci_cases
            if bench.version_in_range(
                solc_version,
                getattr(case, "min_solc", None),
                getattr(case, "max_solc", None),
            )
        ]
        ci_exclusions = sorted(
            case.test_id for case in all_ci_cases if case not in ci_cases
        )
        ci_metadata = {"excluded_cases": ci_exclusions, "expected_failures": []}
        for scope, optimization in (("ci-size-gas", "gas"), ("ci-size", "size")):
            if scope in args.scope:
                metadata = {
                    **metadata_base,
                    **ci_metadata,
                    "scope": scope,
                    "optimization": optimization,
                }
                run_cases(
                    bench,
                    ci_cases,
                    compiler_specs(
                        bench, solar, variants, optimization, output_dir / "wrappers"
                    ),
                    output_dir / f"{scope}.json",
                    metadata,
                    False,
                    "hot",
                    jobs=args.jobs,
                )

        if "ci-gas" in args.scope:
            metadata = {
                **metadata_base,
                **ci_metadata,
                "scope": "ci-gas",
                "optimization": "gas",
            }
            run_cases(
                bench,
                ci_cases,
                compiler_specs(
                    bench, solar, variants, "gas", output_dir / "wrappers"
                ),
                output_dir / "ci-gas.json",
                metadata,
                not args.compile_only,
                "hot",
                reference_spec=None if args.compile_only else reference_spec,
                jobs=args.jobs,
            )

        report = render_markdown(
            output_dir,
            methods,
            metadata_base,
            args.scope,
            include_details=not args.aggregate_only,
        )
        (output_dir / "report.md").write_text(report)
        print(report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
