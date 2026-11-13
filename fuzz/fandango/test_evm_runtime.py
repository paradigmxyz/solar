from __future__ import annotations

import unittest

import evm_runtime as evm


def word(value: int) -> str:
    return f"{value:064x}"


class AnalysisFactTests(unittest.TestCase):
    def test_decodes_a_violated_fact(self) -> None:
        fact = (3 << 104) | (1 << 96) | (1 << 64) | (2 << 32)
        data = evm.ANALYSIS_FACT_VIOLATED + word(fact) + word(20) + word(10)
        self.assertEqual(
            evm.analysis_fact(data),
            {
                "pass": "sccp",
                "function": 1,
                "block": 2,
                "value": "v0",
                "claim": "v0 < 0xa",
                "observed": "0x14",
            },
        )

    def test_names_arguments(self) -> None:
        fact = (4 << 104) | (2 << 96) | (1 << 31) | 1
        data = evm.ANALYSIS_FACT_VIOLATED + word(fact) + word(9) + word(8)
        decoded = evm.analysis_fact(data)
        assert decoded is not None
        self.assertEqual(decoded["value"], "arg1")
        self.assertEqual(decoded["pass"], "check-elim")
        self.assertEqual(decoded["claim"], "arg1 <= 0x8")

    def test_ignores_other_revert_data(self) -> None:
        self.assertIsNone(evm.analysis_fact("0x4e487b71" + word(0x11)))
        self.assertIsNone(evm.analysis_fact(evm.ANALYSIS_FACT_VIOLATED))
        self.assertIsNone(evm.analysis_fact(None))


if __name__ == "__main__":
    unittest.main()
