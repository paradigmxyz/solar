"""Gas-policy configuration and bounded counterexample proposals.

Lean defines and proves the prices. Python's estimates only select concrete
budgets to try; they cannot verify a rewrite or certify a counterexample.
"""

from . import memory
from .common import Unsupported

VERSIONS = {"cancun", "prague", "osaka"}
UNFOLD = (
    "EVM.Gas.execute, EVM.Gas.finish, EVM.Gas.cost, EVM.Gas.baseCost, "
    "EVM.Gas.pureCost, EVM.Gas.exponentCost, EVM.Gas.byteLength, EVM.Gas.touched, "
    "EVM.Gas.expansionCost, EVM.Gas.halted, "
    "EVM.Gas.exhausted, EVM.Gas.refines"
)


def validate(mode: str | None, version: str) -> None:
    if mode is not None and mode not in {"exact", "refine"}:
        raise Unsupported("gas policy must be `exact` or `refine`")
    if mode is not None and version not in VERSIONS:
        raise Unsupported("gas auditing supports Cancun, Prague and Osaka")


def estimate(line: str) -> int:
    if line in {"gas", "msize", "pop", "calldatasize", "returndatasize"}:
        return 2
    if line in {"mul", "div", "sdiv", "mod", "smod", "signextend"}:
        return 5
    if line in {"addmod", "mulmod"}:
        return 8
    if line == "exp":
        # The exponent's bytes add 50 gas each; the estimate takes the minimum.
        return 10
    if line in {"push 0", "push 0x0"}:
        return 2
    return 3


def witnesses(inputs: list[str], before: int, after: int) -> list[list[int]]:
    index = inputs.index("gas")
    patterns = [[0] * len(inputs), *memory.witnesses(inputs)]
    result = []
    for pattern in patterns:
        for budget in sorted({before, max(before, after) + 1}):
            values = pattern.copy()
            values[index] = budget
            result.append(values)
    return result
