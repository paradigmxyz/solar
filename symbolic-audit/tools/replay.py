#!/usr/bin/env python3
"""Concretely replay a solsymdiff counterexample and print both runtimes' results."""
import json, pathlib, re, subprocess, sys, os
proj = pathlib.Path(sys.argv[1]).resolve()
calldata = sys.argv[2] if len(sys.argv) > 2 else json.load(open(proj / "result.json"))["counterexample"]["calldata"]
prefixes = []
for rf in pathlib.Path(__file__).parent.glob("results*.jsonl"):
    for line in rf.read_text().splitlines():
        d = json.loads(line)
        if d.get("project") == str(proj):
            prefixes = d.get("prefixes") or []
TPL = """
        {
            (bool ps1, bytes memory pr1) = solc_.call{gas: 10000000}(hex"%s");
            (bool ps2, bytes memory pr2) = solar_.call{gas: 10000000}(hex"%s");
            ps1; pr1; ps2; pr2;
        }"""
calls = "".join(TPL % (pf[2:], pf[2:]) for pf in prefixes)
t = (proj / "test" / "SymbolicDifferential.t.sol").read_text()
solc_code = re.search(r'SOLC_CODE = hex"([0-9a-f]*)"', t).group(1)
solar_code = re.search(r'SOLAR_CODE = hex"([0-9a-f]*)"', t).group(1)
src = f'''// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract ReplayTest {{
    address constant CONSOLE = 0x000000000000000000636F6e736F6c652e6c6f67;
    function _deploy(bytes memory runtime) internal returns (address a) {{
        bytes memory init = abi.encodePacked(hex"63", uint32(runtime.length), hex"80600e6000396000f3", runtime);
        assembly {{ a := create(0, add(init, 0x20), mload(init)) }}
        require(a != address(0), "deploy");
    }}
    function _logb(string memory s, bytes memory b) internal view {{ (bool ok,) = CONSOLE.staticcall(abi.encodeWithSignature("logBytes(bytes)", b)); ok; }}
    function _logu(string memory s, uint256 u) internal view {{ (bool ok,) = CONSOLE.staticcall(abi.encodeWithSignature("log(string,uint256)", s, u)); ok; }}
    function testReplay() public {{
        address solc_ = _deploy(hex"{solc_code}");
        address solar_ = _deploy(hex"{solar_code}");
        {calls}
        bytes memory cd = hex"{calldata[2:]}";
        (bool s1, bytes memory r1) = solc_.call{{gas: 10000000}}(cd);
        (bool s2, bytes memory r2) = solar_.call{{gas: 10000000}}(cd);
        _logu("solc  success", s1 ? 1 : 0); _logb("solc  return", r1);
        _logu("solar success", s2 ? 1 : 0); _logb("solar return", r2);
    }}
}}
'''
(proj / "test" / "Replay.t.sol").write_text(src)
env = {k: v for k, v in os.environ.items() if not k.startswith("FOUNDRY_")}
r = subprocess.run(["forge", "test", "--root", str(proj), "--use", "/home/doni/.cargo/bin/solc", "--evm-version", "osaka", "--match-contract", "ReplayTest", "-vvvv", "--force"], capture_output=True, text=True, cwd=proj, env=env)
out = r.stdout + r.stderr
lines = out.splitlines()
labels = iter([("solc  p%d" if i % 2 == 0 else "solar p%d") % (i // 2 + 1) for i in range(2 * len(prefixes))] + ["solc  target", "solar target"])
for i, line in enumerate(lines):
    if "::" in line and "console::" not in line and "ReplayTest::" not in line and "→ new" not in line:
        nxt = lines[i+1].strip() if i+1 < len(lines) else ""
        m = re.search(r"← (\[\w+\].*)$", nxt)
        try: lab = next(labels)
        except StopIteration: lab = "?"
        print(lab, m.group(1) if m else nxt)
    if "[FAIL" in line or "Error" in line: print(line.strip())
