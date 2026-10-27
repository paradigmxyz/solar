#!/usr/bin/env python3

from __future__ import annotations

import gzip
import json
import pathlib
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import run_symbolic_campaign as campaign


class CampaignTests(unittest.TestCase):
    def test_passes_arguments_after_separator_to_every_run(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            source = root / "C.sol"
            source.write_text("contract C {}\n")
            runs = []

            def run(source, contract, args, extra):
                runs.append((source.name, contract, extra))
                return {
                    "source": source.name,
                    "contract": contract,
                    "seconds": 0.0,
                    "status": "mismatch" if contract == "B" else "bounded_agreement",
                    "checks": [
                        {
                            "label": "f()",
                            "status": "mismatch"
                            if contract == "B"
                            else "bounded_agreement",
                            "counterexample": {"calls": [{"calldata": "0x26121ff0"}]},
                        }
                    ],
                }

            with (
                patch.object(
                    campaign, "_deployable_contracts", return_value=["A", "B"]
                ),
                patch.object(campaign, "_run", side_effect=run),
                patch("sys.stdout"),
                patch("sys.stderr"),
            ):
                code = campaign.main(
                    [str(source), "--out", str(root / "out"), "--", "--depth", "2"]
                )

            self.assertEqual(code, 1)
            self.assertEqual(
                sorted(runs),
                [("C.sol", "A", ["--depth", "2"]), ("C.sol", "B", ["--depth", "2"])],
            )
            rows = [
                json.loads(line)
                for line in (root / "out" / "results.jsonl").read_text().splitlines()
            ]
            self.assertEqual([row["contract"] for row in rows], ["A", "B"])
            summary = (root / "out" / "summary.md").read_text()

        self.assertIn("| contracts | 1 | 1 | 0 |", summary)
        self.assertIn("| call sequences | 1 | 1 | 0 |", summary)
        self.assertIn("- `B` `f()` differs in an assertion: `0x26121ff0`", summary)

    def test_fails_when_a_source_has_no_contract(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            with (
                patch.object(campaign, "_deployable_contracts", return_value=[]),
                patch.object(campaign, "_run") as run,
                patch("sys.stderr"),
            ):
                code = campaign.main([str(root / "C.sol"), "--out", str(root / "out")])

        self.assertEqual(code, 2)
        run.assert_not_called()

    def test_compiles_projects_once_and_skips_tests(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            archive = root / "p.json.gz"
            archive.write_bytes(
                gzip.compress(
                    json.dumps(
                        {
                            "language": "Solidity",
                            "sources": {
                                "src/A.sol": {"content": ""},
                                "test/A.t.sol": {"content": ""},
                            },
                            "settings": {"viaIR": True},
                        }
                    ).encode()
                )
            )
            output = {
                "contracts": {
                    "src/A.sol": {
                        "A": {"evm": {"bytecode": {"object": "6000"}}},
                        "I": {"evm": {"bytecode": {"object": ""}}},
                        "L": {"evm": {"bytecode": {"object": "6000"}}},
                    }
                },
                "sources": {
                    "src/A.sol": {
                        "ast": {
                            "nodes": [
                                {
                                    "nodeType": "ContractDefinition",
                                    "name": "L",
                                    "contractKind": "library",
                                }
                            ]
                        }
                    }
                },
            }
            inputs = []

            def compile(command, input, **kwargs):
                inputs.append((command[0], json.loads(input)))
                return subprocess.CompletedProcess(command, 0, json.dumps(output), "")

            args = campaign.argparse.Namespace(
                out=root / "out",
                solc="solc",
                solar="solar",
                exclude=campaign.PROJECT_EXCLUDE,
            )
            with patch.object(campaign.subprocess, "run", side_effect=compile):
                jobs = campaign._project_contracts(archive, args)

        self.assertEqual([command for command, _ in inputs], ["solc", "solar"])
        settings = inputs[0][1]["settings"]
        self.assertTrue(settings["viaIR"])
        self.assertEqual(settings["evmVersion"], "osaka")
        self.assertEqual(list(settings["outputSelection"]), ["src/A.sol"])
        [(unit, contract, attempts)] = jobs
        self.assertEqual((unit.as_posix(), contract), ("src/A.sol", "A"))
        self.assertEqual(attempts[0], "--solc-attempt")
        self.assertTrue(attempts[1].endswith("projects/p/solc"))
        self.assertEqual(attempts[2], "--solar-attempt")

    def test_reports_abi_mismatches_skips_and_limitations(self) -> None:
        summary = campaign._summary(
            [
                {
                    "contract": "A",
                    "status": "mismatch",
                    "differs_in": "abi",
                    "checks": [],
                    "skipped": [{"signature": "g(function)", "reason": "unsupported"}],
                    "limitations": ["contracts created with CREATE2 are not compared"],
                },
            ]
        )

        self.assertIn("- `A` `external interface` differs in abi: ``", summary)
        self.assertIn("- `A` `g(function)`", summary)
        self.assertIn(
            "| 1 | contracts created with CREATE2 are not compared |", summary
        )

    def test_summarizes_incomplete_reasons(self) -> None:
        summary = campaign._summary(
            [
                {
                    "contract": "A",
                    "status": "incomplete",
                    "reason": "contracts with immutables require constructor execution",
                    "checks": [],
                },
                {
                    "contract": "B",
                    "status": "incomplete",
                    "checks": [
                        {
                            "label": "f()",
                            "status": "incomplete",
                            "reason": "path limit 12",
                        },
                        {
                            "label": "g()",
                            "status": "incomplete",
                            "reason": "path limit 9",
                        },
                    ],
                },
            ]
        )

        self.assertIn("| 2 | path limit N |", summary)
        self.assertIn(
            "| 1 | contracts with immutables require constructor execution |", summary
        )


if __name__ == "__main__":
    unittest.main()
