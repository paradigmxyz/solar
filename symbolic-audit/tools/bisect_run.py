#!/usr/bin/env python3
"""Compile a contract with solar CLI flags, compare one call against solc concretely.

usage: bisect_run.py file contract 'sig()' [calldata-hex-args...] -- [solar flags...]
"""
import os, pathlib, re, subprocess, sys, tempfile, json
ROOT = pathlib.Path(__file__).resolve().parents[2]
args = sys.argv[1:]
if "--" in args:
    i = args.index("--"); flags = args[i+1:]; args = args[:i]
else:
    flags = []
src, contract, sig, *groups = args
if not groups: groups = [""]
solc = subprocess.run(["solc", "--evm-version", "osaka", "--via-ir", "--optimize", "--bin-runtime", src], capture_output=True, text=True).stdout
m = re.search(rf"======= .*:{contract} =======\nBinary of the runtime part:\n([0-9a-f]+)", solc)
solc_code = m.group(1)
solar = subprocess.run([str(ROOT/"target/debug/solar"), src, "--evm-version", "osaka", "--emit", "bin-runtime"] + flags, capture_output=True, text=True)
solar_code = ""
try:
    d = json.loads(solar.stdout)
    for k, v in d["contracts"].items():
        if k.endswith(":" + contract):
            solar_code = v.get("bin-runtime") or v.get("binRuntime") or ""
            if isinstance(solar_code, dict): solar_code = solar_code.get("object", "")
except Exception:
    pass
solar_code = solar_code.removeprefix("0x")
if not solar_code:
    print("solar produced no runtime:", solar.stdout[-300:], solar.stderr[-800:]); sys.exit(2)
cases = []
for g in groups:
    if g.startswith("raw:"):
        sel = subprocess.run(["cast", "sig", sig], capture_output=True, text=True).stdout.strip()
        cd = sel + "".join(f"{int(w, 0) % (1 << 256):064x}" for w in g[4:].split())
    else:
        cd = subprocess.run(["cast", "calldata", sig] + g.split(), capture_output=True, text=True).stdout.strip()
    cases.append((g, cd))
proj = pathlib.Path(tempfile.mkdtemp(prefix="bisect-", dir=ROOT/"target/solsymdiff"))
(proj/"test").mkdir(); (proj/"src").mkdir()
(proj/"foundry.toml").write_text('[profile.default]\nsrc = "src"\ntest = "test"\nout = "out"\ncache_path = "cache"\nlibs = []\nevm_version = "osaka"\n')
body = "".join(f'''
        {{
            (bool a{i}, bytes memory ra{i}) = solc_.call{{gas: 10000000}}(hex"{cd[2:]}");
            (bool b{i}, bytes memory rb{i}) = solar_.call{{gas: 10000000}}(hex"{cd[2:]}");
            if (a{i} != b{i} || keccak256(ra{i}) != keccak256(rb{i})) _log("MISMATCH case {i}");
        }}''' for i, (g, cd) in enumerate(cases))
(proj/"test/Concrete.t.sol").write_text(f'''// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract ConcreteTest {{
    address constant CONSOLE = 0x000000000000000000636F6e736F6c652e6c6f67;
    function _deploy(bytes memory runtime) internal returns (address a) {{
        bytes memory init = abi.encodePacked(hex"63", uint32(runtime.length), hex"80600e6000396000f3", runtime);
        assembly {{ a := create(0, add(init, 0x20), mload(init)) }}
        require(a != address(0), "deploy");
    }}
    function _log(string memory s) internal view {{ (bool ok,) = CONSOLE.staticcall(abi.encodeWithSignature("log(string)", s)); ok; }}
    function testConcrete() public {{
        address solc_ = _deploy(hex"{solc_code}");
        address solar_ = _deploy(hex"{solar_code}");
        {body}
    }}
}}
''')
env = {k: v for k, v in os.environ.items() if not k.startswith("FOUNDRY_")}
r = subprocess.run(["forge", "test", "--root", str(proj), "--use", "/home/doni/.cargo/bin/solc", "--evm-version", "osaka", "--match-contract", "ConcreteTest", "-vvvv"], capture_output=True, text=True, cwd=proj, env=env)
out = r.stdout + r.stderr
lines = out.splitlines()
results = []
for i, line in enumerate(lines):
    if "::" in line and "console::" not in line and "ConcreteTest::" not in line and "→ new" not in line:
        nxt = lines[i+1].strip() if i+1 < len(lines) else ""
        mm = re.search(r"← (\[\w+\].*)$", nxt); results.append(mm.group(1) if mm else nxt)
flagged = [int(re.search(r"case (\d+)", l).group(1)) for l in lines if "MISMATCH case" in l]
print(f"flags={' '.join(flags) or '(default)'} -> {len(flagged)} mismatches" + (f"  solc={results[0][:60]} solar={results[1][:60]}" if results and len(results) >= 2 else ""))
if "[FAIL" in out or "Compiler run failed" in out: print(out[-600:])
