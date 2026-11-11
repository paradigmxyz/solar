"""Memory state, byte order, expansion and payloads through real IR import."""

import re
import tempfile
import unittest
from pathlib import Path

from test_cli import ROOT, check


class MemoryTests(unittest.TestCase):
    def pair(self, layer, before, after, *, height=0, params="", status=1):
        with tempfile.TemporaryDirectory() as tmp:
            paths = []
            for name, body in [("before", before), ("after", after)]:
                path = Path(tmp) / f"{name}.{layer}"
                text = (
                    f"@module Memory\nfn @f({params}) -> i256 {{\nbb0:\n{body}\n}}\n"
                    if layer == "mir"
                    else f"@module Memory\nbb0 [stack={height}]:\n{body}\n"
                )
                path.write_text(text)
                paths.append(path)
            result = check(layer, *paths)
        self.assertEqual(result.returncode, status, result.stderr + result.stdout)
        return result

    def test_real_dse_pair_and_broken_expansion(self):
        folder = ROOT / "examples/mir/memory"
        correct = check("mir", folder / "before.mir", folder / "after.mir")
        self.assertEqual(correct.returncode, 0, correct.stderr)
        broken = check("mir", folder / "before.mir", folder / "broken.mir")
        self.assertEqual(broken.returncode, 1, broken.stderr)
        self.assertIn("before = return=32, msize=32", broken.stdout)
        self.assertIn("after  = return=64, msize=64", broken.stdout)
        self.assertIn("Active memory size changed from 32 to 64 bytes", broken.stdout)

    def test_real_evm_pair_and_wrong_store_address(self):
        folder = ROOT / "examples/evmir/memory"
        correct = check("evmir", folder / "before.evmir", folder / "after.evmir")
        self.assertEqual(correct.returncode, 0, correct.stderr)
        broken = check("evmir", folder / "before.evmir", folder / "broken.evmir")
        self.assertEqual(broken.returncode, 1, broken.stderr)
        self.assertIn("Returned byte payload changed", broken.stdout)
        self.assertIn("stack0 = 1", broken.stdout)

    def test_unaligned_loads_and_overlapping_byte_stores(self):
        for offset, read, byte_offset, byte_value in [
            (1, 0, 31, 0x1FF),
            (0, 1, 32, 0x1234),
            (33, 32, 63, 0x55),
        ]:
            with self.subTest(offset=offset):
                word = (1 << 255) + 0x1234
                memory = bytearray(256)
                memory[offset : offset + 32] = word.to_bytes(32, "big")
                memory[byte_offset] = byte_value & 255
                expected = int.from_bytes(memory[read : read + 32], "big")
                code = f"mstore {offset}, {word}\nmstore8 {byte_offset}, {byte_value}\nv0 = mload {read}\n"
                result = self.pair("mir", code + "ret v0", code + "ret 7")
                self.assertIn(f"before = return={expected},", result.stdout)

    def test_incoming_memory_is_symbolic(self):
        result = self.pair("mir", "v0 = mload 0\nret v0", "v0 = mload 0\nret 0")
        self.assertIn("memory0 = 1", result.stdout)
        self.assertIn("memoryWords = 8", result.stdout)
        self.assertIn("before = return=1,", result.stdout)

    def test_mload_expands_memory_even_when_its_value_is_dead(self):
        result = self.pair("mir", "v0 = mload 32\nret 7", "ret 7")
        self.assertIn("before = return=7, msize=64", result.stdout)
        self.assertIn("after  = return=7, msize=0", result.stdout)

    def test_stored_bytes_remain_live_at_internal_return(self):
        result = self.pair("mir", "mstore8 1, 7\nret 0", "mstore8 2, 7\nret 0")
        self.assertIn("Memory contents changed in word at byte offset 0", result.stdout)

    def test_evm_edge_keeps_memory_and_active_size(self):
        suffix = "push bb1\njump\nbb1:\nstop"
        result = self.pair(
            "evmir",
            "push 7\npush 1\nmstore8\n" + suffix,
            "push 7\npush 2\nmstore8\n" + suffix,
        )
        self.assertIn("before = bb1, stack=[], msize=32", result.stdout)
        self.assertIn("Memory contents changed", result.stdout)

    def test_revert_payload_and_failure_to_success(self):
        before = "push 0x1ff\npush 0\nmstore8\npush 1\npush 0\nrevert"
        result = self.pair("evmir", before, before.replace("0x1ff", "7"))
        self.assertIn("before = revert(data=[255])", result.stdout)
        self.assertIn("after  = revert(data=[7])", result.stdout)
        result = self.pair("evmir", before, before.replace("revert", "return"))
        self.assertIn("Revert changed to", result.stdout)

    def test_zero_size_output_ignores_maximal_offset(self):
        # A zero-sized output must not overflow, expand or read the huge offset.
        self.pair("evmir", f"push 0\npush {2**256 - 1}\nreturn", "stop", status=0)

    def test_dynamic_and_out_of_prefix_addresses_are_inconclusive(self):
        for code, params, diagnostic in [
            ("v0 = mload arg0\nret v0", "arg0: i256", "must be constant"),
            ("v0 = mload 225\nret v0", "", "256-byte modeled prefix"),
            ("mstore8 256, 1\nret 0", "", "256-byte modeled prefix"),
        ]:
            with self.subTest(code=code):
                result = self.pair("mir", code, code, params=params, status=2)
                self.assertIn(diagnostic, result.stderr)
        for code in ["mload\npop\nstop", "push 0\nreturn"]:
            result = self.pair("evmir", code, code, height=1, status=2)
            self.assertIn("must be constant", result.stderr)

    def test_new_counterexamples_include_replay_and_memory_inputs(self):
        result = self.pair("mir", "mstore8 0, 7\nret 0", "mstore8 0, 8\nret 0")
        self.assertIn("Replay\n", result.stdout)
        assignments = dict(
            re.findall(r"^  (\w+) = (\d+)$", result.stdout, re.MULTILINE)
        )
        self.assertEqual([assignments[f"memory{i}"] for i in range(8)], ["0"] * 8)
        self.assertEqual(assignments["memoryWords"], "0")

    def test_branch_memory_is_local_to_each_path(self):
        before = (
            "jumpi arg0, bb1, bb2\n"
            "bb1:\nmstore 0, 1\nmstore 0, 7\njump bb3\n"
            "bb2:\nmstore 0, 8\njump bb3\n"
            "bb3:\nv0 = msize\nret v0"
        )
        self.pair(
            "mir",
            before,
            before.replace("mstore 0, 1\n", ""),
            params="arg0: i1",
            status=0,
        )

    def test_revert_discards_private_memory(self):
        failing = "v0 = checked_sub uint256, 0, 1\nret 7"
        self.pair("mir", "mstore 0, 7\n" + failing, failing, status=0)
