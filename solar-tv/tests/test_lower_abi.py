"""Check real implicit-to-explicit MIR ABI entries."""

import json
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import abi_group, evaluate, native
from validator.common import Unsupported


class LowerAbiTests(unittest.TestCase):
    def test_real_static_abi_wrapper_and_changed_return(self):
        source = {
            "language": "Solidity",
            "sources": {
                "C.sol": {
                    "content": """
contract C {
    uint256 total;
    constructor() {}
    receive() external payable { pay(); }
    function pay() public payable { total = msg.value; }
    function forward(address target, uint256 amount) external payable {
        assembly {
            if iszero(call(gas(), target, amount, 0, 0, 0, 0)) { revert(0, 0) }
        }
    }
    function decimals() external pure returns (uint8) { return 18; }
    function echo(uint256 x) external pure returns (uint256) { return x; }
    function narrow(address x) external pure returns (address) { return x; }
    function pair(uint256 x) external pure returns (uint256, uint256) { return (x, 19); }
    function name() external pure returns (string memory) { return "Copper"; }
}
"""
                }
            },
            "settings": {
                "optimizer": {"enabled": True},
                "outputSelection": {
                    "*": {
                        "*": [
                            "abi",
                            "evm.bytecode.object",
                            "evm.deployedBytecode.object",
                        ]
                    }
                },
            },
        }
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            baseline, _ = evaluate.compile_input(SOLAR, source, folder)
            captured, _ = evaluate.compile_input(SOLAR, source, folder, capture=True)
            self.assertEqual(baseline, captured)
            row = next(
                row for row in evaluate.captures(folder) if row["pass"] == "lower-abi"
            )
            before, after = row["paths"]
            report = folder / "proof.json"

            def check(function, candidate, *flags):
                result = subprocess.run(
                    [
                        CLI,
                        "mir",
                        before,
                        candidate,
                        "--function",
                        function,
                        "--solar",
                        SOLAR,
                        "--timeout",
                        "30",
                        "--report",
                        report,
                        *flags,
                    ],
                    text=True,
                    capture_output=True,
                    timeout=90,
                    check=False,
                )
                return result, json.loads(report.read_text())

            scopes = {
                **dict.fromkeys(
                    ("decimals", "echo", "narrow", "pair"), "static-abi-entry"
                ),
                "name": "bytes-abi-entry",
                "constructor": "empty-constructor-abi-entry",
                "pay": "void-abi-effect-tree",
                "receive": "void-abi-effect-tree",
                "forward": "abi-contextual-effects",
            }
            entry_reports = []
            for function, scope in scopes.items():
                with self.subTest(function=function):
                    result, proof = check(function, after)
                    self.assertEqual(
                        result.returncode, 0, result.stdout + result.stderr
                    )
                    self.assertEqual(proof["evidence"], "kernel-proof")
                    certificate = f"kernel-checked-{scope}"
                    self.assertEqual(proof["region"]["proof"], certificate)
                    self.assertEqual(
                        proof["import_correspondence"][0]["kind"],
                        certificate,
                    )
                    saved = folder / f"{function}.json"
                    saved.write_bytes(report.read_bytes())
                    entry_reports.append(saved)

            replay = native.default(SOLAR)
            module_report = abi_group.compose(
                row["paths"], entry_reports, replay, "cancun", 30
            )
            self.assertEqual(module_report["status"], "verified")
            self.assertEqual(set(module_report["entry_reports"]), set(scopes))
            with self.assertRaisesRegex(Unsupported, "every public entry"):
                abi_group.compose(
                    row["paths"], entry_reports[:-1], replay, "cancun", 30
                )

            wrong_gas = folder / "wrong-gas.mir"
            changed, replacements = re.subn(
                r"(?m)^(    v\d+ = call )v\d+,", r"\g<1>1,", after.read_text(), count=1
            )
            self.assertEqual(replacements, 1)
            wrong_gas.write_text(changed)
            result, proof = check("forward", wrong_gas)
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertEqual(proof["status"], "unsupported")

            broken = folder / "broken.mir"
            changed = after.read_text().replace("mstore 0, 18", "mstore 0, 19", 1)
            self.assertNotEqual(changed, after.read_text())
            broken.write_text(changed)
            result, proof = check("decimals", broken)
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertEqual(proof["status"], "unsupported")

            fixture = folder / "world.json"
            fixture.write_text('{"version": 1}\n')
            replayed = subprocess.run(
                [
                    CLI,
                    "mir",
                    after,
                    broken,
                    "--function",
                    "decimals",
                    "--world",
                    fixture,
                    "--solar",
                    SOLAR,
                    "--timeout",
                    "30",
                    "--report",
                    report,
                ],
                text=True,
                capture_output=True,
                timeout=90,
                check=False,
            )
            self.assertEqual(replayed.returncode, 1, replayed.stdout + replayed.stderr)
            proof = json.loads(report.read_text())
            self.assertEqual(proof["status"], "counterexample")
            self.assertEqual(
                proof["witness"]["observations"],
                proof["witness"]["native_observations"],
            )
