"""Guard against counting incorrect executions as optimization wins."""

import copy
import unittest

import benchmark


class ComparisonTests(unittest.TestCase):
    def setUp(self):
        self.case = {
            "variants": {
                "solc-upstream-legacy": {"matches_oracle": True, "opcode_gas": 100},
                "solc-upstream-ir": {"matches_oracle": True, "opcode_gas": 120},
                "solc-safe-legacy": {"matches_oracle": True, "opcode_gas": 200},
                "solc-safe-ir": {"matches_oracle": True, "opcode_gas": 150},
                "solar-upstream": {"matches_oracle": True, "opcode_gas": 90},
                "solar-safe": {"matches_oracle": True, "opcode_gas": 80},
            }
        }

    def test_reference_uses_cheaper_upstream_solc_pipeline(self):
        self.assertEqual(benchmark.comparison_delta(self.case), -20)
        self.case["variants"]["solc-upstream-ir"]["opcode_gas"] = 70
        self.assertEqual(benchmark.comparison_delta(self.case), 10)

    def test_any_incorrect_leg_excludes_case(self):
        for label in self.case["variants"]:
            with self.subTest(variant=label):
                case = copy.deepcopy(self.case)
                case["variants"][label] = {"matches_oracle": False, "opcode_gas": 0}
                self.assertIsNone(benchmark.comparison_delta(case))

    def test_missing_correctness_evidence_is_not_accepted(self):
        del self.case["variants"]["solar-safe"]["matches_oracle"]
        with self.assertRaises(KeyError):
            benchmark.comparison_delta(self.case)


class SafetyAuditTests(unittest.TestCase):
    def test_nested_unsafe_blocks_in_dependencies_are_rejected(self):
        output = {
            "sources": {
                "Harness.sol": {"ast": {"nodeType": "SourceUnit"}},
                "src/Dependency.sol": {
                    "ast": {
                        "nodes": [
                            {
                                "body": {
                                    "statements": [
                                        {"nodeType": "UncheckedBlock"},
                                        {"nodeType": "InlineAssembly"},
                                    ]
                                }
                            }
                        ]
                    }
                },
            }
        }
        self.assertEqual(
            benchmark.safety_violations(output),
            [
                ("src/Dependency.sol", "UncheckedBlock"),
                ("src/Dependency.sol", "InlineAssembly"),
            ],
        )

    def test_comments_do_not_create_false_violations(self):
        output = {
            "sources": {
                "src/Safe.sol": {
                    "ast": {
                        "nodeType": "SourceUnit",
                        "documentation": "No assembly or unchecked blocks.",
                    }
                }
            }
        }
        self.assertEqual(benchmark.safety_violations(output), [])

    def test_missing_ast_is_an_error(self):
        with self.assertRaises(KeyError):
            benchmark.safety_violations({"sources": {"src/Missing.sol": {}}})


if __name__ == "__main__":
    unittest.main()
