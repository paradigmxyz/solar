#!/usr/bin/env python3
"""Run a generated Foundry differential fuzz target."""

from __future__ import annotations

import argparse
import copy
import datetime
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import tempfile
from typing import Any

import evm_runtime as evm
import symbolic_differential as symbolic
import write_foundry_target


def main() -> int:
    parser = argparse.ArgumentParser(
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )
    _add_shared_arguments(parser)
    parser.add_argument("--fuzz-runs", type=int, default=64)
    parser.add_argument(
        "--symbolic",
        action="store_true",
        help="symbolically compare one statically sized pure function",
    )
    _add_symbolic_arguments(parser)
    args = parser.parse_args()

    if args.symbolic:
        return _run_symbolic_or_incomplete(args)

    solc_runtime = evm.compile_solc(args.solc, args.source, args.contract, args.timeout)
    solar_runtime = evm.compile_solar(args.solar, args.source, args.contract, args.timeout)

    with tempfile.TemporaryDirectory(prefix="solar-foundry-fuzz-") as tmp:
        project = pathlib.Path(tmp)
        write_foundry_target.write_target(args.source, project, solc_runtime, solar_runtime)
        foundry = _forge_test(args.forge, project, args.fuzz_runs, args.timeout)

    summary = {
        "source": str(args.source),
        "foundry": foundry,
        "match": foundry["status"] == "ok",
    }
    print(json.dumps(summary, indent=2 if args.verbose else None, sort_keys=True))
    return 0 if summary["match"] else 1


def symbolic_main() -> int:
    """Run the focused repository-local `solsymdiff` command."""
    parser = argparse.ArgumentParser(
        prog="solsymdiff",
        description="Find bounded, replayable Solc-vs-Solar runtime differences.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    _add_shared_arguments(parser, contract_required=True)
    _add_symbolic_arguments(parser)
    args = parser.parse_args()
    args.symbolic = True
    return _run_symbolic_or_incomplete(args)


def _add_shared_arguments(
    parser: argparse.ArgumentParser, *, contract_required: bool = False
) -> None:
    parser.add_argument(
        "--source",
        type=pathlib.Path,
        required=True,
        help="root Solidity source file",
    )
    if contract_required:
        parser.add_argument(
            "--contract",
            required=True,
            help="contract name in the root source",
        )
    else:
        parser.add_argument(
            "--contract",
            default="FandangoRuntime",
            help="contract name in the root source",
        )
    parser.add_argument("--solc", default="solc", help="solc executable")
    parser.add_argument(
        "--solar",
        default="target/debug/solar",
        help="Solar executable",
    )
    parser.add_argument("--forge", default="forge", help="Forge executable")
    parser.add_argument("--anvil", default="anvil", help="Anvil executable")
    parser.add_argument(
        "--timeout",
        type=float,
        default=60.0,
        help=(
            "total symbolic wall-clock deadline across compilation, execution, "
            "and replay (per-process timeout for concrete fuzzing)"
        ),
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="pretty-print the JSON summary",
    )


def _add_symbolic_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "--signature",
        help=(
            "canonical function signature; optional only when exactly one "
            "eligible function exists"
        ),
    )
    parser.add_argument(
        "--evm-version",
        default="osaka",
        help="EVM version used by both compilers, Forge, and Anvil",
    )
    parser.add_argument(
        "--symbolic-solver",
        default="z3",
        help="SMT solver executable passed to Forge",
    )
    parser.add_argument(
        "--symbolic-timeout",
        type=int,
        default=30,
        help="Forge symbolic solver-query timeout in seconds",
    )
    parser.add_argument(
        "--symbolic-max-paths",
        type=int,
        default=1024,
        help="maximum symbolic paths explored by Forge",
    )
    parser.add_argument(
        "--symbolic-max-depth",
        type=int,
        help="optional maximum symbolic execution depth",
    )
    parser.add_argument(
        "--max-returndata-bytes",
        type=int,
        default=256,
        help="maximum exact return/revert bytes compared symbolically",
    )
    parser.add_argument(
        "--artifact-dir",
        type=pathlib.Path,
        default=pathlib.Path("fuzz/fandango/out/symbolic-differentials"),
        help="directory for durable result bundles",
    )


def _run_symbolic_or_incomplete(args: argparse.Namespace) -> int:
    try:
        return _run_symbolic(args)
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as err:
        return _symbolic_setup_incomplete(args, err)


def _forge_test(
    forge: str, project: pathlib.Path, fuzz_runs: int, timeout: float
) -> dict[str, Any]:
    env = os.environ.copy()
    try:
        result = subprocess.run(
            [forge, "test", "--fuzz-runs", str(fuzz_runs)],
            cwd=project,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired as err:
        return {
            "status": "timeout",
            "stdout": err.stdout or "",
            "stderr": err.stderr or "",
        }
    return {
        "status": "ok" if result.returncode == 0 else "error",
        "returncode": result.returncode,
        "stdout": result.stdout,
        "stderr": result.stderr,
    }


def _run_symbolic(args: argparse.Namespace) -> int:
    if os.name != "posix":
        raise ValueError(
            "symbolic differential process-tree isolation currently requires "
            "Linux or macOS"
        )
    if args.timeout <= 0:
        raise ValueError("--timeout must be positive")
    if args.symbolic_timeout <= 0:
        raise ValueError("--symbolic-timeout must be positive")
    if args.symbolic_max_paths <= 0:
        raise ValueError("--symbolic-max-paths must be positive")
    if args.symbolic_max_depth is not None and args.symbolic_max_depth <= 0:
        raise ValueError("--symbolic-max-depth must be positive")
    if args.max_returndata_bytes <= 0:
        raise ValueError("--max-returndata-bytes must be positive")

    deadline = evm.Deadline(args.timeout)
    args._deadline = deadline
    args._solc_executable = _resolve_executable(args.solc)
    args._solar_executable = _resolve_executable(args.solar)
    args._forge_executable = _resolve_executable(args.forge)
    args._anvil_executable = _resolve_executable(args.anvil)
    args._solver_executable = _resolve_executable(args.symbolic_solver)
    args._tool_versions = {}
    for label, tool in (
        ("forge", args._forge_executable),
        ("anvil", args._anvil_executable),
        ("solver", args._solver_executable),
    ):
        args._tool_versions[label] = _tool_version(tool, deadline)
    artifact_root = args.artifact_dir.resolve()
    source = args.source.resolve()
    standard_input = evm.materialize_standard_input(
        args._solc_executable,
        source,
        args.timeout,
        args.evm_version,
        deadline=deadline,
    )
    args._standard_input = standard_input
    with tempfile.TemporaryDirectory(
        prefix="solar-symbolic-compilers-"
    ) as compiler_cwd:
        solc_artifact = evm.compile_standard_artifact(
            args._solc_executable,
            source,
            args.contract,
            args.timeout,
            kind="solc",
            evm_version=args.evm_version,
            standard_input=standard_input,
            deadline=deadline,
            compiler_cwd=pathlib.Path(compiler_cwd),
        )
        args._solc_artifact = solc_artifact
        solar_artifact = evm.compile_standard_artifact(
            args._solar_executable,
            source,
            args.contract,
            args.timeout,
            kind="solar",
            evm_version=args.evm_version,
            standard_input=standard_input,
            deadline=deadline,
            compiler_cwd=pathlib.Path(compiler_cwd),
        )
    args._solar_artifact = solar_artifact
    if solc_artifact["settings"] != solar_artifact["settings"]:
        raise ValueError("compiler settings differ")
    if (
        solc_artifact["standard_input_sha256"] != standard_input["sha256"]
        or solar_artifact["standard_input_sha256"] != standard_input["sha256"]
    ):
        raise ValueError("compilers did not receive the materialized Standard JSON input")
    function = symbolic.select_function(solc_artifact, solar_artifact, args.signature)
    expected_test = (
        f"{function['test']}({','.join(function['inputs'])})"
    )

    with tempfile.TemporaryDirectory(prefix="solar-symbolic-differential-") as tmp:
        project = pathlib.Path(tmp)
        write_foundry_target.write_symbolic_target(
            project,
            solc_artifact["runtime"],
            solar_artifact["runtime"],
            function,
            args.max_returndata_bytes,
            args.evm_version,
        )
        forge_run = _forge_symbolic(args, project, expected_test, deadline)
        classified = None
        direct = None
        final_status = "incomplete"
        reason = forge_run.get("reason")
        if forge_run.get("report") is not None:
            try:
                classified = symbolic.classify_forge_json(forge_run["report"])
            except ValueError as err:
                reason = str(err)
            else:
                if classified["test"] != expected_test:
                    reason = (
                        f"Forge ran `{classified['test']}`, expected `{expected_test}`"
                    )
                elif (
                    classified["status"] == "no_mismatch_within_bounds"
                    and forge_run.get("returncode") != 0
                ):
                    reason = (
                        "Forge reported a symbolic pass with process status "
                        f"{forge_run.get('returncode')}"
                    )
                elif (
                    classified["status"] != "no_mismatch_within_bounds"
                    and forge_run.get("returncode") == 0
                ):
                    reason = (
                        "Forge reported a non-pass symbolic result with process status 0"
                    )
                elif classified["status"] == "no_mismatch_within_bounds":
                    final_status = "no_mismatch_within_bounds"
                    reason = None
                elif classified["status"] == "incomplete":
                    incomplete = classified.get("incomplete") or {}
                    reason = incomplete.get("reason") or "symbolic execution was incomplete"
                else:
                    wrapper_calldata = classified["counterexample"]["calldata"]
                    calldata = symbolic.target_calldata(
                        function["selector"], wrapper_calldata
                    )
                    try:
                        direct = symbolic.run_direct_replay(
                            args._solc_executable,
                            args._anvil_executable,
                            args.evm_version,
                            solc_artifact["runtime"],
                            solar_artifact["runtime"],
                            calldata,
                            args.timeout,
                            deadline=deadline,
                        )
                    except (
                        OSError,
                        RuntimeError,
                        ValueError,
                        subprocess.SubprocessError,
                    ) as err:
                        reason = f"independent concrete replay failed: {err}"
                    else:
                        if symbolic.confirm_outcomes(direct["solc"], direct["solar"]):
                            final_status = "replay_confirmed_mismatch"
                            reason = None
                        else:
                            byte_len = (len(direct["solc"]["data"]) - 2) // 2
                            if byte_len > args.max_returndata_bytes:
                                reason = (
                                    "equal concrete outcomes exceeded "
                                    f"--max-returndata-bytes ({byte_len} > "
                                    f"{args.max_returndata_bytes})"
                                )
                            else:
                                reason = (
                                    "Foundry's candidate did not produce different "
                                    "Solc and Solar outcomes in independent replay"
                                )

        manifest = _manifest(
            args,
            source,
            function,
            solc_artifact,
            solar_artifact,
            forge_run,
            classified,
            direct,
            final_status,
            reason,
        )
        bundle = _persist_bundle(
            artifact_root,
            source,
            standard_input,
            project,
            manifest,
            forge_run,
            classified,
            direct,
        )
        persistence_timeout = _deadline_error(deadline, "artifact persistence")
        durable = _durable_replay(
            args._forge_executable,
            args._solc_executable,
            bundle,
            classified,
            args.timeout,
            deadline,
            args.evm_version,
            required=(
                final_status == "replay_confirmed_mismatch"
                and persistence_timeout is None
            ),
        )
        manifest["replay"]["durable_foundry_artifact"] = durable
        if (
            final_status == "replay_confirmed_mismatch"
            and not durable.get("reproduced", False)
        ):
            final_status = "incomplete"
            reason = durable.get("reason") or (
                "copied Foundry counterexample artifact did not durably reproduce"
            )
            manifest["status"] = final_status
            manifest["reason"] = reason
        if persistence_timeout is not None:
            final_status = "incomplete"
            reason = persistence_timeout
            manifest["status"] = final_status
            manifest["reason"] = reason
        final_timeout = _deadline_error(deadline, "final result persistence")
        if final_timeout is not None:
            final_status = "incomplete"
            reason = final_timeout
            manifest["status"] = final_status
            manifest["reason"] = reason
        manifest["bounds"]["elapsed_wall_seconds"] = deadline.elapsed()
        manifest["artifact_dir"] = str(bundle)
        _write_json_atomic(bundle / "manifest.json", manifest)

    summary = {
        "schema": symbolic.RESULT_SCHEMA,
        "status": final_status,
        "reason": reason,
        "source": str(source),
        "contract": args.contract,
        "function": {
            "signature": function["signature"],
            "selector": function["selector"],
        },
        "artifact_dir": str(bundle),
    }
    print(json.dumps(summary, indent=2 if args.verbose else None, sort_keys=True))
    return {
        "no_mismatch_within_bounds": 0,
        "replay_confirmed_mismatch": 1,
        "incomplete": 2,
    }[final_status]


def _forge_symbolic(
    args: argparse.Namespace,
    project: pathlib.Path,
    expected_test: str,
    deadline: evm.Deadline,
) -> dict[str, Any]:
    command = [
        getattr(args, "_forge_executable", args.forge),
        "test",
        "--root",
        str(project),
        "--evm-version",
        args.evm_version,
        "--use",
        args._solc_executable,
        "--symbolic",
        "--match-test",
        f"^{re.escape(expected_test)}$",
        "--symbolic-solver",
        getattr(args, "_solver_executable", args.symbolic_solver),
        "--symbolic-timeout",
        str(args.symbolic_timeout),
        "--symbolic-max-paths",
        str(args.symbolic_max_paths),
        "--json",
    ]
    if args.symbolic_max_depth is not None:
        command.extend(["--symbolic-max-depth", str(args.symbolic_max_depth)])
    try:
        timeout = deadline.remaining("Forge symbolic execution")
        with tempfile.TemporaryDirectory(
            prefix="solar-symbolic-forge-home-"
        ) as forge_home:
            result = _run_process_group(
                command,
                timeout,
                env=_forge_environment(pathlib.Path(forge_home)),
                cwd=project,
            )
    except (subprocess.TimeoutExpired, TimeoutError) as err:
        return {
            "command": command,
            "status": "timeout",
            "reason": f"Forge exceeded the {args.timeout:g}s total wall timeout",
            "stdout": _expired_text(getattr(err, "stdout", None)),
            "stderr": _expired_text(getattr(err, "stderr", None)),
            "report": None,
        }
    try:
        report = symbolic.parse_json_output(
            result.stdout, result.stderr, "Forge symbolic execution"
        )
        reason = None
    except ValueError as err:
        report = None
        reason = str(err)
    return {
        "command": command,
        "status": "completed",
        "returncode": result.returncode,
        "reason": reason,
        "stdout": result.stdout,
        "stderr": result.stderr,
        "report": report,
    }


def _resolve_executable(tool: str) -> str:
    resolved = shutil.which(tool)
    if resolved is None:
        raise OSError(f"executable `{tool}` was not found")
    return str(pathlib.Path(resolved).resolve())


def _tool_version(tool: str, deadline: evm.Deadline) -> str:
    result = subprocess.run(
        [tool, "--version"],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=deadline.remaining(f"{tool} --version"),
    )
    return (result.stdout or result.stderr).strip()


def _forge_environment(home: pathlib.Path) -> dict[str, str]:
    env = os.environ.copy()
    for name in list(env):
        if name.startswith(("FOUNDRY_", "DAPP_")) or name == "SVM_HOME":
            del env[name]
    env["HOME"] = str(home)
    env["XDG_CONFIG_HOME"] = str(home / ".config")
    env["XDG_CACHE_HOME"] = str(home / ".cache")
    env["XDG_DATA_HOME"] = str(home / ".local" / "share")
    return env


def _manifest(
    args: argparse.Namespace,
    source: pathlib.Path,
    function: dict[str, Any],
    solc_artifact: dict[str, Any],
    solar_artifact: dict[str, Any],
    forge_run: dict[str, Any],
    classified: dict[str, Any] | None,
    direct: dict[str, Any] | None,
    status: str,
    reason: str | None,
) -> dict[str, Any]:
    standard_input = solc_artifact["standard_input"]
    source_manifest = _source_manifest(source, standard_input)
    proxy = direct.get("proxy") if direct else None
    return {
        "schema": symbolic.RESULT_SCHEMA,
        "schema_version": 1,
        "created_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "status": status,
        "reason": reason,
        "source": source_manifest,
        "standard_input": _standard_input_manifest(standard_input),
        "contract": args.contract,
        "function": function,
        "settings": solc_artifact["settings"],
        "compilers": {
            "solc": _compiler_manifest(solc_artifact),
            "solar": _compiler_manifest(solar_artifact),
        },
        "bounds": _bounds_manifest(args, classified),
        "tools": _tools_manifest(args),
        "solver": {
            "requested": args.symbolic_solver,
            "executable": getattr(
                args, "_solver_executable", args.symbolic_solver
            ),
            "forge": classified.get("solver") if classified else None,
        },
        "forge": {
            "command": _durable_project_command(forge_run["command"]),
            "process_status": forge_run["status"],
            "returncode": forge_run.get("returncode"),
            "symbolic_status": classified.get("forge_status") if classified else None,
            "incomplete": classified.get("incomplete") if classified else None,
            "suite": classified.get("suite") if classified else None,
            "test": classified.get("test") if classified else None,
            "counterexample": (
                classified.get("counterexample") if classified else None
            ),
            "artifact": classified.get("artifact") if classified else None,
            "assumptions": classified.get("assumptions", []) if classified else [],
            "environment": {
                "cleared_prefixes": ["FOUNDRY_", "DAPP_", "SVM_HOME"],
                "evm_version_from_cli": args.evm_version,
                "home": "isolated temporary directory",
                "solc_from_cli": args._solc_executable,
                "working_directory": "project",
            },
        },
        "replay": {
            "foundry": classified.get("replay") if classified else None,
            "call_kind": direct.get("call_kind") if direct else None,
            "target_calldata": direct.get("calldata") if direct else None,
            "solc": direct.get("solc") if direct else None,
            "solar": direct.get("solar") if direct else None,
            "implementation_address": (
                direct.get("implementation_address") if direct else None
            ),
            "proxy_address": direct.get("proxy_address") if direct else None,
            "rpc_block": direct.get("rpc_block") if direct else None,
            "rpc_transaction": direct.get("rpc_transaction") if direct else None,
            "rpc_environment": {
                "cleared_prefixes": ["ANVIL_", "FOUNDRY_"],
                "host_from_cli": "127.0.0.1",
                "hardfork_from_cli": args.evm_version,
                "working_directory": "isolated temporary directory",
            },
            "anvil": direct.get("anvil") if direct else None,
            "proxy": _proxy_manifest(proxy) if isinstance(proxy, dict) else None,
            "durable_foundry_artifact": None,
        },
        "artifacts": {
            "project": {"path": "project", "sha256": None},
            "source": {
                "path": f"source/{source.name}",
                "sha256": source_manifest["sha256"],
            },
            "standard_input": {
                "path": "standard-input.json",
                "sha256": standard_input["sha256"],
            },
            "foundry_counterexample": None,
            "static_call_proxy_standard_input": None,
            "static_call_proxy_runtime": None,
        },
        "artifact_dir": None,
    }


def _compiler_manifest(artifact: dict[str, Any]) -> dict[str, Any]:
    runtime = bytes.fromhex(artifact["runtime"].removeprefix("0x"))
    return {
        "version": artifact["version"],
        "command": artifact["command"],
        "standard_input_sha256": artifact["standard_input_sha256"],
        "runtime_bytecode_sha256": hashlib.sha256(runtime).hexdigest(),
        "runtime_bytecode_bytes": len(runtime),
        "environment": {
            "filesystem_import_fallback": artifact.get(
                "filesystem_import_fallback", False
            ),
            "working_directory": "isolated temporary directory",
        },
    }


def _standard_input_manifest(standard_input: dict[str, Any]) -> dict[str, Any]:
    return {
        "path": "standard-input.json",
        "sha256": standard_input["sha256"],
        "root_source": standard_input["root_source"],
        "sources": standard_input["sources"],
    }


def _source_manifest(
    source: pathlib.Path, standard_input: dict[str, Any]
) -> dict[str, Any]:
    content = _root_source_content(standard_input)
    return {
        "path": str(source),
        "sha256": hashlib.sha256(content.encode("utf-8")).hexdigest(),
    }


def _root_source_content(standard_input: dict[str, Any]) -> str:
    try:
        value = json.loads(standard_input["json"])
        content = value["sources"][standard_input["root_source"]]["content"]
    except (KeyError, TypeError, json.JSONDecodeError) as err:
        raise ValueError("materialized Standard JSON has no root source content") from err
    if not isinstance(content, str):
        raise ValueError("materialized root source content must be text")
    return content


def _bounds_manifest(
    args: argparse.Namespace, classified: dict[str, Any] | None
) -> dict[str, Any]:
    deadline = getattr(args, "_deadline", None)
    return {
        "total_wall_timeout_seconds": args.timeout,
        "solver_query_timeout_seconds": args.symbolic_timeout,
        "max_paths": args.symbolic_max_paths,
        "max_depth": args.symbolic_max_depth,
        "max_returndata_bytes": args.max_returndata_bytes,
        "forge_effective": classified.get("bounds") if classified else None,
        "elapsed_wall_seconds": deadline.elapsed() if deadline else None,
    }


def _tools_manifest(args: argparse.Namespace) -> dict[str, Any]:
    versions = getattr(args, "_tool_versions", {})
    return {
        "forge": versions.get("forge"),
        "anvil": versions.get("anvil"),
        "solver": versions.get("solver"),
    }


def _proxy_manifest(proxy: dict[str, Any]) -> dict[str, Any]:
    runtime = bytes.fromhex(proxy["runtime"].removeprefix("0x"))
    return {
        "compiler": _compiler_manifest(proxy),
        "standard_input_path": "static-call-proxy-standard-input.json",
        "standard_input_sha256": proxy["standard_input_sha256"],
        "runtime_path": "static-call-proxy-runtime.hex",
        "runtime_bytecode_sha256": hashlib.sha256(runtime).hexdigest(),
        "runtime_bytecode_bytes": len(runtime),
    }


def _durable_project_command(command: list[str]) -> list[str]:
    durable = list(command)
    if "--root" in durable:
        index = durable.index("--root")
        if index + 1 < len(durable):
            durable[index + 1] = "project"
    return durable


def _persist_bundle(
    root: pathlib.Path,
    source: pathlib.Path,
    standard_input: dict[str, Any],
    project: pathlib.Path,
    manifest: dict[str, Any],
    forge_run: dict[str, Any],
    classified: dict[str, Any] | None,
    direct: dict[str, Any] | None,
) -> pathlib.Path:
    root.mkdir(parents=True, exist_ok=True)
    timestamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    selector = manifest["function"]["selector"][2:]
    bundle = root / f"{source.stem}-{selector}-{timestamp}"
    bundle.mkdir()
    provisional = copy.deepcopy(manifest)
    provisional["status"] = "incomplete"
    provisional["reason"] = "artifact finalization and durable replay are pending"
    provisional["artifact_dir"] = str(bundle)
    _write_json_atomic(bundle / "manifest.json", provisional)
    shutil.copytree(
        project,
        bundle / "project",
        ignore=shutil.ignore_patterns("out", "cache"),
    )
    manifest["artifacts"]["project"]["sha256"] = _tree_sha256(bundle / "project")
    source_dir = bundle / "source"
    source_dir.mkdir()
    (source_dir / source.name).write_bytes(
        _root_source_content(standard_input).encode("utf-8")
    )
    (bundle / "standard-input.json").write_bytes(
        standard_input["json"].encode("utf-8")
    )
    (bundle / "forge.json").write_text(
        json.dumps(forge_run.get("report"), indent=2, sort_keys=True) + "\n"
    )
    (bundle / "forge.stdout.txt").write_text(forge_run.get("stdout", ""))
    (bundle / "forge.stderr.txt").write_text(forge_run.get("stderr", ""))
    if direct and isinstance(direct.get("proxy"), dict):
        proxy = direct["proxy"]
        proxy_input = bundle / "static-call-proxy-standard-input.json"
        proxy_runtime = bundle / "static-call-proxy-runtime.hex"
        proxy_input.write_bytes(proxy["standard_input"]["json"].encode("utf-8"))
        proxy_runtime.write_text(proxy["runtime"] + "\n", encoding="utf-8")
        manifest["artifacts"]["static_call_proxy_standard_input"] = {
            "path": proxy_input.name,
            "sha256": _file_sha256(proxy_input),
        }
        manifest["artifacts"]["static_call_proxy_runtime"] = {
            "path": proxy_runtime.name,
            "sha256": _file_sha256(proxy_runtime),
        }
    if classified and isinstance(classified.get("artifact"), dict):
        artifact_path = pathlib.Path(classified["artifact"]["path"])
        if not artifact_path.is_absolute():
            artifact_path = project / artifact_path
        if artifact_path.is_file():
            try:
                artifact = json.loads(artifact_path.read_text())
            except (OSError, json.JSONDecodeError):
                artifact = None
            if isinstance(artifact, dict) and symbolic.counterexample_artifact_matches(
                artifact,
                classified["test"],
                classified["counterexample"]["calldata"],
            ):
                copied = bundle / "foundry-counterexample.json"
                shutil.copy2(artifact_path, copied)
                manifest["artifacts"]["foundry_counterexample"] = {
                    "path": copied.name,
                    "sha256": _file_sha256(copied),
                }
                manifest["forge"]["artifact"]["path"] = copied.name
            else:
                shutil.copy2(
                    artifact_path, bundle / "foundry-counterexample.invalid.json"
                )
    return bundle


def _file_sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _tree_sha256(root: pathlib.Path) -> str:
    digest = hashlib.sha256()
    for path in sorted(item for item in root.rglob("*") if item.is_file()):
        digest.update(path.relative_to(root).as_posix().encode("utf-8"))
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def _write_json_atomic(path: pathlib.Path, value: dict[str, Any]) -> None:
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    temporary.replace(path)


def _durable_replay(
    forge: str,
    solc: str,
    bundle: pathlib.Path,
    classified: dict[str, Any] | None,
    timeout: float,
    deadline: evm.Deadline,
    evm_version: str,
    *,
    required: bool,
) -> dict[str, Any]:
    artifact = bundle / "foundry-counterexample.json"
    if (
        not required
        or not classified
        or classified.get("status") != "replay_confirmed_mismatch"
    ):
        return {"required": False, "reproduced": False}
    if not artifact.is_file():
        return {
            "required": True,
            "reproduced": False,
            "reason": "confirmed Foundry counterexample artifact was not copied",
        }
    command = [
        forge,
        "test",
        "--root",
        str(bundle / "project"),
        "--evm-version",
        evm_version,
        "--use",
        solc,
        "--replay-symbolic-artifact",
        str(artifact),
        "--json",
    ]
    try:
        deadline.remaining("durable Foundry replay setup")
        with tempfile.TemporaryDirectory(
            prefix="solar-symbolic-durable-replay-"
        ) as temporary:
            replay_project = pathlib.Path(temporary) / "project"
            shutil.copytree(bundle / "project", replay_project)
            forge_home = pathlib.Path(temporary) / "home"
            forge_home.mkdir()
            execution_command = list(command)
            execution_command[execution_command.index("--root") + 1] = str(
                replay_project
            )
            remaining = min(timeout, deadline.remaining("durable Foundry replay"))
            result = _run_process_group(
                execution_command,
                remaining,
                env=_forge_environment(forge_home),
                cwd=replay_project,
            )
    except (subprocess.TimeoutExpired, TimeoutError) as err:
        return {
            "required": True,
            "reproduced": False,
            "command": _durable_replay_command(command),
            "reason": "durable replay timed out",
            "stdout": _expired_text(getattr(err, "stdout", None)),
            "stderr": _expired_text(getattr(err, "stderr", None)),
        }
    except OSError as err:
        return {
            "required": True,
            "reproduced": False,
            "command": _durable_replay_command(command),
            "reason": f"durable replay could not start: {err}",
        }
    try:
        report = symbolic.parse_json_output(
            result.stdout, result.stderr, "Forge durable artifact replay"
        )
        reproduced = (
            result.returncode != 0
            and symbolic.unit_replay_reproduced(
                report,
                classified["test"],
                classified["counterexample"]["calldata"],
            )
        )
        reason = None if reproduced else "expected failing replay result was not found"
    except ValueError as err:
        report = None
        reproduced = False
        reason = str(err)
    (bundle / "foundry-replay.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n"
    )
    return {
        "required": True,
        "reproduced": reproduced,
        "command": _durable_replay_command(command),
        "returncode": result.returncode,
        "reason": reason,
    }


def _durable_replay_command(command: list[str]) -> list[str]:
    durable = _durable_project_command(command)
    if "--replay-symbolic-artifact" in durable:
        index = durable.index("--replay-symbolic-artifact")
        if index + 1 < len(durable):
            durable[index + 1] = "foundry-counterexample.json"
    return durable


def _expired_text(value: str | bytes | None) -> str:
    if value is None:
        return ""
    return value.decode(errors="replace") if isinstance(value, bytes) else value


def _deadline_error(deadline: evm.Deadline, operation: str) -> str | None:
    try:
        deadline.remaining(operation)
    except TimeoutError as err:
        return str(err)
    return None


def _run_process_group(
    command: list[str],
    timeout: float,
    *,
    env: dict[str, str] | None = None,
    cwd: pathlib.Path | None = None,
) -> subprocess.CompletedProcess[str]:
    """Run one Forge invocation and always reap its solver process tree."""
    process = subprocess.Popen(
        command,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        cwd=cwd,
        **evm.process_group_options(),
    )
    try:
        with evm.cleanup_process_on_signals(process):
            stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired as err:
        evm.kill_process_tree(process)
        stdout, stderr = process.communicate()
        raise subprocess.TimeoutExpired(
            command,
            timeout,
            output=stdout or _expired_text(err.stdout),
            stderr=stderr or _expired_text(err.stderr),
        ) from err
    except BaseException:
        evm.kill_process_tree(process)
        try:
            process.communicate(timeout=5)
        except (subprocess.SubprocessError, OSError):
            pass
        raise
    evm.kill_process_tree(process)
    return subprocess.CompletedProcess(
        command,
        process.returncode,
        stdout,
        stderr,
    )


def _symbolic_setup_incomplete(args: argparse.Namespace, err: Exception) -> int:
    root = args.artifact_dir.resolve()
    root.mkdir(parents=True, exist_ok=True)
    timestamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    source_stem = args.source.stem if args.source.name else "source"
    bundle = root / f"{source_stem}-incomplete-{timestamp}"
    bundle.mkdir()
    source = args.source.resolve()
    standard_input = getattr(args, "_standard_input", None)
    has_standard_input = isinstance(standard_input, dict)
    source_manifest = (
        _source_manifest(source, standard_input)
        if has_standard_input
        else {"path": str(source), "sha256": None}
    )
    solc_artifact = getattr(args, "_solc_artifact", None)
    solar_artifact = getattr(args, "_solar_artifact", None)
    manifest = {
        "schema": symbolic.RESULT_SCHEMA,
        "schema_version": 1,
        "created_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "status": "incomplete",
        "reason": str(err),
        "source": source_manifest,
        "standard_input": (
            _standard_input_manifest(standard_input) if has_standard_input else None
        ),
        "contract": args.contract,
        "function": (
            {"signature": args.signature} if args.signature is not None else None
        ),
        "settings": standard_input.get("settings") if has_standard_input else None,
        "compilers": {
            "solc": (
                _compiler_manifest(solc_artifact)
                if isinstance(solc_artifact, dict)
                else None
            ),
            "solar": (
                _compiler_manifest(solar_artifact)
                if isinstance(solar_artifact, dict)
                else None
            ),
        },
        "bounds": _bounds_manifest(args, None),
        "tools": _tools_manifest(args),
        "solver": {
            "requested": args.symbolic_solver,
            "executable": getattr(
                args, "_solver_executable", args.symbolic_solver
            ),
            "forge": None,
        },
        "forge": {
            "command": None,
            "process_status": None,
            "returncode": None,
            "symbolic_status": None,
            "incomplete": None,
            "suite": None,
            "test": None,
            "counterexample": None,
            "artifact": None,
            "assumptions": [],
            "environment": {
                "cleared_prefixes": ["FOUNDRY_", "DAPP_", "SVM_HOME"],
                "evm_version_from_cli": args.evm_version,
                "home": "isolated temporary directory",
                "solc_from_cli": getattr(args, "_solc_executable", args.solc),
                "working_directory": "project",
            },
        },
        "replay": {
            "foundry": None,
            "call_kind": None,
            "target_calldata": None,
            "solc": None,
            "solar": None,
            "implementation_address": None,
            "proxy_address": None,
            "rpc_block": None,
            "rpc_transaction": None,
            "rpc_environment": {
                "cleared_prefixes": ["ANVIL_", "FOUNDRY_"],
                "host_from_cli": "127.0.0.1",
                "hardfork_from_cli": args.evm_version,
                "working_directory": "isolated temporary directory",
            },
            "anvil": None,
            "proxy": None,
            "durable_foundry_artifact": None,
        },
        "artifacts": {
            "project": {"path": "project", "sha256": None},
            "source": (
                {
                    "path": f"source/{source.name}",
                    "sha256": source_manifest["sha256"],
                }
                if has_standard_input
                else None
            ),
            "standard_input": (
                {
                    "path": "standard-input.json",
                    "sha256": standard_input["sha256"],
                }
                if has_standard_input
                else None
            ),
            "foundry_counterexample": None,
            "static_call_proxy_standard_input": None,
            "static_call_proxy_runtime": None,
        },
        "artifact_dir": str(bundle),
    }
    if has_standard_input:
        (bundle / "standard-input.json").write_bytes(
            standard_input["json"].encode("utf-8")
        )
        source_dir = bundle / "source"
        source_dir.mkdir()
        (source_dir / source.name).write_bytes(
            _root_source_content(standard_input).encode("utf-8")
        )
    _write_json_atomic(bundle / "manifest.json", manifest)
    summary = {
        "schema": symbolic.RESULT_SCHEMA,
        "status": "incomplete",
        "reason": str(err),
        "source": str(source),
        "contract": args.contract,
        "function": (
            {"signature": args.signature, "selector": None}
            if args.signature is not None
            else None
        ),
        "artifact_dir": str(bundle),
    }
    print(json.dumps(summary, indent=2 if args.verbose else None, sort_keys=True))
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
