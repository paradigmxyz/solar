#!/usr/bin/env python3
"""Make the stage export's manifest independent of where it was written.

    normalize_export.py GEN_DIR

scripts/verify/uniswap_v2_erc20_stages.sh exports into a fresh `mktemp -d` directory and copies the
result into Certified/uv2_erc20/gen. The compiler records its command line, so manifest.json (its
"arguments" and its debug "options") and Manifest.lean (`arguments`) name that temporary
directory, and they differ on every run. The olean of uv2_erc20.gen.UniswapV2ERC20.Manifest then
differs too, and two slots that each export can never have byte-identical replay plans.

This rewrites the one `-Zemit-lean-stages=<dir>/stages` argument to
`-Zemit-lean-stages=EXPORT/stages` in both files, and nothing else. It refuses (exit 1) unless the
argument occurs exactly once in Manifest.lean's `arguments` and in manifest.json's "arguments",
and the directory exactly twice in its "options" debug string (its two spellings there), and the directory is a mktemp directory under /tmp. The statement
pins only `Manifest.arguments.take 3` (the options -Ogas --evm-version osaka), the sources, the
compiler, the EVM version, the optimization and the stage files' SHA-256 (gen.py), so this argument
is outside it; the stage files themselves are not touched.
"""
import json
import pathlib
import re
import sys

ARG = re.compile(r"-Zemit-lean-stages=(/tmp/tmp\.[A-Za-z0-9]{10})/stages")
FIXED = "-Zemit-lean-stages=EXPORT/stages"


def fail(msg):
    sys.exit(f"normalize_export: {msg}")


gen = pathlib.Path(sys.argv[1])
mj, ml = gen / "manifest.json", gen / "Manifest.lean"
m = json.loads(mj.read_text())
args = m["arguments"]
hits = [i for i, a in enumerate(args) if ARG.fullmatch(a)]
if len(hits) != 1:
    fail(f"manifest.json arguments: {len(hits)} export arguments, want 1")
tmp = ARG.fullmatch(args[hits[0]]).group(1)
if hits[0] < 3:
    fail("the export argument is among the first three (the pinned options)")
args[hits[0]] = FIXED
opt = m["options"]
# The debug string names the directory twice: `_unstable: ["emit-lean-stages=<dir>/stages"]` and
# `emit_lean_stages: Some("<dir>/stages")`.
if opt.count(f'"emit-lean-stages={tmp}/stages"') != 1 or opt.count(f'Some("{tmp}/stages")') != 1 \
        or opt.count(tmp) != 2:
    fail("manifest.json options: the export directory is not named exactly twice, in its two forms")
m["options"] = opt.replace(f"{tmp}/stages", "EXPORT/stages")
lean = ml.read_text()
quoted = f'"-Zemit-lean-stages={tmp}/stages"'
if lean.count(quoted) != 1 or lean.count(tmp) != 1:
    fail("Manifest.lean: the export argument does not occur exactly once")
lean = lean.replace(quoted, f'"{FIXED}"')
raw = mj.read_text()
indent = 2 if raw.startswith("{\n  ") else None
mj.write_text(json.dumps(m, indent=indent) + ("\n" if raw.endswith("\n") else ""))
ml.write_text(lean)
if tmp in mj.read_text() or tmp in ml.read_text():
    fail("the export directory is still named")
print(f"normalize_export: {tmp}/stages -> EXPORT/stages in manifest.json and Manifest.lean")
