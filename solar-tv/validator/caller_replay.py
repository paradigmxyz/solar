"""Independently replay the five real ABI entries against the emitted runtime."""

import json

from . import call_tail, native, world
from .callers import SELECTORS, require
from .common import Program

CALLER = 0x10000
TOKEN = 0x40000
ORACLE = 0x45000
MORPHO = 0x50000
MARKET = (TOKEN, TOKEN, ORACLE, 0, 0)

# keccak256(abi.encode(keccak256(abi.encode(MARKET)), uint256(3))).
MARKET_SLOT = 0xB3D96E24609AF73647060D22F776B7A514A301285992D866306826461C6671EB
# keccak256(abi.encode(CALLER, keccak256(abi.encode(marketId, uint256(2))))).
POSITION_SLOT = 0x69AA56D399D4083E3D377F5A1FE57DCF76D093D7B4A7FCA1B5F840DA802A78B0

INITIAL = {
    MARKET_SLOT: 200 + (200_000_000 << 128),
    MARKET_SLOT + 1: 100 + (100_000_000 << 128),
    MARKET_SLOT + 2: 1,
    POSITION_SLOT + 1: 100_000_000 + (100 << 128),
}

TOKEN_IR = """@module Token
bb0:
  push 3
  calldatasize
  lt
  push bb1
  jumpi
  push 0
  calldataload
  push 224
  shr
  push 0x23b872dd
  eq
  push bb2
  jumpi
  jump bb3
bb1:
  calldatasize
  push 0
  tstore
  stop
bb2:
  push 42
  push 0
  sstore
  push 0
  tload
  dup 1
  iszero
  push bb3
  jumpi
  push 1
  eq
  push bb4
  jumpi
  push 0
  push 0
  mstore
  push 32
  push 0
  return
bb3:
  push 1
  push 0
  mstore
  push 32
  push 0
  return
bb4:
  push 0xdeadbeef
  push 0
  mstore
  push 4
  push 28
  revert
"""


def word(value):
    return value.to_bytes(32, "big")


def error(reason):
    return (
        bytes.fromhex("08c379a0")
        + word(32)
        + word(len(reason))
        + reason.ljust(32, b"\0")
    )


def calldata(name):
    if name in {"supply", "repay"}:
        args = (*MARKET, 7, 0, CALLER, 9 * 32)
    elif name == "supplyCollateral":
        args = (*MARKET, 7, CALLER, 8 * 32)
    elif name == "liquidate":
        args = (*MARKET, CALLER, 0, 7_000_000, 9 * 32)
    else:
        args = (TOKEN, 7, 3 * 32)
    return SELECTORS[name].to_bytes(4, "big") + b"".join(map(word, (*args, 0)))


def wrapper(name, mode):
    data = calldata(name)
    # The callback to the root account during flashLoan halts immediately.
    rows = ["@module Caller", "bb0:", "calldatasize", "push bb1", "jumpi"]
    if mode:
        # An argument of one or two bytes selects revert or false for transferFrom.
        rows.extend(
            [
                "push 0",
                "push 0",
                f"push {mode}",
                "push 0",
                "push 0",
                f"push {TOKEN}",
                "push 50000",
                "call",
                "pop",
            ]
        )
    for offset in range(0, len(data), 32):
        chunk = data[offset : offset + 32].ljust(32, b"\0")
        rows.extend(
            [f"push {int.from_bytes(chunk, 'big')}", f"push {offset}", "mstore"]
        )
    rows.extend(
        [
            "push 0",
            "push 0",
            f"push {len(data)}",
            "push 0",
            "push 0",
            f"push {MORPHO}",
            "push 300000",
            "call",
            "push 0",
            "mstore",
            "returndatasize",
            "push 0",
            "push 32",
            "returndatacopy",
            "returndatasize",
            "push 32",
            "add",
            "push 0",
            "return",
            "bb1:",
            "stop",
        ]
    )
    return "\n".join(rows) + "\n"


def replay(code, directory, solar, executable, timeout):
    directory.mkdir()
    token = world.runtime(TOKEN_IR, "evmir", solar, "cancun", timeout)
    oracle = b"\x7f" + word(10**36) + bytes.fromhex("5f5260205ff3")
    fixture = directory / "world.json"
    fixture.write_text(
        json.dumps(
            {
                "version": 1,
                "address": CALLER,
                "gas": 500000,
                "accounts": [
                    {
                        "address": MORPHO,
                        "nonce": 1,
                        "code": "0x" + code.hex(),
                        "storage": {
                            hex(key): str(value) for key, value in INITIAL.items()
                        },
                    },
                    {"address": TOKEN, "nonce": 1, "code": "0x" + token.hex()},
                    {"address": ORACLE, "nonce": 1, "code": "0x" + oracle.hex()},
                ],
            }
        )
    )
    state = world.read(fixture, solar, "exact")
    receipts = {}
    for name in SELECTORS:
        variants = [wrapper(name, mode) for mode in (0, 1, 2)]
        for side, text in zip(("success", "revert", "false"), variants, strict=True):
            (directory / f"{name}-{side}.evmir").write_text(text)
        programs = [
            Program(
                ("world-entry", "evmir"),
                [],
                "",
                "",
                evm_version="cancun",
                world=state,
                replay_ir=text,
                gas="exact",
            )
            for text in variants
        ]
        first = native.replay(
            programs[0], programs[1], [], executable=executable, timeout=timeout
        )
        second = native.replay(
            programs[0], programs[2], [], executable=executable, timeout=timeout
        )
        original, reverted = first["observations"]
        repeated, returned_false = second["observations"]
        require(original == repeated, f"unstable {name} success replay")
        observations = [
            json.loads(item) for item in (original, reverted, returned_false)
        ]
        (directory / f"{name}.json").write_text(json.dumps(observations))
        expected = {
            "supply": [7, 7_000_000],
            "repay": [7, 7_000_000],
            "liquidate": [8, 7],
        }.get(name, [])
        committed = INITIAL.copy()
        if name == "supply":
            committed[POSITION_SLOT] = 7_000_000
            committed[MARKET_SLOT] += 7 + (7_000_000 << 128)
        elif name in {"repay", "liquidate"}:
            committed[MARKET_SLOT + 1] -= 7 + (7_000_000 << 128)
            committed[POSITION_SLOT + 1] -= 7_000_000
            if name == "liquidate":
                committed[POSITION_SLOT + 1] -= 8 << 128
        elif name == "supplyCollateral":
            committed[POSITION_SLOT + 1] += 7 << 128
        for index, observation in enumerate(observations):
            status, data, gas, _, accounts, logs = observation
            by_address = {int(account[0]): account for account in accounts}
            slots = {int(key): int(value) for key, value in by_address[MORPHO][4]}
            token_storage = by_address[TOKEN][4]
            actual = bytes(data)
            if index == 0:
                require(
                    status == "success"
                    and actual == word(1) + b"".join(map(word, expected))
                    and gas > 0
                    and token_storage == [["0", "42"]]
                    and logs
                    and slots == committed,
                    f"{name} ABI success replay returned {actual.hex()}",
                )
            else:
                require(
                    status == "success"
                    and actual
                    == word(0)
                    + error(call_tail.FAILED if index == 1 else call_tail.FALSE)
                    and gas > 0
                    and slots == INITIAL
                    and not token_storage
                    and not logs,
                    f"{name} ABI revert replay returned {actual.hex()}",
                )
        receipts[name] = {"abi_words": expected, "rollback": ["revert", "false"]}
    return receipts
