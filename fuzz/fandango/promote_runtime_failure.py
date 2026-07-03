#!/usr/bin/env python3
"""Promote a runtime failure source into a replayable corpus location."""

from __future__ import annotations

import argparse
import json
import pathlib
import re


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--failure", type=pathlib.Path, required=True)
    parser.add_argument("--name", required=True)
    parser.add_argument(
        "--mode",
        choices=("runtime-corpus", "regression"),
        default="regression",
        help="runtime-corpus is for Fandango-parseable seeds; regression is for replay-only sources",
    )
    args = parser.parse_args()

    failure = json.loads(args.failure.read_text())
    source = failure["source_text"]
    name = _sanitize_name(args.name)

    if args.mode == "runtime-corpus":
        out = pathlib.Path("fuzz/fandango/runtime-corpus") / f"{name}.sol"
    else:
        out = pathlib.Path("fuzz/fandango/runtime-regressions") / f"{name}.sol"
        replay = out.with_suffix(".json")
        replay.parent.mkdir(parents=True, exist_ok=True)
        replay.write_text(json.dumps(failure, indent=2, sort_keys=True) + "\n")

    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(source)
    print(out)
    return 0


def _sanitize_name(name: str) -> str:
    sanitized = re.sub(r"[^A-Za-z0-9_.-]+", "_", name).strip("._")
    if not sanitized:
        raise ValueError("empty promotion name")
    return sanitized


if __name__ == "__main__":
    raise SystemExit(main())
