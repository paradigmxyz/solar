#!/usr/bin/env python3
"""Compare solc and Solar codegen on curated Solidity micro-benchmarks."""

# Adapted from walnuthq/solidity-compiler-benchmarks at
# 01209d2b8ac81645b92e3ef801b5bcdfd61bfd69 under Apache-2.0.

from __future__ import annotations

import argparse
import gzip
import json
import re
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass, field
from functools import lru_cache
from pathlib import Path
from typing import Dict, Optional, Sequence, Tuple

from codegen_benchmark_cases import TEST_CASES, TestCase
from codegen_benchmark_common import (
    DEFAULT_PRIVATE_KEY,
    DEFAULT_RPC_URL,
    load_project,
    parse_receipt_int,
    project_slice,
    run,
    stop_anvil,
)

REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
ROOT = REPOSITORY_ROOT / "testdata/codegen-runtime"
RESULT_ROOT = REPOSITORY_ROOT / "target/codegen-bench"
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
CAST_DEPLOY_TIMEOUT = 45
CAST_TX_TIMEOUT = 30
CAST_READ_TIMEOUT = 10
CAST_RPC_TIMEOUT = 10
CAST_GAS_LIMIT = "80000000"
RUNTIME_FIXTURES = ROOT / "fixtures/runtime/RuntimeFixtures.sol"

RESET = "\033[0m"
GREEN = "\033[32m"
YELLOW = "\033[33m"
RED = "\033[31m"
CYAN = "\033[36m"
BOLD = "\033[1m"
USE_COLOR = sys.stdout.isatty()
ANSI_RE = re.compile(r"\x1B\[[0-?]*[ -/]*[@-~]")


def _color(text: str, color: str) -> str:
    if not USE_COLOR:
        return text
    return f"{color}{text}{RESET}"


def visible_len(text: str) -> int:
    return len(ANSI_RE.sub("", text))


def pad_cell(text: str, width: int) -> str:
    return text + " " * max(0, width - visible_len(text))


def display_path(path: str | Path) -> str:
    path = Path(path)
    if not path.is_absolute():
        return str(path)
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        pass

    try:
        return str(path.relative_to(ROOT.parent).as_posix()).replace("../", "", 1)
    except ValueError:
        pass

    home = Path.home()
    try:
        return "~/" + str(path.relative_to(home))
    except ValueError:
        pass

    return path.name


def display_command(cmd: Sequence[str | Path]) -> str:
    sanitized = []
    for part in cmd:
        text = str(part)
        candidate = Path(text)
        sanitized.append(display_path(candidate) if candidate.is_absolute() else text)
    return " ".join(sanitized)


def verbose_log(enabled: bool, message: str) -> None:
    if enabled:
        print(message, flush=True)


def find_binary(explicit: Optional[str], candidates: Sequence[str]) -> Optional[Path]:
    if explicit:
        path = Path(explicit)
        if path.exists():
            return path.resolve()
        found = shutil.which(explicit)
        if found:
            return Path(found).resolve()
        return None

    for candidate in candidates:
        path = Path(candidate)
        if not path.is_absolute():
            path = REPOSITORY_ROOT / candidate
        if path.exists():
            return path.resolve()
        found = shutil.which(candidate)
        if found:
            return Path(found).resolve()
    return None


def binary_version(path: Path) -> Tuple[str, str]:
    result = run([str(path), "--version"], timeout=30)
    if result.returncode != 0:
        error = (result.stderr or result.stdout or "version command failed").strip()
        return "unavailable", error[:500]
    text = (result.stdout + "\n" + result.stderr).strip()
    match = re.search(r"(\d+\.\d+\.\d+(?:[-+][^\s]+)?)", text)
    version = match.group(1) if match else text.splitlines()[0] if text else "unknown"
    return version, ""


def parse_version_tuple(version: str) -> Optional[Tuple[int, int, int]]:
    match = re.match(r"(\d+)\.(\d+)\.(\d+)", version)
    if not match:
        return None
    return tuple(int(part) for part in match.groups())


def version_in_range(version: str, minimum: Optional[str], maximum: Optional[str]) -> bool:
    parsed = parse_version_tuple(version)
    if parsed is None:
        return True
    if minimum and parsed < parse_version_tuple(minimum):
        return False
    if maximum and parsed > parse_version_tuple(maximum):
        return False
    return True


@dataclass(frozen=True)
class CompilerSpec:
    compiler_id: str
    label: str
    path: Path
    kind: str


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
class SourceCase:
    test_id: str
    description: str
    project: str
    project_file: str
    source: str
    contract_name: str
    test_calls: Sequence[Tuple[str, Sequence[str]]] = field(default_factory=tuple)
    gas_calls: Sequence[GasCall] = field(default_factory=tuple)
    constructor_args: Sequence[str] = field(default_factory=tuple)
    constructor_sig: Optional[str] = None
    runtime_checks: Sequence[RuntimeCheck] = field(default_factory=tuple)
    min_solc: Optional[str] = None
    max_solc: Optional[str] = None
    suite: str = "repo"

    @property
    def project_path(self) -> Path:
        return REPOSITORY_ROOT / self.project_file


REPO_TEST_CASES: Sequence[SourceCase] = (
    SourceCase(
        test_id="uniswap-v2-pair",
        description="Uniswap V2 Pair",
        project="v2-core",
        project_file="testdata/codegen-runtime/projects/uniswap-v2-pair.json.gz",
        source="contracts/UniswapV2Pair.sol",
        contract_name="UniswapV2Pair",
        min_solc="0.5.16",
        max_solc="0.5.16",
    ),
    SourceCase(
        test_id="openzeppelin-erc20-mock",
        description="OpenZeppelin ERC20Mock",
        project="openzeppelin-contracts",
        project_file="testdata/codegen-runtime/projects/openzeppelin-runtime.json.gz",
        source="contracts/mocks/token/ERC20Mock.sol",
        contract_name="ERC20Mock",
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
            RuntimeCheck("spender-balance", "balanceOf(address)(uint256)", (DEFAULT_SPENDER,)),
            RuntimeCheck("third-balance", "balanceOf(address)(uint256)", (DEFAULT_THIRD,)),
            RuntimeCheck("fourth-balance", "balanceOf(address)(uint256)", (DEFAULT_FOURTH,)),
            RuntimeCheck("zero-balance", "balanceOf(address)(uint256)", (ZERO_ADDRESS,)),
            RuntimeCheck("supply", "totalSupply()(uint256)"),
            RuntimeCheck("allowance", "allowance(address,address)(uint256)", (DEFAULT_SENDER, DEFAULT_SPENDER)),
            RuntimeCheck("third-allowance", "allowance(address,address)(uint256)", (DEFAULT_SENDER, DEFAULT_THIRD)),
            RuntimeCheck("fourth-allowance", "allowance(address,address)(uint256)", (DEFAULT_SENDER, DEFAULT_FOURTH)),
            RuntimeCheck("reverse-allowance", "allowance(address,address)(uint256)", (DEFAULT_SPENDER, DEFAULT_SENDER)),
        ),
    ),
    SourceCase(
        test_id="openzeppelin-vesting-wallet",
        description="OpenZeppelin VestingWallet",
        project="openzeppelin-contracts",
        project_file="testdata/codegen-runtime/projects/openzeppelin-runtime.json.gz",
        source="contracts/finance/VestingWallet.sol",
        contract_name="VestingWallet",
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
            RuntimeCheck("released-token", "released(address)(uint256)", (DEFAULT_SPENDER,)),
            RuntimeCheck("releasable", "releasable()(uint256)"),
            RuntimeCheck("vested-before-start", "vestedAmount(uint64)(uint256)", ("999",)),
            RuntimeCheck("vested-at-start", "vestedAmount(uint64)(uint256)", ("1000",)),
            RuntimeCheck("vested-after-start", "vestedAmount(uint64)(uint256)", ("1001",)),
            RuntimeCheck("vested-quarter", "vestedAmount(uint64)(uint256)", ("1025",)),
            RuntimeCheck("vested-half", "vestedAmount(uint64)(uint256)", ("1050",)),
            RuntimeCheck("vested-three-quarter", "vestedAmount(uint64)(uint256)", ("1075",)),
            RuntimeCheck("vested-before-end", "vestedAmount(uint64)(uint256)", ("1099",)),
            RuntimeCheck("vested-end", "vestedAmount(uint64)(uint256)", ("1100",)),
            RuntimeCheck("vested-after-end", "vestedAmount(uint64)(uint256)", ("1101",)),
            RuntimeCheck("vested-far-future", "vestedAmount(uint64)(uint256)", ("999999",)),
        ),
    ),
    SourceCase(
        test_id="nitro-one-step-proof",
        description="Nitro OneStepProofEntry",
        project="nitro-contracts",
        project_file="testdata/codegen-runtime/projects/nitro-one-step-proof.json.gz",
        source="src/osp/OneStepProofEntry.sol",
        contract_name="OneStepProofEntry",
        constructor_args=(DEFAULT_SENDER, DEFAULT_SPENDER, DEFAULT_THIRD, DEFAULT_FOURTH),
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
    SourceCase(
        test_id="aave-l2-encoder",
        description="Aave V3 L2Encoder",
        project="aave-v3-core",
        project_file="testdata/codegen-runtime/projects/aave-l2-encoder.json.gz",
        source="fixtures/aave/L2EncoderHarness.sol",
        contract_name="L2EncoderHarness",
        test_calls=(
            ("POOL()", ()),
            ("encodeSupplyParams(address,uint256,uint16)", (DEFAULT_SPENDER, "123456", "7")),
            ("encodeWithdrawParams(address,uint256)", (DEFAULT_THIRD, MAX_UINT256)),
            ("encodeBorrowParams(address,uint256,uint256,uint16)", (DEFAULT_SPENDER, "2222", "2", "9")),
            ("encodeSetUserUseReserveAsCollateral(address,bool)", (DEFAULT_THIRD, "true")),
            ("encodeRepayWithATokensParams(address,uint256,uint256)", (DEFAULT_FOURTH, MAX_UINT256, "2")),
            ("encodeSwapBorrowRateMode(address,uint256)", (DEFAULT_SENDER, "1")),
            ("encodeRebalanceStableBorrowRate(address,address)", (DEFAULT_SPENDER, DEFAULT_THIRD)),
        ),
        runtime_checks=(
            RuntimeCheck("supply", "encodeSupplyParams(address,uint256,uint16)(bytes32)", (DEFAULT_SPENDER, "123456", "7")),
            RuntimeCheck("supply-zero", "encodeSupplyParams(address,uint256,uint16)(bytes32)", (DEFAULT_SENDER, "0", "0")),
            RuntimeCheck("supply-max-u128", "encodeSupplyParams(address,uint256,uint16)(bytes32)", (DEFAULT_FOURTH, MAX_UINT128, "65535")),
            RuntimeCheck("withdraw-zero", "encodeWithdrawParams(address,uint256)(bytes32)", (DEFAULT_SENDER, "0")),
            RuntimeCheck("withdraw-small", "encodeWithdrawParams(address,uint256)(bytes32)", (DEFAULT_SENDER, "1")),
            RuntimeCheck("withdraw-max", "encodeWithdrawParams(address,uint256)(bytes32)", (DEFAULT_THIRD, MAX_UINT256)),
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
    SourceCase(
        test_id="lilweb3-ens",
        description="LilENS",
        project="lil-web3",
        project_file="testdata/codegen-runtime/projects/lilweb3-ens.json.gz",
        source="src/LilENS.sol",
        contract_name="LilENS",
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
            RuntimeCheck("lookup-long", "lookup(string)(address)", ("long-subdomain-name",)),
            RuntimeCheck("lookup-numeric", "lookup(string)(address)", ("numeric123",)),
            RuntimeCheck("lookup-underscore", "lookup(string)(address)", ("under_score",)),
            RuntimeCheck("missing", "lookup(string)(address)", ("missing",)),
        ),
    ),
    SourceCase(
        test_id="lilweb3-flashloan",
        description="LilFlashloan",
        project="lil-web3",
        project_file="testdata/codegen-runtime/projects/lilweb3-runtime.json.gz",
        source="src/LilFlashloan.sol",
        contract_name="LilFlashloan",
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
            RuntimeCheck("computed-fee-zero", "getFee(address,uint256)(uint256)", (DEFAULT_SPENDER, "0")),
            RuntimeCheck("computed-fee-spender", "getFee(address,uint256)(uint256)", (DEFAULT_SPENDER, "10000")),
            RuntimeCheck("computed-fee-spender-small", "getFee(address,uint256)(uint256)", (DEFAULT_SPENDER, "1")),
            RuntimeCheck(
                "computed-fee-spender-rounded",
                "getFee(address,uint256)(uint256)",
                (DEFAULT_SPENDER, "33333"),
            ),
            RuntimeCheck("computed-fee-third", "getFee(address,uint256)(uint256)", (DEFAULT_THIRD, "12345")),
            RuntimeCheck("computed-fee-fourth", "getFee(address,uint256)(uint256)", (DEFAULT_FOURTH, "12345")),
            RuntimeCheck("computed-fee-sender", "getFee(address,uint256)(uint256)", (DEFAULT_SENDER, "999999")),
            RuntimeCheck("computed-fee-missing", "getFee(address,uint256)(uint256)", (ZERO_ADDRESS, "10000")),
        ),
    ),
    SourceCase(
        test_id="lilweb3-fractional",
        description="LilFractional",
        project="lil-web3",
        project_file="testdata/codegen-runtime/projects/lilweb3-runtime.json.gz",
        source="src/LilFractional.sol",
        contract_name="LilFractional",
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
            RuntimeCheck("empty-vault-zero", "getVault(uint256)(address,uint256,uint256,address)", ("0",)),
            RuntimeCheck("empty-vault-one", "getVault(uint256)(address,uint256,uint256,address)", ("1",)),
            RuntimeCheck("empty-vault-forty-two", "getVault(uint256)(address,uint256,uint256,address)", ("42",)),
            RuntimeCheck("empty-vault-max", "getVault(uint256)(address,uint256,uint256,address)", (MAX_UINT256,)),
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
    SourceCase(
        test_id="maple-erc20",
        description="Maple ERC20",
        project="maple-erc20",
        project_file="testdata/codegen-runtime/projects/maple-erc20.json.gz",
        source="contracts/ERC20.sol",
        contract_name="ERC20",
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
            RuntimeCheck("spender-balance", "balanceOf(address)(uint256)", (DEFAULT_SPENDER,)),
            RuntimeCheck("allowance", "allowance(address,address)(uint256)", (DEFAULT_SENDER, DEFAULT_SPENDER)),
            RuntimeCheck("third-allowance", "allowance(address,address)(uint256)", (DEFAULT_SENDER, DEFAULT_THIRD)),
            RuntimeCheck("fourth-allowance", "allowance(address,address)(uint256)", (DEFAULT_SENDER, DEFAULT_FOURTH)),
            RuntimeCheck("zero-allowance", "allowance(address,address)(uint256)", (DEFAULT_SENDER, ZERO_ADDRESS)),
            RuntimeCheck("reverse-allowance", "allowance(address,address)(uint256)", (DEFAULT_SPENDER, DEFAULT_SENDER)),
            RuntimeCheck("third-reverse-allowance", "allowance(address,address)(uint256)", (DEFAULT_THIRD, DEFAULT_SENDER)),
            RuntimeCheck("fourth-reverse-allowance", "allowance(address,address)(uint256)", (DEFAULT_FOURTH, DEFAULT_SENDER)),
            RuntimeCheck("nonce", "nonces(address)(uint256)", (DEFAULT_SENDER,)),
            RuntimeCheck("spender-nonce", "nonces(address)(uint256)", (DEFAULT_SPENDER,)),
            RuntimeCheck("zero-nonce", "nonces(address)(uint256)", (ZERO_ADDRESS,)),
            RuntimeCheck("permit-typehash", "PERMIT_TYPEHASH()(bytes32)"),
        ),
    ),
)


LARGE_TEST_CASES: Sequence[SourceCase] = (
    SourceCase(
        test_id="openzeppelin-governor",
        description="OpenZeppelin Governor",
        project="openzeppelin-5.6.1",
        project_file="testdata/projects/openzeppelin-5.6.1.json.gz",
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
        ),
        runtime_checks=(
            RuntimeCheck("name", "name()(string)"),
            RuntimeCheck("version", "version()(string)"),
            RuntimeCheck(
                "hash-proposal-empty",
                "hashProposal(address[],uint256[],bytes[],bytes32)(uint256)",
                ("[]", "[]", "[]", "0x" + "00" * 32),
            ),
        ),
        suite="large",
    ),
    SourceCase(
        test_id="solady-signature-checker",
        description="Solady SignatureCheckerLib",
        project="solady-0.1.26",
        project_file="testdata/projects/solady-0.1.26.json.gz",
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
    SourceCase(
        test_id="solady-lib-string",
        description="Solady LibString",
        project="solady-0.1.26",
        project_file="testdata/projects/solady-0.1.26.json.gz",
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
            GasCall("small-string", "toSmallString(string)", ("short string",), repeat=3),
            GasCall("replace-medium", "testStringReplaceMedium()", repeat=3),
            GasCall("replace-long", "testStringReplaceLong()", repeat=3),
        ),
        runtime_checks=(
            RuntimeCheck("serial-number", "checkIsSN(string)(bool)", ("123456789",)),
            RuntimeCheck("not-serial-number", "checkIsSN(string)(bool)", ("12ab",)),
            RuntimeCheck(
                "return-string",
                "returnString(string)(string)",
                ("the quick brown fox jumps over the lazy dog",),
            ),
            RuntimeCheck("small-string", "toSmallString(string)(bytes32)", ("short string",)),
        ),
        suite="large",
    ),
)


HOT_GAS_CALLS: Dict[str, Sequence[GasCall]] = {
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
        GasCall("transfer-third-125", "transfer(address,uint256)", (DEFAULT_THIRD, "125")),
        GasCall("transfer-fourth-25", "transfer(address,uint256)", (DEFAULT_FOURTH, "25")),
        GasCall("approve-spender-250", "approve(address,uint256)", (DEFAULT_SPENDER, "250")),
        GasCall("approve-third-77", "approve(address,uint256)", (DEFAULT_THIRD, "77")),
        GasCall("burn-sender-400", "burn(address,uint256)", (DEFAULT_SENDER, "400")),
        GasCall("transfer-spender-50", "transfer(address,uint256)", (DEFAULT_SPENDER, "50")),
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
            (MIXED_BYTES32, "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            repeat=2,
        ),
    ),
    "aave-l2-encoder": (
        GasCall("pool", "POOL()", repeat=2),
        GasCall("supply", "encodeSupplyParams(address,uint256,uint16)", (DEFAULT_SPENDER, "123456", "7"), repeat=2),
        GasCall("supply-zero", "encodeSupplyParams(address,uint256,uint16)", (DEFAULT_SENDER, "0", "0"), repeat=2),
        GasCall("supply-max", "encodeSupplyParams(address,uint256,uint16)", (DEFAULT_FOURTH, MAX_UINT128, "65535")),
        GasCall("withdraw-zero", "encodeWithdrawParams(address,uint256)", (DEFAULT_SENDER, "0")),
        GasCall("withdraw-max", "encodeWithdrawParams(address,uint256)", (DEFAULT_THIRD, MAX_UINT256), repeat=2),
        GasCall("borrow", "encodeBorrowParams(address,uint256,uint256,uint16)", (DEFAULT_SPENDER, "2222", "2", "9"), repeat=2),
        GasCall("borrow-zero", "encodeBorrowParams(address,uint256,uint256,uint16)", (DEFAULT_THIRD, "0", "1", "0")),
        GasCall("repay", "encodeRepayParams(address,uint256,uint256)", (DEFAULT_THIRD, "3333", "1"), repeat=2),
        GasCall("repay-max", "encodeRepayParams(address,uint256,uint256)", (DEFAULT_FOURTH, MAX_UINT256, "2")),
        GasCall("repay-atokens", "encodeRepayWithATokensParams(address,uint256,uint256)", (DEFAULT_FOURTH, MAX_UINT256, "2")),
        GasCall("swap-rate", "encodeSwapBorrowRateMode(address,uint256)", (DEFAULT_SENDER, "1"), repeat=2),
        GasCall("collateral-true", "encodeSetUserUseReserveAsCollateral(address,bool)", (DEFAULT_THIRD, "true")),
        GasCall("collateral-false", "encodeSetUserUseReserveAsCollateral(address,bool)", (DEFAULT_THIRD, "false")),
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
        GasCall("update-testname", "update(string,address)", ("testname", DEFAULT_SPENDER)),
        GasCall("register-second", "register(string)", ("second",)),
        GasCall("update-second", "update(string,address)", ("second", DEFAULT_THIRD)),
        GasCall("register-untouched", "register(string)", ("untouched",)),
        GasCall("register-empty", "register(string)", ("",)),
        GasCall("register-long", "register(string)", ("long-subdomain-name",)),
        GasCall("update-long", "update(string,address)", ("long-subdomain-name", DEFAULT_FOURTH)),
        GasCall("register-numeric", "register(string)", ("numeric123",)),
        GasCall("register-underscore", "register(string)", ("under_score",)),
        GasCall("update-underscore", "update(string,address)", ("under_score", DEFAULT_SPENDER)),
        GasCall("register-very-long", "register(string)", ("very-long-subdomain-name-with-more-bytes",)),
    ),
    "lilweb3-flashloan": (
        GasCall("manager", "manager()", repeat=2),
        GasCall("set-fee-spender", "setFees(address,uint256)", (DEFAULT_SPENDER, "250")),
        GasCall("set-fee-third", "setFees(address,uint256)", (DEFAULT_THIRD, "1000")),
        GasCall("set-fee-fourth", "setFees(address,uint256)", (DEFAULT_FOURTH, "10000")),
        GasCall("set-fee-sender", "setFees(address,uint256)", (DEFAULT_SENDER, "1")),
        GasCall("get-fee-zero", "getFee(address,uint256)", (DEFAULT_SPENDER, "0"), repeat=2),
        GasCall("get-fee-spender", "getFee(address,uint256)", (DEFAULT_SPENDER, "10000"), repeat=2),
        GasCall("get-fee-rounded", "getFee(address,uint256)", (DEFAULT_SPENDER, "33333"), repeat=2),
        GasCall("get-fee-third", "getFee(address,uint256)", (DEFAULT_THIRD, "12345"), repeat=2),
        GasCall("get-fee-fourth", "getFee(address,uint256)", (DEFAULT_FOURTH, "12345"), repeat=2),
        GasCall("get-fee-missing", "getFee(address,uint256)", (ZERO_ADDRESS, "10000")),
    ),
    "lilweb3-fractional": (
        GasCall("get-vault-zero", "getVault(uint256)", ("0",), repeat=2),
        GasCall("get-vault-one", "getVault(uint256)", ("1",), repeat=2),
        GasCall("get-vault-42", "getVault(uint256)", ("42",), repeat=2),
        GasCall("get-vault-max", "getVault(uint256)", (MAX_UINT256,)),
        GasCall("erc721-empty", "onERC721Received(address,address,uint256,bytes)", (DEFAULT_SENDER, DEFAULT_SPENDER, "7", "0x"), repeat=2),
        GasCall("erc721-nonempty", "onERC721Received(address,address,uint256,bytes)", (DEFAULT_THIRD, DEFAULT_FOURTH, "42", "0x123456"), repeat=2),
        GasCall("erc721-word", "onERC721Received(address,address,uint256,bytes)", (ZERO_ADDRESS, ZERO_ADDRESS, "0", "0x" + "11" * 32)),
    ),
    "maple-erc20": (
        GasCall("approve-spender-100", "approve(address,uint256)", (DEFAULT_SPENDER, "100")),
        GasCall("increase-spender-50", "increaseAllowance(address,uint256)", (DEFAULT_SPENDER, "50")),
        GasCall("decrease-spender-20", "decreaseAllowance(address,uint256)", (DEFAULT_SPENDER, "20")),
        GasCall("increase-spender-70", "increaseAllowance(address,uint256)", (DEFAULT_SPENDER, "70")),
        GasCall("decrease-spender-30", "decreaseAllowance(address,uint256)", (DEFAULT_SPENDER, "30")),
        GasCall("approve-third-77", "approve(address,uint256)", (DEFAULT_THIRD, "77")),
        GasCall("increase-third-23", "increaseAllowance(address,uint256)", (DEFAULT_THIRD, "23")),
        GasCall("decrease-third-20", "decreaseAllowance(address,uint256)", (DEFAULT_THIRD, "20")),
        GasCall("approve-fourth-900", "approve(address,uint256)", (DEFAULT_FOURTH, "900")),
        GasCall("increase-fourth-100", "increaseAllowance(address,uint256)", (DEFAULT_FOURTH, "100")),
        GasCall("decrease-fourth-50", "decreaseAllowance(address,uint256)", (DEFAULT_FOURTH, "50")),
        GasCall("approve-fourth-max", "approve(address,uint256)", (DEFAULT_FOURTH, MAX_UINT256)),
        GasCall("approve-zero-1", "approve(address,uint256)", (ZERO_ADDRESS, "1")),
    ),
}


def default_gas_calls(test_case: TestCase | SourceCase) -> Sequence[GasCall]:
    if isinstance(test_case, SourceCase) and test_case.gas_calls:
        return test_case.gas_calls
    return tuple(
        GasCall(signature, signature, tuple(args))
        for signature, args in test_case.test_calls
    )


def gas_calls(test_case: TestCase | SourceCase, profile: str) -> Sequence[GasCall]:
    if profile == "hot":
        return HOT_GAS_CALLS.get(test_case.test_id, default_gas_calls(test_case))
    return default_gas_calls(test_case)


MICRO_RUNTIME_CHECKS: Dict[str, Sequence[RuntimeCheck]] = {
    "factorial": (RuntimeCheck("result", "getResult()(uint256)"),),
    "counter": (RuntimeCheck("number", "number()(uint256)"),),
    "sum-array": (RuntimeCheck("total", "total()(uint256)"),),
    "arithmetic": (RuntimeCheck("value", "value()(uint256)"),),
}


def runtime_checks(test_case: TestCase | SourceCase) -> Sequence[RuntimeCheck]:
    if isinstance(test_case, SourceCase):
        return test_case.runtime_checks
    return MICRO_RUNTIME_CHECKS.get(test_case.test_id, ())


def standard_json_input(test_case: TestCase) -> str:
    payload = {
        "language": "Solidity",
        "sources": {
            f"{test_case.test_id}.sol": {
                "content": test_case.source_code,
            }
        },
        "settings": {
            "optimizer": {"enabled": True, "runs": 200},
            "viaIR": True,
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
    return json.dumps(payload)


@lru_cache(maxsize=None)
def project_standard_json_input(
    project_file: str,
    source: Optional[str] = None,
    contract_name: Optional[str] = None,
) -> str:
    path = REPOSITORY_ROOT / project_file
    if source is None:
        with gzip.open(path, mode="rt") as compressed:
            return compressed.read()

    project = load_project(path)
    settings = dict(project["settings"])
    if source is not None and contract_name is not None:
        settings["outputSelection"] = {
            source: {
                contract_name: [
                    "abi",
                    "evm.bytecode.object",
                    "evm.deployedBytecode.object",
                ]
            }
        }
    payload = {
        "language": project["language"],
        "sources": project_slice(project, source),
        "settings": settings,
    }
    return json.dumps(payload)


def parse_standard_json_output(
    stdout: str, test_case: TestCase | SourceCase
) -> Tuple[Optional[str], Optional[str], str]:
    try:
        output = json.loads(stdout)
    except json.JSONDecodeError as exc:
        return None, None, f"invalid JSON output: {exc}"

    errors = output.get("errors") or []
    fatal = [
        err.get("formattedMessage") or err.get("message") or str(err)
        for err in errors
        if err.get("severity") == "error"
    ]
    if fatal:
        return None, None, fatal[0][:1000]

    contracts = output.get("contracts") or {}
    for source_contracts in contracts.values():
        if test_case.contract_name in source_contracts:
            contract = source_contracts[test_case.contract_name]
            evm = contract.get("evm") or {}
            bytecode = ((evm.get("bytecode") or {}).get("object") or "").strip()
            deployed = ((evm.get("deployedBytecode") or {}).get("object") or "").strip()
            if bytecode:
                return bytecode, deployed, ""

    available = [
        name
        for source_contracts in contracts.values()
        for name in source_contracts.keys()
    ]
    return None, None, f"contract {test_case.contract_name} not found; available: {', '.join(available)}"


def compile_standard_json(spec: CompilerSpec, test_case: TestCase) -> Dict[str, object]:
    result = {
        "compiler_id": spec.compiler_id,
        "label": spec.label,
        "status": "pending",
        "bytecode": "",
        "runtime_bytecode": "",
        "bytecode_size": 0,
        "runtime_size": 0,
        "compile_time_seconds": 0.0,
        "peak_rss_bytes": None,
        "error": "",
    }
    sj_cmd = [str(spec.path)]
    if spec.kind != "solc":
        # Solar gates its experimental code generator behind `-Zcodegen`.
        sj_cmd.append("-Zcodegen")
    sj_cmd.append("--standard-json")
    started = time.monotonic()
    proc = run(
        sj_cmd,
        input_text=standard_json_input(test_case),
        timeout=120,
        measure_peak_rss=True,
    )
    result["compile_time_seconds"] = time.monotonic() - started
    result["peak_rss_bytes"] = proc.peak_rss_bytes
    result["command"] = display_command(sj_cmd)
    if proc.returncode != 0:
        result["status"] = "failed"
        result["error"] = (proc.stderr or proc.stdout or "compiler failed")[:1000]
        return result

    bytecode, runtime, error = parse_standard_json_output(proc.stdout, test_case)
    if not bytecode:
        result["status"] = "failed"
        result["error"] = error
        return result

    result["status"] = "ok"
    result["bytecode"] = bytecode
    result["runtime_bytecode"] = runtime or ""
    result["bytecode_size"] = len(bytecode) // 2
    result["runtime_size"] = len(runtime or "") // 2
    return result


def compile_repo_case(spec: CompilerSpec, case: SourceCase) -> Dict[str, object]:
    result = {
        "compiler_id": spec.compiler_id,
        "label": spec.label,
        "status": "pending",
        "bytecode": "",
        "runtime_bytecode": "",
        "bytecode_size": 0,
        "runtime_size": 0,
        "compile_time_seconds": 0.0,
        "peak_rss_bytes": None,
        "error": "",
        "source": case.source,
        "project": case.project,
    }

    if not case.project_path.exists():
        result["status"] = "failed"
        result["error"] = f"vendored project not found: {case.project_file}"
        return result

    cmd = [str(spec.path)]
    if spec.kind != "solc":
        cmd.append("-Zcodegen")
    cmd.append("--standard-json")
    started = time.monotonic()
    proc = run(
        cmd,
        input_text=project_standard_json_input(case.project_file, case.source, case.contract_name),
        timeout=180,
        measure_peak_rss=True,
    )
    result["compile_time_seconds"] = time.monotonic() - started
    result["peak_rss_bytes"] = proc.peak_rss_bytes
    result["command"] = display_command(cmd)
    if proc.returncode != 0:
        result["status"] = "failed"
        result["error"] = (proc.stderr or proc.stdout or "compiler failed")[:1000]
        return result

    bytecode, runtime, error = parse_standard_json_output(proc.stdout, case)
    if not bytecode:
        result["status"] = "failed"
        result["error"] = error
        return result

    result["status"] = "ok"
    result["bytecode"] = bytecode
    result["runtime_bytecode"] = runtime or ""
    result["bytecode_size"] = len(bytecode) // 2
    result["runtime_size"] = len(runtime or "") // 2
    return result


@lru_cache(maxsize=None)
def compile_runtime_fixture(solc_path: str, contract_name: str) -> Tuple[Optional[str], str]:
    source_name = str(RUNTIME_FIXTURES.relative_to(ROOT))
    payload = {
        "language": "Solidity",
        "sources": {source_name: {"content": RUNTIME_FIXTURES.read_text()}},
        "settings": {
            "optimizer": {"enabled": True, "runs": 200},
            "outputSelection": {"*": {contract_name: ["evm.bytecode.object"]}},
        },
    }
    proc = run([solc_path, "--standard-json"], input_text=json.dumps(payload), timeout=120)
    if proc.returncode != 0:
        return None, (proc.stderr or proc.stdout or "fixture compiler failed")[:1000]
    try:
        output = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        return None, f"invalid fixture compiler JSON: {exc}"
    errors = [
        error.get("formattedMessage") or error.get("message") or str(error)
        for error in output.get("errors") or []
        if error.get("severity") == "error"
    ]
    if errors:
        return None, errors[0][:1000]
    bytecode = (
        output.get("contracts", {})
        .get(source_name, {})
        .get(contract_name, {})
        .get("evm", {})
        .get("bytecode", {})
        .get("object", "")
    )
    if not bytecode:
        return None, f"fixture contract {contract_name} was not produced"
    return str(bytecode), ""


def check_tool(name: str) -> bool:
    return shutil.which(name) is not None


def start_anvil(rpc_url: str) -> subprocess.Popen[bytes]:
    match = re.fullmatch(r"http://(?:127\.0\.0\.1|localhost):(\d+)", rpc_url)
    if match is None:
        raise ValueError("`--start-anvil` requires a localhost HTTP RPC URL")
    proc = subprocess.Popen(
        [
            "anvil",
            "--port",
            match.group(1),
            "--steps-tracing",
            "--disable-code-size-limit",
            "--gas-limit",
            "100000000",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    for _ in range(100):
        if proc.poll() is not None:
            raise RuntimeError("anvil exited before accepting RPC requests")
        _, error = rpc_request("eth_chainId", (), rpc_url)
        if not error:
            return proc
        time.sleep(0.1)
    stop_anvil(proc)
    raise RuntimeError("anvil did not accept RPC requests")


def abi_encode_constructor(constructor_args: Sequence[str], constructor_sig: Optional[str]) -> Optional[str]:
    if not constructor_args:
        return ""
    if not constructor_sig:
        return None
    proc = run(["cast", "abi-encode", constructor_sig, *constructor_args], timeout=30)
    if proc.returncode != 0:
        return None
    encoded = proc.stdout.strip()
    return encoded[2:] if encoded.startswith("0x") else encoded


def deploy_contract(
    bytecode: str,
    test_case: TestCase | SourceCase,
    rpc_url: str,
    private_key: str,
) -> Tuple[Optional[str], Optional[int], str]:
    return deploy_creation_code(
        bytecode,
        test_case.constructor_args,
        getattr(test_case, "constructor_sig", None),
        rpc_url,
        private_key,
    )


def deploy_creation_code(
    bytecode: str,
    constructor_args: Sequence[str],
    constructor_sig: Optional[str],
    rpc_url: str,
    private_key: str,
) -> Tuple[Optional[str], Optional[int], str]:
    if not bytecode.startswith("0x"):
        bytecode = "0x" + bytecode

    encoded = abi_encode_constructor(constructor_args, constructor_sig)
    if encoded is None:
        return None, None, "constructor args require constructor_sig"
    bytecode += encoded

    proc = run(
        [
            "cast",
            "send",
            "--rpc-url",
            rpc_url,
            "--rpc-timeout",
            str(CAST_RPC_TIMEOUT),
            "--timeout",
            str(CAST_RPC_TIMEOUT),
            "--gas-limit",
            CAST_GAS_LIMIT,
            "--private-key",
            private_key,
            "--json",
            "--create",
            bytecode,
        ],
        timeout=CAST_DEPLOY_TIMEOUT,
    )
    if proc.returncode != 0:
        return None, None, proc.stderr[:1000]
    try:
        data = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        return None, None, f"invalid deploy JSON: {exc}"
    status = parse_receipt_int(data.get("status"))
    gas = data.get("gasUsed")
    deploy_gas = parse_receipt_int(gas)
    if status is not None and status != 1:
        return None, deploy_gas, f"deploy transaction failed (status={status}, gasUsed={deploy_gas})"
    if deploy_gas is None:
        return None, None, "deploy receipt missing gasUsed"
    return data.get("contractAddress"), deploy_gas, ""


def call_contract(
    address: str,
    signature: str,
    args: Sequence[str],
    rpc_url: str,
    private_key: str,
) -> Tuple[Optional[int], str]:
    proc = run(
        [
            "cast",
            "send",
            address,
            signature,
            *args,
            "--rpc-url",
            rpc_url,
            "--rpc-timeout",
            str(CAST_RPC_TIMEOUT),
            "--timeout",
            str(CAST_RPC_TIMEOUT),
            "--gas-limit",
            CAST_GAS_LIMIT,
            "--private-key",
            private_key,
            "--json",
        ],
        timeout=CAST_TX_TIMEOUT,
    )
    if proc.returncode != 0:
        return None, proc.stderr[:1000]
    try:
        data = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        return None, f"invalid call JSON: {exc}"
    status = parse_receipt_int(data.get("status"))
    gas = parse_receipt_int(data.get("gasUsed"))
    if status is not None and status != 1:
        return None, f"transaction failed (status={status}, gasUsed={gas})"
    if gas is None:
        return None, "transaction receipt missing gasUsed"
    return gas, ""


def read_contract(
    address: str,
    signature: str,
    args: Sequence[str],
    rpc_url: str,
) -> Tuple[Optional[str], str]:
    proc = run(
        [
            "cast",
            "call",
            address,
            signature,
            *args,
            "--rpc-url",
            rpc_url,
            "--rpc-timeout",
            str(CAST_RPC_TIMEOUT),
            "--gas-limit",
            CAST_GAS_LIMIT,
        ],
        timeout=CAST_READ_TIMEOUT,
    )
    if proc.returncode != 0:
        return None, proc.stderr[:1000]
    value = " ".join(proc.stdout.split())
    if value.startswith("0x"):
        value = value.lower()
    return value, ""


def rpc_request(method: str, params: Sequence[object], rpc_url: str) -> Tuple[Optional[object], str]:
    proc = run(
        ["cast", "rpc", "--rpc-url", rpc_url, "--raw", method],
        input_text=json.dumps(list(params)),
        timeout=CAST_READ_TIMEOUT,
    )
    if proc.returncode != 0:
        return None, (proc.stderr or proc.stdout or f"{method} failed")[:1000]
    try:
        return json.loads(proc.stdout), ""
    except json.JSONDecodeError as exc:
        return None, f"invalid {method} JSON: {exc}"


def send_value(address: str, amount: str, rpc_url: str, private_key: str) -> str:
    proc = run(
        [
            "cast",
            "send",
            address,
            "--value",
            amount,
            "--rpc-url",
            rpc_url,
            "--rpc-timeout",
            str(CAST_RPC_TIMEOUT),
            "--timeout",
            str(CAST_RPC_TIMEOUT),
            "--gas-limit",
            CAST_GAS_LIMIT,
            "--private-key",
            private_key,
            "--json",
        ],
        timeout=CAST_TX_TIMEOUT,
    )
    if proc.returncode != 0:
        return proc.stderr[:1000]
    try:
        receipt = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        return f"invalid value-transfer JSON: {exc}"
    status = parse_receipt_int(receipt.get("status"))
    return "" if status in (None, 1) else f"value transfer failed (status={status})"


def encode_calldata(signature: str, args: Sequence[str]) -> Tuple[Optional[str], str]:
    proc = run(["cast", "calldata", signature, *args], timeout=30)
    if proc.returncode != 0:
        return None, proc.stderr[:1000]
    return proc.stdout.strip(), ""


def eth_call_raw(
    address: str,
    signature: str,
    args: Sequence[str],
    rpc_url: str,
) -> Tuple[Optional[str], Optional[str], str]:
    calldata, error = encode_calldata(signature, args)
    if calldata is None:
        return None, None, error
    proc = run(
        [
            "cast",
            "rpc",
            "--rpc-url",
            rpc_url,
            "--raw",
            "eth_call",
            json.dumps([{"to": address, "data": calldata}, "latest"]),
        ],
        timeout=CAST_READ_TIMEOUT,
    )
    if proc.returncode == 0:
        try:
            value = json.loads(proc.stdout)
        except json.JSONDecodeError as exc:
            return None, None, f"invalid eth_call JSON: {exc}"
        return str(value).lower(), None, ""

    message = proc.stderr or proc.stdout or "eth_call failed"
    data_matches = re.findall(r"0x[0-9a-fA-F]+", message)
    if data_matches:
        return None, data_matches[-1].lower(), ""
    return None, None, message[:1000]


def runtime_ok(label: str, value: object) -> Dict[str, object]:
    return {"label": label, "status": "ok", "value": str(value)}


def runtime_error(label: str, error: str) -> Dict[str, object]:
    return {"label": label, "status": "failed", "error": error}


def checked_value(label: str, actual: object, expected: object) -> Dict[str, object]:
    actual_text = str(actual)
    expected_text = str(expected)
    if actual_text != expected_text:
        return runtime_error(label, f"expected {expected_text}, got {actual_text}")
    return runtime_ok(label, actual_text)


def read_uint(
    address: str,
    signature: str,
    args: Sequence[str],
    rpc_url: str,
) -> Tuple[Optional[int], str]:
    value, error = read_contract(address, signature, args, rpc_url)
    if value is None:
        return None, error
    try:
        return int(value.split()[0], 0), ""
    except (ValueError, IndexError):
        return None, f"invalid uint result: {value}"


def read_address(
    address: str,
    signature: str,
    args: Sequence[str],
    rpc_url: str,
) -> Tuple[Optional[str], str]:
    value, error = read_contract(address, signature, args, rpc_url)
    if value is None:
        return None, error
    match = re.search(r"0x[0-9a-fA-F]{40}", value)
    return (match.group(0).lower(), "") if match else (None, f"invalid address result: {value}")


def decode_words(data: str) -> List[int]:
    raw = data[2:] if data.startswith("0x") else data
    if len(raw) % 64 != 0:
        raise ValueError(f"ABI result has {len(raw)} hex digits")
    return [int(raw[index:index + 64], 16) for index in range(0, len(raw), 64)]


def run_vesting_cold_paths(
    address: str,
    solc_path: Path,
    rpc_url: str,
    private_key: str,
) -> List[Dict[str, object]]:
    error = send_value(address, "1000", rpc_url, private_key)
    if error:
        return [runtime_error("cold-vesting-eth-setup", error)]
    _, error = call_contract(address, "release()", (), rpc_url, private_key)
    if error:
        return [runtime_error("cold-vesting-eth-release", error)]

    token_bytecode, error = compile_runtime_fixture(str(solc_path), "RuntimeERC20")
    if token_bytecode is None:
        return [runtime_error("cold-vesting-token-compile", error)]
    token, _, error = deploy_creation_code(
        token_bytecode,
        (address, "2000"),
        "constructor(address,uint256)",
        rpc_url,
        private_key,
    )
    if token is None:
        return [runtime_error("cold-vesting-token-deploy", error)]
    _, error = call_contract(address, "release(address)", (token,), rpc_url, private_key)
    if error:
        return [runtime_error("cold-vesting-token-release", error)]

    observations = []
    reads = (
        ("cold-vesting-released-eth", address, "released()(uint256)", (), 1000),
        ("cold-vesting-releasable-eth", address, "releasable()(uint256)", (), 0),
        ("cold-vesting-released-token", address, "released(address)(uint256)", (token,), 2000),
        ("cold-vesting-releasable-token", address, "releasable(address)(uint256)", (token,), 0),
        ("cold-vesting-token-empty", token, "balanceOf(address)(uint256)", (address,), 0),
        ("cold-vesting-owner-token", token, "balanceOf(address)(uint256)", (DEFAULT_SENDER,), 2000),
    )
    for label, target, signature, args, expected in reads:
        value, error = read_uint(target, signature, args, rpc_url)
        observations.append(runtime_error(label, error) if value is None else checked_value(label, value, expected))
    balance, error = rpc_request("eth_getBalance", (address, "latest"), rpc_url)
    if balance is None:
        observations.append(runtime_error("cold-vesting-eth-empty", error))
    else:
        observations.append(checked_value("cold-vesting-eth-empty", int(str(balance), 16), 0))
    return observations


def run_fractional_cold_paths(
    address: str,
    solc_path: Path,
    rpc_url: str,
    private_key: str,
) -> List[Dict[str, object]]:
    nft_bytecode, error = compile_runtime_fixture(str(solc_path), "RuntimeNFT")
    if nft_bytecode is None:
        return [runtime_error("cold-fractional-nft-compile", error)]
    nft, _, error = deploy_creation_code(nft_bytecode, (), None, rpc_url, private_key)
    if nft is None:
        return [runtime_error("cold-fractional-nft-deploy", error)]
    _, error = call_contract(nft, "setApprovalForAll(address,bool)", (address, "true"), rpc_url, private_key)
    if error:
        return [runtime_error("cold-fractional-approve-nft", error)]
    _, error = call_contract(
        address,
        "split(address,uint256,uint256,string,string)",
        (nft, "1", "1000", "Fractionalized NFT", "FRAC"),
        rpc_url,
        private_key,
    )
    if error:
        return [runtime_error("cold-fractional-split", error)]

    vault_data, _, error = eth_call_raw(address, "getVault(uint256)", ("1",), rpc_url)
    if vault_data is None:
        return [runtime_error("cold-fractional-vault", error or "getVault reverted")]
    try:
        vault = decode_words(vault_data)
    except ValueError as exc:
        return [runtime_error("cold-fractional-vault", str(exc))]
    if len(vault) != 4:
        return [runtime_error("cold-fractional-vault", f"expected 4 words, got {len(vault)}")]
    token = "0x" + f"{vault[3]:040x}"

    observations = [
        checked_value("cold-fractional-vault-nft", vault[0] == int(nft, 16), True),
        checked_value("cold-fractional-vault-token-id", vault[1], 1),
        checked_value("cold-fractional-vault-supply", vault[2], 1000),
        checked_value("cold-fractional-share-created", vault[3] != 0, True),
    ]
    nft_owner, error = read_address(nft, "ownerOf(uint256)(address)", ("1",), rpc_url)
    observations.append(
        runtime_error("cold-fractional-nft-custody", error)
        if nft_owner is None
        else checked_value("cold-fractional-nft-custody", nft_owner == address.lower(), True)
    )
    share_balance, error = read_uint(token, "balanceOf(address)(uint256)", (DEFAULT_SENDER,), rpc_url)
    observations.append(
        runtime_error("cold-fractional-share-minted", error)
        if share_balance is None
        else checked_value("cold-fractional-share-minted", share_balance, 1000)
    )

    _, error = call_contract(token, "approve(address,uint256)", (address, MAX_UINT256), rpc_url, private_key)
    if error:
        observations.append(runtime_error("cold-fractional-approve-share", error))
        return observations
    _, error = call_contract(address, "join(uint256)", ("1",), rpc_url, private_key)
    if error:
        observations.append(runtime_error("cold-fractional-join", error))
        return observations

    empty_vault, _, error = eth_call_raw(address, "getVault(uint256)", ("1",), rpc_url)
    if empty_vault is None:
        observations.append(runtime_error("cold-fractional-vault-cleared", error or "getVault reverted"))
    else:
        observations.append(checked_value("cold-fractional-vault-cleared", all(word == 0 for word in decode_words(empty_vault)), True))
    nft_owner, error = read_address(nft, "ownerOf(uint256)(address)", ("1",), rpc_url)
    observations.append(
        runtime_error("cold-fractional-nft-returned", error)
        if nft_owner is None
        else checked_value("cold-fractional-nft-returned", nft_owner, DEFAULT_SENDER.lower())
    )
    share_balance, error = read_uint(token, "balanceOf(address)(uint256)", (DEFAULT_SENDER,), rpc_url)
    observations.append(
        runtime_error("cold-fractional-share-burned", error)
        if share_balance is None
        else checked_value("cold-fractional-share-burned", share_balance, 0)
    )
    return observations


def keccak256(data: bytes) -> bytes:
    proc = run(["cast", "keccak", "0x" + data.hex()], timeout=30)
    if proc.returncode != 0:
        raise RuntimeError(proc.stderr[:1000])
    return bytes.fromhex(proc.stdout.strip().removeprefix("0x"))


@lru_cache(maxsize=None)
def nitro_dispatch_vector(opcode: int) -> Tuple[str, str]:
    zero = bytes(32)
    u32_zero = bytes(4)
    u64_zero = bytes(8)
    u256_zero = bytes(32)
    instruction = opcode.to_bytes(2, "big") + u256_zero
    instructions_hash = keccak256(b"Instructions:" + b"\x01" + instruction)
    functions_root = keccak256(b"Function:" + instructions_hash)
    memory_hash = keccak256(b"Memory:" + u64_zero + u64_zero + zero)
    module = zero + u64_zero + u64_zero + zero + zero + functions_root + zero + u32_zero
    module_hash = keccak256(b"Module:" + zero + memory_hash + zero + functions_root + zero + u32_zero)

    inactive_multi = zero + zero
    multistack_hash = keccak256(b"multistack:" + zero + zero + zero)
    recovery_pc = bytes([0xff]) * 32
    machine = (
        b"\x00"
        + zero + u256_zero
        + inactive_multi
        + zero + u256_zero
        + zero + b"\x00"
        + inactive_multi
        + zero
        + u32_zero + u32_zero + u32_zero
        + recovery_pc
        + module_hash
    )
    before_hash = keccak256(
        b"Machine running:"
        + multistack_hash
        + zero
        + multistack_hash
        + zero
        + u32_zero + u32_zero + u32_zero
        + recovery_pc
        + module_hash
    )
    proof = machine + module + b"\x00" + b"\x01" + instruction + b"\x00\x00"
    return "0x" + before_hash.hex(), "0x" + proof.hex()


def run_nitro_cold_paths(address: str, rpc_url: str) -> List[Dict[str, object]]:
    dispatches = (
        ("prover0", 0x01, DEFAULT_SENDER, 1),
        ("prover-mem", 0x28, DEFAULT_SPENDER, 2),
        ("prover-math", 0x6A, DEFAULT_THIRD, 3),
        ("prover-host-io", 0x8010, DEFAULT_FOURTH, 4),
    )
    observations = []
    try:
        for _, _, prover, marker in dispatches:
            code = f"0x60{marker:02x}60005260206000fd"
            _, error = rpc_request("anvil_setCode", (prover, code), rpc_url)
            if error:
                return [runtime_error("cold-nitro-install-provers", error)]

        for label, opcode, _, marker in dispatches:
            try:
                before_hash, proof = nitro_dispatch_vector(opcode)
            except RuntimeError as exc:
                observations.append(runtime_error(f"cold-nitro-{label}", str(exc)))
                continue
            _, revert_data, error = eth_call_raw(
                address,
                "proveOneStep((uint256,address,bytes32),uint256,bytes32,bytes)",
                (f"(1000000,{ZERO_ADDRESS},0x{'00' * 32})", "0", before_hash, proof),
                rpc_url,
            )
            expected = f"0x{marker:064x}"
            if error:
                observations.append(runtime_error(f"cold-nitro-{label}", error))
            elif revert_data is None:
                observations.append(runtime_error(f"cold-nitro-{label}", "expected mock-prover revert"))
            else:
                observations.append(checked_value(f"cold-nitro-{label}", revert_data, expected))
    finally:
        for _, _, prover, _ in dispatches:
            rpc_request("anvil_setCode", (prover, "0x"), rpc_url)
    return observations


def run_cold_path_checks(
    test_case: TestCase | SourceCase,
    address: str,
    solc_path: Path,
    rpc_url: str,
    private_key: str,
) -> List[Dict[str, object]]:
    if test_case.test_id == "openzeppelin-vesting-wallet":
        return run_vesting_cold_paths(address, solc_path, rpc_url, private_key)
    if test_case.test_id == "lilweb3-fractional":
        return run_fractional_cold_paths(address, solc_path, rpc_url, private_key)
    if test_case.test_id == "nitro-one-step-proof":
        return run_nitro_cold_paths(address, rpc_url)
    return []


def compare_runtime_results(entry: Dict[str, object], specs: Sequence[CompilerSpec]) -> None:
    labels = []
    values_by_compiler: Dict[str, Dict[str, str]] = {}
    failed = False

    for spec in specs:
        data = entry["compilers"].get(spec.compiler_id, {})
        check_results = data.get("runtime_results") or []
        if data.get("runtime_status") == "failed":
            failed = True
        values_by_compiler[spec.compiler_id] = {
            str(result.get("label")): str(result.get("value"))
            for result in check_results
            if result.get("status") == "ok"
        }
        for result in check_results:
            label = str(result.get("label"))
            if label not in labels:
                labels.append(label)

    if not labels:
        entry["runtime_status"] = "skipped"
        return

    mismatches = []
    for label in labels:
        values = {
            spec.compiler_id: values_by_compiler.get(spec.compiler_id, {}).get(label)
            for spec in specs
        }
        if any(value is None for value in values.values()):
            failed = True
            continue
        unique_values = set(values.values())
        if len(unique_values) > 1:
            mismatches.append({"label": label, "values": values})

    entry["runtime_mismatches"] = mismatches
    if mismatches:
        entry["runtime_status"] = "mismatch"
    elif failed:
        entry["runtime_status"] = "failed"
    else:
        entry["runtime_status"] = "ok"


def run_test_case(
    test_case: TestCase | SourceCase,
    specs: Sequence[CompilerSpec],
    include_gas: bool,
    gas_profile: str,
    rpc_url: str,
    private_key: str,
    verbose: bool = False,
) -> Dict[str, object]:
    entry: Dict[str, object] = {
        "test_id": test_case.test_id,
        "description": test_case.description,
        "contract_name": test_case.contract_name,
        "suite": test_case.suite if isinstance(test_case, SourceCase) else "micro",
        "gas_profile": gas_profile,
        "compilers": {},
    }
    if isinstance(test_case, SourceCase):
        entry["project"] = test_case.project
        entry["source"] = test_case.source
    reference_solc = next((spec for spec in specs if spec.kind == "solc"), None)

    for spec in specs:
        verbose_log(verbose, f"[{test_case.test_id}] compiling with {spec.compiler_id}")
        compiled = (
            compile_repo_case(spec, test_case)
            if isinstance(test_case, SourceCase)
            else compile_standard_json(spec, test_case)
        )
        compiler_entry = dict(compiled)
        compiler_entry.pop("bytecode", None)
        compiler_entry.pop("runtime_bytecode", None)
        entry["compilers"][spec.compiler_id] = compiler_entry

        checks = runtime_checks(test_case)
        calls = gas_calls(test_case, gas_profile)
        has_cold_paths = test_case.test_id in {
            "openzeppelin-vesting-wallet",
            "nitro-one-step-proof",
            "lilweb3-fractional",
        }
        if compiled["status"] != "ok" or not include_gas:
            continue
        if not calls and not checks and not has_cold_paths:
            compiler_entry["deploy_status"] = "skipped"
            compiler_entry["runtime_status"] = "skipped"
            continue

        verbose_log(verbose, f"[{test_case.test_id}] {spec.compiler_id} deploy")
        address, deploy_gas, deploy_error = deploy_contract(
            str(compiled["bytecode"]),
            test_case,
            rpc_url,
            private_key,
        )
        if not address:
            compiler_entry["deploy_status"] = "failed"
            compiler_entry["runtime_status"] = "failed" if checks else "skipped"
            compiler_entry["deploy_error"] = deploy_error
            continue

        compiler_entry["deploy_status"] = "ok"
        compiler_entry["deploy_gas"] = deploy_gas
        compiler_entry["address"] = address
        gas_results = []
        total_gas = 0
        gas_failed = False
        for call in calls:
            for index in range(call.repeat):
                label = call.label if call.repeat == 1 else f"{call.label}#{index + 1}"
                verbose_log(verbose, f"[{test_case.test_id}] {spec.compiler_id} tx {label}: {call.signature}")
                gas, error = call_contract(address, call.signature, call.args, rpc_url, private_key)
                if gas is None:
                    gas_failed = True
                    gas_results.append({
                        "label": label,
                        "call": call.signature,
                        "args": list(call.args),
                        "gas": None,
                        "error": error,
                    })
                    continue
                gas_results.append({
                    "label": label,
                    "call": call.signature,
                    "args": list(call.args),
                    "gas": gas,
                })
                total_gas += gas
        compiler_entry["gas_results"] = gas_results
        compiler_entry["gas_status"] = "failed" if gas_failed else "ok"
        compiler_entry["total_gas"] = None if gas_failed else total_gas

        runtime_results = []
        runtime_failed = False
        for check in checks:
            verbose_log(verbose, f"[{test_case.test_id}] {spec.compiler_id} read {check.label}: {check.signature}")
            value, error = read_contract(address, check.signature, check.args, rpc_url)
            if value is None:
                runtime_failed = True
                runtime_results.append({
                    "label": check.label,
                    "call": check.signature,
                    "args": list(check.args),
                    "status": "failed",
                    "error": error,
                })
                continue
            runtime_results.append({
                "label": check.label,
                "call": check.signature,
                "args": list(check.args),
                "status": "ok",
                "value": value,
            })
        if has_cold_paths:
            if reference_solc is None:
                cold_results = [runtime_error("cold-path-setup", "reference solc is required")]
            else:
                verbose_log(verbose, f"[{test_case.test_id}] {spec.compiler_id} cold-path differential")
                cold_results = run_cold_path_checks(
                    test_case,
                    address,
                    reference_solc.path,
                    rpc_url,
                    private_key,
                )
            runtime_results.extend(cold_results)
            runtime_failed |= any(result.get("status") != "ok" for result in cold_results)
        if checks or has_cold_paths:
            compiler_entry["runtime_results"] = runtime_results
            compiler_entry["runtime_status"] = "failed" if runtime_failed else "ok"

    if include_gas:
        compare_runtime_results(entry, specs)

    return entry


def pct_delta(old: int, new: int) -> str:
    if old <= 0 or new <= 0:
        return "N/A"
    delta = ((old - new) / old) * 100.0
    color = GREEN if delta >= 0 else RED
    prefix = "+" if delta >= 0 else ""
    return _color(f"{prefix}{delta:.2f}%", color)


def print_size_table(results: Sequence[Dict[str, object]], specs: Sequence[CompilerSpec]) -> None:
    print("\n" + _color("Code Size Comparison", BOLD))
    test_width = max(22, *(visible_len(str(result["test_id"])) for result in results)) if results else 22
    header = f"{'Test':<{test_width}}"
    for spec in specs:
        header += f" | {spec.compiler_id:<13}"
    header += " | Solar vs solc"
    print(header)
    print("-" * len(header))

    for result in results:
        row = f"{str(result['test_id']):<{test_width}}"
        sizes = []
        for spec in specs:
            data = result["compilers"].get(spec.compiler_id, {})
            size = int(data.get("runtime_size") or data.get("bytecode_size") or 0)
            sizes.append(size)
            if data.get("status") != "ok":
                cell = _color("FAILED", RED)
            else:
                cell = f"{size:,}B"
            row += f" | {pad_cell(cell, 13)}"
        row += f" | {pct_delta(sizes[0], sizes[1]) if len(sizes) >= 2 else 'N/A'}"
        print(row)


def print_memory_table(results: Sequence[Dict[str, object]], specs: Sequence[CompilerSpec]) -> None:
    def peak_rss(result: Dict[str, object], compiler_id: str) -> Optional[int]:
        data = result["compilers"].get(compiler_id, {})
        value = data.get("peak_rss_bytes")
        if data.get("status") != "ok" or not isinstance(value, int):
            return None
        return value

    values_by_compiler = {
        spec.compiler_id: [
            (str(result["test_id"]), value)
            for result in results
            if (value := peak_rss(result, spec.compiler_id)) is not None
        ]
        for spec in specs
    }
    if not any(values_by_compiler.values()):
        return

    print("\n" + _color("Peak RSS Comparison", BOLD))
    test_width = max(22, *(visible_len(str(result["test_id"])) for result in results)) if results else 22
    header = f"{'Test':<{test_width}}"
    for spec in specs:
        header += f" | {spec.compiler_id:<13}"
    if {"solar", "solc"}.issubset(values_by_compiler):
        header += " | Solar vs solc"
    print(header)
    print("-" * len(header))

    for result in results:
        row = f"{str(result['test_id']):<{test_width}}"
        values = {}
        for spec in specs:
            value = peak_rss(result, spec.compiler_id)
            values[spec.compiler_id] = value
            cell = "N/A" if value is None else f"{value / (1024 * 1024):,.1f} MiB"
            row += f" | {cell:<13}"
        if "solar" in values and "solc" in values:
            row += f" | {pct_delta(values['solc'] or 0, values['solar'] or 0)}"
        print(row)

    print("\nPeak RSS Summary")
    print("Compiler      | Benches | Average       | Maximum       | Maximum bench")
    print("--------------|---------|---------------|---------------|--------------")
    for spec in specs:
        values = values_by_compiler[spec.compiler_id]
        if not values:
            continue
        max_bench, maximum = max(values, key=lambda item: item[1])
        average = sum(value for _, value in values) / len(values)
        print(
            f"{spec.compiler_id:<13} | {len(values):>7} | "
            f"{average / (1024 * 1024):>9,.1f} MiB | "
            f"{maximum / (1024 * 1024):>9,.1f} MiB | {max_bench}"
        )


def print_runtime_table(results: Sequence[Dict[str, object]]) -> None:
    print("\n" + _color("Runtime Result Match", BOLD))
    test_width = max(22, *(visible_len(str(result["test_id"])) for result in results)) if results else 22
    header = f"{'Test':<{test_width}} | Status       | Checks"
    print(header)
    print("-" * len(header))

    for result in results:
        status = str(result.get("runtime_status") or "skipped")
        if status == "ok":
            cell = _color("OK", GREEN)
        elif status == "mismatch":
            cell = _color("MISMATCH", RED)
        elif status == "failed":
            cell = _color("ERROR", RED)
        else:
            cell = _color("N/A", YELLOW)
        checks = max(
            (len((data or {}).get("runtime_results") or []) for data in result["compilers"].values()),
            default=0,
        )
        row = f"{str(result['test_id']):<{test_width}} | {pad_cell(cell, 12)} | {checks if checks else 'N/A'}"
        print(row)


def print_gas_table(results: Sequence[Dict[str, object]], specs: Sequence[CompilerSpec]) -> None:
    print("\n" + _color("Gas Comparison", BOLD))
    test_width = max(22, *(visible_len(str(result["test_id"])) for result in results)) if results else 22
    header = f"{'Test':<{test_width}}"
    for spec in specs:
        header += f" | {spec.compiler_id:<13}"
    header += " | Solar vs solc"
    print(header)
    print("-" * len(header))

    for result in results:
        row = f"{str(result['test_id']):<{test_width}}"
        totals = []
        for spec in specs:
            data = result["compilers"].get(spec.compiler_id, {})
            raw_gas = data.get("total_gas")
            gas = int(raw_gas) if raw_gas is not None else None
            totals.append(gas)
            if data.get("status") != "ok":
                cell = _color("COMPILE_ERR", RED)
            elif data.get("deploy_status") == "failed":
                cell = _color("DEPLOY_ERR", RED)
            elif data.get("gas_status") == "failed":
                cell = _color("TX_ERR", RED)
            elif gas is None or gas <= 0:
                cell = _color("N/A", YELLOW)
            else:
                cell = f"{gas:,}"
            row += f" | {pad_cell(cell, 13)}"
        row += f" | {pct_delta(totals[0], totals[1]) if len(totals) >= 2 and None not in totals else 'N/A'}"
        print(row)


def print_gas_breakdown_table(results: Sequence[Dict[str, object]], specs: Sequence[CompilerSpec]) -> None:
    print("\n" + _color("Per-call Gas Breakdown", BOLD))
    for result in results:
        if not result.get("compilers"):
            continue
        rows_by_compiler = {
            spec.compiler_id: result["compilers"].get(spec.compiler_id, {}).get("gas_results") or []
            for spec in specs
        }
        if not any(rows_by_compiler.values()):
            continue

        print(f"\n{result['test_id']}")
        label_width = max(
            12,
            *(
                visible_len(str(row.get("label") or row.get("call") or ""))
                for rows in rows_by_compiler.values()
                for row in rows
            ),
        )
        header = f"{'Call':<{label_width}}"
        for spec in specs:
            header += f" | {spec.compiler_id:<13}"
        header += " | Solar vs solc"
        print(header)
        print("-" * len(header))

        max_len = max((len(rows) for rows in rows_by_compiler.values()), default=0)
        for index in range(max_len):
            label = ""
            values = []
            row = None
            for spec in specs:
                rows = rows_by_compiler.get(spec.compiler_id) or []
                if index < len(rows):
                    row = rows[index]
                    label = label or str(row.get("label") or row.get("call") or "")
                    break
            line = f"{label:<{label_width}}"
            for spec in specs:
                rows = rows_by_compiler.get(spec.compiler_id) or []
                if index >= len(rows):
                    values.append(0)
                    line += f" | {pad_cell(_color('N/A', YELLOW), 13)}"
                    continue
                row = rows[index]
                gas = row.get("gas")
                if gas is None:
                    values.append(0)
                    line += f" | {pad_cell(_color('ERR', RED), 13)}"
                else:
                    gas_int = int(gas)
                    values.append(gas_int)
                    line += f" | {gas_int:<13,}"
            line += f" | {pct_delta(values[0], values[1]) if len(values) >= 2 else 'N/A'}"
            print(line)


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        description="Benchmark solc vs Solar codegen on inline and repository contracts"
    )
    parser.add_argument("--solc", default="solc", help="Path to solc binary (default: solc)")
    parser.add_argument(
        "--solar",
        help="Path to solar binary (default: solar or target/{release,debug}/solar)",
    )
    parser.add_argument(
        "--suite",
        choices=("micro", "repo", "large", "all"),
        default="micro",
        help="Benchmark suite to run",
    )
    parser.add_argument("--tests", nargs="*", help="Subset of test IDs to run")
    parser.add_argument("--projects", nargs="*", help="Subset of repository project names to run")
    parser.add_argument("--list-tests", action="store_true", help="List available tests and exit")
    parser.add_argument(
        "--include-incompatible",
        action="store_true",
        help="Run repo contracts even when their pragma is incompatible with the selected solc",
    )
    parser.add_argument("--gas", action="store_true", help="Deploy, execute gas calls, and compare runtime results with cast/anvil")
    parser.add_argument(
        "--gas-profile",
        choices=("smoke", "hot"),
        default="smoke",
        help="Gas workload profile: smoke preserves the existing calls, hot runs a broader optimizer workload",
    )
    parser.add_argument("--gas-breakdown", action="store_true", help="Print per-call gas results")
    parser.add_argument("--start-anvil", action="store_true", help="Start anvil automatically for --gas")
    parser.add_argument("--rpc-url", default=DEFAULT_RPC_URL, help=f"RPC URL (default: {DEFAULT_RPC_URL})")
    parser.add_argument("--private-key", default=DEFAULT_PRIVATE_KEY, help="Private key for transactions")
    parser.add_argument("--output", help="Output JSON path")
    parser.add_argument("--verbose", action="store_true", help="Print compiler errors for failed rows")
    parser.add_argument(
        "--allow-failures",
        action="store_true",
        help="Exit successfully even if a compiler fails for one or more tests",
    )
    args = parser.parse_args(argv)

    all_tests = []
    if args.suite in ("micro", "all"):
        all_tests.extend(TEST_CASES)
    if args.suite in ("repo", "all"):
        all_tests.extend(REPO_TEST_CASES)
    if args.suite in ("large", "all"):
        all_tests.extend(LARGE_TEST_CASES)

    if args.projects:
        project_set = set(args.projects)
        all_tests = [
            test for test in all_tests
            if isinstance(test, SourceCase) and test.project in project_set
        ]

    test_map = {test.test_id: test for test in all_tests}
    if args.list_tests:
        for test in all_tests:
            if isinstance(test, SourceCase):
                print(f"{test.test_id}	{test.project}	{test.source}	{test.contract_name}")
            else:
                print(f"{test.test_id}	micro	inline	{test.contract_name}")
        return 0

    solc = find_binary(args.solc, ["solc"])
    if not solc:
        print(_color(f"solc not found: {args.solc}", RED), file=sys.stderr)
        return 1

    solar = find_binary(
        args.solar,
        [
            "solar",
            "target/release/solar",
            "target/debug/solar",
        ],
    )
    if not solar:
        print(_color("solar binary not found; build Solar or pass --solar /path/to/solar", RED), file=sys.stderr)
        return 1

    if args.gas and not check_tool("cast"):
        print(_color("cast not found; install Foundry or omit --gas", RED), file=sys.stderr)
        return 1

    solc_version, solc_version_error = binary_version(solc)
    solar_version, solar_version_error = binary_version(solar)

    specs = [
        CompilerSpec("solc", f"solc {solc_version}", solc, "solc"),
        CompilerSpec("solar", f"solar {solar_version}", solar, "solar"),
    ]

    if args.tests:
        missing = [test_id for test_id in args.tests if test_id not in test_map]
        if missing:
            print(_color(f"unknown test IDs: {', '.join(missing)}", RED), file=sys.stderr)
            return 1
        tests = [test_map[test_id] for test_id in args.tests]
    else:
        tests = list(all_tests)

    skipped = []
    if not args.include_incompatible:
        compatible_tests = []
        for test in tests:
            if isinstance(test, SourceCase) and not version_in_range(solc_version, test.min_solc, test.max_solc):
                skipped.append(test)
            else:
                compatible_tests.append(test)
        tests = compatible_tests

    if args.gas and any(isinstance(test, SourceCase) and not gas_calls(test, args.gas_profile) and not runtime_checks(test) for test in tests):
        print(_color("repo contracts without gas calls or runtime checks will show N/A in the gas table", YELLOW), file=sys.stderr)

    if args.gas and args.start_anvil and not check_tool("anvil"):
        print(_color("anvil not found; install Foundry or omit --start-anvil", RED), file=sys.stderr)
        return 1

    print(f"Using {specs[0].label}")
    if solc_version_error:
        print(
            _color(
                "Warning: `solc --version` failed. If this is solc-select, run "
                "`solc-select install 0.8.30 && solc-select use 0.8.30` or pass --solc /path/to/solc.",
                YELLOW,
            ),
            file=sys.stderr,
        )
    print(f"Using {specs[1].label}")
    if solar_version_error:
        print(_color(f"Warning: `solar --version` failed: {solar_version_error}", YELLOW), file=sys.stderr)
    if skipped:
        skipped_ids = ", ".join(test.test_id for test in skipped)
        print(_color(f"Skipping {len(skipped)} incompatible tests for solc {solc_version}: {skipped_ids}", YELLOW))
    print(f"Running {len(tests)} tests")

    results = []
    timings = {}
    started = time.monotonic()
    anvil_proc = None
    try:
        if args.gas and args.start_anvil:
            print("Starting anvil...")
            anvil_proc = start_anvil(args.rpc_url)
        current_suite = None
        suite_started = None
        for test in tests:
            suite = test.suite if isinstance(test, SourceCase) else "micro"
            if suite != current_suite:
                if current_suite is not None and suite_started is not None:
                    timings[current_suite] = timings.get(current_suite, 0.0) + time.monotonic() - suite_started
                current_suite = suite
                suite_started = time.monotonic()
            results.append(
                run_test_case(
                    test,
                    specs,
                    args.gas,
                    args.gas_profile,
                    args.rpc_url,
                    args.private_key,
                    args.verbose,
                )
            )
        if current_suite is not None and suite_started is not None:
            timings[current_suite] = timings.get(current_suite, 0.0) + time.monotonic() - suite_started
    finally:
        if anvil_proc:
            print("Stopping anvil...")
            stop_anvil(anvil_proc)

    print_size_table(results, specs)
    print_memory_table(results, specs)
    if args.gas:
        print_runtime_table(results)
        print_gas_table(results, specs)
        if args.gas_breakdown or args.gas_profile == "hot":
            print_gas_breakdown_table(results, specs)

    if args.verbose:
        for result in results:
            for compiler_id, data in result["compilers"].items():
                if data.get("status") != "ok":
                    print(f"\n{result['test_id']} {compiler_id} error:\n{data.get('error', '')}")
                for check in data.get("runtime_results") or []:
                    if check.get("status") != "ok":
                        print(
                            f"\n{result['test_id']} {compiler_id} runtime {check.get('label')} error:\n"
                            f"{check.get('error', '')}"
                        )
                for gas_result in data.get("gas_results") or []:
                    if gas_result.get("gas") is None:
                        print(
                            f"\n{result['test_id']} {compiler_id} tx {gas_result.get('label')} error:\n"
                            f"{gas_result.get('error', '')}"
                        )
            for mismatch in result.get("runtime_mismatches") or []:
                values = ", ".join(
                    f"{compiler_id}={value}"
                    for compiler_id, value in mismatch.get("values", {}).items()
                )
                print(f"\n{result['test_id']} runtime mismatch {mismatch.get('label')}: {values}")

    RESULT_ROOT.mkdir(parents=True, exist_ok=True)
    output = Path(args.output) if args.output else RESULT_ROOT / "solar_latest.json"
    document = {
        "schema_version": 1,
        "wall_time_seconds": time.monotonic() - started,
        "timings": timings,
        "results": results,
    }
    output.write_text(json.dumps(document, indent=2) + "\n")
    print(f"\nResults saved to {display_path(output)}")

    failed = [
        (result["test_id"], compiler_id)
        for result in results
        for compiler_id, data in result["compilers"].items()
        if data.get("status") != "ok"
    ]
    runtime_failed = [
        result["test_id"]
        for result in results
        if result.get("runtime_status") in ("failed", "mismatch")
    ]
    gas_failed = [
        (result["test_id"], compiler_id)
        for result in results
        for compiler_id, data in result["compilers"].items()
        if data.get("gas_status") == "failed"
    ]
    if (failed or runtime_failed or gas_failed) and not args.allow_failures:
        if failed:
            print(
                _color(f"{len(failed)} compiler runs failed; use --allow-failures to keep exit code 0", RED),
                file=sys.stderr,
            )
        if runtime_failed:
            print(
                _color(f"{len(runtime_failed)} runtime checks failed; use --allow-failures to keep exit code 0", RED),
                file=sys.stderr,
            )
        if gas_failed:
            print(
                _color(f"{len(gas_failed)} gas transaction runs failed; use --allow-failures to keep exit code 0", RED),
                file=sys.stderr,
            )
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
