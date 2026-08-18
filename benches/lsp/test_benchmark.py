#!/usr/bin/env python3

from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any
from unittest import mock


MODULE_PATH = Path(__file__).with_name("benchmark.py")
MODULE_SPEC = importlib.util.spec_from_file_location("solar_lsp_benchmark", MODULE_PATH)
if MODULE_SPEC is None or MODULE_SPEC.loader is None:
    raise RuntimeError(f"could not load {MODULE_PATH}")
benchmark = importlib.util.module_from_spec(MODULE_SPEC)
sys.modules[MODULE_SPEC.name] = benchmark
MODULE_SPEC.loader.exec_module(benchmark)

FILTER_MODULE_PATH = Path(__file__).with_name("lsp_filter.py")
FILTER_MODULE_SPEC = importlib.util.spec_from_file_location(
    "solar_lsp_filter", FILTER_MODULE_PATH
)
if FILTER_MODULE_SPEC is None or FILTER_MODULE_SPEC.loader is None:
    raise RuntimeError(f"could not load {FILTER_MODULE_PATH}")
lsp_filter = importlib.util.module_from_spec(FILTER_MODULE_SPEC)
sys.modules[FILTER_MODULE_SPEC.name] = lsp_filter
FILTER_MODULE_SPEC.loader.exec_module(lsp_filter)


CONTEXT = benchmark.Context(
    repository="paradigmxyz/solar",
    head_repository="0xKarl98/solar",
    workflow_repository="0xKarl98/solar",
    pr_number=1195,
    base_sha="1" * 40,
    head_sha="2" * 40,
    run_url="https://github.com/0xKarl98/solar/actions/runs/12345",
)
RESPONSE_CONFIG = {"project": str(Path("/fixture").resolve())}


def fixture_uri(file_name: str, config: dict[str, Any] = RESPONSE_CONFIG) -> str:
    return (Path(config["project"]) / file_name).as_uri()


def lsp_range(line: int, character: int = 0) -> dict[str, Any]:
    return {
        "start": {"line": line, "character": character},
        "end": {"line": line, "character": character + 1},
    }


def location(
    file_name: str,
    line: int,
    config: dict[str, Any] = RESPONSE_CONFIG,
    *,
    character: int = 0,
) -> dict[str, Any]:
    return {
        "uri": fixture_uri(file_name, config),
        "range": lsp_range(line, character),
    }


def valid_response(method: str, config: dict[str, Any] = RESPONSE_CONFIG) -> Any:
    if method == "initialize":
        return "ok"
    if method == "textDocument/diagnostic":
        return {
            "uri": fixture_uri("Main.sol", config),
            "diagnostics": [
                {
                    "range": {
                        "start": {"line": 16, "character": 4},
                        "end": {"line": 18, "character": 5},
                    },
                    "severity": 2,
                    "code": "2018",
                    "message": "function state mutability can be restricted to view",
                }
            ],
        }
    if method == "textDocument/hover":
        return {
            "contents": {
                "kind": "markdown",
                "value": "function double(uint256 value) returns (uint256)",
            }
        }
    if method == "textDocument/definition":
        return [location("Math.sol", 4, config)]
    if method == "textDocument/references":
        return [
            location("Main.sol", 8, config, character=13),
            location("Main.sol", 13, config, character=15),
        ]
    if method == "textDocument/completion":
        return {"isIncomplete": False, "items": [{"label": "value"}]}
    if method == "textDocument/documentSymbol":
        return [
            {"name": name, "kind": kind, "location": location("Main.sol", line, config)}
            for name, kind, line in (
                ("Main", 5, 5),
                ("value", 8, 6),
                ("double", 12, 8),
                ("compute", 12, 12),
                ("completions", 12, 16),
            )
        ]
    raise AssertionError(f"no test response for {method}")


def write_json(path: Path, value: Any) -> bytes:
    data = (json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n").encode()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return data


class RawArtifact:
    def __init__(self, root: Path) -> None:
        self.root = root
        self.configs: dict[str, dict[str, Any]] = {}
        self.results: dict[str, dict[str, Any]] = {}
        self.manifest = {
            "schema_version": benchmark.RAW_SCHEMA_VERSION,
            "kind": benchmark.RAW_KIND,
            "context": {
                "repository": CONTEXT.repository,
                "head_repository": CONTEXT.head_repository,
                "workflow_repository": CONTEXT.workflow_repository,
                "pr_number": CONTEXT.pr_number,
                "base_sha": CONTEXT.base_sha,
                "head_sha": CONTEXT.head_sha,
                "run_url": CONTEXT.run_url,
            },
            "protocol": {
                "warmup_iterations": benchmark.WARMUP_ITERATIONS,
                "measured_iterations_per_pass": benchmark.MEASURED_ITERATIONS,
                "passes": [name for name, _ in benchmark.PASSES],
                "methods": list(benchmark.METHODS),
                "threshold_percent": benchmark.THRESHOLD_PERCENT,
            },
            "upstream": benchmark.pinned_upstream(),
            "fixture": {"sha256": benchmark.fixture_sha256()},
            "binaries": {
                "base": {"sha256": "a" * 64},
                "head": {"sha256": "b" * 64},
            },
            "passes": [],
        }

        commands = {
            "base": root / "binaries" / "base-solar",
            "head": root / "binaries" / "head-solar",
        }
        for pass_index, (pass_name, server_order) in enumerate(benchmark.PASSES):
            config = benchmark.generated_config(
                root / "runtime" / pass_name / "project",
                root / "runtime" / pass_name / "output",
                commands,
                server_order,
            )
            results = self._results(config, server_order, pass_index)
            self.configs[pass_name] = config
            self.results[pass_name] = results
            self.manifest["passes"].append(
                {
                    "name": pass_name,
                    "server_order": list(server_order),
                    "config": {
                        "path": f"passes/{pass_name}/config.json",
                        "sha256": "",
                    },
                    "results": {
                        "path": f"passes/{pass_name}/results.json",
                        "sha256": "",
                    },
                }
            )
            self.rewrite_config(pass_name, rewrite_manifest=False)
            self.rewrite_results(pass_name, rewrite_manifest=False)
        self.rewrite_manifest()

    @staticmethod
    def _results(
        config: dict[str, Any], server_order: tuple[str, ...], pass_index: int
    ) -> dict[str, Any]:
        benchmarks = []
        for method_index, method in enumerate(benchmark.METHODS):
            rows = []
            for role_index, role in enumerate(server_order):
                start = 10.0 + pass_index * 2 + role_index + method_index / 10
                iterations = [
                    {
                        "ms": start + iteration / 100,
                        "response": copy.deepcopy(valid_response(method, config)),
                    }
                    for iteration in range(benchmark.MEASURED_ITERATIONS)
                ]
                rows.append(
                    {
                        "server": role,
                        "status": "ok",
                        "p50_ms": start + 0.04,
                        "p95_ms": start + 0.09,
                        "mean_ms": start + 0.045,
                        "rss_kb": 1024 + role_index,
                        "response": copy.deepcopy(valid_response(method, config)),
                        "iterations": iterations,
                    }
                )
            benchmark_result = {"name": method, "servers": rows}
            benchmark_input = benchmark._expected_benchmark_input(method, config)
            if benchmark_input is not None:
                benchmark_result["input"] = json.dumps(benchmark_input, separators=(",", ":"))
            benchmarks.append(benchmark_result)
        return {
            "timestamp": "2026-08-17T13:00:00Z",
            "date": "2026-08-17",
            "settings": {
                "iterations": benchmark.MEASURED_ITERATIONS,
                "warmup": benchmark.WARMUP_ITERATIONS,
                "timeout_secs": benchmark.REQUEST_TIMEOUT_SECONDS,
                "index_timeout_secs": benchmark.INDEX_TIMEOUT_SECONDS,
                "project": config["project"],
                "file": "Main.sol",
                "line": 13,
                "col": 17,
                "methods": benchmark.RESULT_METHOD_CONFIG,
            },
            "servers": [
                {"name": role, "version": f"solar-{role}"} for role in server_order
            ],
            "benchmarks": benchmarks,
        }

    def pass_entry(self, pass_name: str) -> dict[str, Any]:
        return next(entry for entry in self.manifest["passes"] if entry["name"] == pass_name)

    def rewrite_config(self, pass_name: str, *, rewrite_manifest: bool = True) -> None:
        path = self.root / "passes" / pass_name / "config.json"
        data = write_json(path, self.configs[pass_name])
        self.pass_entry(pass_name)["config"]["sha256"] = hashlib.sha256(data).hexdigest()
        if rewrite_manifest:
            self.rewrite_manifest()

    def rewrite_results(self, pass_name: str, *, rewrite_manifest: bool = True) -> None:
        path = self.root / "passes" / pass_name / "results.json"
        data = write_json(path, self.results[pass_name])
        self.pass_entry(pass_name)["results"]["sha256"] = hashlib.sha256(data).hexdigest()
        if rewrite_manifest:
            self.rewrite_manifest()

    def rewrite_manifest(self) -> None:
        write_json(self.root / "manifest.json", self.manifest)


class ConfigTests(unittest.TestCase):
    def test_generated_config_pins_protocol_and_server_order(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = benchmark.generated_config(
                root / "project",
                root / "output",
                {"base": root / "base", "head": root / "head"},
                ("head", "base"),
            )

        self.assertEqual(config["file"], "Main.sol")
        self.assertEqual(config["iterations"], 10)
        self.assertEqual(config["warmup"], 5)
        self.assertEqual(config["response"], "full")
        self.assertEqual(config["benchmarks"], list(benchmark.METHODS))
        self.assertEqual(config["methods"], benchmark.METHOD_CONFIG)
        python = str(Path(sys.executable).resolve())
        filter_path = str(benchmark.FILTER_PATH.resolve())
        self.assertEqual(
            config["servers"],
            [
                {
                    "label": "head",
                    "cmd": python,
                    "args": [filter_path, str((root / "head").resolve()), "lsp"],
                },
                {
                    "label": "base",
                    "cmd": python,
                    "args": [filter_path, str((root / "base").resolve()), "lsp"],
                },
            ],
        )
        self.assertNotIn("commit", config["servers"][0])
        self.assertNotIn("repo", config["servers"][0])
        self.assertIs(benchmark._validate_generated_config(config, ("head", "base")), config)

    def test_generated_config_validation_rejects_contract_drift(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = benchmark.generated_config(
                root / "project",
                root / "output",
                {"base": root / "base", "head": root / "head"},
                ("base", "head"),
            )
        config["unexpected"] = True

        with self.assertRaisesRegex(benchmark.ValidationError, "config fields"):
            benchmark._validate_generated_config(config, ("base", "head"))

    def test_upstream_release_metadata_is_exactly_pinned(self) -> None:
        upstream = benchmark.pinned_upstream()

        self.assertEqual(upstream["version"], "0.3.3")
        self.assertEqual(
            upstream["commit"], "ca0651f86f430290dacdbeb62c9c6987a3ad6966"
        )
        self.assertEqual(
            upstream["asset"]["sha256"],
            "cf66d5237951046b0dd83726b86e0c8b23fc20fe3315f184fea48543337a23df",
        )

    def test_subprocess_environment_is_fixed_and_isolated(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            first = benchmark.sanitized_environment(root / "first")
            second = benchmark.sanitized_environment(root / "second")

        self.assertEqual(first["PATH"], os.defpath)
        self.assertNotIn("GITHUB_TOKEN", first)
        self.assertEqual(set(first), set(second))
        self.assertNotEqual(first["HOME"], second["HOME"])

    def test_binary_version_must_match_the_pinned_release(self) -> None:
        good = mock.Mock(
            returncode=0,
            stdout=f"{benchmark._expected_upstream_version()}\n",
        )
        with mock.patch.object(benchmark.subprocess, "run", return_value=good) as run:
            benchmark._verify_upstream_binary(Path("/tmp/lsp-bench"))
        self.assertNotIn("GITHUB_TOKEN", run.call_args.kwargs["env"])

        bad = mock.Mock(returncode=0, stdout="lsp-bench 0.3.4\n")
        with mock.patch.object(benchmark.subprocess, "run", return_value=bad):
            with self.assertRaises(benchmark.ExecutionError):
                benchmark._verify_upstream_binary(Path("/tmp/lsp-bench"))

    def test_run_rejects_the_same_binary_path_for_both_roles(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "solar"
            binary.write_text("binary")
            binary.chmod(0o755)

            with self.assertRaisesRegex(benchmark.ExecutionError, "distinct paths"):
                benchmark.run_benchmark(
                    binary,
                    binary,
                    binary,
                    CONTEXT,
                    Path(directory) / "output",
                )


class ContextTests(unittest.TestCase):
    def test_context_rejects_markdown_and_url_injection(self) -> None:
        invalid = (
            (
                "owner/repo|bad",
                CONTEXT.head_repository,
                CONTEXT.workflow_repository,
                1,
                "1" * 40,
                "2" * 40,
                CONTEXT.run_url,
            ),
            (
                CONTEXT.repository,
                CONTEXT.head_repository,
                CONTEXT.workflow_repository,
                0,
                "1" * 40,
                "2" * 40,
                CONTEXT.run_url,
            ),
            (
                CONTEXT.repository,
                CONTEXT.head_repository,
                CONTEXT.workflow_repository,
                1,
                "A" * 40,
                "2" * 40,
                CONTEXT.run_url,
            ),
            (
                CONTEXT.repository,
                CONTEXT.head_repository,
                CONTEXT.workflow_repository,
                1,
                "1" * 40,
                "2" * 40,
                "https://[invalid",
            ),
            (
                CONTEXT.repository,
                CONTEXT.head_repository,
                CONTEXT.workflow_repository,
                1,
                "1" * 40,
                "2" * 40,
                f"{CONTEXT.run_url}?injected=1",
            ),
            (
                CONTEXT.repository,
                CONTEXT.head_repository,
                CONTEXT.workflow_repository,
                1,
                "1" * 40,
                "2" * 40,
                f"{CONTEXT.run_url}\n",
            ),
            (
                CONTEXT.repository,
                CONTEXT.head_repository,
                CONTEXT.workflow_repository,
                1,
                "1" * 40,
                "2" * 40,
                f"{CONTEXT.run_url}\t",
            ),
        )
        for values in invalid:
            with self.subTest(values=values):
                with self.assertRaises(benchmark.ValidationError):
                    benchmark.validate_context(*values)


class ArgumentParserTests(unittest.TestCase):
    def test_run_and_render_commands_parse_the_full_context(self) -> None:
        context_arguments = [
            "--repository",
            CONTEXT.repository,
            "--head-repository",
            CONTEXT.head_repository,
            "--workflow-repository",
            CONTEXT.workflow_repository,
            "--pr-number",
            str(CONTEXT.pr_number),
            "--base-sha",
            CONTEXT.base_sha,
            "--head-sha",
            CONTEXT.head_sha,
            "--run-url",
            CONTEXT.run_url,
        ]
        cases = (
            (
                [
                    "run",
                    "--lsp-bench",
                    "/tmp/lsp-bench",
                    "--base-binary",
                    "/tmp/base",
                    "--head-binary",
                    "/tmp/head",
                    "--output",
                    "/tmp/raw",
                ],
                "run",
            ),
            (
                [
                    "render",
                    "--input",
                    "/tmp/raw",
                    "--report",
                    "/tmp/report.md",
                    "--comparison",
                    "/tmp/comparison.json",
                ],
                "render",
            ),
        )
        for arguments, command in cases:
            with self.subTest(command=command):
                parsed = benchmark.argument_parser().parse_args(
                    [*arguments, *context_arguments]
                )

                self.assertEqual(parsed.command, command)
                self.assertEqual(parsed.pr_number, CONTEXT.pr_number)
                self.assertEqual(parsed.workflow_repository, CONTEXT.workflow_repository)


class NotificationFilterTests(unittest.TestCase):
    def test_drops_invalid_initialize_responses(self) -> None:
        invalid = (
            {
                "jsonrpc": "2.0",
                "id": 1,
                "error": {"code": -1, "message": "initialization failed"},
            },
            {"jsonrpc": "2.0", "id": 1, "result": {}},
        )
        for response in invalid:
            with self.subTest(response=response):
                filter_ = lsp_filter.NotificationFilter()
                filter_.observe_client(
                    {"jsonrpc": "2.0", "id": 1, "method": "initialize"}
                )

                self.assertEqual(filter_.server_messages(response, b"invalid"), [])

        filter_ = lsp_filter.NotificationFilter()
        filter_.observe_client({"jsonrpc": "2.0", "id": 1, "method": "initialize"})
        valid = {
            "jsonrpc": "2.0",
            "id": 1,
            "result": {"capabilities": {"hoverProvider": True}},
        }
        self.assertEqual(filter_.server_messages(valid, b"valid"), [b"valid"])

    def test_waits_for_main_diagnostics_and_replays_progress_end(self) -> None:
        filter_ = lsp_filter.NotificationFilter()
        filter_.observe_client({"jsonrpc": "2.0", "method": "initialized"})
        filter_.observe_client(
            {
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {"textDocument": {"uri": "file:///fixture/Main.sol"}},
            }
        )

        self.assertEqual(
            filter_.server_messages(
                {"jsonrpc": "2.0", "method": "window/logMessage"}, b"log"
            ),
            [],
        )
        progress = {
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {"value": {"kind": "end"}},
        }
        self.assertEqual(filter_.server_messages(progress, b"end"), [])
        math = {
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {"uri": "file:///fixture/Math.sol", "diagnostics": []},
        }
        self.assertEqual(filter_.server_messages(math, b"math"), [])
        main = {
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {"uri": "file:///fixture/Main.sol", "diagnostics": []},
        }
        self.assertEqual(filter_.server_messages(main, b"empty"), [])
        main["params"]["diagnostics"] = valid_response("textDocument/diagnostic")[
            "diagnostics"
        ]
        self.assertEqual(
            filter_.server_messages(main, b"main"), [b"main", b"end"]
        )
        self.assertEqual(
            filter_.server_messages(
                {"jsonrpc": "2.0", "method": "window/logMessage"}, b"later"
            ),
            [b"later"],
        )

    def test_synthesizes_index_completion_after_fixture_diagnostics(self) -> None:
        filter_ = lsp_filter.NotificationFilter()
        filter_.observe_client({"jsonrpc": "2.0", "method": "initialized"})
        filter_.observe_client(
            {
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {"textDocument": {"uri": "file:///fixture/Main.sol"}},
            }
        )
        notification = {
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": valid_response("textDocument/diagnostic"),
        }

        forwarded = filter_.server_messages(notification, b"diagnostics")

        self.assertEqual(forwarded[0], b"diagnostics")
        self.assertEqual(
            json.loads(forwarded[1]),
            {
                "jsonrpc": "2.0",
                "method": "$/progress",
                "params": {
                    "token": "solar-lsp-bench-index",
                    "value": {"kind": "end"},
                },
            },
        )

    def test_requires_the_exact_did_open_fixture_uri(self) -> None:
        filter_ = lsp_filter.NotificationFilter()
        filter_.observe_client({"jsonrpc": "2.0", "method": "initialized"})
        filter_.observe_client(
            {
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {"textDocument": {"uri": "file:///fixture/Main.sol"}},
            }
        )
        notification = {
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": "file:///other/Main.sol",
                "diagnostics": valid_response("textDocument/diagnostic")[
                    "diagnostics"
                ],
            },
        }

        self.assertEqual(filter_.server_messages(notification, b"wrong"), [])

    def test_rejects_malformed_progress_notifications(self) -> None:
        invalid = (
            {"jsonrpc": "2.0", "method": "$/progress", "params": []},
            {
                "jsonrpc": "2.0",
                "method": "$/progress",
                "params": {"value": []},
            },
        )
        for message in invalid:
            with self.subTest(message=message):
                with self.assertRaisesRegex(RuntimeError, "progress"):
                    lsp_filter.NotificationFilter().server_messages(message, b"invalid")

    def test_forwards_server_requests_while_filtering_notifications(self) -> None:
        filter_ = lsp_filter.NotificationFilter()
        filter_.observe_client({"jsonrpc": "2.0", "method": "initialized"})
        request = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "window/workDoneProgress/create",
        }

        self.assertEqual(filter_.server_messages(request, b"request"), [b"request"])

    def test_rejects_malformed_server_envelopes(self) -> None:
        invalid = (
            None,
            {"id": 1, "result": {}},
            {"jsonrpc": "1.0", "id": 1, "result": {}},
            {"jsonrpc": "2.0", "id": True, "result": {}},
            {"jsonrpc": "2.0", "id": 1, "result": {}, "error": {}},
            {"jsonrpc": "2.0", "id": 1, "error": {"code": -1}},
            {"jsonrpc": "2.0", "method": "notify", "params": 1},
        )

        for message in invalid:
            with self.subTest(message=message):
                with self.assertRaisesRegex(RuntimeError, "JSON-RPC"):
                    lsp_filter.NotificationFilter().server_messages(message, b"invalid")

    def test_rejects_non_strict_json_messages(self) -> None:
        invalid = (
            b"{",
            b'{"jsonrpc":"2.0","jsonrpc":"2.0"}',
            b'{"jsonrpc":"2.0","id":1,"result":NaN}',
        )

        for message in invalid:
            with self.subTest(message=message):
                with self.assertRaisesRegex(RuntimeError, "strict JSON"):
                    lsp_filter.parse_message(message)


class ResponseValidationTests(unittest.TestCase):
    def test_accepts_semantically_correct_responses(self) -> None:
        for method in benchmark.METHODS:
            with self.subTest(method=method):
                benchmark._validate_response(
                    method, valid_response(method), "response", RESPONSE_CONFIG
                )

        benchmark._validate_response(
            "textDocument/completion",
            [{"label": "value"}],
            "response",
            RESPONSE_CONFIG,
        )
        benchmark._validate_response(
            "textDocument/definition",
            [
                {
                    "targetUri": fixture_uri("Math.sol"),
                    "targetRange": lsp_range(4),
                    "targetSelectionRange": lsp_range(4),
                }
            ],
            "response",
            RESPONSE_CONFIG,
        )

    def test_rejects_semantically_incorrect_responses(self) -> None:
        invalid = {
            "initialize": {"capabilities": {}},
            "textDocument/diagnostic": {
                "uri": "file:///fixture/Main.sol",
                "diagnostics": [],
            },
            "textDocument/hover": {"contents": "function double(address)"},
            "textDocument/definition": [location("Math.sol", 5)],
            "textDocument/references": [location("Main.sol", 8)],
            "textDocument/completion": {"items": [{"label": "other"}]},
            "textDocument/documentSymbol": [{"name": "Main"}],
        }
        for method, response in invalid.items():
            with self.subTest(method=method):
                with self.assertRaises(benchmark.ValidationError):
                    benchmark._validate_response(
                        method, response, "response", RESPONSE_CONFIG
                    )

    def test_rejects_deceptive_hover_and_malformed_completion(self) -> None:
        invalid = (
            (
                "textDocument/hover",
                {"contents": "not a signature: double takes address and returns uint256"},
            ),
            (
                "textDocument/completion",
                {
                    "isIncomplete": False,
                    "items": [{"label": "value", "kind": "function"}],
                },
            ),
        )

        for method, response in invalid:
            with self.subTest(method=method):
                with self.assertRaises(benchmark.ValidationError):
                    benchmark._validate_response(
                        method, response, "response", RESPONSE_CONFIG
                    )

    def test_references_reject_malformed_extra_locations(self) -> None:
        response = [location("Main.sol", 8), location("Main.sol", 13), {"uri": 7}]

        with self.assertRaisesRegex(benchmark.ValidationError, "invalid reference"):
            benchmark._validate_response(
                "textDocument/references", response, "response", RESPONSE_CONFIG
            )

    def test_rejects_non_fixture_uri_suffix_matches(self) -> None:
        cases = {
            "textDocument/diagnostic": valid_response("textDocument/diagnostic"),
            "textDocument/definition": valid_response("textDocument/definition"),
            "textDocument/references": valid_response("textDocument/references"),
            "textDocument/documentSymbol": valid_response("textDocument/documentSymbol"),
        }
        cases["textDocument/diagnostic"]["uri"] = "file:///untrusted/Main.sol"
        cases["textDocument/definition"][0]["uri"] = "file:///untrusted/Math.sol"
        cases["textDocument/references"][0]["uri"] = "file:///untrusted/Main.sol"
        cases["textDocument/documentSymbol"][0]["location"]["uri"] = (
            "file:///untrusted/Main.sol"
        )

        for method, response in cases.items():
            with self.subTest(method=method):
                with self.assertRaises(benchmark.ValidationError):
                    benchmark._validate_response(
                        method, response, "response", RESPONSE_CONFIG
                    )


class ArtifactValidationTests(unittest.TestCase):
    def test_valid_artifact_merges_both_pass_orders(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            samples = benchmark.validate_artifact(artifact.root, CONTEXT)

        for role in ("base", "head"):
            self.assertEqual(set(samples[role]), set(benchmark.METHODS))
            for method in benchmark.METHODS:
                self.assertEqual(len(samples[role][method]), 20)
        self.assertEqual(samples["base"]["initialize"][:2], [10.0, 10.01])
        self.assertEqual(samples["base"]["initialize"][10:12], [13.0, 13.01])

    def test_rejects_manifest_from_different_trusted_context(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            artifact.manifest["context"]["head_sha"] = "3" * 40
            artifact.rewrite_manifest()

            with self.assertRaisesRegex(benchmark.ValidationError, "trusted workflow context"):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_tampered_result_digest(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            path = artifact.root / "passes" / "base-first" / "results.json"
            path.write_bytes(path.read_bytes() + b" ")

            with self.assertRaisesRegex(benchmark.ValidationError, "digest"):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_missing_pass_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            (artifact.root / "passes" / "head-first" / "config.json").unlink()

            with self.assertRaisesRegex(benchmark.ValidationError, "layout is incomplete"):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_files_not_covered_by_the_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            (artifact.root / "workflow.txt").write_text("untrusted sibling")

            with self.assertRaisesRegex(benchmark.ValidationError, "unexpected entry workflow.txt"):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_unexpected_config_fields_after_digest_verification(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            artifact.configs["base-first"]["unexpected"] = True
            artifact.rewrite_config("base-first")

            with self.assertRaisesRegex(benchmark.ValidationError, "config fields"):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_incomplete_method_set(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            artifact.results["base-first"]["benchmarks"].pop()
            artifact.rewrite_results("base-first")

            with self.assertRaisesRegex(benchmark.ValidationError, "core method set"):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_missing_or_incorrect_benchmark_input(self) -> None:
        mutations = (
            lambda item: item.pop("input"),
            lambda item: item.update(
                input='{"jsonrpc":"2.0","id":1,"method":"textDocument/hover","params":{}}'
            ),
            lambda item: item.update(
                input=item["input"].replace('"id":1', '"id":true', 1)
            ),
            lambda item: item.update(
                input=item["input"].replace('"line":13', '"line":13.0', 1)
            ),
        )
        for mutate in mutations:
            with self.subTest(mutate=mutate), tempfile.TemporaryDirectory() as directory:
                artifact = RawArtifact(Path(directory))
                hover = next(
                    item
                    for item in artifact.results["base-first"]["benchmarks"]
                    if item["name"] == "textDocument/hover"
                )
                mutate(hover)
                artifact.rewrite_results("base-first")

                with self.assertRaises(benchmark.ValidationError):
                    benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_failed_or_incomplete_measurements(self) -> None:
        mutations = {
            "failed status": lambda row: row.update(status="error"),
            "missing sample": lambda row: row["iterations"].pop(),
            "non-positive timing": lambda row: row["iterations"][0].update(ms=0),
            "timing rounds to zero": lambda row: row["iterations"][0].update(
                ms=0.00001
            ),
            "wrong canonical response": lambda row: row.update(response=None),
        }
        for name, mutate in mutations.items():
            with self.subTest(case=name), tempfile.TemporaryDirectory() as directory:
                artifact = RawArtifact(Path(directory))
                row = artifact.results["base-first"]["benchmarks"][0]["servers"][0]
                mutate(row)
                artifact.rewrite_results("base-first")

                with self.assertRaises(benchmark.ValidationError):
                    benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_nonfinite_json_constant(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            path = artifact.root / "passes" / "base-first" / "results.json"
            data = path.read_bytes().replace(b'"ms": 10.0', b'"ms": NaN', 1)
            path.write_bytes(data)
            artifact.pass_entry("base-first")["results"]["sha256"] = hashlib.sha256(
                data
            ).hexdigest()
            artifact.rewrite_manifest()

            with self.assertRaisesRegex(benchmark.ValidationError, "valid strict JSON"):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_malformed_and_duplicate_key_json(self) -> None:
        invalid_documents = (b"{", b'{"schema_version": 1, "schema_version": 1}')
        for document in invalid_documents:
            with self.subTest(document=document), tempfile.TemporaryDirectory() as directory:
                artifact = RawArtifact(Path(directory))
                (artifact.root / "manifest.json").write_bytes(document)

                with self.assertRaisesRegex(benchmark.ValidationError, "valid strict JSON"):
                    benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_incorrect_measured_response(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            hover = next(
                item
                for item in artifact.results["base-first"]["benchmarks"]
                if item["name"] == "textDocument/hover"
            )
            hover["servers"][0]["iterations"][4]["response"] = {
                "contents": "unrelated"
            }
            artifact.rewrite_results("base-first")

            with self.assertRaisesRegex(benchmark.ValidationError, "double function hover"):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_invalid_rss_even_though_rss_is_not_a_verdict_metric(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            row = artifact.results["base-first"]["benchmarks"][0]["servers"][0]
            row["rss_kb"] = -1
            artifact.rewrite_results("base-first")

            with self.assertRaisesRegex(benchmark.ValidationError, "non-negative"):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_upstream_commit_mode_in_generated_config(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            artifact.configs["base-first"]["servers"][0]["commit"] = "main"
            artifact.rewrite_config("base-first")

            with self.assertRaisesRegex(benchmark.ValidationError, "unsupported server fields"):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    @unittest.skipIf(os.name == "nt", "symlink semantics differ on Windows")
    def test_rejects_symlinked_result_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            result = artifact.root / "passes" / "base-first" / "results.json"
            target = artifact.root / "outside.json"
            result.rename(target)
            result.symlink_to(target)

            with self.assertRaisesRegex(benchmark.ValidationError, "unexpected entry"):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_render_artifact_turns_invalid_input_into_escaped_inconclusive_report(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = RawArtifact(root / "raw")
            artifact.manifest["passes"] = []
            artifact.rewrite_manifest()
            report = root / "report.md"
            comparison_path = root / "comparison.json"

            valid = benchmark.render_artifact(
                artifact.root, CONTEXT, report, comparison_path
            )
            comparison = json.loads(comparison_path.read_text())

            self.assertFalse(valid)
            self.assertEqual(comparison["overall"], "inconclusive")
            self.assertIn("wrong number of passes", comparison["error"])
            self.assertIn("raw benchmark artifact did not pass validation", report.read_text())

    def test_render_artifact_rejects_delta_that_overflows_after_rounding(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = RawArtifact(root / "raw")
            for pass_name, _ in benchmark.PASSES:
                initialize = next(
                    item
                    for item in artifact.results[pass_name]["benchmarks"]
                    if item["name"] == "initialize"
                )
                head = next(row for row in initialize["servers"] if row["server"] == "head")
                head.update(p50_ms=1e308, p95_ms=1e308, mean_ms=1e308)
                for iteration in head["iterations"]:
                    iteration["ms"] = 1e308
                artifact.rewrite_results(pass_name)
            report = root / "report.md"
            comparison_path = root / "comparison.json"

            valid = benchmark.render_artifact(
                artifact.root, CONTEXT, report, comparison_path
            )
            comparison_text = comparison_path.read_text()
            comparison = json.loads(comparison_text)

            self.assertFalse(valid)
            self.assertEqual(comparison["schema_version"], 1)
            self.assertEqual(comparison["overall"], "inconclusive")
            self.assertEqual(comparison["methods"], [])
            self.assertIn("delta must remain finite", comparison["error"])
            self.assertNotIn("Infinity", comparison_text)
            self.assertIn("**Overall:** `inconclusive`", report.read_text())


class StatisticsTests(unittest.TestCase):
    def test_percentile_uses_nearest_rank(self) -> None:
        samples = list(range(20, 0, -1))

        self.assertEqual(benchmark.percentile(samples, 50), 10)
        self.assertEqual(benchmark.percentile(samples, 95), 19)
        self.assertEqual(benchmark.percentile(samples, 100), 20)

    def test_percentile_rejects_empty_samples_and_invalid_percentages(self) -> None:
        with self.assertRaisesRegex(ValueError, "at least one sample"):
            benchmark.percentile([], 50)
        for percent in (0, -1, 101):
            with self.subTest(percent=percent):
                with self.assertRaisesRegex(ValueError, "must be in"):
                    benchmark.percentile([1], percent)

    def test_method_verdict_requires_both_percentiles_to_cross_threshold(self) -> None:
        cases = (
            ((10.0, 10.0), "regression"),
            ((-10.0, -10.0), "improvement"),
            ((20.0, 9.99), "stable"),
            ((-20.0, -9.99), "stable"),
            ((20.0, -20.0), "stable"),
        )
        for deltas, expected in cases:
            with self.subTest(deltas=deltas):
                self.assertEqual(benchmark.method_verdict(*deltas), expected)

    def test_build_comparison_treats_exact_decimal_ten_percent_as_the_boundary(self) -> None:
        cases = ((0.1, 0.11, "regression"), (0.3, 0.27, "improvement"))
        for base_ms, head_ms, expected in cases:
            with self.subTest(expected=expected):
                samples = {
                    "base": {method: [base_ms] * 20 for method in benchmark.METHODS},
                    "head": {method: [head_ms] * 20 for method in benchmark.METHODS},
                }

                comparison = benchmark.build_comparison(samples, CONTEXT)

                self.assertEqual(comparison["overall"], expected)
                self.assertEqual(comparison["methods"][0]["verdict"], expected)

    def test_build_comparison_recomputes_metrics_and_prioritizes_regressions(self) -> None:
        samples = {
            role: {method: [10.0] * 20 for method in benchmark.METHODS}
            for role in ("base", "head")
        }
        samples["head"][benchmark.METHODS[0]] = [12.0] * 20
        samples["head"][benchmark.METHODS[1]] = [8.0] * 20

        comparison = benchmark.build_comparison(samples, CONTEXT)

        self.assertEqual(comparison["overall"], "regression")
        self.assertEqual(comparison["methods"][0]["sample_count"], 20)
        self.assertEqual(comparison["methods"][0]["base"]["p95_ms"], 10.0)
        self.assertEqual(comparison["methods"][0]["head"]["p50_ms"], 12.0)
        self.assertEqual(comparison["methods"][0]["delta_percent"]["p50"], 20.0)
        self.assertEqual(comparison["methods"][0]["verdict"], "regression")
        self.assertEqual(comparison["methods"][1]["verdict"], "improvement")

    def test_overall_verdict_covers_stable_and_improvement(self) -> None:
        for head_ms, expected in ((10.0, "stable"), (8.0, "improvement")):
            with self.subTest(expected=expected):
                samples = {
                    "base": {method: [10.0] * 20 for method in benchmark.METHODS},
                    "head": {method: [head_ms] * 20 for method in benchmark.METHODS},
                }

                comparison = benchmark.build_comparison(samples, CONTEXT)

                self.assertEqual(comparison["overall"], expected)
                self.assertEqual(
                    {method["verdict"] for method in comparison["methods"]},
                    {expected},
                )


class MarkdownTests(unittest.TestCase):
    def test_markdown_escape_handles_table_markup_html_and_newlines(self) -> None:
        self.assertEqual(
            benchmark.markdown_escape("|`[]\\\r\n<&"),
            "\\|\\`\\[\\]\\\\<br>&lt;&amp;",
        )

    def test_render_markdown_formats_a_comparison_table(self) -> None:
        comparison = {
            "repository": CONTEXT.repository,
            "head_repository": CONTEXT.head_repository,
            "base_sha": CONTEXT.base_sha,
            "head_sha": CONTEXT.head_sha,
            "run_url": CONTEXT.run_url,
            "overall": "regression",
            "methods": [
                {
                    "name": "text|document",
                    "sample_count": 20,
                    "base": {"p50_ms": 1.234, "p95_ms": 2.345},
                    "head": {"p50_ms": 1.5, "p95_ms": 3.0},
                    "delta_percent": {"p50": 21.56, "p95": -10.0},
                    "verdict": "stable",
                }
            ],
        }

        rendered = benchmark.render_markdown(comparison)

        expected_row = (
            "| text\\|document | 20 | 1.23 ms | 1.50 ms | +21.56% | "
            "2.35 ms | 3.00 ms | -10.00% | stable |"
        )
        self.assertTrue(rendered.startswith("<!-- solar-lsp-benchmark -->\n## LSP benchmark\n"))
        self.assertIn("**Overall:** `regression`", rendered)
        self.assertIn(expected_row, rendered)
        self.assertIn(f"/commit/{CONTEXT.base_sha}", rendered)
        self.assertIn("both recomputed percentiles", rendered)

    def test_render_markdown_escapes_inconclusive_reason(self) -> None:
        comparison = benchmark.inconclusive_comparison(
            CONTEXT, "bad | `artifact` <value>\nsecond line"
        )

        rendered = benchmark.render_markdown(comparison)

        self.assertIn(
            "Reason: bad \\| \\`artifact\\` &lt;value&gt;<br>second line",
            rendered,
        )
        self.assertNotIn("| Method | Samples |", rendered)


if __name__ == "__main__":
    unittest.main()
