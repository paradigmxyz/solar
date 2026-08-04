"""Inline contracts and calls used by the codegen runtime benchmark."""

# Adapted from walnuthq/solidity-compiler-benchmarks at
# 01209d2b8ac81645b92e3ef801b5bcdfd61bfd69 under Apache-2.0.

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Sequence


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
TESTDATA_ROOT = REPOSITORY_ROOT / "testdata"


@dataclass(frozen=True)
class TestCase:
    test_id: str
    description: str
    source_code: str
    contract_name: str
    test_calls: Sequence[tuple[str, Sequence[str]]]
    constructor_args: Sequence[str] = field(default_factory=tuple)


def source(name: str) -> str:
    return (TESTDATA_ROOT / name).read_text()


TEST_CASES: Sequence[TestCase] = (
    TestCase(
        test_id="factorial",
        description="Factorial with storage caching opportunity",
        source_code=source("Factorial.sol"),
        contract_name="FactorialStorage",
        test_calls=(
            ("computeFactorial(uint256)", ("5",)),
            ("computeFactorial(uint256)", ("10",)),
            ("computeFactorial(uint256)", ("20",)),
        ),
    ),
    TestCase(
        test_id="counter",
        description="Simple counter with setter and increment",
        source_code=(TESTDATA_ROOT / "Counter.sol").read_text(),
        contract_name="Counter",
        test_calls=(
            ("setNumber(uint256)", ("10",)),
            ("increment()", ()),
            ("setNumber(uint256)", ("50",)),
        ),
    ),
    TestCase(
        test_id="sum-array",
        description="Sum computation with storage writes",
        source_code=source("SumArray.sol"),
        contract_name="SumStorage",
        test_calls=(
            ("sumRange(uint256,uint256)", ("1", "10")),
            ("sumRange(uint256,uint256)", ("1", "50")),
            ("sumRange(uint256,uint256)", ("1", "100")),
        ),
    ),
    TestCase(
        test_id="arithmetic",
        description="Mixed arithmetic operations",
        source_code=source("Arithmetic.sol"),
        contract_name="Arithmetic",
        test_calls=(
            ("compute(uint256,uint256,uint256)", ("100", "3", "10")),
            ("compute(uint256,uint256,uint256)", ("100", "3", "50")),
        ),
    ),
)
