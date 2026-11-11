"""Strict import of the fragment printed by the compiler's IR parsers."""

import re
from dataclasses import dataclass


class Unsupported(ValueError):
    pass


@dataclass
class Program:
    interface: tuple[str, ...]
    inputs: list[str]
    result_type: str
    body: str


def lines(text: str) -> list[str]:
    # Backend layout records use comments in MIR and can affect execution.
    if "backend-memory" in text or "backend-stack" in text:
        raise Unsupported("backend memory records")
    result = [line.split(";", 1)[0].strip() for line in text.splitlines()]
    result = [line for line in result if line]
    if not result or not re.fullmatch(r"@module [\w.]+", result.pop(0)):
        raise Unsupported("expected one IR module")
    if len(result) > 512:
        raise Unsupported("limit of 512 lines per region")
    return result


def literal(token: str) -> str:
    if not re.fullmatch(r"(?:0x[0-9a-fA-F]{1,64}|[0-9]{1,78})", token):
        raise Unsupported(f"unsupported word operand {token!r}")
    value = int(token, 16 if token.startswith("0x") else 10)
    if value >= 2**256:
        raise Unsupported("word literal out of range")
    return f"{value}#256"
