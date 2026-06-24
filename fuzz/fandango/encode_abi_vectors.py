#!/usr/bin/env python3
"""Convert Fandango ABI argument vectors into calldata JSONL.

Input is newline-delimited JSON objects shaped like:

    {"signature":"f(uint256,bool,bytes,string)","args":["1",true,"0x00","hello"]}

The script delegates ABI encoding to Foundry's `cast calldata`, keeping this
prototype focused on plumbing rather than duplicating ABI encoding logic.
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from typing import Any


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--seed", help="seed used to generate the input vectors")
    parser.add_argument("--cast", default=shutil.which("cast") or "cast")
    args = parser.parse_args()

    for index, line in enumerate(sys.stdin):
        line = line.strip()
        if not line:
            continue

        vector = json.loads(line)
        signature = vector["signature"]
        values = [_cast_arg(value) for value in vector.get("args", [])]
        calldata = _cast_calldata(args.cast, signature, values)

        out: dict[str, Any] = dict(vector)
        out.update({
            "index": index,
            "signature": signature,
            "args": vector.get("args", []),
            "calldata": calldata,
        })
        if args.seed is not None:
            out["seed"] = args.seed
        print(json.dumps(out, separators=(",", ":")))

    return 0


def _cast_arg(value: Any) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, list):
        return "[" + ",".join(_cast_arg(item) for item in value) + "]"
    return str(value)


def _cast_calldata(cast: str, signature: str, args: list[str]) -> str:
    result = subprocess.run(
        [cast, "calldata", signature, *args],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return result.stdout.strip()


if __name__ == "__main__":
    raise SystemExit(main())
