"""Case catalog and workloads for the codegen runtime benchmark."""

# Adapted from walnuthq/solidity-compiler-benchmarks at
# 01209d2b8ac81645b92e3ef801b5bcdfd61bfd69 under Apache-2.0.

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass, field
from pathlib import Path

from common import PROJECTS_ROOT, TESTDATA_ROOT

DEFAULT_SENDER = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
DEFAULT_SPENDER = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8"
DEFAULT_THIRD = "0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC"
DEFAULT_FOURTH = "0x90F79bf6EB2c4f870365E785982E1f101E93b906"
ZERO_ADDRESS = "0x0000000000000000000000000000000000000000"
MAX_UINT256 = str((1 << 256) - 1)
MAX_UINT128 = str((1 << 128) - 1)
EDGE_BYTES32 = "0x" + "ff" * 31 + "f0"
MIXED_BYTES32 = "0x" + "ff" * 30 + "0000"
SIGNED_HASH = "0x7d768af957ef8cbf6219a37e743d5546d911dae3e46449d8a5810522db2ef65e"

# Exercise both array iteration and bytes tails around the ABI word boundary.
GOVERNOR_PROPOSALS = tuple(
    (
        f"hash-proposal-{length}-elements",
        (
            "[" + ",".join(f"0x{i + 1:040x}" for i in range(length)) + "]",
            "[" + ",".join(str(i) for i in range(length)) + "]",
            "["
            + ",".join("0x" + "42" * (0, 1, 31, 32, 33)[i % 5] for i in range(length))
            + "]",
            "0x" + "42" * 32,
        ),
    )
    for length in (1, 2, 8)
)


# Nonuniform bytes exercise both nibbles and the ABI/loop word boundaries.
ENCODING_INPUTS = tuple(
    (f"mixed-{length}", "0x" + bytes((i * 37 + 11) % 256 for i in range(length)).hex())
    for length in (0, 1, 15, 16, 31, 32, 33, 63, 64, 65, 256)
) + (
    ("ascii-65", "0x" + "41" * 65),
    ("high-first", "0x80" + "41" * 64),
    ("high-boundary", "0x" + "41" * 31 + "80" + "41" * 33),
    ("high-tail", "0x" + "41" * 64 + "80"),
)
# Additional boundaries and long scans are checked independently of the original
# short-input tuning set, using a different nonuniform byte pattern.
ENCODING_INPUTS += tuple(
    (f"boundary-{length}", "0x" + bytes((i * 73 + 19) % 256 for i in range(length)).hex())
    for length in (2, 7, 8, 17, 47, 95, 127, 128, 129, 255, 257, 511, 512, 513, 1023, 1024)
) + tuple(
    (f"ascii-{length}", "0x" + "41" * length)
    for length in (0, 1, 2, 31, 32, 33, 63, 64, 127, 128, 129, 256, 257, 1024)
) + tuple(
    (f"high-{position}-{length}", "0x" + "41" * offset + "80" + "41" * (length - offset - 1))
    for length in (257, 1024)
    for position, offset in (("first", 0), ("boundary", 31), ("tail", length - 1))
)


# Isolated decimal and sorting calls avoid upstream assertions and gas-dependent
# memory stress. Use the same inputs for timing and return-value comparison.
ALGORITHM_INPUTS = tuple(
    (f"decimal-{value}", "decimal(uint256)", "string", str(value))
    for value in (
        0, 1, 9, 10, 99, 100, (1 << 64) - 1, 10**31 - 1, 10**31,
        (1 << 255) - 1, (1 << 256) - 1,
    )
) + tuple(
    (f"signed-{value}", "signedDecimal(int256)", "string", str(value))
    for value in (-1, -10, -(1 << 255), 0, 1, (1 << 255) - 1)
) + tuple(
    (
        f"{method}-{pattern}-{length}",
        f"{method}(uint256[])",
        "uint256[]",
        "[" + ",".join(map(str, values)) + "]",
    )
    for length in (0, 1, 2, 4, 12, 13, 32, 64)
    for pattern, values in (
        ("sorted", range(length)),
        ("reversed", range(length - 1, -1, -1)),
        ("equal", [1] * length),
        ("mixed", [(i * 73 + 7) % 37 for i in range(length)]),
    )
    for method in ("insertion", "sort")
) + tuple(
    (label, "decode(string)", "bytes", value)
    for label, value in (
        ("base64-empty", ""),
        ("base64-one", "YQ=="),
        ("base64-three", "YWJj"),
        ("base64-long", "YWJj" * 32),
    )
)


@dataclass(frozen=True)
class RuntimeCheck:
    label: str
    signature: str
    args: Sequence[str] = field(default_factory=tuple)


@dataclass(frozen=True)
class GasCall:
    label: str
    signature: str
    args: Sequence[str] = field(default_factory=tuple)
    repeat: int = 1


@dataclass(frozen=True)
class TestCase:
    test_id: str
    description: str
    contract_name: str
    test_calls: Sequence[tuple[str, Sequence[str]]] = field(default_factory=tuple)
    source_code: str | None = None
    source_name: str = ""
    project: str = ""
    project_file: str | None = None
    settings_profile: str = ""
    source: str = ""
    gas_calls: Sequence[GasCall] = field(default_factory=tuple)
    constructor_args: Sequence[str] = field(default_factory=tuple)
    constructor_sig: str | None = None
    runtime_checks: Sequence[RuntimeCheck] = field(default_factory=tuple)
    min_solc: str | None = None
    max_solc: str | None = None
    suite: str = "micro"
    # Compile every contract in the project input instead of one selected
    # contract. Compile-time only: no bytecode is extracted, so size, gas,
    # and runtime checks do not apply.
    whole_project: bool = False

    @property
    def project_path(self) -> Path:
        if self.project_file is None:
            raise ValueError(f"inline case {self.test_id} has no project archive")
        return PROJECTS_ROOT / self.project_file


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
        source_name="Factorial.sol",
        runtime_checks=(RuntimeCheck("result", "getResult()(uint256)"),),
    ),
    TestCase(
        test_id="counter",
        description="Simple counter with setter and increment",
        source_code=source("Counter.sol"),
        contract_name="Counter",
        test_calls=(
            ("setNumber(uint256)", ("10",)),
            ("increment()", ()),
            ("setNumber(uint256)", ("50",)),
        ),
        source_name="Counter.sol",
        runtime_checks=(RuntimeCheck("number", "number()(uint256)"),),
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
        source_name="SumArray.sol",
        runtime_checks=(RuntimeCheck("total", "total()(uint256)"),),
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
        source_name="Arithmetic.sol",
        runtime_checks=(RuntimeCheck("value", "value()(uint256)"),),
    ),
    TestCase(
        test_id="verified-words",
        description="Synthetic bitwise and signed arithmetic loops for verified rules",
        source_code=(TESTDATA_ROOT / "runtime/VerifiedWords.sol").read_text(),
        source_name="VerifiedWords.sol",
        contract_name="VerifiedWords",
        gas_calls=(
            GasCall("mix", "mix(uint256,uint256,uint256)", ("32769", "65408", "64")),
            GasCall("merge", "merge(uint256,uint256,uint256)", ("32769", "65408", "64")),
            GasCall("negate", "negate(uint256,uint256)", (str(1 << 255), "64")),
        ),
        runtime_checks=(
            RuntimeCheck("mix", "mix(uint256,uint256,uint256)(uint256)", ("32769", "65408", "64")),
            RuntimeCheck("merge", "merge(uint256,uint256,uint256)(uint256)", ("32769", "65408", "64")),
            RuntimeCheck("negate", "negate(uint256,uint256)(uint256)", (str(1 << 255), "64")),
            RuntimeCheck("zero-rounds", "mix(uint256,uint256,uint256)(uint256)", (MAX_UINT256, "1", "0")),
        ),
    ),
    TestCase(
        test_id="word-recipes",
        description="Synthetic arithmetic, factoring, byte extraction and bounded comparisons",
        source_code=(TESTDATA_ROOT / "runtime/WordRecipes.sol").read_text(),
        source_name="WordRecipes.sol",
        contract_name="WordRecipes",
        gas_calls=(
            GasCall("mixed", "mixed(uint256,uint256,uint256)", ("32769", "65408", "64")),
            GasCall("factored", "factored(uint256,uint256,uint256,uint256)", ("32769", "65408", MAX_UINT256, "64")),
            GasCall("packed", "packed(uint256,uint256)", (MAX_UINT256, "64")),
            GasCall("bounded", "bounded(uint256)", (str((1 << 160) - 1),)),
        ),
        runtime_checks=(
            RuntimeCheck("mixed", "mixed(uint256,uint256,uint256)(uint256)", ("32769", "65408", "64")),
            RuntimeCheck("factored", "factored(uint256,uint256,uint256,uint256)(uint256)", ("32769", "65408", MAX_UINT256, "64")),
            RuntimeCheck("packed", "packed(uint256,uint256)(uint256)", (MAX_UINT256, "64")),
            RuntimeCheck("bounded-max", "bounded(uint256)(bool)", (str((1 << 160) - 1),)),
            RuntimeCheck("bounded-overflow", "bounded(uint256)(bool)", (str(1 << 160),)),
        ),
    ),
    TestCase(
        test_id="seeded-words",
        description="Synthetic mixed-word reductions found by seeded discovery",
        source_code=(TESTDATA_ROOT / "runtime/SeededWords.sol").read_text(),
        source_name="SeededWords.sol",
        contract_name="SeededWords",
        gas_calls=tuple(
            GasCall(name, f"{name}(uint256,uint256,uint256)", ("32769", "65408", "64"))
            for name in ("difference", "sumDifference", "complement", "absorb")
        ),
        runtime_checks=tuple(
            RuntimeCheck(f"{name}-{label}", f"{name}(uint256,uint256,uint256)(uint256)", args)
            for name in ("difference", "sumDifference", "complement", "absorb")
            for label, args in (
                ("loop", ("32769", "65408", "64")),
                ("wrap", (MAX_UINT256, "1", "64")),
                ("zero", ("0", "0", "0")),
            )
        ),
    ),
    TestCase(
        test_id="compiler-optimizations",
        description="CFG scalars, range proofs, shared constants, and storage writes",
        source_code=(TESTDATA_ROOT / "runtime/CompilerOptimizations.sol").read_text(),
        source_name="CompilerOptimizations.sol",
        contract_name="CompilerOptimizations",
        gas_calls=(
            GasCall("aggregate-true", "aggregate(bool,uint256)", ("true", "30"), repeat=2),
            GasCall("aggregate-false", "aggregate(bool,uint256)", ("false", "30"), repeat=2),
            GasCall("bounds-left", "bounds(bool,uint256,uint256)", ("true", "99", "79")),
            GasCall("bounds-right", "bounds(bool,uint256,uint256)", ("false", "99", "79")),
            GasCall("packed", "packed(uint8,uint8)", ("255", "128"), repeat=3),
            GasCall("overwrite", "overwrite(bool,uint256)", ("true", "37"), repeat=3),
            GasCall("stack-equal", "stackShape(uint256,uint256)", ("7", "7"), repeat=2),
            GasCall("stack-different", "stackShape(uint256,uint256)", ("9", "4"), repeat=2),
            GasCall("shared-first", "first(uint256)", ("17",)),
            GasCall("shared-second", "second(uint256)", ("23",)),
            GasCall("shared-third", "third(uint256)", ("31",)),
        ),
        runtime_checks=(
            RuntimeCheck("aggregate", "aggregate(bool,uint256)(uint256)", ("true", "30")),
            RuntimeCheck("bounds", "bounds(bool,uint256,uint256)(uint256)", ("false", "99", "79")),
            RuntimeCheck("stack-shape", "stackShape(uint256,uint256)(uint256,uint256,bool)", ("9", "4")),
            RuntimeCheck("generic", "generic(bool,uint256)(uint256)", ("true", "3")),
            RuntimeCheck("word", "word()(uint256)"),
            RuntimeCheck("low", "low()(uint8)"),
            RuntimeCheck("high", "high()(uint8)"),
        ),
    ),
    TestCase(
        test_id="uniswap-v2-pair",
        description="Uniswap V2 Pair",
        project="v2-core",
        project_file="uniswap-v2-pair.json.gz",
        source="contracts/UniswapV2Pair.sol",
        contract_name="UniswapV2Pair",
        suite="repository",
        settings_profile="runtime",
        min_solc="0.5.16",
        max_solc="0.5.16",
    ),
    TestCase(
        test_id="openzeppelin-erc20-mock",
        description="OpenZeppelin ERC20Mock",
        project="openzeppelin-contracts",
        project_file="openzeppelin-5.6.1.json.gz",
        source="contracts/mocks/token/ERC20Mock.sol",
        contract_name="ERC20Mock",
        suite="repository",
        settings_profile="runtime",
        test_calls=(
            ("mint(address,uint256)", (DEFAULT_SENDER, "1000")),
            ("burn(address,uint256)", (DEFAULT_SENDER, "400")),
            ("approve(address,uint256)", (DEFAULT_SPENDER, "250")),
            ("transfer(address,uint256)", (DEFAULT_THIRD, "125")),
        ),
        runtime_checks=(
            RuntimeCheck("name", "name()(string)"),
            RuntimeCheck("symbol", "symbol()(string)"),
            RuntimeCheck("decimals", "decimals()(uint8)"),
            RuntimeCheck("balance", "balanceOf(address)(uint256)", (DEFAULT_SENDER,)),
            RuntimeCheck(
                "spender-balance", "balanceOf(address)(uint256)", (DEFAULT_SPENDER,)
            ),
            RuntimeCheck(
                "third-balance", "balanceOf(address)(uint256)", (DEFAULT_THIRD,)
            ),
            RuntimeCheck(
                "fourth-balance", "balanceOf(address)(uint256)", (DEFAULT_FOURTH,)
            ),
            RuntimeCheck(
                "zero-balance", "balanceOf(address)(uint256)", (ZERO_ADDRESS,)
            ),
            RuntimeCheck("supply", "totalSupply()(uint256)"),
            RuntimeCheck(
                "allowance",
                "allowance(address,address)(uint256)",
                (DEFAULT_SENDER, DEFAULT_SPENDER),
            ),
            RuntimeCheck(
                "third-allowance",
                "allowance(address,address)(uint256)",
                (DEFAULT_SENDER, DEFAULT_THIRD),
            ),
            RuntimeCheck(
                "fourth-allowance",
                "allowance(address,address)(uint256)",
                (DEFAULT_SENDER, DEFAULT_FOURTH),
            ),
            RuntimeCheck(
                "reverse-allowance",
                "allowance(address,address)(uint256)",
                (DEFAULT_SPENDER, DEFAULT_SENDER),
            ),
        ),
    ),
    TestCase(
        test_id="openzeppelin-vesting-wallet",
        description="OpenZeppelin VestingWallet",
        project="openzeppelin-contracts",
        project_file="openzeppelin-5.6.1.json.gz",
        source="contracts/finance/VestingWallet.sol",
        contract_name="VestingWallet",
        suite="repository",
        settings_profile="runtime",
        constructor_args=(DEFAULT_SENDER, "1000", "100"),
        constructor_sig="constructor(address,uint64,uint64)",
        test_calls=(
            ("vestedAmount(uint64)", ("999",)),
            ("vestedAmount(uint64)", ("1050",)),
            ("releasable()", ()),
        ),
        runtime_checks=(
            RuntimeCheck("owner", "owner()(address)"),
            RuntimeCheck("start", "start()(uint256)"),
            RuntimeCheck("duration", "duration()(uint256)"),
            RuntimeCheck("end", "end()(uint256)"),
            RuntimeCheck("released", "released()(uint256)"),
            RuntimeCheck(
                "released-token", "released(address)(uint256)", (DEFAULT_SPENDER,)
            ),
            RuntimeCheck("releasable", "releasable()(uint256)"),
            RuntimeCheck(
                "vested-before-start", "vestedAmount(uint64)(uint256)", ("999",)
            ),
            RuntimeCheck("vested-at-start", "vestedAmount(uint64)(uint256)", ("1000",)),
            RuntimeCheck(
                "vested-after-start", "vestedAmount(uint64)(uint256)", ("1001",)
            ),
            RuntimeCheck("vested-quarter", "vestedAmount(uint64)(uint256)", ("1025",)),
            RuntimeCheck("vested-half", "vestedAmount(uint64)(uint256)", ("1050",)),
            RuntimeCheck(
                "vested-three-quarter", "vestedAmount(uint64)(uint256)", ("1075",)
            ),
            RuntimeCheck(
                "vested-before-end", "vestedAmount(uint64)(uint256)", ("1099",)
            ),
            RuntimeCheck("vested-end", "vestedAmount(uint64)(uint256)", ("1100",)),
            RuntimeCheck(
                "vested-after-end", "vestedAmount(uint64)(uint256)", ("1101",)
            ),
            RuntimeCheck(
                "vested-far-future", "vestedAmount(uint64)(uint256)", ("999999",)
            ),
        ),
    ),
    TestCase(
        test_id="nitro-one-step-proof",
        description="Nitro OneStepProofEntry",
        project="nitro-contracts",
        project_file="nitro-one-step-proof.json.gz",
        source="src/osp/OneStepProofEntry.sol",
        contract_name="OneStepProofEntry",
        suite="repository",
        settings_profile="runtime",
        constructor_args=(
            DEFAULT_SENDER,
            DEFAULT_SPENDER,
            DEFAULT_THIRD,
            DEFAULT_FOURTH,
        ),
        constructor_sig="constructor(address,address,address,address)",
        test_calls=(
            ("prover0()", ()),
            ("proverMem()", ()),
            ("proverMath()", ()),
            ("proverHostIo()", ()),
            (
                "getStartMachineHash(bytes32,bytes32)",
                (
                    "0x0000000000000000000000000000000000000000000000000000000000000011",
                    "0x0000000000000000000000000000000000000000000000000000000000000022",
                ),
            ),
            (
                "getStartMachineHash(bytes32,bytes32)",
                (
                    "0x0000000000000000000000000000000000000000000000000000000000000000",
                    EDGE_BYTES32,
                ),
            ),
            (
                "getStartMachineHash(bytes32,bytes32)",
                (
                    MIXED_BYTES32,
                    "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                ),
            ),
        ),
        runtime_checks=(
            RuntimeCheck("prover0", "prover0()(address)"),
            RuntimeCheck("proverMem", "proverMem()(address)"),
            RuntimeCheck("proverMath", "proverMath()(address)"),
            RuntimeCheck("proverHostIo", "proverHostIo()(address)"),
            RuntimeCheck(
                "start-machine-hash",
                "getStartMachineHash(bytes32,bytes32)(bytes32)",
                (
                    "0x0000000000000000000000000000000000000000000000000000000000000011",
                    "0x0000000000000000000000000000000000000000000000000000000000000022",
                ),
            ),
            RuntimeCheck(
                "start-machine-hash-edge",
                "getStartMachineHash(bytes32,bytes32)(bytes32)",
                (
                    "0x0000000000000000000000000000000000000000000000000000000000000000",
                    EDGE_BYTES32,
                ),
            ),
            RuntimeCheck(
                "start-machine-hash-mixed",
                "getStartMachineHash(bytes32,bytes32)(bytes32)",
                (
                    MIXED_BYTES32,
                    "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                ),
            ),
        ),
    ),
    TestCase(
        test_id="aave-l2-encoder",
        description="Aave V3 L2Encoder",
        project="aave-v3-core",
        project_file="aave-l2-encoder.json.gz",
        source="fixtures/aave/L2EncoderHarness.sol",
        contract_name="L2EncoderHarness",
        suite="repository",
        settings_profile="runtime",
        test_calls=(
            ("POOL()", ()),
            (
                "encodeSupplyParams(address,uint256,uint16)",
                (DEFAULT_SPENDER, "123456", "7"),
            ),
            ("encodeWithdrawParams(address,uint256)", (DEFAULT_THIRD, MAX_UINT256)),
            (
                "encodeBorrowParams(address,uint256,uint256,uint16)",
                (DEFAULT_SPENDER, "2222", "2", "9"),
            ),
            (
                "encodeSetUserUseReserveAsCollateral(address,bool)",
                (DEFAULT_THIRD, "true"),
            ),
            (
                "encodeRepayWithATokensParams(address,uint256,uint256)",
                (DEFAULT_FOURTH, MAX_UINT256, "2"),
            ),
            ("encodeSwapBorrowRateMode(address,uint256)", (DEFAULT_SENDER, "1")),
            (
                "encodeRebalanceStableBorrowRate(address,address)",
                (DEFAULT_SPENDER, DEFAULT_THIRD),
            ),
        ),
        runtime_checks=(
            RuntimeCheck(
                "supply",
                "encodeSupplyParams(address,uint256,uint16)(bytes32)",
                (DEFAULT_SPENDER, "123456", "7"),
            ),
            RuntimeCheck(
                "supply-zero",
                "encodeSupplyParams(address,uint256,uint16)(bytes32)",
                (DEFAULT_SENDER, "0", "0"),
            ),
            RuntimeCheck(
                "supply-max-u128",
                "encodeSupplyParams(address,uint256,uint16)(bytes32)",
                (DEFAULT_FOURTH, MAX_UINT128, "65535"),
            ),
            RuntimeCheck(
                "withdraw-zero",
                "encodeWithdrawParams(address,uint256)(bytes32)",
                (DEFAULT_SENDER, "0"),
            ),
            RuntimeCheck(
                "withdraw-small",
                "encodeWithdrawParams(address,uint256)(bytes32)",
                (DEFAULT_SENDER, "1"),
            ),
            RuntimeCheck(
                "withdraw-max",
                "encodeWithdrawParams(address,uint256)(bytes32)",
                (DEFAULT_THIRD, MAX_UINT256),
            ),
            RuntimeCheck(
                "borrow",
                "encodeBorrowParams(address,uint256,uint256,uint16)(bytes32)",
                (DEFAULT_SPENDER, "2222", "2", "9"),
            ),
            RuntimeCheck(
                "borrow-zero",
                "encodeBorrowParams(address,uint256,uint256,uint16)(bytes32)",
                (DEFAULT_THIRD, "0", "1", "0"),
            ),
            RuntimeCheck(
                "borrow-stable",
                "encodeBorrowParams(address,uint256,uint256,uint16)(bytes32)",
                (DEFAULT_SENDER, "1", "1", "0"),
            ),
            RuntimeCheck(
                "repay-zero",
                "encodeRepayParams(address,uint256,uint256)(bytes32)",
                (DEFAULT_SENDER, "0", "1"),
            ),
            RuntimeCheck(
                "repay",
                "encodeRepayParams(address,uint256,uint256)(bytes32)",
                (DEFAULT_THIRD, "3333", "1"),
            ),
            RuntimeCheck(
                "repay-max",
                "encodeRepayParams(address,uint256,uint256)(bytes32)",
                (DEFAULT_FOURTH, MAX_UINT256, "2"),
            ),
            RuntimeCheck(
                "supply-permit",
                "encodeSupplyWithPermitParams(address,uint256,uint16,uint256,uint8,bytes32,bytes32)(bytes32,bytes32,bytes32)",
                (
                    DEFAULT_SPENDER,
                    "4444",
                    "11",
                    "123456789",
                    "27",
                    "0x00000000000000000000000000000000000000000000000000000000000000aa",
                    "0x00000000000000000000000000000000000000000000000000000000000000bb",
                ),
            ),
            RuntimeCheck(
                "repay-permit",
                "encodeRepayWithPermitParams(address,uint256,uint256,uint256,uint8,bytes32,bytes32)(bytes32,bytes32,bytes32)",
                (
                    DEFAULT_THIRD,
                    MAX_UINT256,
                    "2",
                    "987654321",
                    "28",
                    "0x00000000000000000000000000000000000000000000000000000000000000cc",
                    "0x00000000000000000000000000000000000000000000000000000000000000dd",
                ),
            ),
            RuntimeCheck(
                "repay-atokens",
                "encodeRepayWithATokensParams(address,uint256,uint256)(bytes32)",
                (DEFAULT_FOURTH, MAX_UINT256, "2"),
            ),
            RuntimeCheck(
                "swap-rate",
                "encodeSwapBorrowRateMode(address,uint256)(bytes32)",
                (DEFAULT_SENDER, "1"),
            ),
            RuntimeCheck(
                "swap-rate-variable",
                "encodeSwapBorrowRateMode(address,uint256)(bytes32)",
                (DEFAULT_FOURTH, "2"),
            ),
            RuntimeCheck(
                "rebalance-zero-user",
                "encodeRebalanceStableBorrowRate(address,address)(bytes32)",
                (DEFAULT_FOURTH, ZERO_ADDRESS),
            ),
            RuntimeCheck(
                "rebalance",
                "encodeRebalanceStableBorrowRate(address,address)(bytes32)",
                (DEFAULT_SPENDER, DEFAULT_THIRD),
            ),
            RuntimeCheck(
                "collateral-true",
                "encodeSetUserUseReserveAsCollateral(address,bool)(bytes32)",
                (DEFAULT_THIRD, "true"),
            ),
            RuntimeCheck(
                "collateral-false",
                "encodeSetUserUseReserveAsCollateral(address,bool)(bytes32)",
                (DEFAULT_THIRD, "false"),
            ),
            RuntimeCheck(
                "liquidation",
                "encodeLiquidationCall(address,address,address,uint256,bool)(bytes32,bytes32)",
                (DEFAULT_SPENDER, DEFAULT_THIRD, DEFAULT_FOURTH, "5555", "false"),
            ),
            RuntimeCheck(
                "liquidation-max",
                "encodeLiquidationCall(address,address,address,uint256,bool)(bytes32,bytes32)",
                (DEFAULT_THIRD, DEFAULT_SPENDER, DEFAULT_SENDER, MAX_UINT256, "true"),
            ),
            RuntimeCheck(
                "liquidation-zero",
                "encodeLiquidationCall(address,address,address,uint256,bool)(bytes32,bytes32)",
                (DEFAULT_SENDER, DEFAULT_FOURTH, ZERO_ADDRESS, "0", "true"),
            ),
        ),
    ),
    TestCase(
        test_id="lilweb3-ens",
        description="LilENS",
        project="lil-web3",
        project_file="lilweb3-ens.json.gz",
        source="src/LilENS.sol",
        contract_name="LilENS",
        suite="repository",
        settings_profile="runtime",
        test_calls=(
            ("register(string)", ("testname",)),
            ("update(string,address)", ("testname", DEFAULT_SPENDER)),
            ("register(string)", ("second",)),
            ("update(string,address)", ("second", DEFAULT_THIRD)),
            ("register(string)", ("untouched",)),
            ("register(string)", ("",)),
            ("register(string)", ("long-subdomain-name",)),
            ("update(string,address)", ("long-subdomain-name", DEFAULT_FOURTH)),
            ("register(string)", ("numeric123",)),
            ("register(string)", ("under_score",)),
            ("update(string,address)", ("under_score", DEFAULT_SPENDER)),
        ),
        runtime_checks=(
            RuntimeCheck("lookup-updated", "lookup(string)(address)", ("testname",)),
            RuntimeCheck("lookup-second", "lookup(string)(address)", ("second",)),
            RuntimeCheck("lookup-untouched", "lookup(string)(address)", ("untouched",)),
            RuntimeCheck("lookup-empty", "lookup(string)(address)", ("",)),
            RuntimeCheck(
                "lookup-long", "lookup(string)(address)", ("long-subdomain-name",)
            ),
            RuntimeCheck("lookup-numeric", "lookup(string)(address)", ("numeric123",)),
            RuntimeCheck(
                "lookup-underscore", "lookup(string)(address)", ("under_score",)
            ),
            RuntimeCheck("missing", "lookup(string)(address)", ("missing",)),
        ),
    ),
    TestCase(
        test_id="lilweb3-flashloan",
        description="LilFlashloan",
        project="lil-web3",
        project_file="lilweb3-runtime.json.gz",
        source="src/LilFlashloan.sol",
        contract_name="LilFlashloan",
        suite="repository",
        settings_profile="runtime",
        test_calls=(
            ("manager()", ()),
            ("setFees(address,uint256)", (DEFAULT_SPENDER, "250")),
            ("setFees(address,uint256)", (DEFAULT_THIRD, "1000")),
            ("setFees(address,uint256)", (DEFAULT_FOURTH, "10000")),
            ("setFees(address,uint256)", (DEFAULT_SENDER, "1")),
        ),
        runtime_checks=(
            RuntimeCheck("manager", "manager()(address)"),
            RuntimeCheck("fee-spender", "fees(address)(uint256)", (DEFAULT_SPENDER,)),
            RuntimeCheck("fee-third", "fees(address)(uint256)", (DEFAULT_THIRD,)),
            RuntimeCheck("fee-fourth", "fees(address)(uint256)", (DEFAULT_FOURTH,)),
            RuntimeCheck("fee-sender", "fees(address)(uint256)", (DEFAULT_SENDER,)),
            RuntimeCheck("fee-missing", "fees(address)(uint256)", (ZERO_ADDRESS,)),
            RuntimeCheck(
                "computed-fee-zero",
                "getFee(address,uint256)(uint256)",
                (DEFAULT_SPENDER, "0"),
            ),
            RuntimeCheck(
                "computed-fee-spender",
                "getFee(address,uint256)(uint256)",
                (DEFAULT_SPENDER, "10000"),
            ),
            RuntimeCheck(
                "computed-fee-spender-small",
                "getFee(address,uint256)(uint256)",
                (DEFAULT_SPENDER, "1"),
            ),
            RuntimeCheck(
                "computed-fee-spender-rounded",
                "getFee(address,uint256)(uint256)",
                (DEFAULT_SPENDER, "33333"),
            ),
            RuntimeCheck(
                "computed-fee-third",
                "getFee(address,uint256)(uint256)",
                (DEFAULT_THIRD, "12345"),
            ),
            RuntimeCheck(
                "computed-fee-fourth",
                "getFee(address,uint256)(uint256)",
                (DEFAULT_FOURTH, "12345"),
            ),
            RuntimeCheck(
                "computed-fee-sender",
                "getFee(address,uint256)(uint256)",
                (DEFAULT_SENDER, "999999"),
            ),
            RuntimeCheck(
                "computed-fee-missing",
                "getFee(address,uint256)(uint256)",
                (ZERO_ADDRESS, "10000"),
            ),
        ),
    ),
    TestCase(
        test_id="lilweb3-fractional",
        description="LilFractional",
        project="lil-web3",
        project_file="lilweb3-runtime.json.gz",
        source="src/LilFractional.sol",
        contract_name="LilFractional",
        suite="repository",
        settings_profile="runtime",
        test_calls=(
            ("getVault(uint256)", ("0",)),
            ("getVault(uint256)", ("1",)),
            ("getVault(uint256)", ("42",)),
            (
                "onERC721Received(address,address,uint256,bytes)",
                (DEFAULT_SENDER, DEFAULT_SPENDER, "7", "0x"),
            ),
            (
                "onERC721Received(address,address,uint256,bytes)",
                (DEFAULT_THIRD, DEFAULT_FOURTH, "42", "0x123456"),
            ),
        ),
        runtime_checks=(
            RuntimeCheck(
                "empty-vault-zero",
                "getVault(uint256)(address,uint256,uint256,address)",
                ("0",),
            ),
            RuntimeCheck(
                "empty-vault-one",
                "getVault(uint256)(address,uint256,uint256,address)",
                ("1",),
            ),
            RuntimeCheck(
                "empty-vault-forty-two",
                "getVault(uint256)(address,uint256,uint256,address)",
                ("42",),
            ),
            RuntimeCheck(
                "empty-vault-max",
                "getVault(uint256)(address,uint256,uint256,address)",
                (MAX_UINT256,),
            ),
            RuntimeCheck(
                "erc721-receiver-empty-data",
                "onERC721Received(address,address,uint256,bytes)(bytes4)",
                (DEFAULT_SENDER, DEFAULT_SPENDER, "7", "0x"),
            ),
            RuntimeCheck(
                "erc721-receiver-nonempty-data",
                "onERC721Received(address,address,uint256,bytes)(bytes4)",
                (DEFAULT_THIRD, DEFAULT_FOURTH, "42", "0x123456"),
            ),
            RuntimeCheck(
                "erc721-receiver-word-data",
                "onERC721Received(address,address,uint256,bytes)(bytes4)",
                (ZERO_ADDRESS, ZERO_ADDRESS, "0", "0x" + "11" * 32),
            ),
        ),
    ),
    TestCase(
        test_id="maple-erc20",
        description="Maple ERC20",
        project="maple-erc20",
        project_file="maple-erc20.json.gz",
        source="contracts/ERC20.sol",
        contract_name="ERC20",
        suite="repository",
        settings_profile="runtime",
        constructor_args=("Maple Token", "MPL", "18"),
        constructor_sig="constructor(string,string,uint8)",
        test_calls=(
            ("approve(address,uint256)", (DEFAULT_SPENDER, "100")),
            ("increaseAllowance(address,uint256)", (DEFAULT_SPENDER, "50")),
            ("decreaseAllowance(address,uint256)", (DEFAULT_SPENDER, "20")),
            ("approve(address,uint256)", (DEFAULT_THIRD, "77")),
            ("increaseAllowance(address,uint256)", (DEFAULT_THIRD, "23")),
            ("decreaseAllowance(address,uint256)", (DEFAULT_THIRD, "20")),
            ("approve(address,uint256)", (DEFAULT_FOURTH, MAX_UINT256)),
            ("approve(address,uint256)", (ZERO_ADDRESS, "1")),
        ),
        runtime_checks=(
            RuntimeCheck("name", "name()(string)"),
            RuntimeCheck("symbol", "symbol()(string)"),
            RuntimeCheck("decimals", "decimals()(uint8)"),
            RuntimeCheck("total-supply", "totalSupply()(uint256)"),
            RuntimeCheck("balance", "balanceOf(address)(uint256)", (DEFAULT_SENDER,)),
            RuntimeCheck(
                "spender-balance", "balanceOf(address)(uint256)", (DEFAULT_SPENDER,)
            ),
            RuntimeCheck(
                "allowance",
                "allowance(address,address)(uint256)",
                (DEFAULT_SENDER, DEFAULT_SPENDER),
            ),
            RuntimeCheck(
                "third-allowance",
                "allowance(address,address)(uint256)",
                (DEFAULT_SENDER, DEFAULT_THIRD),
            ),
            RuntimeCheck(
                "fourth-allowance",
                "allowance(address,address)(uint256)",
                (DEFAULT_SENDER, DEFAULT_FOURTH),
            ),
            RuntimeCheck(
                "zero-allowance",
                "allowance(address,address)(uint256)",
                (DEFAULT_SENDER, ZERO_ADDRESS),
            ),
            RuntimeCheck(
                "reverse-allowance",
                "allowance(address,address)(uint256)",
                (DEFAULT_SPENDER, DEFAULT_SENDER),
            ),
            RuntimeCheck(
                "third-reverse-allowance",
                "allowance(address,address)(uint256)",
                (DEFAULT_THIRD, DEFAULT_SENDER),
            ),
            RuntimeCheck(
                "fourth-reverse-allowance",
                "allowance(address,address)(uint256)",
                (DEFAULT_FOURTH, DEFAULT_SENDER),
            ),
            RuntimeCheck("nonce", "nonces(address)(uint256)", (DEFAULT_SENDER,)),
            RuntimeCheck(
                "spender-nonce", "nonces(address)(uint256)", (DEFAULT_SPENDER,)
            ),
            RuntimeCheck("zero-nonce", "nonces(address)(uint256)", (ZERO_ADDRESS,)),
            RuntimeCheck("permit-typehash", "PERMIT_TYPEHASH()(bytes32)"),
        ),
    ),
    TestCase(
        test_id="openzeppelin-governor",
        description="OpenZeppelin Governor",
        project="openzeppelin-5.6.1",
        project_file="openzeppelin-5.6.1.json.gz",
        source="test/governance/Governor.t.sol",
        contract_name="GovernorInternalTest",
        gas_calls=(
            GasCall(
                "hash-proposal-empty-zero",
                "hashProposal(address[],uint256[],bytes[],bytes32)",
                ("[]", "[]", "[]", "0x" + "00" * 32),
                repeat=3,
            ),
            GasCall(
                "hash-proposal-empty-nonzero",
                "hashProposal(address[],uint256[],bytes[],bytes32)",
                ("[]", "[]", "[]", "0x" + "42" * 32),
                repeat=3,
            ),
            GasCall("name", "name()", repeat=3),
            GasCall("version", "version()", repeat=3),
            *(
                GasCall(
                    label,
                    "hashProposal(address[],uint256[],bytes[],bytes32)",
                    args,
                    repeat=3,
                )
                for label, args in GOVERNOR_PROPOSALS
            ),
        ),
        runtime_checks=(
            RuntimeCheck("name", "name()(string)"),
            RuntimeCheck("version", "version()(string)"),
            RuntimeCheck(
                "hash-proposal-empty",
                "hashProposal(address[],uint256[],bytes[],bytes32)(uint256)",
                ("[]", "[]", "[]", "0x" + "00" * 32),
            ),
            *(
                RuntimeCheck(
                    label,
                    "hashProposal(address[],uint256[],bytes[],bytes32)(uint256)",
                    args,
                )
                for label, args in GOVERNOR_PROPOSALS
            ),
        ),
        suite="large",
    ),
    TestCase(
        test_id="solady-signature-checker",
        description="Solady SignatureCheckerLib",
        project="solady-0.1.26",
        project_file="solady-0.1.26.json.gz",
        source="test/SignatureCheckerLib.t.sol",
        contract_name="SignatureCheckerLibTest",
        gas_calls=(
            GasCall(
                "empty-signature",
                "isValidSignatureNowCalldata(address,bytes32,bytes)",
                (DEFAULT_SPENDER, SIGNED_HASH, "0x"),
                repeat=3,
            ),
            GasCall("empty-helpers", "testEmptyCalldataHelpers()", repeat=3),
            GasCall(
                "eth-signed-hash-word",
                "testToEthSignedMessageHashDifferential(bytes32)",
                (SIGNED_HASH,),
                repeat=3,
            ),
            GasCall(
                "eth-signed-hash-bytes",
                "testToEthSignedMessageHashDifferential(bytes)",
                ("0x" + "ab" * 96,),
                repeat=3,
            ),
        ),
        runtime_checks=(
            RuntimeCheck(
                "empty-signature",
                "isValidSignatureNowCalldata(address,bytes32,bytes)(bool)",
                (DEFAULT_SPENDER, SIGNED_HASH, "0x"),
            ),
        ),
        suite="large",
    ),
    TestCase(
        test_id="solady-lib-string",
        description="Solady LibString",
        project="solady-0.1.26",
        project_file="solady-0.1.26.json.gz",
        source="test/LibString.t.sol",
        contract_name="LibStringTest",
        gas_calls=(
            GasCall("serial-number", "checkIsSN(string)", ("123456789",), repeat=3),
            GasCall("not-serial-number", "checkIsSN(string)", ("12ab",), repeat=3),
            GasCall(
                "return-string",
                "returnString(string)",
                ("the quick brown fox jumps over the lazy dog",),
                repeat=3,
            ),
            GasCall(
                "small-string", "toSmallString(string)", ("short string",), repeat=3
            ),
            GasCall("replace-medium", "testStringReplaceMedium()", repeat=3),
            GasCall("replace-long", "testStringReplaceLong()", repeat=3),
            GasCall("hex-bytes-no-prefix", "testBytesToHexStringNoPrefix()", repeat=3),
            GasCall("hex-bytes", "testBytesToHexString()", repeat=3),
            GasCall("ascii-all-bytes", "testStringIs7BitASCII()", repeat=3),
            *(
                GasCall(f"hex-tail-{length}", "testBytesToHexStringNoPrefix(bytes)", ("0x" + "42" * length,))
                for length in (0, 1, 31, 32, 33, 64, 65)
            ),
            *(
                GasCall(f"hex-prefixed-tail-{length}", "testBytesToHexString(bytes)", ("0x" + "ff" * length,))
                for length in (0, 1, 31, 32, 33, 64, 65)
            ),
            *(
                GasCall(label, "testStringIs7BitASCIIDifferential(bytes)", (value,))
                for label, value in (
                    ("ascii-empty", "0x"),
                    ("ascii-31", "0x" + "41" * 31),
                    ("ascii-32", "0x" + "41" * 32),
                    ("ascii-33", "0x" + "41" * 33),
                    ("ascii-65", "0x" + "41" * 65),
                    ("ascii-high-first", "0x80" + "41" * 64),
                    ("ascii-high-boundary", "0x" + "41" * 31 + "80" + "41" * 33),
                    ("ascii-high-tail", "0x" + "41" * 64 + "80"),
                )
            ),
            *(
                GasCall(label, "testStringRuneCountDifferential(string)", (value,), repeat=3)
                for label, value in (
                    ("runes-empty", ""),
                    ("runes-one", "A"),
                    ("runes-31", "A" * 31),
                    ("runes-32", "A" * 32),
                    ("runes-33", "A" * 33),
                    ("runes-utf8-two-byte", "\u03bb" * 64),
                    ("runes-utf8-four-byte", "\U0001f600" * 64),
                )
            ),
        ),
        runtime_checks=(
            RuntimeCheck("serial-number", "checkIsSN(string)(bool)", ("123456789",)),
            RuntimeCheck("not-serial-number", "checkIsSN(string)(bool)", ("12ab",)),
            RuntimeCheck(
                "return-string",
                "returnString(string)(string)",
                ("the quick brown fox jumps over the lazy dog",),
            ),
            RuntimeCheck(
                "small-string", "toSmallString(string)(bytes32)", ("short string",)
            ),
        ),
        suite="large",
    ),
    TestCase(
        test_id="solady-encoding",
        description="Solady hex and ASCII with ordinary ABI calls",
        min_solc="0.8.20",
        project="solady-0.1.26",
        project_file="solady-0.1.26.json.gz",
        source="Encoding.sol",
        source_code=(TESTDATA_ROOT / "runtime/Encoding.sol").read_text(),
        contract_name="Encoding",
        settings_profile="runtime",
        gas_calls=tuple(
            GasCall(f"{name}-{label}", f"{name}(bytes)", (value,))
            for name in ("hexNoPrefix", "hexPrefixed", "ascii")
            for label, value in ENCODING_INPUTS
        ),
        runtime_checks=tuple(
            RuntimeCheck(f"{name}-{label}", f"{name}(bytes)({result})", (value,))
            for name, result in (
                ("hexNoPrefix", "string"),
                ("hexPrefixed", "string"),
                ("ascii", "bool"),
            )
            for label, value in ENCODING_INPUTS
        ),
        suite="repository",
    ),
    TestCase(
        test_id="solady-algorithms",
        description="Solady decimal conversion, sorting and Base64 decoding",
        min_solc="0.8.20",
        project="solady-0.1.26",
        project_file="solady-0.1.26.json.gz",
        source="Algorithms.sol",
        source_code=(TESTDATA_ROOT / "runtime/Algorithms.sol").read_text(),
        contract_name="Algorithms",
        settings_profile="runtime",
        gas_calls=tuple(
            GasCall(label, signature, (value,))
            for label, signature, _, value in ALGORITHM_INPUTS
        ),
        runtime_checks=tuple(
            RuntimeCheck(label, f"{signature}({result})", (value,))
            for label, signature, result, value in ALGORITHM_INPUTS
        ),
        suite="repository",
    ),
    TestCase(
        test_id="seaport-1.6-project",
        description="Seaport 1.6 full project",
        contract_name="*",
        project="seaport-1.6",
        project_file="seaport-1.6.json.gz",
        whole_project=True,
        suite="heavy",
    ),
    TestCase(
        test_id="v4-core-project",
        description="Uniswap v4-core full project (viaIR)",
        contract_name="*",
        project="v4-core-4.0.0",
        project_file="v4-core-4.0.0.json.gz",
        whole_project=True,
        suite="heavy",
    ),
    TestCase(
        test_id="morpho-blue-project",
        description="Morpho Blue full project (viaIR)",
        contract_name="*",
        project="morpho-blue-1.0.0",
        project_file="morpho-blue-1.0.0.json.gz",
        whole_project=True,
        suite="heavy",
    ),
    TestCase(
        test_id="openzeppelin-5.6.1-project",
        description="OpenZeppelin 5.6.1 full project",
        contract_name="*",
        project="openzeppelin-5.6.1",
        project_file="openzeppelin-5.6.1.json.gz",
        whole_project=True,
        suite="heavy",
    ),
    TestCase(
        test_id="solady-0.1.26-project",
        description="Solady 0.1.26 full project",
        contract_name="*",
        project="solady-0.1.26",
        project_file="solady-0.1.26.json.gz",
        whole_project=True,
        suite="heavy",
    ),
    TestCase(
        test_id="forge-std-1.16.1-project",
        description="Forge Std 1.16.1 full project",
        contract_name="*",
        project="forge-std-1.16.1",
        project_file="forge-std-1.16.1.json.gz",
        whole_project=True,
        suite="heavy",
    ),
    TestCase(
        test_id="prb-math-4.1.1-project",
        description="PRBMath 4.1.1 full project",
        contract_name="*",
        project="prb-math-4.1.1",
        project_file="prb-math-4.1.1.json.gz",
        whole_project=True,
        suite="heavy",
    ),
    TestCase(
        test_id="solmate-6-project",
        description="Solmate 6 full project",
        contract_name="*",
        project="solmate-6",
        project_file="solmate-6.json.gz",
        whole_project=True,
        suite="heavy",
    ),
    TestCase(
        test_id="solarray-a547630-project",
        description="Solarray full project",
        contract_name="*",
        project="solarray-a547630",
        project_file="solarray-a547630.json.gz",
        whole_project=True,
        suite="heavy",
    ),
)


HOT_GAS_CALLS: dict[str, Sequence[GasCall]] = {
    "factorial": (
        GasCall("factorial-5", "computeFactorial(uint256)", ("5",)),
        GasCall("factorial-10", "computeFactorial(uint256)", ("10",)),
        GasCall("factorial-20", "computeFactorial(uint256)", ("20",)),
        GasCall("factorial-30", "computeFactorial(uint256)", ("30",)),
        GasCall("factorial-40", "computeFactorial(uint256)", ("40",)),
        GasCall("factorial-50", "computeFactorial(uint256)", ("50",)),
    ),
    "counter": (
        GasCall("set-number-10", "setNumber(uint256)", ("10",)),
        GasCall("increment", "increment()"),
        GasCall("set-number-50", "setNumber(uint256)", ("50",)),
        GasCall("increment-again", "increment()"),
        GasCall("set-number-100", "setNumber(uint256)", ("100",)),
    ),
    "sum-array": (
        GasCall("sum-1-10", "sumRange(uint256,uint256)", ("1", "10")),
        GasCall("sum-1-50", "sumRange(uint256,uint256)", ("1", "50")),
        GasCall("sum-1-100", "sumRange(uint256,uint256)", ("1", "100")),
        GasCall("sum-10-200", "sumRange(uint256,uint256)", ("10", "200")),
    ),
    "arithmetic": (
        GasCall("compute-10", "compute(uint256,uint256,uint256)", ("100", "3", "10")),
        GasCall("compute-50", "compute(uint256,uint256,uint256)", ("100", "3", "50")),
        GasCall("compute-100", "compute(uint256,uint256,uint256)", ("777", "9", "100")),
    ),
    "openzeppelin-erc20-mock": (
        GasCall("mint-sender-1000", "mint(address,uint256)", (DEFAULT_SENDER, "1000")),
        GasCall("mint-spender-250", "mint(address,uint256)", (DEFAULT_SPENDER, "250")),
        GasCall(
            "transfer-third-125", "transfer(address,uint256)", (DEFAULT_THIRD, "125")
        ),
        GasCall(
            "transfer-fourth-25", "transfer(address,uint256)", (DEFAULT_FOURTH, "25")
        ),
        GasCall(
            "approve-spender-250", "approve(address,uint256)", (DEFAULT_SPENDER, "250")
        ),
        GasCall("approve-third-77", "approve(address,uint256)", (DEFAULT_THIRD, "77")),
        GasCall("burn-sender-400", "burn(address,uint256)", (DEFAULT_SENDER, "400")),
        GasCall(
            "transfer-spender-50", "transfer(address,uint256)", (DEFAULT_SPENDER, "50")
        ),
    ),
    "openzeppelin-vesting-wallet": (
        GasCall("vested-before-start", "vestedAmount(uint64)", ("999",), repeat=2),
        GasCall("vested-start", "vestedAmount(uint64)", ("1000",), repeat=2),
        GasCall("vested-quarter", "vestedAmount(uint64)", ("1025",), repeat=2),
        GasCall("vested-half", "vestedAmount(uint64)", ("1050",), repeat=2),
        GasCall("vested-three-quarter", "vestedAmount(uint64)", ("1075",), repeat=2),
        GasCall("vested-end", "vestedAmount(uint64)", ("1100",), repeat=2),
        GasCall("vested-future", "vestedAmount(uint64)", ("999999",), repeat=2),
        GasCall("releasable", "releasable()", repeat=2),
    ),
    "nitro-one-step-proof": (
        GasCall("prover0", "prover0()", repeat=2),
        GasCall("prover-mem", "proverMem()", repeat=2),
        GasCall("prover-math", "proverMath()", repeat=2),
        GasCall("prover-host-io", "proverHostIo()", repeat=2),
        GasCall(
            "start-machine-small",
            "getStartMachineHash(bytes32,bytes32)",
            (
                "0x0000000000000000000000000000000000000000000000000000000000000011",
                "0x0000000000000000000000000000000000000000000000000000000000000022",
            ),
            repeat=2,
        ),
        GasCall(
            "start-machine-edge",
            "getStartMachineHash(bytes32,bytes32)",
            (
                "0x0000000000000000000000000000000000000000000000000000000000000000",
                EDGE_BYTES32,
            ),
            repeat=2,
        ),
        GasCall(
            "start-machine-mixed",
            "getStartMachineHash(bytes32,bytes32)",
            (
                MIXED_BYTES32,
                "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
            repeat=2,
        ),
    ),
    "aave-l2-encoder": (
        GasCall("pool", "POOL()", repeat=2),
        GasCall(
            "supply",
            "encodeSupplyParams(address,uint256,uint16)",
            (DEFAULT_SPENDER, "123456", "7"),
            repeat=2,
        ),
        GasCall(
            "supply-zero",
            "encodeSupplyParams(address,uint256,uint16)",
            (DEFAULT_SENDER, "0", "0"),
            repeat=2,
        ),
        GasCall(
            "supply-max",
            "encodeSupplyParams(address,uint256,uint16)",
            (DEFAULT_FOURTH, MAX_UINT128, "65535"),
        ),
        GasCall(
            "withdraw-zero",
            "encodeWithdrawParams(address,uint256)",
            (DEFAULT_SENDER, "0"),
        ),
        GasCall(
            "withdraw-max",
            "encodeWithdrawParams(address,uint256)",
            (DEFAULT_THIRD, MAX_UINT256),
            repeat=2,
        ),
        GasCall(
            "borrow",
            "encodeBorrowParams(address,uint256,uint256,uint16)",
            (DEFAULT_SPENDER, "2222", "2", "9"),
            repeat=2,
        ),
        GasCall(
            "borrow-zero",
            "encodeBorrowParams(address,uint256,uint256,uint16)",
            (DEFAULT_THIRD, "0", "1", "0"),
        ),
        GasCall(
            "repay",
            "encodeRepayParams(address,uint256,uint256)",
            (DEFAULT_THIRD, "3333", "1"),
            repeat=2,
        ),
        GasCall(
            "repay-max",
            "encodeRepayParams(address,uint256,uint256)",
            (DEFAULT_FOURTH, MAX_UINT256, "2"),
        ),
        GasCall(
            "repay-atokens",
            "encodeRepayWithATokensParams(address,uint256,uint256)",
            (DEFAULT_FOURTH, MAX_UINT256, "2"),
        ),
        GasCall(
            "swap-rate",
            "encodeSwapBorrowRateMode(address,uint256)",
            (DEFAULT_SENDER, "1"),
            repeat=2,
        ),
        GasCall(
            "collateral-true",
            "encodeSetUserUseReserveAsCollateral(address,bool)",
            (DEFAULT_THIRD, "true"),
        ),
        GasCall(
            "collateral-false",
            "encodeSetUserUseReserveAsCollateral(address,bool)",
            (DEFAULT_THIRD, "false"),
        ),
        GasCall(
            "liquidation",
            "encodeLiquidationCall(address,address,address,uint256,bool)",
            (DEFAULT_SPENDER, DEFAULT_THIRD, DEFAULT_FOURTH, "5555", "false"),
        ),
        GasCall(
            "liquidation-max",
            "encodeLiquidationCall(address,address,address,uint256,bool)",
            (DEFAULT_THIRD, DEFAULT_SPENDER, DEFAULT_SENDER, MAX_UINT256, "true"),
        ),
    ),
    "lilweb3-ens": (
        GasCall("register-testname", "register(string)", ("testname",)),
        GasCall(
            "update-testname", "update(string,address)", ("testname", DEFAULT_SPENDER)
        ),
        GasCall("register-second", "register(string)", ("second",)),
        GasCall("update-second", "update(string,address)", ("second", DEFAULT_THIRD)),
        GasCall("register-untouched", "register(string)", ("untouched",)),
        GasCall("register-empty", "register(string)", ("",)),
        GasCall("register-long", "register(string)", ("long-subdomain-name",)),
        GasCall(
            "update-long",
            "update(string,address)",
            ("long-subdomain-name", DEFAULT_FOURTH),
        ),
        GasCall("register-numeric", "register(string)", ("numeric123",)),
        GasCall("register-underscore", "register(string)", ("under_score",)),
        GasCall(
            "update-underscore",
            "update(string,address)",
            ("under_score", DEFAULT_SPENDER),
        ),
        GasCall(
            "register-very-long",
            "register(string)",
            ("very-long-subdomain-name-with-more-bytes",),
        ),
    ),
    "lilweb3-flashloan": (
        GasCall("manager", "manager()", repeat=2),
        GasCall(
            "set-fee-spender", "setFees(address,uint256)", (DEFAULT_SPENDER, "250")
        ),
        GasCall("set-fee-third", "setFees(address,uint256)", (DEFAULT_THIRD, "1000")),
        GasCall(
            "set-fee-fourth", "setFees(address,uint256)", (DEFAULT_FOURTH, "10000")
        ),
        GasCall("set-fee-sender", "setFees(address,uint256)", (DEFAULT_SENDER, "1")),
        GasCall(
            "get-fee-zero", "getFee(address,uint256)", (DEFAULT_SPENDER, "0"), repeat=2
        ),
        GasCall(
            "get-fee-spender",
            "getFee(address,uint256)",
            (DEFAULT_SPENDER, "10000"),
            repeat=2,
        ),
        GasCall(
            "get-fee-rounded",
            "getFee(address,uint256)",
            (DEFAULT_SPENDER, "33333"),
            repeat=2,
        ),
        GasCall(
            "get-fee-third",
            "getFee(address,uint256)",
            (DEFAULT_THIRD, "12345"),
            repeat=2,
        ),
        GasCall(
            "get-fee-fourth",
            "getFee(address,uint256)",
            (DEFAULT_FOURTH, "12345"),
            repeat=2,
        ),
        GasCall("get-fee-missing", "getFee(address,uint256)", (ZERO_ADDRESS, "10000")),
    ),
    "lilweb3-fractional": (
        GasCall("get-vault-zero", "getVault(uint256)", ("0",), repeat=2),
        GasCall("get-vault-one", "getVault(uint256)", ("1",), repeat=2),
        GasCall("get-vault-42", "getVault(uint256)", ("42",), repeat=2),
        GasCall("get-vault-max", "getVault(uint256)", (MAX_UINT256,)),
        GasCall(
            "erc721-empty",
            "onERC721Received(address,address,uint256,bytes)",
            (DEFAULT_SENDER, DEFAULT_SPENDER, "7", "0x"),
            repeat=2,
        ),
        GasCall(
            "erc721-nonempty",
            "onERC721Received(address,address,uint256,bytes)",
            (DEFAULT_THIRD, DEFAULT_FOURTH, "42", "0x123456"),
            repeat=2,
        ),
        GasCall(
            "erc721-word",
            "onERC721Received(address,address,uint256,bytes)",
            (ZERO_ADDRESS, ZERO_ADDRESS, "0", "0x" + "11" * 32),
        ),
    ),
    "maple-erc20": (
        GasCall(
            "approve-spender-100", "approve(address,uint256)", (DEFAULT_SPENDER, "100")
        ),
        GasCall(
            "increase-spender-50",
            "increaseAllowance(address,uint256)",
            (DEFAULT_SPENDER, "50"),
        ),
        GasCall(
            "decrease-spender-20",
            "decreaseAllowance(address,uint256)",
            (DEFAULT_SPENDER, "20"),
        ),
        GasCall(
            "increase-spender-70",
            "increaseAllowance(address,uint256)",
            (DEFAULT_SPENDER, "70"),
        ),
        GasCall(
            "decrease-spender-30",
            "decreaseAllowance(address,uint256)",
            (DEFAULT_SPENDER, "30"),
        ),
        GasCall("approve-third-77", "approve(address,uint256)", (DEFAULT_THIRD, "77")),
        GasCall(
            "increase-third-23",
            "increaseAllowance(address,uint256)",
            (DEFAULT_THIRD, "23"),
        ),
        GasCall(
            "decrease-third-20",
            "decreaseAllowance(address,uint256)",
            (DEFAULT_THIRD, "20"),
        ),
        GasCall(
            "approve-fourth-900", "approve(address,uint256)", (DEFAULT_FOURTH, "900")
        ),
        GasCall(
            "increase-fourth-100",
            "increaseAllowance(address,uint256)",
            (DEFAULT_FOURTH, "100"),
        ),
        GasCall(
            "decrease-fourth-50",
            "decreaseAllowance(address,uint256)",
            (DEFAULT_FOURTH, "50"),
        ),
        GasCall(
            "approve-fourth-max",
            "approve(address,uint256)",
            (DEFAULT_FOURTH, MAX_UINT256),
        ),
        GasCall("approve-zero-1", "approve(address,uint256)", (ZERO_ADDRESS, "1")),
    ),
}


def default_gas_calls(test_case: TestCase) -> Sequence[GasCall]:
    if test_case.gas_calls:
        return test_case.gas_calls
    return tuple(
        GasCall(signature, signature, tuple(args))
        for signature, args in test_case.test_calls
    )


def gas_calls(test_case: TestCase, profile: str) -> Sequence[GasCall]:
    if profile == "hot":
        return HOT_GAS_CALLS.get(test_case.test_id, default_gas_calls(test_case))
    return default_gas_calls(test_case)


def runtime_checks(test_case: TestCase) -> Sequence[RuntimeCheck]:
    return test_case.runtime_checks
