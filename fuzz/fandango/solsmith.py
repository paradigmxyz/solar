#!/usr/bin/env python3
"""Generate Solidity runtime harnesses from a small typed model.

This is the first CSmith-style layer for the codegen differential tests. The
Fandango grammars are still useful for sampling and mutation, but this generator
owns type-aware construction: it chooses from typed Solidity fragments and emits
only contracts with the fixed `setup/run/observe` harness expected by
`run_source_runtime.py`.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import random
from dataclasses import dataclass
from typing import Self


U256_MASK = (1 << 256) - 1

DEFAULT_REQUIRED_FEATURES = (
    "arithmetic",
    "branch",
    "calldata-bytes",
    "loop",
    "mapping-write",
    "memory-array",
)


@dataclass(frozen=True)
class Program:
    source: str
    features: tuple[str, ...]


@dataclass(frozen=True)
class Expr:
    code: str
    features: tuple[str, ...] = ()

    def with_feature(self, feature: str) -> Self:
        return Expr(self.code, (*self.features, feature))


class Generator:
    def __init__(self, rng: random.Random) -> None:
        self.rng = rng

    def program(self) -> Program:
        body, features = self._body()
        source = _PREFIX + body + _SUFFIX
        return Program(source=source, features=tuple(sorted(set(features))))

    def program_for_feature(self, feature: str) -> Program:
        builder = {
            "arithmetic": self._arithmetic_body,
            "branch": self._branch_body,
            "calldata-bytes": self._bytes_body,
            "loop": self._loop_body,
            "mapping-write": self._mapping_body,
            "memory-array": self._memory_array_body,
        }.get(feature, self._combined_body)
        body, features = builder()
        source = _PREFIX + body + _SUFFIX
        return Program(source=source, features=tuple(sorted(set(features))))

    def _body(self) -> tuple[str, list[str]]:
        builders = [
            self._arithmetic_body,
            self._branch_body,
            self._loop_body,
            self._mapping_body,
            self._memory_array_body,
            self._bytes_body,
            self._combined_body,
        ]
        return self.rng.choice(builders)()

    def _u256_expr(self, depth: int = 0) -> Expr:
        if depth >= 2:
            return self.rng.choice([
                Expr("a"),
                Expr("b"),
                Expr("value", ("storage-read",)),
                Expr("values[a & 7]", ("mapping-read",)),
                Expr("values[b & 7]", ("mapping-read",)),
            ])

        left = self._u256_expr(depth + 1)
        right = self._u256_expr(depth + 1)
        op = self.rng.choice(["+", "^", "&", "|"])
        if op == "&":
            right = self.rng.choice([Expr("15"), Expr("255"), right])
        expr = Expr(f"({left.code} {op} {right.code})", (*left.features, *right.features, "arith"))
        if self.rng.random() < 0.35:
            expr = Expr(f"helper({expr.code})", (*expr.features, "internal-call"))
        return expr

    def _condition(self) -> Expr:
        lhs = self._u256_expr()
        rhs = self.rng.choice([Expr("a"), Expr("b"), Expr("value", ("storage-read",)), Expr("7")])
        op = self.rng.choice(["<", ">", "==", "!="])
        return Expr(f"(({lhs.code}) & 255) {op} (({rhs.code}) & 255)", (*lhs.features, *rhs.features))

    def _arithmetic_body(self) -> tuple[str, list[str]]:
        expr = self._u256_expr()
        return f"r = {expr.code};", ["arithmetic", *expr.features]

    def _branch_body(self) -> tuple[str, list[str]]:
        cond = self._condition()
        then_expr = self._u256_expr()
        else_expr = self._u256_expr()
        body = f"if ({cond.code}) {{ r = {then_expr.code}; }} else {{ r = {else_expr.code}; }}"
        return body, ["branch", *cond.features, *then_expr.features, *else_expr.features]

    def _loop_body(self) -> tuple[str, list[str]]:
        expr = self._u256_expr()
        limit = self.rng.choice(["a & 7", "(b & 3) + 1", "value & 7"])
        body = f"uint256 limit = {limit}; r = value; for (uint256 i = 0; i < limit; ++i) {{ r = r + i + ({expr.code}); }}"
        return body, ["loop", "storage-read", *expr.features]

    def _mapping_body(self) -> tuple[str, list[str]]:
        expr = self._u256_expr()
        key = self.rng.choice(["a & 7", "b & 7", "(a + b) & 7", "(value + a) & 7"])
        update = self.rng.choice([
            f"values[key] = values[key] + ({expr.code}) + 1; r = values[key] + value;",
            f"r = values[key] ^ ({expr.code}); values[(key + 1) & 7] = r;",
        ])
        return f"uint256 key = {key}; {update}", [
            "mapping-read",
            "mapping-write",
            "storage-read",
            *expr.features,
        ]

    def _memory_array_body(self) -> tuple[str, list[str]]:
        expr0 = self._u256_expr()
        expr1 = self._u256_expr()
        body = f"uint256[3] memory xs; xs[0] = {expr0.code}; xs[1] = {expr1.code}; xs[2] = value; r = xs[a % 3] + xs[b % 3];"
        return body, ["memory-array", "storage-read", *expr0.features, *expr1.features]

    def _bytes_body(self) -> tuple[str, list[str]]:
        body = self.rng.choice([
            "r = value + data.length; if (data.length != 0) { r += uint8(data[0]); }",
            "uint256 limit = data.length < 5 ? data.length : 5; r = value; for (uint256 i = 0; i < limit; ++i) { r = (r << 1) + uint8(data[i]); }",
            "r = data.length; if (data.length > 1) { r += uint8(data[1]) + value; }",
        ])
        return body, ["calldata-bytes", "storage-read"]

    def _combined_body(self) -> tuple[str, list[str]]:
        expr = self._u256_expr()
        body = self.rng.choice([
            f"uint256 key = a & 7; uint256[2] memory xs = [values[key], {expr.code}]; r = xs[0] + xs[1] + value;",
            f"uint256 limit = (a & 3) + 1; r = values[b & 7]; for (uint256 i = 0; i < limit; ++i) {{ r += mix(i, {expr.code}); }}",
            f"r = mix(a, b); if (data.length > 0) {{ values[uint8(data[0]) & 7] = {expr.code}; }} r += values[a & 7];",
        ])
        return body, [
            "combined",
            "mapping-read",
            "storage-read",
            *expr.features,
            *(["mapping-write", "calldata-bytes"] if "data.length" in body else []),
            *(["loop"] if "for (" in body else []),
            *(["memory-array"] if "memory xs" in body else []),
        ]


_PREFIX = (
    "/* SPDX-License-Identifier: MIT */\n"
    "pragma solidity ^0.8.0;\n\n"
    "contract FandangoRuntime {\n"
    "    event Seen(uint256 indexed tag, uint256 value);\n"
    "    uint256 public value;\n"
    "    mapping(uint256 => uint256) public values;\n\n"
    "    function setup(uint256 seed) external {\n"
    "        unchecked {\n"
    "            value = seed & 1023;\n"
    "            values[seed & 7] = seed + 1;\n"
    "            emit Seen(0, value);\n"
    "        }\n"
    "    }\n\n"
    "    function observe(uint256 key) external view returns (uint256, uint256) {\n"
    "        return (value, values[key & 7]);\n"
    "    }\n\n"
    "    function helper(uint256 x) internal pure returns (uint256) {\n"
    "        unchecked {\n"
    "            return (x * 7) ^ 3;\n"
    "        }\n"
    "    }\n\n"
    "    function mix(uint256 x, uint256 y) internal pure returns (uint256) {\n"
    "        unchecked {\n"
    "            return (x ^ y) + (x & 15);\n"
    "        }\n"
    "    }\n\n"
    "    function run(uint256 a, uint256 b, bytes calldata data) external returns (uint256 r) {\n"
    "        unchecked {\n"
    "            "
)

_SUFFIX = (
    "\n"
    "            value = r;\n"
    "            values[a & 7] = r;\n"
    "            emit Seen(1, r);\n"
    "            return r;\n"
    "        }\n"
    "    }\n"
    "}\n"
)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--count", type=int, default=16)
    parser.add_argument("--out-dir", type=pathlib.Path, required=True)
    parser.add_argument("--metadata", type=pathlib.Path)
    parser.add_argument(
        "--require-feature",
        action="append",
        default=[],
        help="require the generated batch to cover this feature; can be repeated",
    )
    parser.add_argument(
        "--require-default-features",
        action="store_true",
        help="require the default CI feature floor",
    )
    parser.add_argument("--max-attempts", type=int, default=512)
    args = parser.parse_args()

    rng = random.Random(args.seed)
    generator = Generator(rng)
    args.out_dir.mkdir(parents=True, exist_ok=True)

    required_features = set(args.require_feature)
    if args.require_default_features:
        required_features.update(DEFAULT_REQUIRED_FEATURES)

    programs = _generate_batch(generator, args.count, required_features, args.max_attempts)
    metadata = []
    feature_counts: dict[str, int] = {}
    for index, program in enumerate(programs):
        path = args.out_dir / f"solsmith-{index:04}.sol"
        path.write_text(program.source)
        for feature in program.features:
            feature_counts[feature] = feature_counts.get(feature, 0) + 1
        metadata.append({
            "path": str(path),
            "seed": args.seed,
            "index": index,
            "features": list(program.features),
        })

    if args.metadata is not None:
        args.metadata.parent.mkdir(parents=True, exist_ok=True)
        args.metadata.write_text(json.dumps({
            "sources": metadata,
            "feature_counts": feature_counts,
            "required_features": sorted(required_features),
        }, indent=2, sort_keys=True) + "\n")

    print(json.dumps({
        "sources": len(programs),
        "seed": args.seed,
        "out_dir": str(args.out_dir),
        "features": feature_counts,
    }, separators=(",", ":")))
    return 0


def _generate_batch(
    generator: Generator,
    count: int,
    required_features: set[str],
    max_attempts: int,
) -> list[Program]:
    programs: list[Program] = []
    covered: set[str] = set()
    attempts = 0

    while len(programs) < count or not required_features.issubset(covered):
        attempts += 1
        if attempts > max_attempts:
            missing = sorted(required_features - covered)
            raise RuntimeError(f"could not cover required features: {missing}")

        missing = required_features - covered
        if missing:
            program = generator.program_for_feature(sorted(missing)[0])
        else:
            program = generator.program()
        if len(programs) < count:
            programs.append(program)
            covered.update(program.features)
            continue

        program_features = set(program.features)
        if program_features - covered:
            programs.append(program)
            covered.update(program_features)

    return programs


if __name__ == "__main__":
    raise SystemExit(main())
