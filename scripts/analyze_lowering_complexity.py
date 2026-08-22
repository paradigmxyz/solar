#!/usr/bin/env python3
"""Compare HIR and progressive MIR lowering complexity across git refs."""

from __future__ import annotations

import argparse
import fnmatch
import json
import re
import subprocess
from dataclasses import dataclass, asdict
from pathlib import Path


PATTERNS = {
    "raw_ops": re.compile(
        r"builder\.(?:mload|mstore|mstore8|calldataload|calldatacopy|"
        r"returndatacopy|sload|sstore)\b"
    ),
    "semantic_ops": re.compile(
        r"builder\.(?:abi_decode|memory_object_|slice_|storage_|frame_|"
        r"alloc_|returndata_|revert_returndata)"
    ),
    "match": re.compile(r"\bmatch\b"),
    "if_": re.compile(r"\bif\b"),
    "if_let": re.compile(r"\bif\s+let\b"),
    "else_if": re.compile(r"\belse\s+if\b"),
    "unsupported": re.compile(r"report_unsupported|unsupported!"),
    "unreachable": re.compile(r"unreachable!"),
    "memory_kind_layout": re.compile(r"MemoryObject(?:Kind|Layout)"),
    "slice_types": re.compile(r"MirType::Slice"),
    "error_selectors": re.compile(r'keccak256\("(?:Error\(string\)|Panic\(uint256\))"\)'),
}

METRIC_NAMES = ("files", "lines", *PATTERNS)


@dataclass
class Metrics:
    files: int = 0
    lines: int = 0
    raw_ops: int = 0
    semantic_ops: int = 0
    match: int = 0
    if_: int = 0
    if_let: int = 0
    else_if: int = 0
    unsupported: int = 0
    unreachable: int = 0
    memory_kind_layout: int = 0
    slice_types: int = 0
    error_selectors: int = 0

    def add(self, text: str) -> None:
        self.lines += len(text.splitlines())
        for name, pattern in PATTERNS.items():
            setattr(self, name, getattr(self, name) + len(pattern.findall(text)))


def git(root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), *args],
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout


def repo_root() -> Path:
    return Path(git(Path.cwd(), "rev-parse", "--show-toplevel").strip())


def base_files(root: Path, ref: str, path: str) -> list[str]:
    if "*" in path:
        pathspec = path.rsplit("/", 1)[0]
        names = git(root, "ls-tree", "-r", "--name-only", ref, "--", pathspec)
        return [
            line
            for line in names.splitlines()
            if line.endswith(".rs") and fnmatch.fnmatch(line, path)
        ]
    output = git(root, "ls-tree", "-r", "--name-only", ref, "--", path)
    return [line for line in output.splitlines() if line.endswith(".rs")]


def current_files(root: Path, path: str) -> list[Path]:
    directory = root / path
    if path.endswith("*.rs"):
        return sorted(directory.parent.glob(directory.name))
    return sorted(directory.rglob("*.rs"))


def measure_current(root: Path, path: str) -> Metrics:
    metrics = Metrics()
    for file in current_files(root, path):
        metrics.files += 1
        metrics.add(file.read_text())
    return metrics


def measure_base(root: Path, ref: str, path: str) -> Metrics:
    metrics = Metrics()
    for file in base_files(root, ref, path):
        metrics.files += 1
        metrics.add(git(root, "show", f"{ref}:{file}"))
    return metrics


def delta(current: Metrics, base: Metrics) -> dict[str, int]:
    return {
        name: getattr(current, name) - getattr(base, name)
        for name in METRIC_NAMES
    }


def signed(value: int) -> str:
    return f"{value:+d}"


def print_report(base_ref: str, base: dict[str, Metrics], current: dict[str, Metrics]) -> None:
    columns = (
        "files",
        "lines",
        "raw_ops",
        "semantic_ops",
        "match",
        "if_let",
        "unsupported",
        "kind/layout",
        "slice_types",
    )
    names = {
        "kind/layout": "memory_kind_layout",
    }
    print(f"base: {base_ref}")
    print("scope    " + " ".join(f"{column:>13}" for column in columns))
    for scope in current:
        values = current[scope]
        print(
            f"{scope:<8}"
            + " ".join(
                f"{getattr(values, names.get(column, column)):>13}" for column in columns
            )
        )
        changes = delta(values, base[scope])
        print(
            f"  delta  "
            + " ".join(
                f"{signed(changes[names.get(column, column)]):>13}" for column in columns
            )
        )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--base",
        default="origin/main",
        help="git ref to compare against (default: origin/main)",
    )
    parser.add_argument(
        "--root",
        type=Path,
        help="repository root (default: the current git repository)",
    )
    parser.add_argument("--json", action="store_true", help="print machine-readable JSON")
    args = parser.parse_args()

    root = (args.root or repo_root()).resolve()
    scopes = {
        "hir": "crates/codegen/src/lower",
        "passes": "crates/codegen/src/transform/lower_*.rs",
    }
    base = {scope: measure_base(root, args.base, path) for scope, path in scopes.items()}
    current = {scope: measure_current(root, path) for scope, path in scopes.items()}

    if args.json:
        payload = {
            "base": args.base,
            "scopes": {
                scope: {
                    "base": asdict(base[scope]),
                    "current": asdict(current[scope]),
                    "delta": delta(current[scope], base[scope]),
                }
                for scope in scopes
            },
        }
        print(json.dumps(payload, indent=2, sort_keys=True))
    else:
        print_report(args.base, base, current)


if __name__ == "__main__":
    main()
