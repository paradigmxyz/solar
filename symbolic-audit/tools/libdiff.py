#!/usr/bin/env python3
"""Linked-library differential: compile a library and a caller with both compilers, etch each
compiler's library runtime at a fixed address, deploy both callers, and compare fixed calls.

usage: libdiff.py file.sol Library Contract --fixed "sig(types) args" ... [--evm-version V] [--solar BIN]
                  [--no-optimize|--optimizer-runs N] [--forge-evm V]
"""
from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
import subprocess
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import statediff  # noqa: E402

LIB_ADDR = "0x00000000000000000000000000000000000000Ab"

TEST = """// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;
interface Vm {{ function etch(address, bytes calldata) external; }}
contract LibDiffTest {{
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    event Result(string who, uint256 i, bool ok, bytes ret);
    function _deploy(bytes memory init) internal returns (address a) {{
        assembly {{ a := create(0, add(init, 0x20), mload(init)) }}
        require(a != address(0), "deploy");
    }}
    function _run(string memory who, address t) internal {{
        bytes[{n}] memory calls = [{calls}];
        for (uint256 i; i < calls.length; i++) {{
            (bool ok, bytes memory ret) = t.call{{gas: 5000000}}(calls[i]);
            emit Result(who, i, ok, ret);
        }}
    }}
    function testRun() public {{
        vm.etch(address(uint160({libint})), hex"{solc_lib}");
        address a = _deploy(hex"{solc_c}");
        _run("solc", a);
        vm.etch(address(uint160({libint})), hex"{solar_lib}");
        address b = _deploy(hex"{solar_c}");
        _run("solar", b);
    }}
}}
"""


def compile_linked(compiler, inp, rel, lib, contract, label):
    inp = json.loads(json.dumps(inp))
    inp["settings"]["libraries"] = {rel: {lib: LIB_ADDR}}
    inp["settings"]["outputSelection"] = {"*": {"*": ["abi", "evm.bytecode.object", "evm.deployedBytecode.object",
                                                      "evm.bytecode.linkReferences", "evm.methodIdentifiers"]}}
    r = subprocess.run([compiler, "--standard-json"], input=json.dumps(inp), capture_output=True, text=True, timeout=600)
    out = json.loads(r.stdout)
    errs = [e.get("formattedMessage") for e in out.get("errors", []) if e.get("severity") == "error"]
    if errs:
        raise ValueError(f"{label}: " + "\n".join(errs)[:2000])
    cs = out["contracts"][rel]
    return {"lib_runtime": cs[lib]["evm"]["deployedBytecode"]["object"].removeprefix("0x"),
            "code": cs[contract]["evm"]["bytecode"]["object"].removeprefix("0x"),
            "abi": cs[contract]["abi"], "ids": cs[contract]["evm"]["methodIdentifiers"]}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("source", type=pathlib.Path)
    ap.add_argument("library")
    ap.add_argument("contract")
    ap.add_argument("--fixed", action="append", default=[])
    ap.add_argument("--evm-version", default="osaka")
    ap.add_argument("--forge-evm", default=None)
    ap.add_argument("--solar", default=str(statediff.ROOT / "target/debug/solar"))
    ap.add_argument("--no-optimize", action="store_true")
    ap.add_argument("--optimizer-runs", type=int, default=200)
    args = ap.parse_args()
    inp, rel = statediff.standard_input(args.source, args.source.parent, args.evm_version, not args.no_optimize,
                                        args.optimizer_runs, "solc")
    a = compile_linked("solc", inp, rel, args.library, args.contract, "solc")
    b = compile_linked(args.solar, inp, rel, args.library, args.contract, "solar")
    sigs = {f"{e['name']}({','.join(statediff.canonical(i) for i in e['inputs'])})": e for e in a["abi"] if e.get("type") == "function"}
    calls = []
    for spec in args.fixed:
        m = re.match(r"^\s*(\w+\([^)]*\))\s*(.*)$", spec)
        sig, rest = m.group(1), m.group(2)
        types = [statediff.parse_type(statediff.canonical(i)) for i in sigs[sig]["inputs"]]
        cd = a["ids"][sig] + statediff.encode_tuple(types, statediff.coerce_args(types, statediff.parse_values(rest)))
        calls.append((sig, rest, cd))
    proj = statediff.OUT / f"libdiff-{args.source.stem}-{args.evm_version}-{time.time_ns()}"
    (proj / "test").mkdir(parents=True)
    (proj / "src").mkdir()
    forge_evm = args.forge_evm or ("osaka" if statediff.evm_index(args.evm_version) < statediff.evm_index("byzantium") else args.evm_version)
    (proj / "test/LibDiff.t.sol").write_text(TEST.format(
        n=len(calls), calls=", ".join(f'bytes(hex"{cd}")' for _, _, cd in calls), libint=int(LIB_ADDR, 16),
        solc_lib=a["lib_runtime"], solar_lib=b["lib_runtime"], solc_c=a["code"], solar_c=b["code"]))
    (proj / "foundry.toml").write_text(f'[profile.default]\nsrc = "src"\ntest = "test"\nout = "out"\nlibs = []\noptimizer = false\nevm_version = "{forge_evm}"\ngas_limit = 9223372036854775807\noffline = true\n')
    env = {k: v for k, v in os.environ.items() if not k.startswith(("FOUNDRY_", "DAPP_"))}
    r = subprocess.run(["forge", "test", "--root", str(proj), "--use", "solc", "--evm-version", forge_evm, "--match-contract", "LibDiffTest", "-vvvv", "--force"],
                       capture_output=True, text=True, cwd=proj, env=env)
    rows = re.findall(r'Result\(who: "(\w+)", i: (\d+), ok: (true|false), ret: (0x[0-9a-f]*)\)', r.stdout)
    by = {}
    for who, i, ok, ret in rows:
        by.setdefault(int(i), {})[who] = (ok == "true", ret)
    mism = []
    for i, (sig, rest, _) in enumerate(calls):
        s, l = by.get(i, {}).get("solc"), by.get(i, {}).get("solar")
        if s != l:
            mism.append({"call": i, "sig": sig, "args": rest, "solc": s, "solar": l})
    print(json.dumps({"status": "mismatch" if mism else ("agree" if rows else "error"), "calls": len(calls),
                      "mismatches": mism, "project": str(proj), "forge_status": r.returncode,
                      **({"tail": (r.stdout + r.stderr)[-1500:]} if not rows else {})}, indent=1))


if __name__ == "__main__":
    main()
