"""Shared finite memory projection and exact constant-address import limits."""

import re

from .common import Unsupported

CAPACITY = 256
INPUTS = [*(f"memory{i}" for i in range(CAPACITY // 32)), "memoryWords"]
WIDTHS = (*(256 for _ in range(CAPACITY // 32)), 251)
INITIAL = "Memory.initial [" + ", ".join(INPUTS[:-1]) + "] memoryWords"
UNFOLD = (
    "Memory.expand, Memory.msize, Memory.store, Memory.storeByte, Memory.byte, "
    "Memory.load, Memory.read, EVM.Memory.execute, EVM.Memory.finish, EVM.Memory.step, Except.map"
)


def witnesses(inputs: list[str]) -> list[list[int]]:
    # These only propose counterexamples; the kernel must replay every mismatch.
    # The second pattern exercises pre-existing memory rather than fresh zeros.
    return [
        [int(not name.startswith("memory")) for name in inputs],
        [
            CAPACITY // 32
            if name == "memoryWords"
            else int(name.removeprefix("memory")) + 1
            if name in INPUTS[:-1]
            else 0
            for name in inputs
        ],
    ]


def address(token: str, size: int) -> int:
    token = token.removeprefix("i256 ")
    if not re.fullmatch(r"0x[0-9a-fA-F]+|[0-9]+", token):
        raise Unsupported("memory addresses and output sizes must be constant")
    offset = int(token, 16 if token.startswith("0x") else 10)
    check_range(offset, size)
    return offset


def check_range(offset: int | None, size: int | None) -> None:
    if size == 0:
        return
    if offset is None or size is None:
        raise Unsupported("memory addresses and output sizes must be constant")
    if offset < 0 or size < 0 or offset + size > CAPACITY:
        raise Unsupported(f"memory access exceeds the {CAPACITY}-byte modeled prefix")


def active_words(offset: int | None, size: int | None) -> int:
    check_range(offset, size)
    if size == 0:
        return 0
    assert offset is not None and size is not None
    return (offset + size + 31) // 32
