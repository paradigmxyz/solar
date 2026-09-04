#!/usr/bin/env python3

from __future__ import annotations

import copy
import hashlib
import importlib.util
import itertools
import json
import os
import sys
import tempfile
import unittest
from decimal import Decimal, localcontext
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

CONTEXT = benchmark.Context(
    repository="paradigmxyz/solar",
    pr_head_repository="0xKarl98/solar",
    workflow_repository="0xKarl98/solar",
    pr_number=1195,
    base_sha="1" * 40,
    head_sha="2" * 40,
    main_sha="1" * 40,
    pr_head_sha="4" * 40,
    merge_candidate_sha="2" * 40,
    run_url="https://github.com/0xKarl98/solar/actions/runs/12345",
)
CURRENT_MAIN_SHA = "1" * 40
CURRENT_PR_HEAD_SHA = "4" * 40
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
    data = (
        json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n"
    ).encode()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return data


class RawArtifact:
    def __init__(self, root: Path) -> None:
        self.root = root
        self.configs: dict[tuple[str, int], dict[str, Any]] = {}
        self.results: dict[tuple[str, int], dict[str, Any]] = {}
        self.manifest = {
            "schema_version": benchmark.RAW_SCHEMA_VERSION,
            "kind": benchmark.RAW_KIND,
            "context": {
                "comparison_mode": CONTEXT.comparison_mode,
                "repository": CONTEXT.repository,
                "pr_head_repository": CONTEXT.pr_head_repository,
                "workflow_repository": CONTEXT.workflow_repository,
                "pr_number": CONTEXT.pr_number,
                "base_sha": CONTEXT.base_sha,
                "head_sha": CONTEXT.head_sha,
                "main_sha": CONTEXT.main_sha,
                "pr_head_sha": CONTEXT.pr_head_sha,
                "merge_candidate_sha": CONTEXT.merge_candidate_sha,
                "run_url": CONTEXT.run_url,
            },
            "protocol": {
                "warmup_iterations": benchmark.WARMUP_ITERATIONS,
                "measured_iterations_per_session": benchmark.MEASURED_ITERATIONS,
                "sessions_per_order": benchmark.SESSIONS_PER_ORDER,
                "passes": [name for name, _ in benchmark.PASSES],
                "methods": list(benchmark.METHODS),
                "sample_unit": benchmark.SAMPLE_UNIT,
                "sample_precision": benchmark.SAMPLE_PRECISION,
                "threshold_percent": benchmark.THRESHOLD_PERCENT,
                "threshold_absolute_ms": benchmark.THRESHOLD_ABSOLUTE_MS,
                "confidence_level": benchmark.CONFIDENCE_LEVEL,
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
        for pass_index, (pass_name, session, server_order) in enumerate(
            benchmark.PASS_SESSIONS
        ):
            config = benchmark.generated_config(
                root / "runtime" / pass_name / str(session) / "project",
                root / "runtime" / pass_name / str(session) / "output",
                commands,
                server_order,
            )
            results = self._results(config, server_order, pass_index)
            key = (pass_name, session)
            self.configs[key] = config
            self.results[key] = results
            self.manifest["passes"].append(
                {
                    "name": pass_name,
                    "session": session,
                    "server_order": list(server_order),
                    "config": {
                        "path": f"passes/{pass_name}/{session}/config.json",
                        "sha256": "",
                    },
                    "results": {
                        "path": f"passes/{pass_name}/{session}/results.json",
                        "sha256": "",
                    },
                }
            )
            self.rewrite_config(pass_name, session, rewrite_manifest=False)
            self.rewrite_results(pass_name, session, rewrite_manifest=False)
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
                benchmark_result["input"] = json.dumps(
                    benchmark_input, separators=(",", ":")
                )
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

    def pass_entry(self, pass_name: str, session: int = 1) -> dict[str, Any]:
        return next(
            entry
            for entry in self.manifest["passes"]
            if entry["name"] == pass_name and entry["session"] == session
        )

    def rewrite_config(
        self, pass_name: str, session: int = 1, *, rewrite_manifest: bool = True
    ) -> None:
        path = self.root / "passes" / pass_name / str(session) / "config.json"
        data = write_json(path, self.configs[(pass_name, session)])
        self.pass_entry(pass_name, session)["config"]["sha256"] = hashlib.sha256(
            data
        ).hexdigest()
        if rewrite_manifest:
            self.rewrite_manifest()

    def rewrite_results(
        self, pass_name: str, session: int = 1, *, rewrite_manifest: bool = True
    ) -> None:
        path = self.root / "passes" / pass_name / str(session) / "results.json"
        data = write_json(path, self.results[(pass_name, session)])
        self.pass_entry(pass_name, session)["results"]["sha256"] = hashlib.sha256(
            data
        ).hexdigest()
        if rewrite_manifest:
            self.rewrite_manifest()

    def rewrite_manifest(self) -> None:
        write_json(self.root / "manifest.json", self.manifest)


def constant_sessions(
    base_ms: float = 10.0,
    head_ms: float = 10.0,
    *,
    by_order: dict[str, tuple[float, float]] | None = None,
) -> list[benchmark.BenchmarkSession]:
    sessions = []
    for order, session, _ in benchmark.PASS_SESSIONS:
        order_base, order_head = (by_order or {}).get(order, (base_ms, head_ms))
        samples = {
            "base": {
                method: [order_base] * benchmark.MEASURED_ITERATIONS
                for method in benchmark.METHODS
            },
            "head": {
                method: [order_head] * benchmark.MEASURED_ITERATIONS
                for method in benchmark.METHODS
            },
        }
        sessions.append(benchmark.BenchmarkSession(order, session, samples))
    return sessions


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
        self.assertEqual(
            config["servers"],
            [
                {
                    "label": "head",
                    "cmd": str((root / "head").resolve()),
                    "args": ["lsp"],
                },
                {
                    "label": "base",
                    "cmd": str((root / "base").resolve()),
                    "args": ["lsp"],
                },
            ],
        )
        self.assertNotIn("commit", config["servers"][0])
        self.assertNotIn("repo", config["servers"][0])
        self.assertIs(
            benchmark._validate_generated_config(config, ("head", "base")), config
        )

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
        self.assertEqual(upstream["commit"], "ca0651f86f430290dacdbeb62c9c6987a3ad6966")
        self.assertEqual(
            upstream["source"]["sha256"],
            "145dc03c5606d6b5ec66647d233486bab9f4e65022275763bf445bc26414470e",
        )
        self.assertEqual(
            upstream["adapter"]["sha256"],
            "8d5242a63ff812056b449dedffd96e3f60bcc475dd8f39142b340acae7dbf7a2",
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
        with (
            mock.patch.object(benchmark.subprocess, "run", return_value=bad),
            self.assertRaises(benchmark.ExecutionError),
        ):
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
    def test_context_requires_base_to_be_main_and_head_to_be_merge_candidate(
        self,
    ) -> None:
        with self.assertRaisesRegex(benchmark.ValidationError, "equal main"):
            benchmark.validate_context(
                CONTEXT.repository,
                CONTEXT.pr_head_repository,
                CONTEXT.workflow_repository,
                CONTEXT.pr_number,
                CONTEXT.base_sha,
                CONTEXT.head_sha,
                "3" * 40,
                CONTEXT.pr_head_sha,
                CONTEXT.merge_candidate_sha,
                CONTEXT.run_url,
            )

        with self.assertRaisesRegex(benchmark.ValidationError, "equal merge candidate"):
            benchmark.validate_context(
                CONTEXT.repository,
                CONTEXT.pr_head_repository,
                CONTEXT.workflow_repository,
                CONTEXT.pr_number,
                CONTEXT.base_sha,
                CONTEXT.head_sha,
                CONTEXT.main_sha,
                CONTEXT.pr_head_sha,
                "3" * 40,
                CONTEXT.run_url,
            )

    def test_context_rejects_markdown_and_url_injection(self) -> None:
        invalid = (
            {"repository": "owner/repo|bad"},
            {"pr_head_repository": "owner/repo|bad"},
            {"pr_number": 0},
            {"base_sha": "A" * 40},
            {"head_sha": "A" * 40},
            {"main_sha": "A" * 40},
            {"pr_head_sha": "A" * 40},
            {"merge_candidate_sha": "A" * 40},
            {"run_url": "https://[invalid"},
            {"run_url": f"{CONTEXT.run_url}?injected=1"},
            {"run_url": f"{CONTEXT.run_url}\n"},
            {"run_url": f"{CONTEXT.run_url}\t"},
        )
        for values in invalid:
            with (
                self.subTest(values=values),
                self.assertRaises(benchmark.ValidationError),
            ):
                context = {
                    key: value
                    for key, value in CONTEXT.__dict__.items()
                    if key != "comparison_mode"
                } | values
                benchmark.validate_context(**context)


class ArgumentParserTests(unittest.TestCase):
    def test_run_and_render_commands_parse_the_full_context(self) -> None:
        context_arguments = [
            "--repository",
            CONTEXT.repository,
            "--pr-head-repository",
            CONTEXT.pr_head_repository,
            "--workflow-repository",
            CONTEXT.workflow_repository,
            "--pr-number",
            str(CONTEXT.pr_number),
            "--base-sha",
            CONTEXT.base_sha,
            "--head-sha",
            CONTEXT.head_sha,
            "--main-sha",
            CONTEXT.main_sha,
            "--pr-head-sha",
            CONTEXT.pr_head_sha,
            "--merge-candidate-sha",
            CONTEXT.merge_candidate_sha,
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
                self.assertEqual(
                    parsed.workflow_repository, CONTEXT.workflow_repository
                )


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
            with (
                self.subTest(method=method),
                self.assertRaises(benchmark.ValidationError),
            ):
                benchmark._validate_response(
                    method, response, "response", RESPONSE_CONFIG
                )

    def test_rejects_deceptive_hover_and_malformed_completion(self) -> None:
        invalid = (
            (
                "textDocument/hover",
                {
                    "contents": "not a signature: double takes address and returns uint256"
                },
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
            with (
                self.subTest(method=method),
                self.assertRaises(benchmark.ValidationError),
            ):
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
            "textDocument/documentSymbol": valid_response(
                "textDocument/documentSymbol"
            ),
        }
        cases["textDocument/diagnostic"]["uri"] = "file:///untrusted/Main.sol"
        cases["textDocument/definition"][0]["uri"] = "file:///untrusted/Math.sol"
        cases["textDocument/references"][0]["uri"] = "file:///untrusted/Main.sol"
        cases["textDocument/documentSymbol"][0]["location"]["uri"] = (
            "file:///untrusted/Main.sol"
        )

        for method, response in cases.items():
            with (
                self.subTest(method=method),
                self.assertRaises(benchmark.ValidationError),
            ):
                benchmark._validate_response(
                    method, response, "response", RESPONSE_CONFIG
                )


class ArtifactValidationTests(unittest.TestCase):
    def test_valid_artifact_preserves_order_session_and_sample_precision(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            precise = 10.1234567890123
            artifact.results[("base-first", 1)]["benchmarks"][0]["servers"][0][
                "iterations"
            ][0]["ms"] = precise
            artifact.rewrite_results("base-first")
            sessions = benchmark.validate_artifact(artifact.root, CONTEXT)

        self.assertEqual(
            [(session.order, session.session) for session in sessions],
            [(name, session) for name, session, _ in benchmark.PASS_SESSIONS],
        )
        self.assertEqual(len(sessions), 10)
        for session in sessions:
            for role in ("base", "head"):
                self.assertEqual(set(session.samples[role]), set(benchmark.METHODS))
                for method in benchmark.METHODS:
                    self.assertEqual(
                        len(session.samples[role][method]),
                        benchmark.MEASURED_ITERATIONS,
                    )
        self.assertEqual(sessions[0].samples["base"]["initialize"][0], precise)
        self.assertEqual(sessions[1].samples["base"]["initialize"][:2], [13.0, 13.01])

    def test_rejects_manifest_from_different_trusted_context(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            artifact.manifest["context"]["head_sha"] = "3" * 40
            artifact.rewrite_manifest()

            with self.assertRaisesRegex(
                benchmark.ValidationError, "trusted workflow context"
            ):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_tampered_result_digest(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            path = artifact.root / "passes" / "base-first" / "1" / "results.json"
            path.write_bytes(path.read_bytes() + b" ")

            with self.assertRaisesRegex(benchmark.ValidationError, "digest"):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_missing_pass_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            (artifact.root / "passes" / "head-first" / "1" / "config.json").unlink()

            with self.assertRaisesRegex(
                benchmark.ValidationError, "layout is incomplete"
            ):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_files_not_covered_by_the_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            (artifact.root / "workflow.txt").write_text("untrusted sibling")

            with self.assertRaisesRegex(
                benchmark.ValidationError, "unexpected entry workflow.txt"
            ):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_unexpected_config_fields_after_digest_verification(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            artifact.configs[("base-first", 1)]["unexpected"] = True
            artifact.rewrite_config("base-first")

            with self.assertRaisesRegex(benchmark.ValidationError, "config fields"):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_incomplete_method_set(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            artifact.results[("base-first", 1)]["benchmarks"].pop()
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
            with (
                self.subTest(mutate=mutate),
                tempfile.TemporaryDirectory() as directory,
            ):
                artifact = RawArtifact(Path(directory))
                hover = next(
                    item
                    for item in artifact.results[("base-first", 1)]["benchmarks"]
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
                row = artifact.results[("base-first", 1)]["benchmarks"][0]["servers"][0]
                mutate(row)
                artifact.rewrite_results("base-first")

                with self.assertRaises(benchmark.ValidationError):
                    benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_nonfinite_json_constant(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            path = artifact.root / "passes" / "base-first" / "1" / "results.json"
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
            with (
                self.subTest(document=document),
                tempfile.TemporaryDirectory() as directory,
            ):
                artifact = RawArtifact(Path(directory))
                (artifact.root / "manifest.json").write_bytes(document)

                with self.assertRaisesRegex(
                    benchmark.ValidationError, "valid strict JSON"
                ):
                    benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_incorrect_measured_response(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            hover = next(
                item
                for item in artifact.results[("base-first", 1)]["benchmarks"]
                if item["name"] == "textDocument/hover"
            )
            hover["servers"][0]["iterations"][4]["response"] = {"contents": "unrelated"}
            artifact.rewrite_results("base-first")

            with self.assertRaisesRegex(
                benchmark.ValidationError, "double function hover"
            ):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_invalid_rss_even_though_rss_is_not_a_verdict_metric(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            row = artifact.results[("base-first", 1)]["benchmarks"][0]["servers"][0]
            row["rss_kb"] = -1
            artifact.rewrite_results("base-first")

            with self.assertRaisesRegex(benchmark.ValidationError, "non-negative"):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_rejects_upstream_commit_mode_in_generated_config(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            artifact.configs[("base-first", 1)]["servers"][0]["commit"] = "main"
            artifact.rewrite_config("base-first")

            with self.assertRaisesRegex(
                benchmark.ValidationError, "unsupported server fields"
            ):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    @unittest.skipIf(os.name == "nt", "symlink semantics differ on Windows")
    def test_rejects_symlinked_result_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            result = artifact.root / "passes" / "base-first" / "1" / "results.json"
            target = artifact.root / "outside.json"
            result.rename(target)
            result.symlink_to(target)

            with self.assertRaisesRegex(benchmark.ValidationError, "unexpected entry"):
                benchmark.validate_artifact(artifact.root, CONTEXT)

    def test_render_artifact_turns_invalid_input_into_escaped_inconclusive_report(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = RawArtifact(root / "raw")
            artifact.manifest["passes"] = []
            artifact.rewrite_manifest()
            report = root / "report.md"
            comparison_path = root / "comparison.json"

            valid = benchmark.render_artifact(
                artifact.root,
                CONTEXT,
                report,
                comparison_path,
                CURRENT_MAIN_SHA,
                CURRENT_PR_HEAD_SHA,
            )
            comparison = json.loads(comparison_path.read_text())

            self.assertFalse(valid)
            self.assertEqual(comparison["overall"], "inconclusive")
            self.assertIn("wrong number of passes", comparison["error"])
            self.assertIn("could not be validated", report.read_text())

    def test_render_artifact_fails_closed_without_current_state(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = RawArtifact(root / "raw")
            report = root / "report.md"
            comparison_path = root / "comparison.json"

            valid = benchmark.render_artifact(
                artifact.root, CONTEXT, report, comparison_path
            )
            comparison = json.loads(comparison_path.read_text())

        self.assertFalse(valid)
        self.assertEqual(comparison["overall"], "inconclusive")
        self.assertIn("current publication state query", comparison["error"])
        self.assertNotIn("freshness", comparison)
        self.assertNotIn("current_main_sha", comparison)
        self.assertNotIn("current_pr_head_sha", comparison)

    def test_render_artifact_rejects_delta_that_overflows_after_rounding(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = RawArtifact(root / "raw")
            for pass_name, session, _ in benchmark.PASS_SESSIONS:
                initialize = next(
                    item
                    for item in artifact.results[(pass_name, session)]["benchmarks"]
                    if item["name"] == "initialize"
                )
                head = next(
                    row for row in initialize["servers"] if row["server"] == "head"
                )
                head.update(p50_ms=1e308, p95_ms=1e308, mean_ms=1e308)
                for iteration in head["iterations"]:
                    iteration["ms"] = 1e308
                artifact.rewrite_results(pass_name, session)
            report = root / "report.md"
            comparison_path = root / "comparison.json"

            valid = benchmark.render_artifact(
                artifact.root,
                CONTEXT,
                report,
                comparison_path,
                CURRENT_MAIN_SHA,
                CURRENT_PR_HEAD_SHA,
            )
            comparison_text = comparison_path.read_text()
            comparison = json.loads(comparison_text)

            self.assertFalse(valid)
            self.assertEqual(
                comparison["schema_version"], benchmark.COMPARISON_SCHEMA_VERSION
            )
            self.assertEqual(comparison["overall"], "inconclusive")
            self.assertEqual(comparison["methods"], [])
            self.assertIn("delta must remain finite", comparison["error"])
            self.assertNotIn("Infinity", comparison_text)
            self.assertIn("**Overall:** `inconclusive`", report.read_text())


class PublicationStateTests(unittest.TestCase):
    def test_freshness_matrix_prioritizes_pr_head_changes(self) -> None:
        states = (
            (CURRENT_MAIN_SHA, CURRENT_PR_HEAD_SHA, "current"),
            ("5" * 40, CURRENT_PR_HEAD_SHA, "main-advanced"),
            (CURRENT_MAIN_SHA, "6" * 40, "superseded"),
            ("5" * 40, "6" * 40, "superseded"),
        )
        for current_main, current_head, expected in states:
            with self.subTest(expected=expected):
                state = benchmark.validate_publication_state(
                    CONTEXT, current_main, current_head
                )
                self.assertEqual(state.value, expected)

    def test_freshness_never_rewrites_performance_verdicts(self) -> None:
        for head_ms, expected in (
            (12.0, "regression"),
            (8.0, "improvement"),
            (10.0, "stable"),
        ):
            frozen = benchmark.build_comparison(
                constant_sessions(head_ms=head_ms), CONTEXT
            )
            for current_main, current_head, freshness in (
                ("5" * 40, CURRENT_PR_HEAD_SHA, "main-advanced"),
                (CURRENT_MAIN_SHA, "6" * 40, "superseded"),
            ):
                with self.subTest(expected=expected, freshness=freshness):
                    comparison = benchmark.add_publication_state(
                        frozen, CONTEXT, current_main, current_head
                    )
                    self.assertEqual(comparison["freshness"], freshness)
                    self.assertEqual(comparison["overall"], expected)
                    self.assertEqual(
                        {method["verdict"] for method in comparison["methods"]},
                        {expected},
                    )

    def test_stale_conclusive_artifact_preserves_performance_verdict(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = RawArtifact(root / "raw")
            report = root / "report.md"
            comparison_path = root / "comparison.json"

            valid = benchmark.render_artifact(
                artifact.root,
                CONTEXT,
                report,
                comparison_path,
                "5" * 40,
                CURRENT_PR_HEAD_SHA,
            )
            comparison = json.loads(comparison_path.read_text())
            report_text = report.read_text()

        self.assertTrue(valid)
        self.assertEqual(comparison["freshness"], "main-advanced")
        self.assertEqual(comparison["overall"], "stable")
        self.assertIn("reference only", report_text)
        self.assertIn("Rerun the benchmark before merging", report_text)


class StatisticsTests(unittest.TestCase):
    def test_comparison_names_the_diagnostics_timing_boundary(self) -> None:
        comparison = benchmark.build_comparison(constant_sessions(), CONTEXT)

        self.assertEqual(benchmark.METHODS[1], "textDocument/diagnostic")
        self.assertEqual(comparison["methods"][1]["name"], "didOpen/publishDiagnostics")
        rendered = benchmark.render_markdown(
            benchmark.add_publication_state(
                comparison, CONTEXT, CURRENT_MAIN_SHA, CURRENT_PR_HEAD_SHA
            )
        )
        self.assertIn("| didOpen/publishDiagnostics |", rendered)
        self.assertNotIn("| textDocument/diagnostic |", rendered)

    def test_percentile_uses_nearest_rank(self) -> None:
        samples = list(range(20, 0, -1))

        self.assertEqual(benchmark.percentile(samples, 50), 10)
        self.assertEqual(benchmark.percentile(samples, 95), 19)
        self.assertEqual(benchmark.percentile(samples, 100), 20)

    def test_percentile_rejects_empty_samples_and_invalid_percentages(self) -> None:
        with self.assertRaisesRegex(ValueError, "at least one sample"):
            benchmark.percentile([], 50)
        for percent in (0, -1, 101):
            with (
                self.subTest(percent=percent),
                self.assertRaisesRegex(ValueError, "must be in"),
            ):
                benchmark.percentile([1], percent)

    def test_bootstrap_count_vectors_cover_ordered_resample_weights(self) -> None:
        vectors = benchmark._bootstrap_count_vectors(5)

        self.assertEqual(len(vectors), 126)
        self.assertEqual(sum(weight for _, weight in vectors), 5**5)
        weights = dict(vectors)
        self.assertEqual(weights[(5, 0, 0, 0, 0)], 1)
        self.assertEqual(weights[(2, 2, 1, 0, 0)], 30)

    def test_paired_bootstrap_matches_ordered_resample_reference(self) -> None:
        base = tuple(
            Decimal(value) for value in ("1.1", "1.1", "2.2", "4.4", "4.4")
        )
        head = tuple(
            Decimal(value) for value in ("1.4", "1.4", "2.0", "5.1", "5.1")
        )
        absolute_deltas = []
        percent_deltas = []
        for indices in itertools.product(range(len(base)), repeat=len(base)):
            base_estimate = sum(
                (base[index] for index in indices), Decimal()
            ) / Decimal(len(base))
            head_estimate = sum(
                (head[index] for index in indices), Decimal()
            ) / Decimal(len(head))
            absolute_delta = head_estimate - base_estimate
            absolute_deltas.append(absolute_delta)
            percent_deltas.append(absolute_delta / base_estimate * Decimal(100))

        tail = (1.0 - benchmark.CONFIDENCE_LEVEL) * 50
        expected = {
            "delta_ms": (
                benchmark._decimal_percentile(absolute_deltas, tail),
                benchmark._decimal_percentile(absolute_deltas, 100 - tail),
            ),
            "delta_percent": (
                benchmark._decimal_percentile(percent_deltas, tail),
                benchmark._decimal_percentile(percent_deltas, 100 - tail),
            ),
        }

        benchmark._paired_bootstrap_interval.cache_clear()
        actual = benchmark._paired_bootstrap_interval(base, head)

        self.assertEqual(actual, expected)

    def test_paired_bootstrap_falls_back_when_grouped_sums_could_round(
        self,
    ) -> None:
        base = tuple(
            Decimal(value) for value in ("1.1", "1.2", "1.3", "1.4", "1.5")
        )
        head = tuple(
            Decimal(value) for value in ("1.2", "1.3", "1.4", "1.5", "1.6")
        )
        benchmark._paired_bootstrap_interval.cache_clear()
        self.addCleanup(benchmark._paired_bootstrap_interval.cache_clear)

        with localcontext() as context:
            context.prec = 2
            expected = benchmark._paired_bootstrap_interval_exhaustive(base, head)
            with mock.patch.object(
                benchmark,
                "_paired_bootstrap_interval_exhaustive",
                wraps=benchmark._paired_bootstrap_interval_exhaustive,
            ) as exhaustive:
                actual = benchmark._paired_bootstrap_interval(base, head)

        exhaustive.assert_called_once_with(base, head)
        self.assertEqual(actual, expected)

    def test_weighted_decimal_percentile_uses_nearest_rank(self) -> None:
        samples = ((Decimal("1"), 3), (Decimal("2"), 2))

        self.assertEqual(
            benchmark._weighted_decimal_percentile(samples, 50), Decimal("1")
        )
        self.assertEqual(
            benchmark._weighted_decimal_percentile(samples, 95), Decimal("2")
        )

    def test_method_verdict_requires_both_order_strata(self) -> None:
        cases = (
            (
                constant_sessions(
                    by_order={
                        "base-first": (10.0, 12.0),
                        "head-first": (10.0, 8.0),
                    }
                ),
                "stable",
            ),
            (
                constant_sessions(
                    by_order={
                        "base-first": (10.0, 12.0),
                        "head-first": (10.0, 10.0),
                    }
                ),
                "stable",
            ),
        )
        for sessions, expected in cases:
            with self.subTest(expected=expected):
                comparison = benchmark.build_comparison(sessions, CONTEXT)
                self.assertEqual(comparison["methods"][0]["verdict"], expected)

    def test_method_verdict_without_both_strata_is_stable(self) -> None:
        self.assertEqual(benchmark.method_verdict([]), "stable")

    def test_build_comparison_requires_percent_and_absolute_thresholds(self) -> None:
        cases = (
            (10.0, 11.0, "regression"),
            (10.0, 9.0, "improvement"),
            (1.0, 1.2, "stable"),
        )
        for base_ms, head_ms, expected in cases:
            with self.subTest(expected=expected):
                comparison = benchmark.build_comparison(
                    constant_sessions(base_ms, head_ms), CONTEXT
                )

                self.assertEqual(comparison["overall"], expected)
                self.assertEqual(comparison["methods"][0]["verdict"], expected)

    def test_near_threshold_output_remains_visibly_below_the_boundary(self) -> None:
        comparison = benchmark.build_comparison(
            constant_sessions(10.0, 10.99996), CONTEXT
        )
        method = comparison["methods"][0]

        self.assertEqual(method["verdict"], "stable")
        self.assertEqual(method["delta_ms"]["p50"], 0.9999)
        self.assertEqual(method["delta_percent"]["p50"], 9.99)
        self.assertEqual(
            method["strata"][0]["confidence_interval_95"]["delta_ms"]["p50"],
            {"lower": 0.9999, "upper": 1.0},
        )
        rendered = benchmark.render_markdown(
            benchmark.add_publication_state(
                comparison, CONTEXT, CURRENT_MAIN_SHA, CURRENT_PR_HEAD_SHA
            )
        )
        self.assertIn("+0.9999 ms (+9.99%)", rendered)

    def test_build_comparison_requires_p50_and_p95_evidence(self) -> None:
        sessions = constant_sessions()
        for session in sessions:
            session.samples["base"][benchmark.METHODS[0]] = [10.0] * 5 + [20.0] * 5
            session.samples["head"][benchmark.METHODS[0]] = [12.0] * 5 + [20.0] * 5

        comparison = benchmark.build_comparison(sessions, CONTEXT)

        self.assertEqual(comparison["methods"][0]["delta_ms"], {"p50": 2.0, "p95": 0.0})
        self.assertEqual(comparison["methods"][0]["verdict"], "stable")

    def test_build_comparison_recomputes_metrics_and_prioritizes_regressions(
        self,
    ) -> None:
        sessions = constant_sessions()
        for session in sessions:
            session.samples["head"][benchmark.METHODS[0]] = [12.0] * 10
            session.samples["head"][benchmark.METHODS[1]] = [8.0] * 10

        comparison = benchmark.build_comparison(sessions, CONTEXT)

        self.assertEqual(comparison["overall"], "regression")
        self.assertEqual(comparison["methods"][0]["sample_count"], 100)
        self.assertEqual(comparison["methods"][0]["session_count"], 10)
        self.assertEqual(comparison["methods"][0]["base"]["p95_ms"], 10.0)
        self.assertEqual(comparison["methods"][0]["head"]["p50_ms"], 12.0)
        self.assertEqual(comparison["methods"][0]["delta_ms"]["p50"], 2.0)
        self.assertEqual(comparison["methods"][0]["delta_percent"]["p50"], 20.0)
        self.assertEqual(comparison["methods"][0]["verdict"], "regression")
        self.assertEqual(comparison["methods"][1]["verdict"], "improvement")
        for stratum in comparison["methods"][0]["strata"]:
            self.assertEqual(stratum["session_count"], 5)
            self.assertEqual(stratum["sample_count"], 50)
            self.assertEqual(
                stratum["confidence_interval_95"]["delta_ms"]["p50"],
                {"lower": 2.0, "upper": 2.0},
            )

    def test_overall_verdict_covers_stable_and_improvement(self) -> None:
        for head_ms, expected in ((10.0, "stable"), (8.0, "improvement")):
            with self.subTest(expected=expected):
                comparison = benchmark.build_comparison(
                    constant_sessions(head_ms=head_ms), CONTEXT
                )

                self.assertEqual(comparison["overall"], expected)
                self.assertEqual(
                    {method["verdict"] for method in comparison["methods"]},
                    {expected},
                )

    def test_descriptive_metrics_are_means_of_session_percentiles(self) -> None:
        sessions = constant_sessions()
        for index, session in enumerate(sessions, start=1):
            for method in benchmark.METHODS:
                session.samples["base"][method] = [float(index)] * 10
                session.samples["head"][method] = [float(index + 1)] * 10

        comparison = benchmark.build_comparison(sessions, CONTEXT)

        self.assertEqual(comparison["methods"][0]["base"]["p50_ms"], 5.5)
        self.assertEqual(comparison["methods"][0]["head"]["p95_ms"], 6.5)


class MarkdownTests(unittest.TestCase):
    def test_markdown_escape_handles_table_markup_html_and_newlines(self) -> None:
        self.assertEqual(
            benchmark.markdown_escape("|`[]\\\r\n<&"),
            "\\|\\`\\[\\]\\\\<br>&lt;&amp;",
        )

    def test_render_markdown_formats_a_comparison_table(self) -> None:
        comparison = benchmark.add_publication_state(
            benchmark.build_comparison(constant_sessions(10.0, 12.0), CONTEXT),
            CONTEXT,
            CURRENT_MAIN_SHA,
            CURRENT_PR_HEAD_SHA,
        )
        comparison["methods"] = [comparison["methods"][0]]
        comparison["methods"][0]["name"] = "text|document"

        rendered = benchmark.render_markdown(comparison)

        expected_row = (
            "| text\\|document | 10 | 100 | 10.00 ms | 12.00 ms | "
            "+2.0000 ms (+20.00%) | 10.00 ms | 12.00 ms | "
            "+2.0000 ms (+20.00%) | regression |"
        )
        self.assertTrue(
            rendered.startswith("<!-- solar-lsp-benchmark -->\n## LSP benchmark\n")
        )
        self.assertIn("**Overall:** `regression`", rendered)
        self.assertIn(expected_row, rendered)
        self.assertIn(f"/commit/{CONTEXT.merge_candidate_sha}", rendered)
        self.assertIn(f"/commit/{CONTEXT.pr_head_sha}", rendered)
        self.assertIn(f"/commit/{CONTEXT.main_sha}", rendered)
        self.assertIn("main-merge-candidate", rendered)
        self.assertNotIn("Order-stratified paired bootstrap evidence", rendered)
        self.assertNotIn("| Metric | Order | Sessions |", rendered)
        self.assertNotIn("| text\\|document | base-first | 5 |", rendered)
        methodology = (
            "Verdicts change only when, in both run orders, the paired 95% confidence "
            "intervals for p50 and p95 lie entirely beyond both the 10% and 1.0 ms "
            "thresholds in the same direction. "
            f"[Methodology](https://github.com/{CONTEXT.workflow_repository}"
            "/blob/HEAD/benches/lsp/README.md#lsp-pull-request-benchmark)"
        )
        self.assertIn(methodology, rendered)
        self.assertNotIn("Base and Head values are means", rendered)
        self.assertNotIn("Criterion/CodSpeed", rendered)

    def test_stale_markdown_keeps_the_full_table_and_recommends_rerunning(self) -> None:
        frozen = benchmark.build_comparison(constant_sessions(), CONTEXT)
        cases = (
            (
                "5" * 40,
                CURRENT_PR_HEAD_SHA,
                "main-advanced",
                "frozen measurement for reference only",
                "Rerun the benchmark before merging",
            ),
            (
                CURRENT_MAIN_SHA,
                "6" * 40,
                "superseded",
                "historical measurement",
                "PR head used for the merge candidate has been replaced",
            ),
        )
        for current_main, current_head, freshness, message, guidance in cases:
            with self.subTest(freshness=freshness):
                comparison = benchmark.add_publication_state(
                    frozen, CONTEXT, current_main, current_head
                )
                rendered = benchmark.render_markdown(comparison)

                self.assertIn(f"Freshness: `{freshness}`", rendered)
                self.assertIn("| Metric | Sessions | Samples |", rendered)
                self.assertIn(message, rendered)
                self.assertIn(guidance, rendered)

    def test_render_markdown_escapes_inconclusive_reason(self) -> None:
        comparison = benchmark.inconclusive_comparison(
            CONTEXT, "bad | `artifact` <value>\nsecond line"
        )

        rendered = benchmark.render_markdown(comparison)

        self.assertIn(
            "Reason: bad \\| \\`artifact\\` &lt;value&gt;<br>second line",
            rendered,
        )
        self.assertNotIn("| Metric | Sessions | Samples |", rendered)


if __name__ == "__main__":
    unittest.main()
