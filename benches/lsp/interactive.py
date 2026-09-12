#!/usr/bin/env python3
"""Measure real-project LSP editing and requests over the production stdio protocol."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import platform
import select
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from decimal import Decimal
from pathlib import Path
from typing import Any

from benchmark import _paired_bootstrap_interval

ROOT = Path(__file__).resolve().parents[2]
PROJECT = ROOT / "tests/foundry/unifap-v2"
PROJECT_SHA256 = "e445607a180748a10aaac946e4ecd925d915a9b4e63d783e33e12648c7a28bb0"
ROUTER = "src/UnifapV2Router.sol"
PAIR = "src/UnifapV2Pair.sol"
FACTORY = "src/UnifapV2Factory.sol"
METHODS = ("completion", "hover", "definition", "references", "documentSymbol", "signatureHelp", "foldingRange", "selectionRange", "importCompletion")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def comparable_initialization(initialization: dict[str, Any]) -> dict[str, Any]:
    """Drop only the build-specific server version from initialization parity."""
    result = dict(initialization["result"])
    if "serverInfo" in result:
        result["serverInfo"] = {key: value for key, value in result["serverInfo"].items() if key != "version"}
    return {"params": initialization["params"], "result": result}


def digest(value: Any) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()).hexdigest()


def project_files() -> list[Path]:
    return sorted(p for p in PROJECT.rglob("*") if p.is_file() and (p.suffix == ".sol" or p.name in ("foundry.toml", "remappings.txt")))


def project_identity() -> dict[str, Any]:
    aggregate = hashlib.sha256(b"solar-lsp-real-project-v1\0")
    files = {}
    for path in project_files():
        name, data = path.relative_to(PROJECT).as_posix(), path.read_bytes()
        aggregate.update(name.encode() + b"\0" + data + b"\0")
        files[name] = hashlib.sha256(data).hexdigest()
    require(aggregate.hexdigest() == PROJECT_SHA256, "real project changed: review anchors and update PROJECT_SHA256 deliberately")
    return {"path": str(PROJECT.relative_to(ROOT)), "sha256": aggregate.hexdigest(), "files": files}


def position(text: str, offset: int) -> dict[str, int]:
    before = text[:offset]
    return {"line": before.count("\n"), "character": len(before.rsplit("\n", 1)[-1].encode("utf-16-le")) // 2}


def anchor(text: str, needle: str, delta: int = 0) -> dict[str, int]:
    require(text.count(needle) == 1, f"anchor is missing or ambiguous: {needle!r}")
    return position(text, text.index(needle) + delta)


def normalize(value: Any, root: Path) -> Any:
    if isinstance(value, dict):
        return {key: normalize(item, root) for key, item in value.items()}
    if isinstance(value, list):
        return [normalize(item, root) for item in value]
    if isinstance(value, str):
        return value.replace(root.as_uri(), "file:///PROJECT")
    return value


class Rpc:
    """One sequential client; framing, parsing and transport remain in measured latency."""

    def __init__(self, binary: Path, project: Path, runtime: Path, stderr: Path, timeout: float):
        env = {"PATH": os.defpath, "LANG": "C.UTF-8", "LC_ALL": "C.UTF-8", "TZ": "UTC", "NO_COLOR": "1", "RUST_BACKTRACE": "0"}
        for key, name in (("HOME", "home"), ("TMPDIR", "tmp"), ("XDG_CACHE_HOME", "cache"), ("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data")):
            directory = runtime / name
            directory.mkdir(parents=True)
            env[key] = str(directory)
        self.log = stderr.open("wb")
        self.process = subprocess.Popen([str(binary), "lsp"], cwd=project, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log, bufsize=0)
        self.buffer = bytearray()
        self.next_id = 0
        self.timeout = timeout
        self.notifications = 0
        self.server_requests = 0

    def send(self, message: dict[str, Any]) -> None:
        body = json.dumps(message, separators=(",", ":")).encode()
        frame = b"Content-Length: " + str(len(body)).encode() + b"\r\n\r\n" + body
        view = memoryview(frame)
        while view:
            written = os.write(self.process.stdin.fileno(), view)
            view = view[written:]

    def notify(self, method: str, params: Any) -> None:
        self.send({"jsonrpc": "2.0", "method": method, "params": params})

    def read(self, deadline: float) -> dict[str, Any]:
        while True:
            head_end = self.buffer.find(b"\r\n\r\n")
            if head_end >= 0:
                fields = dict(line.split(b":", 1) for line in bytes(self.buffer[:head_end]).split(b"\r\n"))
                length = int(next(value for key, value in fields.items() if key.lower() == b"content-length"))
                require(length <= 32 * 1024 * 1024, "unexpectedly large LSP response")
                end = head_end + 4 + length
                if len(self.buffer) >= end:
                    result = json.loads(self.buffer[head_end + 4:end])
                    del self.buffer[:end]
                    return result
            remaining = deadline - time.monotonic()
            require(remaining > 0, "LSP request timed out")
            ready, _, _ = select.select([self.process.stdout], [], [], remaining)
            require(bool(ready), "LSP request timed out")
            chunk = os.read(self.process.stdout.fileno(), 65536)
            require(bool(chunk), "LSP server exited before responding; inspect stderr log")
            self.buffer.extend(chunk)

    def request(self, method: str, params: Any) -> tuple[Any, float]:
        self.next_id += 1
        request_id = self.next_id
        start = time.perf_counter_ns()
        self.send({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params})
        deadline = time.monotonic() + self.timeout
        while True:
            message = self.read(deadline)
            if "method" in message:
                if "id" in message:
                    self.server_requests += 1
                    # Refresh and progress acknowledgements; configuration is not advertised.
                    self.send({"jsonrpc": "2.0", "id": message["id"], "result": None})
                else:
                    self.notifications += 1
            else:
                require(message.get("id") == request_id, "unexpected JSON-RPC response ID")
                require("error" not in message, f"{method}: {message.get('error')}")
                return message["result"], (time.perf_counter_ns() - start) / 1e6

    def close(self, graceful: bool) -> None:
        try:
            if graceful:
                self.request("shutdown", None)
                self.notify("exit", None)
                self.process.wait(timeout=5)
                require(self.process.returncode == 0, "server shutdown failed")
        finally:
            if self.process.poll() is None:
                self.process.kill()
                self.process.wait()
            self.process.stdin.close()
            self.process.stdout.close()
            self.log.close()


class Scenario:
    def __init__(self, project: Path):
        self.project = project
        self.texts = {name: (project / name).read_text() for name in (ROUTER, PAIR, FACTORY)}
        self.version = 1
        self.responses: dict[str, Any] = {}
        self.samples: dict[str, list[float]] = {}
        self.records: list[dict[str, Any]] = []
        self.initialization: Any = None
        self.raw_diagnostic_responses: list[Any] = []
        self.diagnostic_ids: dict[tuple[str, str], str] = {}
        router = self.texts[ROUTER]
        self.requests = {
            "completion": self.at(ROUTER, "factory.createPair(tokenA, tokenB)", len("factory.")),
            "hover": self.at(PAIR, "SELECTOR"),
            "definition": self.at(ROUTER, "sortPairs"),
            "references": {**self.at(FACTORY, "getAllPairLength"), "context": {"includeDeclaration": True}},
            "documentSymbol": {"textDocument": {"uri": self.uri(ROUTER)}},
            "signatureHelp": self.at(ROUTER, "_safeTransferFrom(tokenB, msg.sender, pair, amountB)", len("_safeTransferFrom(tokenB, ")),
            "importCompletion": self.at(ROUTER, "./interfaces/IUnifapV2Factory.sol", len("./interfaces/")),
            "foldingRange": {"textDocument": {"uri": self.uri(ROUTER)}},
            "selectionRange": {"textDocument": {"uri": self.uri(ROUTER)}, "positions": [anchor(router, "block.timestamp > deadline", len("block.timestamp > "))]},
        }
        import_start = router.index('./interfaces/') + len('./interfaces/')
        import_end = router.index('";', import_start)
        self.import_range = {"start": position(router, import_start), "end": position(router, import_end)}
        self.import_prefix = position(router, import_start)
        self.edit_range = {"start": anchor(router, "if (block.timestamp > deadline)", len("if (block.timestamp > ")), "end": anchor(router, "if (block.timestamp > deadline)", len("if (block.timestamp > deadline"))}

    def uri(self, name: str) -> str:
        return (self.project / name).as_uri()

    def at(self, name: str, needle: str, delta: int = 0) -> dict[str, Any]:
        return {"textDocument": {"uri": self.uri(name)}, "position": anchor(self.texts[name], needle, delta)}

    def record(self, metric: str, method: str, params: Any, response: Any, elapsed: float, measured: bool) -> None:
        require(math.isfinite(elapsed) and elapsed > 0, f"invalid latency for {metric}")
        normalized = normalize(response, self.project)
        response_hash = digest(normalized)
        self.responses[response_hash] = normalized
        if measured:
            self.samples.setdefault(metric, []).append(elapsed)
            self.records.append({"metric": metric, "method": method, "params": normalize(params, self.project), "response_sha256": response_hash})

    def diagnostics(self, rpc: Rpc, metric: str, invalid: bool, measured: bool, start: int | None = None) -> None:
        params = {"previousResultIds": []}
        response, elapsed = rpc.request("workspace/diagnostic", params)
        if start is not None:
            elapsed = (time.perf_counter_ns() - start) / 1e6
        self.raw_diagnostic_responses.append(normalize(response, self.project))
        reports = response["items"]
        router = [item for item in reports if item["uri"] == self.uri(ROUTER)]
        require(len(router) == 1 and router[0]["version"] == self.version, f"diagnostics are stale for version {self.version}: {router}")
        errors = [diagnostic for report in reports for diagnostic in report["items"] if diagnostic.get("severity") == 1]
        if invalid:
            require(len(errors) == 1 and errors[0]["message"] == "unresolved symbol `deadlinx`", f"expected undeclared deadlinx diagnostic: {errors}")
            require(errors[0]["range"] == self.edit_range, f"wrong diagnostic range: {errors[0]}")
        else:
            require(all(not report["items"] for report in reports), f"valid project has unexpected diagnostics: {reports}")
        require(len(reports) == 25, f"incomplete real project indexing: {len(reports)} reports")
        # Result IDs are opaque server tokens assigned in hash-map traversal order.
        # Preserve their equality contract by replacing them with content hashes;
        # every diagnostic, URI, version and array order is still compared exactly.
        for report in reports:
            token = report.get("resultId")
            require(isinstance(token, str) and bool(token), "missing diagnostic result ID")
            semantic_hash = digest(normalize(report["items"], self.project))
            key = (report["uri"], token)
            require(self.diagnostic_ids.get(key, semantic_hash) == semantic_hash, "diagnostic result ID reused for changed content")
            self.diagnostic_ids[key] = semantic_hash
            report["resultId"] = "content:" + semantic_hash
        self.record(metric, "workspace/diagnostic", params, response, elapsed, measured)

    def query(self, rpc: Rpc, name: str, metric: str, measured: bool) -> None:
        params = self.requests[name]
        method = "textDocument/completion" if name == "importCompletion" else "textDocument/" + name
        result, elapsed = rpc.request(method, params)
        if name == "importCompletion":
            self.check_import_candidates(result, "")
        elif name == "completion":
            items = result if isinstance(result, list) else result["items"]
            require({"createPair", "pairs"} <= {item["label"] for item in items}, "completion missed factory members")
        elif name == "hover":
            require(result and "SELECTOR" in str(result["contents"]), "hover missed SELECTOR declaration")
        elif name == "definition":
            require(len(result) == 1 and result[0]["uri"].endswith("/src/libraries/UnifapV2Library.sol"), "definition did not navigate to imported sortPairs")
        elif name == "references":
            require(len(result) == 4, f"getAllPairLength should have 4 references: {result}")
            require(sum(item["uri"].endswith("/src/test/UnifapV2Factory.t.sol") for item in result) == 3, "references missed project test callers")
        elif name == "documentSymbol":
            def names(items: list[Any]) -> set[str]:
                return {item["name"] for item in items} | set().union(*(names(item.get("children", [])) for item in items))
            require({"UnifapV2Router", "addLiquidity", "removeLiquidity", "_safeTransferFrom"} <= names(result), "document symbol outline is incomplete")
        elif name == "signatureHelp":
            require(result and result["activeParameter"] == 1 and len(result["signatures"]) == 1 and result["signatures"][0]["label"].startswith("function _safeTransferFrom("), f"incorrect signature help: {result}")
        elif name == "foldingRange":
            require(isinstance(result, list) and len(result) == 16, "folding ranges are incomplete")
            require(any(item["startLine"] == 8 and item["endLine"] == 158 for item in result), "folding ranges missed the router contract")
        elif name == "selectionRange":
            require(isinstance(result, list) and len(result) == 1 and result[0]["range"] == self.edit_range, "selection range missed the edited identifier")
            require(result[0]["parent"]["range"] == {"start": {"line": 28, "character": 12}, "end": {"line": 28, "character": 38}}, "selection range missed the enclosing comparison")
        self.record(metric, method, params, result, elapsed, measured)

    @staticmethod
    def check_import_candidates(result: Any, prefix: str) -> None:
        items = result if isinstance(result, list) else result["items"]
        expected = {"./interfaces/" + name for name in ("IERC20.sol", "IUnifapV2Factory.sol", "IUnifapV2Pair.sol") if name.startswith(prefix)}
        labels = {item["label"] for item in items}
        require(labels == expected, f"wrong import candidates for {prefix!r}: {labels}")

    def import_completion(self, rpc: Rpc, measured: bool) -> None:
        previous = "IUnifapV2Factory.sol"
        for typed in ("I", "IU", "IUn", "IUnifapV2Factory.sol"):
            self.version += 1
            end = {**self.import_prefix, "character": self.import_prefix["character"] + len(previous)}
            change = {"textDocument": {"uri": self.uri(ROUTER), "version": self.version}, "contentChanges": [{"range": {"start": self.import_prefix, "end": end}, "rangeLength": len(previous), "text": typed}]}
            start = time.perf_counter_ns()
            rpc.notify("textDocument/didChange", change)
            if typed == "IUnifapV2Factory.sol":
                self.diagnostics(rpc, "import-restore-diagnostics", False, measured, start)
            else:
                params = {"textDocument": {"uri": self.uri(ROUTER)}, "position": {**self.import_prefix, "character": self.import_prefix["character"] + len(typed)}}
                result, elapsed = rpc.request("textDocument/completion", params)
                self.check_import_candidates(result, typed)
                self.record("importCompletion-" + typed, "textDocument/completion", params, result, elapsed, measured)
            if measured:
                self.records[-1]["preceding_notification"] = {"method": "textDocument/didChange", "params": normalize(change, self.project)}
            previous = typed

    def edit(self, rpc: Rpc, invalid: bool, measured: bool) -> None:
        self.version += 1
        change = {"textDocument": {"uri": self.uri(ROUTER), "version": self.version}, "contentChanges": [{"range": self.edit_range, "rangeLength": 8, "text": "deadlinx" if invalid else "deadline"}]}
        start = time.perf_counter_ns()
        rpc.notify("textDocument/didChange", change)
        self.diagnostics(rpc, "edit-diagnostics" if invalid else "undo-diagnostics", invalid, measured, start)
        # Include edit provenance without adding another timing sample.
        if measured:
            self.records[-1]["preceding_notification"] = {"method": "textDocument/didChange", "params": normalize(change, self.project)}
        suffix = "-after-edit" if invalid else "-after-undo"
        self.query(rpc, "completion", "completion" + suffix, measured)
        self.query(rpc, "foldingRange", "foldingRange" + suffix, measured)
        self.query(rpc, "selectionRange", "selectionRange" + suffix, measured)


def percentile(values: list[float], fraction: float) -> float:
    require(bool(values), "empty latency sample")
    return sorted(values)[math.ceil(len(values) * fraction) - 1]


def summary(samples: dict[str, list[float]]) -> dict[str, Any]:
    return {name: {"count": len(values), "p50_ms": percentile(values, .5), "p95_ms": percentile(values, .95)} for name, values in samples.items()}


def run_session(binary: Path, output: Path, session: int, role: str, order: str, args: argparse.Namespace) -> dict[str, Any]:
    session_dir = output / f"session-{session:02}-{role}"
    session_dir.mkdir()
    with tempfile.TemporaryDirectory(prefix="solar-lsp-interactive-") as temporary:
        runtime = Path(temporary).resolve()
        project = runtime / "project"
        for path in project_files():
            destination = project / path.relative_to(PROJECT)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, destination)
        scenario = Scenario(project)
        rpc = Rpc(binary, project, runtime / "runtime", session_dir / "stderr.log", args.timeout)
        print(f"session {session} {order} {role}: pid {rpc.process.pid}", file=sys.stderr, flush=True)
        success = False
        try:
            initialize = {"processId": None, "rootUri": project.as_uri(), "workspaceFolders": [{"uri": project.as_uri(), "name": "unifap-v2"}], "capabilities": {"textDocument": {"diagnostic": {"relatedDocumentSupport": True}, "completion": {"completionItem": {"snippetSupport": True}}}, "workspace": {"diagnostics": {"refreshSupport": True}}}}
            result, elapsed = rpc.request("initialize", initialize)
            require("capabilities" in result, "initialize did not negotiate capabilities")
            scenario.initialization = {"params": normalize(initialize, project), "result": normalize(result, project), "ms": elapsed}
            rpc.notify("initialized", {})
            start = time.perf_counter_ns()
            for name, text in scenario.texts.items():
                rpc.notify("textDocument/didOpen", {"textDocument": {"uri": scenario.uri(name), "languageId": "solidity", "version": 1, "text": text}})
            scenario.diagnostics(rpc, "open-diagnostics", False, True, start)
            for _ in range(args.warmups):
                for name in METHODS:
                    scenario.query(rpc, name, name, False)
            if args.profile:
                for _ in range(args.iterations):
                    scenario.query(rpc, args.profile, args.profile, True)
            else:
                for iteration in range(args.iterations):
                    scenario.import_completion(rpc, True)
                    scenario.edit(rpc, True, True)
                    scenario.edit(rpc, False, True)
                    for name in METHODS:
                        scenario.query(rpc, name, name, True)
            for metric, values in scenario.samples.items():
                require(len(values) == (1 if metric == "open-diagnostics" else args.iterations), f"wrong sample count for {metric}")
            success = True
        finally:
            rpc.close(success)
        result = {"session": session, "role": role, "order": order, "samples_ms": scenario.samples, "summary": summary(scenario.samples), "records": scenario.records, "responses": scenario.responses, "initialization": scenario.initialization, "raw_diagnostic_responses": scenario.raw_diagnostic_responses}
        result["scenario_sha256"] = digest([{key: value for key, value in record.items() if key != "response_sha256"} for record in scenario.records])
        (session_dir / "results.json").write_text(json.dumps(result, indent=2, allow_nan=False) + "\n")
        return result


def compare(roles: dict[str, Any]) -> dict[str, Any]:
    baseline, candidate = roles["base"]["sessions"], roles["candidate"]["sessions"]
    require(len(baseline) == len(candidate), "missing paired session")
    for base, head in zip(baseline, candidate):
        require(comparable_initialization(base["initialization"]) == comparable_initialization(head["initialization"]), f"initialize parity failed in session {base['session']}; only serverInfo.version may differ")
        require(base["records"] == head["records"], f"request/response parity failed in session {base['session']}; inspect saved results")
    result = {}
    for method in baseline[0]["summary"]:
        metrics = {}
        for key in ("p50_ms", "p95_ms"):
            a = [s["summary"][method][key] for s in baseline]
            b = [s["summary"][method][key] for s in candidate]
            mean_a, mean_b = statistics.mean(a), statistics.mean(b)
            metrics[key] = {"base_ms": mean_a, "candidate_ms": mean_b, "delta_ms": mean_b - mean_a, "reduction_percent": (1 - mean_b / mean_a) * 100, "paired_reduction_percent": [(1 - y / x) * 100 for x, y in zip(a, b)]}
        strata = {}
        for order in ("base-candidate", "candidate-base"):
            pairs = [(a, b) for a, b in zip(baseline, candidate) if a["order"] == order]
            strata[order] = {"pairs": len(pairs)}
            if len(pairs) < 2:
                continue
            require(len(pairs) <= 10, "exact bootstrap supports at most ten sessions per order")
            for key in ("p50_ms", "p95_ms"):
                interval = _paired_bootstrap_interval(
                    tuple(Decimal(str(a["summary"][method][key])) for a, _ in pairs),
                    tuple(Decimal(str(b["summary"][method][key])) for _, b in pairs),
                )
                strata[order][key] = {
                    "delta_ms_95_ci": [float(x) for x in interval["delta_ms"]],
                    "reduction_percent_95_ci": [-float(x) for x in reversed(interval["delta_percent"])],
                }
        metrics["order_strata"] = strata
        metrics["over_20_percent_in_both_orders"] = all(
            stratum["pairs"] >= 5
            and all(stratum[key]["reduction_percent_95_ci"][0] > 20 for key in ("p50_ms", "p95_ms"))
            for stratum in strata.values()
        )
        result[method] = metrics
    return {"exact_response_parity": True, "methods": result}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--base", type=Path)
    parser.add_argument("--candidate", type=Path)
    parser.add_argument("--sessions", type=int, default=10, help="independent pairs (or processes with --binary)")
    parser.add_argument("--iterations", type=int, default=20)
    parser.add_argument("--warmups", type=int, default=3)
    parser.add_argument("--timeout", type=float, default=30)
    parser.add_argument("--profile", choices=METHODS, help="repeat one warm request, skipping edit cycles; not acceptance results")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    require(bool(args.binary) != bool(args.base or args.candidate), "use --binary or both --base and --candidate")
    require(bool(args.base) == bool(args.candidate), "paired mode needs both binaries")
    require(args.profile is None or args.binary is not None, "--profile is only supported with --binary; paired acceptance cannot use profiling")
    require(args.sessions > 0 and args.iterations > 0 and args.warmups >= 0, "invalid sample counts")
    output = args.output.resolve()
    require(not output.exists() or not any(output.iterdir()), "output directory must be fresh")
    output.mkdir(parents=True, exist_ok=True)
    commands = {"binary": args.binary.resolve()} if args.binary else {"base": args.base.resolve(), "candidate": args.candidate.resolve()}
    manifest = {"schema_version": 1, "project": project_identity(), "harness_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), "host": {"platform": platform.platform(), "machine": platform.machine(), "python": sys.version}, "configuration": {"iterations": args.iterations, "warmups": args.warmups, "sessions": args.sessions, "profile": args.profile, "server_args": ["lsp"], "latency_unit": "milliseconds", "percentile": "nearest-rank within each independent session"}, "roles": {role: {"binary": str(binary), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "sessions": []} for role, binary in commands.items()}}
    for session in range(1, args.sessions + 1):
        order = list(commands)
        if session % 2 == 0:
            order.reverse()
        order_name = "-".join(order)
        for role in order:
            result = run_session(commands[role], output, session, role, order_name, args)
            manifest["roles"][role]["sessions"].append(result)
            (output / "results.json").write_text(json.dumps(manifest, indent=2, allow_nan=False) + "\n")
    if not args.binary:
        manifest["comparison"] = compare(manifest["roles"])
    (output / "results.json").write_text(json.dumps(manifest, indent=2, allow_nan=False) + "\n")
    for role, result in manifest["roles"].items():
        print(role)
        for method in result["sessions"][0]["summary"]:
            p50 = statistics.mean(s["summary"][method]["p50_ms"] for s in result["sessions"])
            p95 = statistics.mean(s["summary"][method]["p95_ms"] for s in result["sessions"])
            print(f"  {method}: P50 {p50:.6f} ms; P95 {p95:.6f} ms")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
