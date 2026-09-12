#!/usr/bin/env python3
"""Correctness checks for the real-project protocol benchmark and comparisons."""

import copy
import json
import os
import sys
import tempfile
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import interactive


class InteractiveTests(unittest.TestCase):
    def test_pinned_project_and_ranged_edit(self):
        identity = interactive.project_identity()
        self.assertEqual(len(identity["files"]), 28)
        scenario = interactive.Scenario(interactive.PROJECT)
        self.assertEqual(
            scenario.edit_range,
            {"start": {"line": 28, "character": 30}, "end": {"line": 28, "character": 38}},
        )
        self.assertEqual(
            scenario.requests["completion"]["position"],
            {"line": 120, "character": 20},
        )

    def test_positions_count_utf16_and_reject_ambiguous_anchors(self):
        self.assertEqual(interactive.anchor("abc\n🙂 hello", "hello"), {"line": 1, "character": 3})
        with self.assertRaisesRegex(RuntimeError, "missing or ambiguous"):
            interactive.anchor("token token", "token")

    def test_framing_retains_following_message(self):
        first = {"jsonrpc": "2.0", "id": 1, "result": "🙂"}
        second = {"jsonrpc": "2.0", "method": "workspace/diagnostic/refresh"}
        read_fd, write_fd = os.pipe()
        with os.fdopen(read_fd, "rb", buffering=0) as source:
            client = interactive.Rpc.__new__(interactive.Rpc)
            client.process = SimpleNamespace(stdout=source)
            client.buffer = bytearray()
            frames = bytearray()
            for message in (first, second):
                body = json.dumps(message, ensure_ascii=False).encode()
                frames += f"Content-Length: {len(body)}\r\n\r\n".encode() + body
            try:
                # Exercise a header prefix already buffered from an earlier read.
                client.buffer.extend(frames[:7])
                os.write(write_fd, frames[7:])
                self.assertEqual(client.read(time.monotonic() + 1), first)
                self.assertEqual(client.read(time.monotonic() + 1), second)
            finally:
                os.close(write_fd)

    def test_unresponsive_server_has_a_deadline(self):
        read_fd, write_fd = os.pipe()
        with os.fdopen(read_fd, "rb", buffering=0) as source:
            client = interactive.Rpc.__new__(interactive.Rpc)
            client.process = SimpleNamespace(stdout=source)
            client.buffer = bytearray()
            try:
                with self.assertRaisesRegex(RuntimeError, "timed out"):
                    client.read(time.monotonic() + .005)
            finally:
                os.close(write_fd)

    def test_normalization_preserves_semantics(self):
        root = Path(tempfile.gettempdir()).resolve() / "project"
        original = {"uri": (root / "src/A.sol").as_uri(), "range": {"line": 2}, "items": ["hello"]}
        normalized = interactive.normalize(original, root)
        self.assertEqual(normalized, {"uri": "file:///PROJECT/src/A.sol", "range": {"line": 2}, "items": ["hello"]})
        changed = copy.deepcopy(normalized)
        changed["range"]["line"] = 3
        self.assertNotEqual(interactive.digest(changed), interactive.digest(normalized))

    def test_percentiles_are_nearest_rank(self):
        self.assertEqual(interactive.percentile(list(range(1, 21)), .5), 10)
        self.assertEqual(interactive.percentile(list(range(1, 21)), .95), 19)
        self.assertEqual(interactive.percentile([1, 100], .95), 100)

    @staticmethod
    def roles(ratio):
        roles = {"base": {"sessions": []}, "candidate": {"sessions": []}}
        for index in range(10):
            for role, multiplier in (("base", 1), ("candidate", ratio)):
                roles[role]["sessions"].append({
                    "session": index + 1,
                    "order": "base-candidate" if index % 2 == 0 else "candidate-base",
                    "initialization": {
                        "params": {"rootUri": "file:///PROJECT"},
                        "result": {"capabilities": {"textDocumentSync": 1}, "serverInfo": {"name": "solar", "version": role}},
                    },
                    "records": [{"response_sha256": "same"}],
                    "summary": {"completion": {"p50_ms": 10 * multiplier, "p95_ms": 20 * multiplier}},
                })
        return roles

    def test_initialization_parity_allows_version_only_change(self):
        roles = self.roles(.7)
        roles["candidate"]["sessions"][0]["initialization"]["result"]["serverInfo"]["version"] = "different-build"
        original = copy.deepcopy(roles)
        result = interactive.compare(roles)["methods"]["completion"]
        self.assertTrue(result["over_20_percent_in_both_orders"])
        self.assertEqual(roles, original)

    def test_initialization_parity_rejects_semantic_changes(self):
        for keys, value in (
            (("params", "rootUri"), "file:///OTHER"),
            (("result", "capabilities", "textDocumentSync"), 2),
            (("result", "serverInfo", "name"), "other-server"),
        ):
            with self.subTest(keys=keys):
                roles = self.roles(.7)
                target = roles["candidate"]["sessions"][0]["initialization"]
                for key in keys[:-1]:
                    target = target[key]
                target[keys[-1]] = value
                with self.assertRaisesRegex(RuntimeError, "initialize parity failed"):
                    interactive.compare(roles)

    def test_profile_is_rejected_for_paired_mode(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "must-not-be-created"
            argv = ["interactive.py", "--base", "missing-base", "--candidate", "missing-candidate", "--profile", "completion", "--output", str(output)]
            with patch.object(sys, "argv", argv):
                with self.assertRaisesRegex(RuntimeError, "paired acceptance cannot use profiling"):
                    interactive.main()
            self.assertFalse(output.exists())

    def test_paired_threshold_requires_both_orders_and_both_percentiles(self):
        result = interactive.compare(self.roles(.7))["methods"]["completion"]
        self.assertTrue(result["over_20_percent_in_both_orders"])
        self.assertEqual(result["p50_ms"]["delta_ms"], -3)
        result = interactive.compare(self.roles(.81))["methods"]["completion"]
        self.assertFalse(result["over_20_percent_in_both_orders"])
        roles = self.roles(.7)
        for session in roles["candidate"]["sessions"]:
            if session["order"] == "candidate-base":
                session["summary"]["completion"]["p95_ms"] = 20
        result = interactive.compare(roles)["methods"]["completion"]
        self.assertFalse(result["over_20_percent_in_both_orders"])

    def test_response_mismatch_rejects_comparison(self):
        roles = self.roles(.5)
        roles["candidate"]["sessions"][0]["records"][0]["response_sha256"] = "changed"
        with self.assertRaisesRegex(RuntimeError, "parity failed"):
            interactive.compare(roles)


if __name__ == "__main__":
    unittest.main()
