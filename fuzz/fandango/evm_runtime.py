#!/usr/bin/env python3
"""Shared EVM/anvil helpers for Fandango differential runners."""

from __future__ import annotations

import contextlib
import hashlib
import json
import os
import pathlib
import signal
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request
from collections.abc import Iterator
from typing import Any


SOLC_ADDRESS = "0x1000000000000000000000000000000000000001"
SOLAR_ADDRESS = "0x1000000000000000000000000000000000000002"
STATIC_PROXY_ADDRESS = "0x1000000000000000000000000000000000000003"
# Well-known anvil dev account 0; unlocked, so `eth_sendTransaction` needs no
# signature.
ANVIL_SENDER = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
# Explicit gas limit so a reverting transaction is mined with status 0x0 instead
# of being rejected during gas estimation. Safely below anvil's block limit.
TX_GAS = "0x1000000"


class InfraError(RuntimeError):
    """Transient local infrastructure failure, not a compiler finding."""


def process_group_options() -> dict[str, Any]:
    """Return platform-specific Popen options for owning a child process tree."""
    if os.name == "posix":
        return {"start_new_session": True}
    if os.name == "nt":
        return {
            "creationflags": getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
        }
    return {}


def kill_process_tree(process: subprocess.Popen[Any]) -> None:
    """Kill a managed process and every child in its isolated process group."""
    if os.name == "posix":
        try:
            os.killpg(process.pid, signal.SIGKILL)
            return
        except ProcessLookupError:
            return
        except OSError:
            pass
    if process.poll() is not None:
        return
    if os.name == "nt":
        try:
            result = subprocess.run(
                ["taskkill", "/PID", str(process.pid), "/T", "/F"],
                check=False,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            if result.returncode == 0:
                return
        except OSError:
            pass
    try:
        process.kill()
    except ProcessLookupError:
        pass


def terminate_process_tree(
    process: subprocess.Popen[Any], grace_seconds: float = 1.0
) -> None:
    """Gracefully stop a managed process tree, then force-kill it if needed."""
    if os.name == "posix":
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            return
        except OSError:
            if process.poll() is not None:
                return
            process.terminate()
        deadline = time.monotonic() + grace_seconds
        while True:
            try:
                os.killpg(process.pid, 0)
            except ProcessLookupError:
                try:
                    process.wait(timeout=0)
                except subprocess.TimeoutExpired:
                    pass
                return
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                break
            try:
                process.wait(timeout=min(0.05, remaining))
            except subprocess.TimeoutExpired:
                pass
        # The leader may have exited while a descendant ignored SIGTERM. The
        # process group still exists and must be force-killed independently of
        # the leader's return code.
        kill_process_tree(process)
        try:
            process.wait(timeout=grace_seconds)
        except subprocess.TimeoutExpired:
            pass
        return
    elif process.poll() is not None:
        return
    elif os.name == "nt":
        try:
            result = subprocess.run(
                ["taskkill", "/PID", str(process.pid), "/T"],
                check=False,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            if result.returncode != 0:
                process.terminate()
        except OSError:
            process.terminate()
    else:
        process.terminate()
    try:
        process.wait(timeout=grace_seconds)
    except subprocess.TimeoutExpired:
        kill_process_tree(process)
        try:
            process.wait(timeout=grace_seconds)
        except subprocess.TimeoutExpired:
            pass


@contextlib.contextmanager
def cleanup_process_on_signals(
    process: subprocess.Popen[Any],
) -> Iterator[None]:
    """Kill a managed child tree before propagating SIGINT or SIGTERM."""
    if threading.current_thread() is not threading.main_thread():
        yield
        return

    previous: dict[int, Any] = {}

    def handle(signum: int, frame: Any) -> None:
        kill_process_tree(process)
        prior = previous[signum]
        if prior == signal.SIG_IGN:
            return
        if callable(prior):
            prior(signum, frame)
            return
        if signum == signal.SIGINT:
            raise KeyboardInterrupt
        raise SystemExit(128 + signum)

    signals = [signal.SIGINT]
    if hasattr(signal, "SIGTERM"):
        signals.append(signal.SIGTERM)
    try:
        for signum in signals:
            previous[signum] = signal.getsignal(signum)
            signal.signal(signum, handle)
    except (OSError, ValueError):
        for signum, prior in previous.items():
            signal.signal(signum, prior)
        yield
        return
    try:
        yield
    finally:
        for signum, prior in previous.items():
            if signal.getsignal(signum) is handle:
                signal.signal(signum, prior)


def run_process_group(
    command: list[str],
    timeout: float,
    *,
    input: str | None = None,
    check: bool = False,
    cwd: pathlib.Path | str | None = None,
    env: dict[str, str] | None = None,
    encoding: str = "utf-8",
) -> subprocess.CompletedProcess[str]:
    """Run one bounded tool invocation and always reap its process tree."""
    process = subprocess.Popen(
        command,
        stdin=subprocess.PIPE if input is not None else None,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding=encoding,
        cwd=cwd,
        env=env,
        **process_group_options(),
    )
    try:
        with cleanup_process_on_signals(process):
            stdout, stderr = process.communicate(input=input, timeout=timeout)
    except subprocess.TimeoutExpired as err:
        kill_process_tree(process)
        stdout, stderr = process.communicate()
        raise subprocess.TimeoutExpired(
            command,
            timeout,
            output=stdout or _timeout_text(err.stdout),
            stderr=stderr or _timeout_text(err.stderr),
        ) from err
    except BaseException:
        kill_process_tree(process)
        try:
            process.communicate(timeout=5)
        except (subprocess.SubprocessError, OSError):
            pass
        raise
    kill_process_tree(process)
    result = subprocess.CompletedProcess(
        command,
        process.returncode,
        stdout,
        stderr,
    )
    if check:
        result.check_returncode()
    return result


def _timeout_text(value: str | bytes | None) -> str:
    if value is None:
        return ""
    return value.decode(errors="replace") if isinstance(value, bytes) else value


class Deadline:
    """One monotonic wall-clock budget shared by a multi-step operation."""

    def __init__(self, total_seconds: float):
        if total_seconds <= 0:
            raise ValueError("deadline must be positive")
        self.total_seconds = float(total_seconds)
        self.started_at = time.monotonic()
        self.expires_at = self.started_at + self.total_seconds

    def remaining(self, operation: str) -> float:
        remaining = self.expires_at - time.monotonic()
        if remaining <= 0:
            raise TimeoutError(
                f"{operation} exceeded the {self.total_seconds:g}s total wall timeout"
            )
        return remaining

    def elapsed(self) -> float:
        return time.monotonic() - self.started_at


def materialize_standard_input(
    solc: str,
    source: pathlib.Path,
    timeout: float,
    evm_version: str,
    *,
    deadline: Deadline | None = None,
) -> dict[str, Any]:
    """Resolve imports once, then embed the complete source closure."""
    source = source.resolve()
    if deadline:
        deadline.remaining("root source materialization")
    discovery_input = {
        "language": "Solidity",
        "sources": {source.name: {"content": source.read_text(encoding="utf-8")}},
        "settings": {"outputSelection": {"*": {"": ["ast"]}}},
    }
    command = [solc, "--base-path", str(source.parent), "--standard-json"]
    with tempfile.TemporaryDirectory(prefix="solar-import-discovery-") as compile_cwd:
        result = run_process_group(
            command,
            input=json.dumps(discovery_input),
            timeout=_operation_timeout(timeout, deadline, "Solidity import discovery"),
            cwd=compile_cwd,
        )
    output = _compiler_output(result, "solc import discovery")
    discovered = output.get("sources")
    if not isinstance(discovered, dict) or source.name not in discovered:
        raise ValueError("solc import discovery did not return the root source unit")

    sources: dict[str, dict[str, str]] = {}
    provenance = []
    for name in sorted(discovered):
        if deadline:
            deadline.remaining(f"source unit `{name}` materialization")
        path = _source_unit_path(source, name)
        content = path.read_text(encoding="utf-8")
        if deadline:
            deadline.remaining(f"source unit `{name}` materialization")
        sources[name] = {"content": content}
        provenance.append(
            {
                "name": name,
                "original_path": str(path),
                "sha256": hashlib.sha256(content.encode()).hexdigest(),
            }
        )

    value = {
        "language": "Solidity",
        "sources": sources,
        "settings": _standard_settings(evm_version),
    }
    serialized = json.dumps(
        value, ensure_ascii=False, separators=(",", ":"), sort_keys=True
    )
    if deadline:
        deadline.remaining("Standard JSON serialization")
    return {
        "root_source": source.name,
        "json": serialized,
        "sha256": hashlib.sha256(serialized.encode()).hexdigest(),
        "settings": value["settings"],
        "sources": provenance,
    }


def compile_standard_artifact(
    compiler: str,
    source: pathlib.Path,
    contract: str,
    timeout: float,
    *,
    kind: str,
    evm_version: str,
    standard_input: dict[str, Any] | None = None,
    deadline: Deadline | None = None,
    compiler_cwd: pathlib.Path | None = None,
) -> dict[str, Any]:
    """Compile one source with settings shared by the symbolic differential lane."""
    if kind not in {"solc", "solar"}:
        raise ValueError(f"unsupported compiler kind `{kind}`")

    if standard_input is None:
        standard_input = _single_source_standard_input(source, evm_version)
    source_name = standard_input["root_source"]
    settings = standard_input["settings"]
    command = [compiler]
    if kind == "solar":
        command.append("-Zcodegen")
    command.append("--standard-json")
    cwd_context: Any
    if compiler_cwd is None:
        cwd_context = tempfile.TemporaryDirectory(prefix=f"solar-{kind}-compile-")
    else:
        cwd_context = contextlib.nullcontext(compiler_cwd)
    with cwd_context as compile_cwd:
        result = run_process_group(
            command,
            input=standard_input["json"],
            timeout=_operation_timeout(timeout, deadline, f"{kind} compilation"),
            cwd=compile_cwd,
        )
    output = _compiler_output(result, kind)

    contracts = output.get("contracts")
    if not isinstance(contracts, dict):
        raise ValueError(f"{kind} output has no contracts object")
    source_contracts = contracts.get(source_name)
    if not isinstance(source_contracts, dict):
        raise ValueError(f"{source_name} was not found in {kind} output")
    artifact = source_contracts.get(contract)
    if not isinstance(artifact, dict):
        raise ValueError(f"contract {source_name}:{contract} not found in {kind} output")
    evm_artifact = artifact.get("evm")
    deployed = (
        evm_artifact.get("deployedBytecode")
        if isinstance(evm_artifact, dict)
        else None
    )
    runtime = deployed.get("object") if isinstance(deployed, dict) else None
    if not isinstance(runtime, str) or not runtime:
        raise ValueError(f"contract {source_name}:{contract} has no runtime bytecode")
    runtime_payload = runtime.removeprefix("0x")
    if not runtime_payload:
        raise ValueError(f"contract {source_name}:{contract} has no runtime bytecode")
    if len(runtime_payload) % 2:
        raise ValueError(
            f"contract {source_name}:{contract} runtime bytecode is not byte-aligned"
        )
    try:
        runtime_bytes = bytes.fromhex(runtime_payload)
    except ValueError as err:
        raise ValueError(
            f"contract {source_name}:{contract} runtime bytecode is not hex"
        ) from err
    if runtime_bytes.hex() != runtime_payload.lower():
        raise ValueError(
            f"contract {source_name}:{contract} runtime bytecode is not hex"
        )
    abi = artifact.get("abi", [])
    identifiers = (
        evm_artifact.get("methodIdentifiers")
        if isinstance(evm_artifact, dict)
        else None
    )
    if not isinstance(abi, list) or not isinstance(identifiers, dict):
        raise ValueError(f"contract {source_name}:{contract} has malformed {kind} output")
    return {
        "abi": abi,
        "runtime": "0x" + runtime_payload,
        "method_identifiers": identifiers,
        "settings": settings,
        "version": compiler_version(compiler, timeout, deadline=deadline),
        "command": command,
        "standard_input": standard_input,
        "standard_input_sha256": standard_input["sha256"],
        "filesystem_import_fallback": False,
    }


def compiler_version(
    compiler: str,
    timeout: float,
    *,
    deadline: Deadline | None = None,
) -> str:
    result = run_process_group(
        [compiler, "--version"],
        check=True,
        timeout=_operation_timeout(timeout, deadline, f"{compiler} --version"),
    )
    return result.stdout.strip()


def _standard_settings(evm_version: str) -> dict[str, Any]:
    return {
        "optimizer": {"enabled": True, "runs": 200},
        "viaIR": True,
        "evmVersion": evm_version,
        "metadata": {"bytecodeHash": "none"},
        "outputSelection": {
            "*": {"*": ["abi", "evm.deployedBytecode.object", "evm.methodIdentifiers"]}
        },
    }


def _single_source_standard_input(
    source: pathlib.Path, evm_version: str
) -> dict[str, Any]:
    source = source.resolve()
    content = source.read_text(encoding="utf-8")
    value = {
        "language": "Solidity",
        "sources": {source.name: {"content": content}},
        "settings": _standard_settings(evm_version),
    }
    serialized = json.dumps(
        value, ensure_ascii=False, separators=(",", ":"), sort_keys=True
    )
    return {
        "root_source": source.name,
        "json": serialized,
        "sha256": hashlib.sha256(serialized.encode()).hexdigest(),
        "settings": value["settings"],
        "sources": [
            {
                "name": source.name,
                "original_path": str(source),
                "sha256": hashlib.sha256(content.encode()).hexdigest(),
            }
        ],
    }


def _source_unit_path(root: pathlib.Path, name: str) -> pathlib.Path:
    if name == root.name:
        return root
    unit = pathlib.Path(name)
    candidate = unit if unit.is_absolute() else root.parent / unit
    candidate = candidate.resolve()
    if not candidate.is_file():
        raise ValueError(
            f"could not snapshot imported source unit `{name}` at `{candidate}`"
        )
    return candidate


def _compiler_output(
    result: subprocess.CompletedProcess[str], label: str
) -> dict[str, Any]:
    if result.returncode != 0:
        raise RuntimeError(
            f"{label} exited with status {result.returncode}: {result.stderr.strip()}"
        )
    try:
        output = json.loads(result.stdout)
    except json.JSONDecodeError as err:
        raise RuntimeError(f"{label} emitted invalid JSON: {err}") from err
    if not isinstance(output, dict):
        raise RuntimeError(f"{label} JSON output must be an object")
    raw_errors = output.get("errors", [])
    if not isinstance(raw_errors, list) or not all(
        isinstance(error, dict) for error in raw_errors
    ):
        raise RuntimeError(f"{label} JSON contains malformed compiler diagnostics")
    errors = [error for error in raw_errors if error.get("severity") == "error"]
    if errors:
        messages = "\n".join(
            error.get("formattedMessage") or error.get("message") or str(error)
            for error in errors
        )
        raise RuntimeError(f"{label} compilation failed:\n{messages}")
    return output


def _operation_timeout(
    timeout: float, deadline: Deadline | None, operation: str
) -> float:
    return min(timeout, deadline.remaining(operation)) if deadline else timeout


def compile_solc(solc: str, source: pathlib.Path, contract: str, timeout: float) -> str:
    result = run_process_group(
        [
            solc,
            "--via-ir",
            "--optimize",
            "--metadata-hash",
            "none",
            "--combined-json",
            "bin-runtime",
            str(source),
        ],
        check=True,
        timeout=timeout,
    )
    return runtime_from_contracts(json.loads(result.stdout)["contracts"], contract)


def compile_solar(solar: str, source: pathlib.Path, contract: str, timeout: float) -> str:
    result = run_process_group(
        [solar, "-Zcodegen", "--emit=bin-runtime", "--pretty-json", str(source)],
        check=True,
        timeout=timeout,
    )
    return runtime_from_contracts(json.loads(result.stdout)["contracts"], contract)


def cast_calldata(cast: str, signature: str, args: list[str]) -> str:
    result = subprocess.run(
        [cast, "calldata", signature, *args],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return result.stdout.strip()


def runtime_from_contracts(contracts: dict[str, Any], contract: str) -> str:
    suffix = f":{contract}"
    for name, artifact in contracts.items():
        if name.endswith(suffix):
            runtime = artifact.get("bin-runtime")
            if not runtime:
                raise ValueError(f"contract {name} has no runtime bytecode")
            return "0x" + runtime.removeprefix("0x")
    raise ValueError(f"contract {contract} not found")


def rpc(
    url: str,
    method: str,
    params: list[Any],
    timeout: float,
    retries: int = 2,
    *,
    deadline: Deadline | None = None,
) -> dict[str, Any]:
    payload = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
    request = urllib.request.Request(
        url, data=payload.encode(), headers={"Content-Type": "application/json"}
    )
    for attempt in range(retries + 1):
        try:
            request_timeout = _operation_timeout(
                timeout, deadline, f"JSON-RPC {method}"
            )
            with urllib.request.urlopen(request, timeout=request_timeout) as response:
                try:
                    decoded = json.loads(response.read().decode())
                except (json.JSONDecodeError, UnicodeDecodeError) as err:
                    raise InfraError(
                        f"JSON-RPC {method} returned invalid JSON: {err}"
                    ) from err
                if not isinstance(decoded, dict):
                    raise InfraError(
                        f"JSON-RPC {method} response must be an object"
                    )
                return decoded
        except InfraError:
            raise
        except (urllib.error.URLError, TimeoutError) as err:
            if attempt >= retries:
                raise InfraError(f"JSON-RPC transport error for {method}: {err}") from err
            delay = 0.1 * (attempt + 1)
            if deadline:
                delay = min(delay, deadline.remaining(f"JSON-RPC {method} retry"))
            time.sleep(delay)

    raise InfraError(f"JSON-RPC transport error for {method}")


def set_code(
    url: str,
    address: str,
    runtime: str,
    timeout: float,
    *,
    deadline: Deadline | None = None,
) -> None:
    response = rpc(
        url, "anvil_setCode", [address, runtime], timeout, deadline=deadline
    )
    if "result" not in response or "error" in response:
        raise InfraError(
            f"anvil_setCode for {address} returned no result: {response!r}"
        )


def eth_call(
    url: str,
    address: str,
    calldata: str,
    timeout: float,
    envelope: dict[str, str] | None = None,
    *,
    deadline: Deadline | None = None,
) -> dict[str, Any]:
    tx = {"to": address, "data": calldata}
    if envelope is not None:
        tx.update(envelope)
    response = rpc(url, "eth_call", [tx, "latest"], timeout, deadline=deadline)
    if "result" in response and "error" in response:
        raise InfraError("eth_call returned both result and error")
    if "result" in response:
        return {
            "status": "ok",
            "data": _strict_rpc_hex(response["result"], "eth_call result"),
        }
    error = response.get("error")
    if _is_evm_execution_error(error):
        return {"status": "revert", "data": _strict_revert_data(error)}
    raise InfraError(f"eth_call failed outside EVM execution: {error!r}")


def send_tx(
    url: str, sender: str, address: str, calldata: str, timeout: float
) -> dict[str, Any]:
    response = rpc(
        url,
        "eth_sendTransaction",
        [{"from": sender, "to": address, "data": calldata, "gas": TX_GAS}],
        timeout,
    )
    if "result" not in response:
        # Submission rejected outright (should not happen with an explicit gas
        # limit, but record the reason for differential comparison anyway).
        return {"status": "rejected", "data": revert_data(response.get("error"))}

    receipt = wait_for_receipt(url, response["result"], timeout)
    if not receipt:
        return {"status": "no-receipt"}
    return {
        "status": "ok" if receipt.get("status") == "0x1" else "revert",
        "logs": [normalize_log(log) for log in receipt.get("logs", [])],
        # Storage-trie root: a digest of all persisted storage. Solidity assigns
        # the same slots for the same source, so this catches storage divergence
        # even when no later `eth_call` reads the written slot back.
        "storage": storage_root(url, address, timeout),
    }


def storage_root(url: str, address: str, timeout: float) -> str:
    response = rpc(url, "eth_getProof", [address, [], "latest"], timeout)
    if "error" in response:
        raise RuntimeError(f"eth_getProof for {address} failed: {response['error']}")
    proof = response.get("result")
    if isinstance(proof, dict) and isinstance(proof.get("storageHash"), str):
        return proof["storageHash"].lower()
    raise RuntimeError(f"eth_getProof for {address} did not return storageHash")


def wait_for_receipt(url: str, tx_hash: str, timeout: float) -> dict[str, Any] | None:
    """Polls for a transaction receipt.

    anvil returns the hash from `eth_sendTransaction` before the receipt is
    queryable, so a single lookup races the miner.
    """
    deadline = time.monotonic() + timeout
    while True:
        receipt = rpc(url, "eth_getTransactionReceipt", [tx_hash], timeout).get("result")
        if receipt:
            return receipt
        if time.monotonic() >= deadline:
            return None
        time.sleep(0.05)


def revert_data(error: Any) -> str:
    """Extracts raw revert-data hex from a JSON-RPC error.

    The human-readable `message` is the node's decode, not contract output.
    """
    if not isinstance(error, dict):
        return "0x"
    data = error.get("data")
    if isinstance(data, dict):
        data = data.get("data") or data.get("result")
    return normalize_hex(data)


def _is_evm_execution_error(error: Any) -> bool:
    if not isinstance(error, dict):
        return False
    code = error.get("code")
    message = error.get("message")
    if code == 3:
        return True
    if code != -32003 or not isinstance(message, str):
        return False
    lowered = message.lower()
    if lowered.startswith("evm error "):
        return True
    return any(
        marker in lowered
        for marker in ("execution reverted", "out of gas", "invalid opcode")
    )


def _strict_revert_data(error: dict[str, Any]) -> str:
    data = error.get("data")
    if isinstance(data, dict):
        data = data.get("data") or data.get("result")
    if data is None:
        return "0x"
    return _strict_rpc_hex(data, "eth_call revert data")


def _strict_rpc_hex(value: Any, label: str) -> str:
    if (
        not isinstance(value, str)
        or not value.startswith("0x")
        or len(value) % 2
    ):
        raise InfraError(f"{label} must be 0x-prefixed byte-aligned hex")
    try:
        int(value[2:] or "0", 16)
    except ValueError as err:
        raise InfraError(f"{label} is not hex") from err
    return value.lower()


def normalize_log(log: dict[str, Any]) -> dict[str, Any]:
    """Compares logs by topics + data only.

    The emitting address is excluded because the runtimes run at different
    addresses.
    """
    return {
        "topics": [normalize_hex(topic) for topic in log.get("topics", [])],
        "data": normalize_hex(log.get("data")),
    }


def normalize_hex(value: Any) -> str:
    if isinstance(value, str) and value.startswith("0x"):
        return value.lower()
    return "0x"
