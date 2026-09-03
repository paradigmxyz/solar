#!/usr/bin/env python3
"""Concretely compare solc and solar runtimes on hand-picked calldata.

usage: concrete.py [--no-optimize|--optimizer-runs N ...] file contract 'sig(args)' 'arg1 arg2' ['arg1 arg2' ...]
Each quoted arg group is passed to `cast calldata`.
"""
import json, os, pathlib, re, subprocess, sys
ROOT = pathlib.Path(__file__).resolve().parents[2]
args = sys.argv[1:]
extra = []
while args and args[0].startswith("--"):
    extra.append(args.pop(0))
    if extra[-1] in ("--optimizer-runs", "--project-root", "--solar", "--evm-version"): extra.append(args.pop(0))
src, contract, sig, *groups = args
# Build a project once via solsymdiff (any function works; it just compiles both).
r = subprocess.run([sys.executable, str(ROOT/"fuzz/bin/solsymdiff"), "--source", src, "--contract", contract,
                    "--signature", sig, "--include-view", "--include-stateful", "--max-paths", "1", "--timeout", "60"] + extra,
                   capture_output=True, text=True, cwd=ROOT)
try:
    proj = json.loads(r.stdout)["project"]
except Exception:
    print("solsymdiff failed:", (r.stdout or r.stderr)[-800:]); sys.exit(2)
t = (pathlib.Path(proj)/"test/SymbolicDifferential.t.sol").read_text()
solc_code = re.search(r'SOLC_CODE = hex"([0-9a-f]*)"', t).group(1)
solar_code = re.search(r'SOLAR_CODE = hex"([0-9a-f]*)"', t).group(1)
cases = []
for g in groups:
    if g.startswith("rawhex:"):
        cd = "0x" + g[7:].strip()
    elif g.startswith("raw:"):
        sel = subprocess.run(["cast", "sig", sig], capture_output=True, text=True).stdout.strip()
        cd = sel + "".join(f"{int(w, 0) % (1 << 256):064x}" for w in g[4:].split())
    else:
        cd = subprocess.run(["cast", "calldata", sig] + g.split(), capture_output=True, text=True).stdout.strip()
    cases.append((g, cd))
body = "".join(f'''
        {{
            (bool a{i}, bytes memory ra{i}) = solc_.call{{gas: 10000000}}(hex"{cd[2:]}");
            (bool b{i}, bytes memory rb{i}) = solar_.call{{gas: 10000000}}(hex"{cd[2:]}");
            if (a{i} != b{i} || keccak256(ra{i}) != keccak256(rb{i})) {{
                _log("MISMATCH case {i}: {g.replace(chr(34), "'")}");
                _logu("  solc  ok", a{i} ? 1 : 0); _logb("  solc  ret", ra{i});
                _logu("  solar ok", b{i} ? 1 : 0); _logb("  solar ret", rb{i});
            }}
        }}''' for i, (g, cd) in enumerate(cases))
test = f'''// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract ConcreteTest {{
    address constant CONSOLE = 0x000000000000000000636F6e736F6c652e6c6f67;
    function _deploy(bytes memory runtime) internal returns (address a) {{
        bytes memory init = abi.encodePacked(hex"63", uint32(runtime.length), hex"80600e6000396000f3", runtime);
        assembly {{ a := create(0, add(init, 0x20), mload(init)) }}
        require(a != address(0), "deploy");
    }}
    function _log(string memory s) internal view {{ (bool ok,) = CONSOLE.staticcall(abi.encodeWithSignature("log(string)", s)); ok; }}
    function _logb(string memory s, bytes memory b) internal view {{ (bool ok,) = CONSOLE.staticcall(abi.encodeWithSignature("logBytes(bytes)", b)); ok; }}
    function _logu(string memory s, uint256 u) internal view {{ (bool ok,) = CONSOLE.staticcall(abi.encodeWithSignature("log(string,uint256)", s, u)); ok; }}
    function testConcrete() public {{
        address solc_ = _deploy(hex"{solc_code}");
        address solar_ = _deploy(hex"{solar_code}");
        {body}
    }}
}}
'''
(pathlib.Path(proj)/"test/Concrete.t.sol").write_text(test)
env = {k: v for k, v in os.environ.items() if not k.startswith("FOUNDRY_")}
r = subprocess.run(["forge", "test", "--root", proj, "--use", "/home/doni/.cargo/bin/solc", "--evm-version", "osaka",
                    "--match-contract", "ConcreteTest", "-vvvv", "--force"], capture_output=True, text=True, cwd=proj, env=env)
out = r.stdout + r.stderr
(pathlib.Path(proj)/"concrete-trace.txt").write_text(out)
lines = out.splitlines()
# Collect target call results from the trace, in order: two per case (solc, solar).
results = []
for i, line in enumerate(lines):
    if "::" in line and "console::" not in line and "ConcreteTest::" not in line and "→ new" not in line and "::testConcrete" not in line:
        nxt = lines[i+1].strip() if i+1 < len(lines) else ""
        m = re.search(r"← (\[\w+\].*)$", nxt)
        results.append(m.group(1) if m else nxt)
flagged = [int(re.search(r"case (\d+)", l).group(1)) for l in lines if "MISMATCH case" in l]
print(f"{sig}: {len(cases)} cases, {len(flagged)} mismatches")
for i in flagged:
    g = cases[i][0]
    a = results[2*i] if 2*i < len(results) else "?"
    b = results[2*i+1] if 2*i+1 < len(results) else "?"
    print(f"  MISMATCH case {i}: {g}\n    solc  {a[:140]}\n    solar {b[:140]}")
for l in lines:
    if "[FAIL" in l or "Error" in l: print("  " + l.strip())
