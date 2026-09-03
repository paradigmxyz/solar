#!/usr/bin/env python3
"""Run a caller contract compiled by solc and by solar on a real pre-byzantium forge EVM.

usage: prebyz_gas.py file.sol Caller Callee 'sig(address)' [--evm-version homestead] [--gas N ...]
                     [--solar BIN] [--keep]

The generated Foundry test is compiled for the same EVM version and uses only assembly calls
with a static 32-byte output buffer and events, so it needs no RETURNDATASIZE. It deploys the
callee (solc build), then the caller from each compiler, calls `sig(callee)` on each caller with
the given gas limits, and emits the success flag, returned word, and gas used. Results are read
from the forge trace.
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

ROOT = statediff.ROOT

TEST = """// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;
contract PreByzTest {{
    event Result(string who, uint256 gasLimit, bool ok, uint256 ret, uint256 gasUsed);
    function _deploy(bytes memory init) internal returns (address a) {{
        assembly {{ a := create(0, add(init, 0x20), mload(init)) }}
        require(a != address(0));
    }}
    function _call(address t, bytes4 sel, address arg, uint256 g) internal returns (bool ok, uint256 ret, uint256 used) {{
        uint256 g0 = gasleft();
        assembly {{
            let p := mload(0x40)
            mstore(p, sel)
            mstore(add(p, 4), arg)
            mstore(add(p, 36), 0)
            ok := call(g, t, 0, p, 36, add(p, 36), 32)
            ret := mload(add(p, 36))
        }}
        used = g0 - gasleft();
    }}
    function testRun() public {{
        address callee = _deploy(hex"{callee}");
        address a = _deploy(hex"{solc}");
        address b = _deploy(hex"{solar}");
        uint256[{n}] memory gs = [{gas_list}];
        for (uint256 i; i < gs.length; i++) {{
            (bool oa, uint256 ra, uint256 ua) = _call(a, {sel}, callee, gs[i]);
            emit Result("solc", gs[i], oa, ra, ua);
            (bool ob, uint256 rb, uint256 ub) = _call(b, {sel}, callee, gs[i]);
            emit Result("solar", gs[i], ob, rb, ub);
        }}
    }}
}}
"""


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("source", type=pathlib.Path)
    ap.add_argument("caller")
    ap.add_argument("callee")
    ap.add_argument("sig")
    ap.add_argument("--evm-version", default="homestead")
    ap.add_argument("--gas", type=int, action="append", default=None)
    ap.add_argument("--solar", default=str(ROOT / "target/debug/solar"))
    ap.add_argument("--keep", action="store_true")
    args = ap.parse_args()
    gas = args.gas or [100_000, 1_000_000, 10_000_000]
    solc = "solc"
    inp, rel = statediff.standard_input(args.source, args.source.parent, args.evm_version, True, 200, solc)
    a = statediff.compile_one(solc, inp, rel, args.caller, "solc")
    b = statediff.compile_one(args.solar, inp, rel, args.caller, "solar")
    callee = statediff.compile_one(solc, inp, rel, args.callee, "solc")
    sel = "0x" + a["ids"][args.sig]
    proj = statediff.OUT / f"prebyz-{args.source.stem}-{args.evm_version}-{time.time_ns()}-{os.getpid()}"
    (proj / "test").mkdir(parents=True)
    (proj / "src").mkdir()
    (proj / "test/PreByz.t.sol").write_text(TEST.format(
        callee=callee["code"], solc=a["code"], solar=b["code"], n=len(gas),
        gas_list=", ".join(f"uint256({g})" for g in gas), sel=sel))
    (proj / "foundry.toml").write_text(
        f'[profile.default]\nsrc = "src"\ntest = "test"\nout = "out"\nlibs = []\noptimizer = false\n'
        f'evm_version = "{args.evm_version}"\ngas_limit = 9223372036854775807\noffline = true\n')
    env = {k: v for k, v in os.environ.items() if not k.startswith(("FOUNDRY_", "DAPP_"))}
    r = subprocess.run(["forge", "test", "--root", str(proj), "--use", solc, "--evm-version", args.evm_version,
                        "--match-contract", "PreByzTest", "-vvvv", "--force"], capture_output=True, text=True,
                       cwd=proj, env=env)
    (proj / "trace.txt").write_text(r.stdout + r.stderr)
    rows = []
    for m in re.finditer(r'Result\(who: "(\w+)", gasLimit: (\d+) \[[^\]]*\], ok: (true|false), ret: (\d+)(?: \[[^\]]*\])?, gasUsed: (\d+)', r.stdout):
        rows.append({"who": m.group(1), "gas_limit": int(m.group(2)), "ok": m.group(3) == "true",
                     "ret": int(m.group(4)), "gas_used": int(m.group(5))})
    out = {"file": str(args.source), "evm_version": args.evm_version, "forge_status": r.returncode,
           "rows": rows, "project": str(proj)}
    if not rows:
        out["tail"] = (r.stdout + r.stderr)[-2000:]
    print(json.dumps(out, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
