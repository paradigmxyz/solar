#!/usr/bin/env python3
"""Derives the runtime a Seaport variant has when deployed where the TLOAD
probe fails, from its Cancun runtime: the only other difference is the
_tstoreInitialSupport immutable, the one immutable whose value is 1, which
becomes 0. Fails unless exactly one immutable holds 1.

Usage: sstore_mode.py ARTIFACT.json IN.hex OUT.hex
"""
import json
import sys

artifact, src, dst = sys.argv[1:]
refs = json.load(open(artifact))["deployedBytecode"]["immutableReferences"]
code = bytearray.fromhex(open(src).read().strip().removeprefix("0x"))
one = (1).to_bytes(32, "big")
flags = [
    spans
    for spans in refs.values()
    if all(code[s["start"] : s["start"] + s["length"]] == one for s in spans)
]
if len(flags) != 1:
    sys.exit(f"{artifact}: expected one immutable equal to 1, found {len(flags)}")
for s in flags[0]:
    code[s["start"] : s["start"] + s["length"]] = bytes(32)
open(dst, "w").write("0x" + code.hex())
