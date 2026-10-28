#!/usr/bin/env python3
"""Compiles like Solc after applying one source mutation.

`SOLSYMDIFF_MUTATION` holds a JSON `[old, new]` pair that must match exactly
one source unit. `SOLSYMDIFF_SOLC` names the Solc executable.
"""

import json
import os
import subprocess
import sys

solc = os.environ.get("SOLSYMDIFF_SOLC", "solc")
args = sys.argv[1:]
if "--standard-json" not in args:
    raise SystemExit(subprocess.call([solc, *args]))
old, new = json.loads(os.environ["SOLSYMDIFF_MUTATION"])
request = json.load(sys.stdin)
matches = [
    source for source in request["sources"].values() if old in source.get("content", "")
]
if len(matches) != 1:
    raise SystemExit(f"mutation {old!r} matched {len(matches)} sources")
matches[0]["content"] = matches[0]["content"].replace(old, new)
result = subprocess.run(
    [solc, *args], input=json.dumps(request), text=True, check=False
)
raise SystemExit(result.returncode)
