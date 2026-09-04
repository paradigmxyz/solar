#!/usr/bin/env python3
"""Stateful solc-vs-solar differential.

usage: statediff.py file.sol Contract [--seed N --calls N --seqs N] [--ctor "args"]
                    [--evm-version V] [--no-optimize|--optimizer-runs N] [--project-root DIR]
                    [--solar BIN] [--solc BIN] [--fixed "sig(types) a b"]... [--skip REGEX]
                    [--gas N] [--keep] [--value-max WEI]

Compiles the contract with both compilers (Standard JSON, via-IR for solc), deploys both
creation codes with the same constructor arguments inside a generated Foundry test, runs
randomized (or --fixed) call sequences against both deployments, and compares success,
return data, logs, and a storage snapshot after every call. Both deployment addresses are
normalized before comparison. Targets compiled for a pre-byzantium EVM run on an osaka EVM
and their revert data is ignored (the harness itself needs RETURNDATASIZE).

Prints one JSON object; "status" is "agree", "mismatch", or "error". Exit status is 0 for
agree, 1 for mismatch, 2 for error. The generated project is kept on mismatch or --keep;
its res/gas.txt lists per-call gas for both compilers.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import random
import re
import shutil
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = ROOT / "target" / "symaudit" / "sd-projects"
EVM_VERSIONS = [
    "homestead", "tangerineWhistle", "spuriousDragon", "byzantium", "constantinople",
    "petersburg", "istanbul", "berlin", "london", "paris", "shanghai", "cancun", "prague", "osaka",
]
PLACEHOLDER = "ad" * 20
WORD = (1 << 256) - 1


def evm_index(v: str) -> int:
    return EVM_VERSIONS.index(v)


# ---------------------------------------------------------------- compilation

def standard_input(src: pathlib.Path, root: pathlib.Path, evm: str, optimize: bool, runs: int,
                   solc: str, remappings: list[str] | None = None) -> tuple[dict, str]:
    """Snapshot the source and its imports (discovered through solc) into Standard JSON."""
    src = src.resolve()
    root = root.resolve()
    rel = src.relative_to(root).as_posix()
    disc = {"language": "Solidity", "sources": {rel: {"content": src.read_text()}},
            "settings": {"evmVersion": evm, "outputSelection": {"*": {"": ["ast"]}},
                         **({"remappings": remappings} if remappings else {})}}
    r = subprocess.run([solc, "--base-path", str(root), "--standard-json"], input=json.dumps(disc),
                       capture_output=True, text=True, cwd=root)
    out = json.loads(r.stdout)
    sources = {}
    for name in sorted(out.get("sources", {rel: {}})):
        p = src if name == rel else (root / name)
        if not p.is_file():
            raise ValueError(f"could not snapshot imported source {name}")
        sources[name] = {"content": p.read_text()}
    settings = {
        "optimizer": {"enabled": optimize, "runs": runs},
        "viaIR": True,
        "evmVersion": evm,
        "metadata": {"bytecodeHash": "none"},
        "outputSelection": {"*": {"*": ["abi", "evm.bytecode.object", "evm.bytecode.linkReferences",
                                        "evm.methodIdentifiers"]}},
        **({"remappings": remappings} if remappings else {}),
    }
    return {"language": "Solidity", "sources": sources, "settings": settings}, rel


def compile_one(compiler: str, inp: dict, rel: str, contract: str, label: str) -> dict:
    r = subprocess.run([compiler, "--standard-json"], input=json.dumps(inp), capture_output=True,
                       text=True, cwd=ROOT, timeout=600)
    try:
        out = json.loads(r.stdout)
    except json.JSONDecodeError:
        raise ValueError(f"{label} did not emit JSON: {(r.stderr or r.stdout)[-1500:]}")
    errs = [e.get("formattedMessage") or e.get("message") for e in out.get("errors", [])
            if e.get("severity") == "error"]
    if errs or r.returncode != 0:
        raise ValueError(f"{label} compilation failed: " + "\n".join(errs)[:3000])
    art = out.get("contracts", {}).get(rel, {}).get(contract)
    if art is None:
        # Fall back to any source unit that defines the contract (imports).
        for _, cs in out.get("contracts", {}).items():
            if contract in cs:
                art = cs[contract]
                break
    if art is None:
        raise ValueError(f"{label} did not emit contract {contract}")
    bc = art.get("evm", {}).get("bytecode", {})
    code = bc.get("object", "") or ""
    if not code:
        raise ValueError(f"{label} emitted no creation code for {contract} (abstract or interface?)")
    if bc.get("linkReferences"):
        raise ValueError(f"{label}: {contract} needs library linking (unsupported)")
    return {"code": code.removeprefix("0x").lower(), "abi": art["abi"],
            "ids": art.get("evm", {}).get("methodIdentifiers", {})}


# ---------------------------------------------------------------- ABI encoding

def canonical(item: dict) -> str:
    t = item["type"]
    if t.startswith("tuple"):
        return "(" + ",".join(canonical(c) for c in item["components"]) + ")" + t[5:]
    return t


def split_top(s: str, sep: str = ",") -> list[str]:
    parts, depth, cur = [], 0, ""
    for ch in s:
        if ch in "([":
            depth += 1
        elif ch in ")]":
            depth -= 1
        if ch == sep and depth == 0:
            parts.append(cur)
            cur = ""
        else:
            cur += ch
    if cur or parts:
        parts.append(cur)
    return parts


def parse_type(t: str):
    """Return a nested type description: ('tuple', [types]) | ('array', inner, n|None) | ('base', name)."""
    m = re.match(r"^(.*)\[(\d*)\]$", t)
    if m:
        return ("array", parse_type(m.group(1)), int(m.group(2)) if m.group(2) else None)
    if t.startswith("("):
        inner = t[1:-1]
        return ("tuple", [parse_type(x) for x in split_top(inner)] if inner else [])
    return ("base", t)


def is_dynamic(ty) -> bool:
    k = ty[0]
    if k == "base":
        return ty[1] in ("bytes", "string")
    if k == "array":
        return ty[2] is None or is_dynamic(ty[1])
    return any(is_dynamic(c) for c in ty[1])


def enc_word(v: int) -> str:
    return f"{v & WORD:064x}"


def enc_bytes(b: bytes) -> str:
    return enc_word(len(b)) + b.hex() + "00" * ((-len(b)) % 32)


def encode(ty, v) -> str:
    """ABI-encode a value; returns hex (no 0x)."""
    k = ty[0]
    if k == "base":
        t = ty[1]
        if t in ("bytes", "string"):
            return enc_bytes(v if isinstance(v, bytes) else str(v).encode())
        if t == "bool":
            return enc_word(1 if v else 0)
        if t == "address":
            return enc_word(int(v))
        if t == "function":
            b = v if isinstance(v, bytes) else int(v).to_bytes(24, "big")
            return b.ljust(32, b"\0").hex()
        if t.startswith("bytes"):
            n = int(t[5:])
            b = v if isinstance(v, bytes) else int(v).to_bytes(n, "big")
            return b.ljust(32, b"\0").hex()
        return enc_word(int(v))
    if k == "array":
        elems = [ty[1]] * len(v)
        body = encode_tuple(elems, list(v))
        return (enc_word(len(v)) if ty[2] is None else "") + body
    return encode_tuple(ty[1], list(v))


def encode_tuple(types, values) -> str:
    heads, tails = [], []
    head_len = sum(32 if is_dynamic(t) else len(encode(t, v)) // 2 for t, v in zip(types, values))
    off = head_len
    for t, v in zip(types, values):
        e = encode(t, v)
        if is_dynamic(t):
            heads.append(enc_word(off))
            tails.append(e)
            off += len(e) // 2
        else:
            heads.append(e)
    return "".join(heads) + "".join(tails)


# ---------------------------------------------------------------- value generation

def gen_value(ty, rng: random.Random, depth: int = 0):
    k = ty[0]
    if k == "base":
        t = ty[1]
        if t == "bool":
            return rng.random() < 0.5
        if t == "address":
            return rng.choice([0, 1, 2, rng.getrandbits(160), (1 << 160) - 1, 0xdead])
        if t == "function":
            return rng.choice([0, rng.getrandbits(192), 1])
        if t in ("bytes", "string"):
            n = rng.choice([0, 1, 2, 31, 32, 33, 64, rng.randint(0, 80)])
            if t == "string":
                return "".join(rng.choice("abcxyz09 !") for _ in range(n))
            return bytes(rng.getrandbits(8) for _ in range(n))
        if t.startswith("bytes"):
            n = int(t[5:])
            return rng.choice([0, 1, (1 << (8 * n)) - 1, rng.getrandbits(8 * n)])
        if t.startswith("uint"):
            bits = int(t[4:] or 256)
            mx = (1 << bits) - 1
            return rng.choice([0, 1, 2, 3, 7, 8, 31, 32, 33, 255, 256, 1000, mx, mx - 1, mx // 2,
                               rng.randint(0, min(mx, 10 ** 6)), rng.getrandbits(bits), rng.getrandbits(bits)])
        if t.startswith("int"):
            bits = int(t[3:] or 256)
            mx = (1 << (bits - 1)) - 1
            mn = -(1 << (bits - 1))
            return rng.choice([0, 1, -1, 2, -2, mx, mn, mx - 1, mn + 1, rng.randint(-1000, 1000),
                               rng.randint(mn, mx)])
        return 0
    if k == "array":
        n = ty[2] if ty[2] is not None else rng.choice([0, 1, 2, 3] if depth < 2 else [0, 1])
        return [gen_value(ty[1], rng, depth + 1) for _ in range(n)]
    return [gen_value(c, rng, depth + 1) for c in ty[1]]


# ---------------------------------------------------------------- literal parsing for --fixed/--ctor

class Hex(bytes):
    """A hex literal: raw bytes for `bytes`/`string`, an integer for everything else."""


def tokenize(s: str) -> list[str]:
    toks, i = [], 0
    while i < len(s):
        ch = s[i]
        if ch.isspace():
            i += 1
        elif ch in "[](),":
            toks.append(ch)
            i += 1
        elif ch == '"':
            j = s.index('"', i + 1)
            toks.append(s[i:j + 1])
            i = j + 1
        else:
            j = i
            while j < len(s) and not s[j].isspace() and s[j] not in "[](),":
                j += 1
            toks.append(s[i:j])
            i = j
    return toks


def parse_values(s: str) -> list:
    toks = tokenize(s)
    pos = [0]

    def value():
        t = toks[pos[0]]
        pos[0] += 1
        if t in "[(":
            close = "]" if t == "[" else ")"
            items = []
            while toks[pos[0]] != close:
                items.append(value())
                if toks[pos[0]] == ",":
                    pos[0] += 1
            pos[0] += 1
            return items
        if t.startswith('"'):
            return t[1:-1]
        if t == "true":
            return True
        if t == "false":
            return False
        if t.startswith("hex"):
            return bytes.fromhex(t[3:].strip('"'))
        if t.startswith("0x") and len(t) > 2:
            h = t[2:]
            return Hex(bytes.fromhex(("0" if len(h) % 2 else "") + h))
        try:
            return int(t, 0)
        except ValueError:
            return t

    out = []
    while pos[0] < len(toks):
        out.append(value())
        if pos[0] < len(toks) and toks[pos[0]] == ",":
            pos[0] += 1
    return out


def coerce_flat(types, values):
    """Coerce a flat scalar list (solc semantic-test style) into nested values for types."""
    it = iter(values)

    def take(ty):
        k = ty[0]
        if k == "base":
            return coerce(ty, next(it))
        if k == "array":
            if ty[2] is None:
                n = int(next(it))
                return [take(ty[1]) for _ in range(n)]
            return [take(ty[1]) for _ in range(ty[2])]
        return [take(c) for c in ty[1]]

    out = [take(t) for t in types]
    rest = list(it)
    if rest:
        raise ValueError(f"{len(rest)} extra constructor values")
    return out


def coerce_args(types, values):
    """Coerce parsed literals for a parameter list, accepting either nested or flat spellings."""
    if len(values) == len(types) and all(
        isinstance(v, list) or t[0] == "base" for t, v in zip(types, values)
    ):
        return [coerce(t, v) for t, v in zip(types, values)]
    return coerce_flat(types, values)


def coerce(ty, v):
    """Coerce a parsed literal into what encode() expects for ty."""
    k = ty[0]
    if k == "base":
        t = ty[1]
        if t == "bytes":
            return bytes(v) if isinstance(v, bytes) else (str(v).encode() if not isinstance(v, int) else v.to_bytes(32, "big"))
        if t == "string":
            return v.decode() if isinstance(v, bytes) else str(v)
        if t.startswith("bytes") or t == "function":
            return int.from_bytes(v, "big") if isinstance(v, bytes) else v
        if t == "address":
            return int.from_bytes(v, "big") if isinstance(v, bytes) else int(v)
        if t == "bool":
            return bool(v)
        return int.from_bytes(v, "big") if isinstance(v, bytes) else int(v)
    if k == "array":
        return [coerce(ty[1], x) for x in v]
    return [coerce(c, x) for c, x in zip(ty[1], v)]


# ---------------------------------------------------------------- forge project

VM_IFACE = """
interface Vm {
    struct Log { bytes32[] topics; bytes data; address emitter; }
    function record() external;
    function accesses(address) external returns (bytes32[] memory reads, bytes32[] memory writes);
    function recordLogs() external;
    function getRecordedLogs() external returns (Log[] memory);
    function readFile(string calldata) external view returns (string memory);
    function readLine(string calldata) external view returns (string memory);
    function closeFile(string calldata) external;
    function parseBytes(string calldata) external pure returns (bytes memory);
    function parseUint(string calldata) external pure returns (uint256);
    function writeLine(string calldata, string calldata) external;
    function toString(bytes calldata) external pure returns (string memory);
    function toString(uint256) external pure returns (string memory);
    function toString(address) external pure returns (string memory);
    function toString(bytes32) external pure returns (string memory);
    function load(address, bytes32) external view returns (bytes32);
    function deal(address, uint256) external;
}
"""

TEST = """// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;
{vm_iface}
contract StateDiffTest {{
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    string constant RES = "res/results.txt";
    uint256 constant GAS = {gas};
    uint256 constant SLOT_CAP = 3000;
    mapping(uint256 => mapping(bytes32 => bool)) seen;
    bytes32[] slots;

    function _deploy(bytes memory init) internal returns (address a) {{
        assembly {{ a := create(0, add(init, 0x20), mload(init)) }}
    }}

    function _w(string memory s) internal {{ vm.writeLine(RES, s); }}

    function _addSlots(uint256 seq, bytes32[] memory ws) internal {{
        for (uint256 j; j < ws.length; j++) {{
            if (!seen[seq][ws[j]]) {{ seen[seq][ws[j]] = true; slots.push(ws[j]); }}
        }}
    }}

    // Write only the slots whose raw values differ between the two deployments; equal slots
    // cannot produce a mismatch and would otherwise dominate the output.
    function _snapshot(uint256 i, address a, address b) internal {{
        for (uint256 j; j < slots.length; j++) {{
            bytes32 va = vm.load(a, slots[j]);
            bytes32 vb = vm.load(b, slots[j]);
            if (va != vb) {{
                _w(string.concat("S ", vm.toString(i), " ", vm.toString(slots[j]), " ", vm.toString(va), " ", vm.toString(vb)));
            }}
        }}
    }}

    function _logs(string memory who, uint256 i) internal {{
        Vm.Log[] memory ls = vm.getRecordedLogs();
        for (uint256 j; j < ls.length; j++) {{
            string memory s = string.concat("L ", who, " ", vm.toString(i), " ", vm.toString(ls[j].emitter), " ",
                vm.toString(ls[j].topics.length));
            for (uint256 k; k < ls[j].topics.length; k++) s = string.concat(s, " ", vm.toString(ls[j].topics[k]));
            _w(string.concat(s, " ", vm.toString(ls[j].data)));
        }}
    }}

    function _call(string memory who, uint256 i, address t, uint256 value, bytes memory cd) internal {{
        uint256 g0 = gasleft();
        (bool ok, bytes memory ret) = t.call{{gas: GAS, value: value}}(cd);
        uint256 used = g0 - gasleft();
        _w(string.concat("C ", who, " ", vm.toString(i), " ", ok ? "1" : "0", " ", vm.toString(used), " ", vm.toString(ret)));
        _logs(who, i);
    }}

    bytes solcCode; bytes solarCode; bytes ctor;
    bool big;

    function _parseLine(string memory line) internal pure returns (uint256 value, bytes memory cd) {{
        bytes memory lb = bytes(line);
        uint256 sp;
        while (lb[sp] != 0x20) sp++;
        bytes memory vs = new bytes(sp);
        for (uint256 k; k < sp; k++) vs[k] = lb[k];
        bytes memory cs = new bytes(lb.length - sp - 1);
        for (uint256 k; k < cs.length; k++) cs[k] = lb[sp + 1 + k];
        value = vm.parseUint(string(vs));
        cd = vm.parseBytes(string(cs));
    }}

    function _accesses(uint256 seq, address a, address b) internal {{
        (, bytes32[] memory wa) = vm.accesses(a);
        (, bytes32[] memory wb) = vm.accesses(b);
        _addSlots(seq, wa); _addSlots(seq, wb);
        if (slots.length > SLOT_CAP) big = true;
        // Restart recording so the next access list only covers the next call; the read list
        // of a warm-slot loop can reach hundreds of thousands of entries per call.
        vm.record();
    }}

    function _step(uint256 seq, uint256 i, bool last, address a, address b, string memory line) internal {{
        (uint256 value, bytes memory cd) = _parseLine(line);
        _call("solc", i, a, value, cd);
        _call("solar", i, b, value, cd);
        // Contracts that touch thousands of slots make the per-call access lists and
        // snapshots exhaust the test's memory; past the cap, compare storage only after
        // the last call of the sequence.
        if (!big || last) {{
            _accesses(seq, a, b);
            _snapshot(i, a, b);
        }} else {{
            _w(string.concat("X ", vm.toString(i)));
        }}
    }}

    function _runSeq(uint256 seq) internal {{
        delete slots;
        big = false;
        string memory path = string.concat("data/seq", vm.toString(seq), ".txt");
        uint256 n = vm.parseUint(vm.readLine(path));
        address a = _deploy(bytes.concat(solcCode, ctor));
        _w(string.concat("D solc ", vm.toString(seq), " ", a == address(0) ? "0" : "1", " ", vm.toString(a)));
        address b = _deploy(bytes.concat(solarCode, ctor));
        _w(string.concat("D solar ", vm.toString(seq), " ", b == address(0) ? "0" : "1", " ", vm.toString(b)));
        vm.getRecordedLogs();
        if (a != address(0) && b != address(0)) {{
            _accesses(seq, a, b);
            _snapshot(1000000, a, b);
            for (uint256 i; i < n; i++) {{
                _step(seq, i, i + 1 == n, a, b, vm.readLine(path));
            }}
        }}
        vm.closeFile(path);
        _w(string.concat("E ", vm.toString(seq)));
    }}

    function testRun() public {{
        vm.deal(address(this), 1e36);
        solcCode = vm.parseBytes(vm.readFile("data/solc.hex"));
        solarCode = vm.parseBytes(vm.readFile("data/solar.hex"));
        ctor = vm.parseBytes(vm.readFile("data/ctor.hex"));
        uint256 nseq = vm.parseUint(vm.readFile("data/nseq.txt"));
        vm.record();
        vm.recordLogs();
        for (uint256 seq; seq < nseq; seq++) _runSeq(seq);
    }}
}}
"""

FOUNDRY_TOML = """[profile.default]
src = "src"
test = "test"
out = "out"
libs = []
optimizer = false
evm_version = "{evm}"
fs_permissions = [{{ access = "read-write", path = "./" }}]
gas_limit = 9223372036854775807
memory_limit = 8589934592
offline = true
"""


def normalize(s: str, addrs: list[str]) -> str:
    s = s.lower()
    for a in addrs:
        s = s.replace(a, PLACEHOLDER)
    return s


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("source", type=pathlib.Path)
    ap.add_argument("contract")
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--calls", type=int, default=30)
    ap.add_argument("--seqs", type=int, default=3)
    ap.add_argument("--ctor", default="")
    ap.add_argument("--evm-version", default="osaka")
    ap.add_argument("--no-optimize", action="store_true")
    ap.add_argument("--optimizer-runs", type=int, default=200)
    ap.add_argument("--project-root", type=pathlib.Path)
    ap.add_argument("--solar", default=str(ROOT / "target/debug/solar"))
    ap.add_argument("--solc", default="solc")
    ap.add_argument("--fixed", action="append", default=[])
    ap.add_argument("--skip", default=None)
    ap.add_argument("--gas", type=int, default=20_000_000)
    ap.add_argument("--value-max", type=int, default=10 ** 18)
    ap.add_argument("--keep", action="store_true")
    ap.add_argument("--random-ctor", action="store_true", help="generate constructor arguments from the seed when --ctor is absent")
    ap.add_argument("--remapping", action="append", default=[], help="prefix=target, as in Standard JSON")
    ap.add_argument("--forge-evm", default=None, help="EVM version for the forge run (default: target, or osaka pre-byzantium)")
    args = ap.parse_args()

    solc = shutil.which(args.solc) or args.solc
    solar = str(pathlib.Path(args.solar).resolve()) if "/" in args.solar else shutil.which(args.solar)
    root = args.project_root or args.source.parent
    result = {"file": str(args.source), "contract": args.contract, "evm_version": args.evm_version}
    t0 = time.time()
    try:
        inp, rel = standard_input(args.source, root, args.evm_version, not args.no_optimize,
                                  args.optimizer_runs, solc, args.remapping)
        a = compile_one(solc, inp, rel, args.contract, "solc")
        b = compile_one(solar, inp, rel, args.contract, "solar")
    except Exception as err:  # noqa: BLE001
        result.update(status="error", reason=str(err)[:3000])
        print(json.dumps(result, indent=1))
        return 2

    abi = a["abi"]
    funcs = [e for e in abi if e.get("type") == "function"]
    sigs = {f"{e['name']}({','.join(canonical(i) for i in e['inputs'])})": e for e in funcs}
    # methodIdentifiers may spell a signature differently (e.g. enum or contract types); fall
    # back to hashing the canonical signature.
    for sig_ in sigs:
        if sig_ not in a["ids"]:
            a["ids"][sig_] = subprocess.run(["cast", "sig", sig_], capture_output=True, text=True).stdout.strip().removeprefix("0x")
    ctor = next((e for e in abi if e.get("type") == "constructor"), None)
    ctor_types = [parse_type(canonical(i)) for i in (ctor["inputs"] if ctor else [])]
    rng = random.Random(args.seed)
    if ctor_types and not args.ctor and not args.random_ctor:
        result.update(status="error", reason="constructor needs arguments (--ctor)")
        print(json.dumps(result, indent=1))
        return 2
    try:
        if ctor_types and not args.ctor:
            ctor_vals = [gen_value(t, rng) for t in ctor_types]
            result["ctor_random"] = repr(ctor_vals)[:300]
            ctor_hex = encode_tuple(ctor_types, ctor_vals)
        else:
            ctor_hex = encode_tuple(ctor_types, coerce_args(ctor_types, parse_values(args.ctor))) if ctor_types else ""
    except Exception as err:  # noqa: BLE001
        result.update(status="error", reason=f"bad --ctor: {err}")
        print(json.dumps(result, indent=1))
        return 2

    skip = re.compile(args.skip) if args.skip else None
    callable_sigs = [s for s in sigs if not (skip and skip.search(s))]
    sequences: list[list[tuple[str, str, int, str]]] = []  # (sig, args text, value, calldata)
    if args.fixed:
        seq = []
        for spec in args.fixed:
            m = re.match(r"^\s*([A-Za-z_$][\w$]*)(\(.*?\))?(?:\s+(.*))?$", spec, re.S)
            if not m:
                result.update(status="error", reason=f"bad --fixed spec: {spec}")
                print(json.dumps(result, indent=1))
                return 2
            name, types_txt, rest = m.group(1), m.group(2), m.group(3) or ""
            # Find the signature: exact if types given, otherwise by unique name.
            if types_txt:
                # The regex is non-greedy; rebuild by matching balanced parens.
                depth, j = 0, len(name)
                while j < len(spec.lstrip()):
                    ch = spec.lstrip()[j]
                    depth += ch == "("
                    depth -= ch == ")"
                    j += 1
                    if depth == 0:
                        break
                sig = spec.lstrip()[:j]
                rest = spec.lstrip()[j:].strip()
            else:
                cands = [s for s in sigs if s.startswith(name + "(")]
                if len(cands) != 1:
                    result.update(status="error", reason=f"ambiguous or unknown function {name}: {cands}")
                    print(json.dumps(result, indent=1))
                    return 2
                sig = cands[0]
            if sig not in sigs:
                result.update(status="error", reason=f"unknown signature {sig}; have {sorted(sigs)}")
                print(json.dumps(result, indent=1))
                return 2
            value = 0
            vm_ = re.search(r"\bvalue=(\d+)\s*$", rest)
            if vm_:
                value = int(vm_.group(1))
                rest = rest[:vm_.start()].strip()
            e = sigs[sig]
            types = [parse_type(canonical(i)) for i in e["inputs"]]
            vals = parse_values(rest)
            try:
                coerced = coerce_args(types, vals)
            except Exception as err:  # noqa: BLE001
                result.update(status="error", reason=f"{sig}: bad arguments {vals!r}: {err}")
                print(json.dumps(result, indent=1))
                return 2
            cd = a["ids"][sig] + encode_tuple(types, coerced)
            seq.append((sig, rest, value, cd))
        sequences.append(seq)
    else:
        if not callable_sigs:
            result.update(status="error", reason="no callable functions")
            print(json.dumps(result, indent=1))
            return 2
        for _ in range(args.seqs):
            seq = []
            for _ in range(args.calls):
                sig = rng.choice(callable_sigs)
                e = sigs[sig]
                types = [parse_type(canonical(i)) for i in e["inputs"]]
                vals = [gen_value(t, rng) for t in types]
                value = 0
                if e.get("stateMutability") == "payable" and rng.random() < 0.5 and args.value_max:
                    value = rng.choice([1, 1000, args.value_max])
                cd = a["ids"][sig] + encode_tuple(types, vals)
                seq.append((sig, repr(vals)[:200], value, cd))
            sequences.append(seq)

    # Write the project.
    OUT.mkdir(parents=True, exist_ok=True)
    tag = hashlib.sha1(f"{args.source}{args.contract}{args.seed}{time.time()}{os.getpid()}".encode()).hexdigest()[:10]
    proj = OUT / f"{args.source.stem}-{args.contract}-{tag}"
    (proj / "src").mkdir(parents=True)
    (proj / "test").mkdir()
    (proj / "data").mkdir()
    (proj / "res").mkdir()
    (proj / "data/solc.hex").write_text("0x" + a["code"])
    (proj / "data/solar.hex").write_text("0x" + b["code"])
    (proj / "data/ctor.hex").write_text("0x" + ctor_hex)
    (proj / "data/nseq.txt").write_text(str(len(sequences)))
    for k, seq in enumerate(sequences):
        (proj / f"data/seq{k}.txt").write_text(f"{len(seq)}\n" + "".join(f"{v} 0x{cd}\n" for _, _, v, cd in seq))
    (proj / "test/StateDiff.t.sol").write_text(TEST.format(vm_iface=VM_IFACE, gas=args.gas))
    pre_byz = evm_index(args.evm_version) < evm_index("byzantium")
    forge_evm = args.forge_evm or ("osaka" if pre_byz else args.evm_version)
    (proj / "foundry.toml").write_text(FOUNDRY_TOML.format(evm=forge_evm))
    (proj / "src/.keep").write_text("")

    env = {k: v for k, v in os.environ.items() if not k.startswith(("FOUNDRY_", "DAPP_"))}
    cmd = ["forge", "test", "--root", str(proj), "--use", solc, "--evm-version", forge_evm,
           "--match-contract", "StateDiffTest", "--force"] + (["-vvvv"] if args.keep else [])
    r = subprocess.run(cmd, capture_output=True, text=True, cwd=proj, env=env, timeout=3600)
    (proj / "res/forge.txt").write_text(r.stdout + r.stderr)
    if args.keep:
        (proj / "trace.txt").write_text(r.stdout)
    res_path = proj / "res/results.txt"
    if not res_path.exists():
        result.update(status="error", reason="forge produced no results: " + (r.stdout + r.stderr)[-2500:],
                      project=str(proj))
        print(json.dumps(result, indent=1))
        return 2

    # Parse results.
    lines = res_path.read_text().splitlines()
    deploys: dict[tuple[int, str], tuple[bool, str]] = {}
    calls: dict[tuple[int, int, str], dict] = {}
    seq_i = 0
    skipped_snapshots = 0
    addrs: list[str] = []
    for line in lines:
        parts = line.split(" ")
        kind = parts[0]
        if kind == "D":
            who, s, ok, addr = parts[1], int(parts[2]), parts[3] == "1", parts[4].lower()
            deploys[(s, who)] = (ok, addr)
            if ok:
                addrs.append(addr[2:])
            seq_i = s
        elif kind == "C":
            who, i, ok, gas, ret = parts[1], int(parts[2]), parts[3] == "1", int(parts[4]), parts[5]
            calls[(seq_i, i, who)] = {"ok": ok, "gas": gas, "ret": ret, "logs": [], "storage": {}}
        elif kind == "L":
            who, i, emitter, nt = parts[1], int(parts[2]), parts[3], int(parts[4])
            topics = parts[5:5 + nt]
            data = parts[5 + nt] if len(parts) > 5 + nt else "0x"
            calls[(seq_i, i, who)]["logs"].append({"emitter": emitter, "topics": topics, "data": data})
        elif kind == "S":
            i, slot, va, vb = int(parts[1]), parts[2], parts[3], parts[4]
            for who, val in (("solc", va), ("solar", vb)):
                key = (seq_i, i, who)
                if key not in calls:
                    calls[key] = {"ok": None, "gas": 0, "ret": "", "logs": [], "storage": {}}
                calls[key]["storage"][slot] = val
        elif kind == "X":
            skipped_snapshots += 1
        elif kind == "E":
            pass

    mismatches = []
    gas_rows = []
    ncalls = 0
    aborted = False
    for s, seq in enumerate(sequences):
        da, db = deploys.get((s, "solc")), deploys.get((s, "solar"))
        if da is None or db is None:
            mismatches.append({"seq": s, "field": "deploy", "detail": "missing deployment record (test aborted?)"})
            continue
        if da[0] != db[0]:
            mismatches.append({"seq": s, "field": "deploy", "solc": da[0], "solar": db[0]})
            continue
        if not da[0]:
            continue
        # Post-constructor storage (recorded under call index 1000000).
        for idx, (sig, argtxt, value, cd) in [(1000000, ("<constructor>", args.ctor, 0, ""))] + list(enumerate(seq)):
            ca, cb = calls.get((s, idx, "solc")), calls.get((s, idx, "solar"))
            if ca is None or cb is None:
                if idx != 1000000:
                    aborted = True
                continue
            if idx != 1000000:
                ncalls += 1
                gas_rows.append((s, idx, sig, argtxt, ca["gas"], cb["gas"]))
                if ca["ok"] != cb["ok"]:
                    # A side that failed with (almost) the whole call gas consumed while the other
                    # side succeeded is a gas-limit divergence, not a semantic one.
                    failed_gas = cb["gas"] if ca["ok"] else ca["gas"]
                    field = "gas_limit" if failed_gas >= args.gas - 64 else "success"
                    mismatches.append({"seq": s, "call": idx, "sig": sig, "args": argtxt, "value": value, "field": field,
                                       "solc": ca["ok"], "solar": cb["ok"], "solc_ret": ca["ret"][:400], "solar_ret": cb["ret"][:400],
                                       "solc_gas": ca["gas"], "solar_gas": cb["gas"]})
                    break
                ra, rb = normalize(ca["ret"], addrs), normalize(cb["ret"], addrs)
                if ra != rb and (ca["ok"] or not pre_byz):
                    mismatches.append({"seq": s, "call": idx, "sig": sig, "args": argtxt, "value": value, "field": "returndata",
                                       "ok": ca["ok"], "solc": ca["ret"][:600], "solar": cb["ret"][:600]})
                    break
                la = [(normalize(l["emitter"], addrs), [normalize(t, addrs) for t in l["topics"]], normalize(l["data"], addrs)) for l in ca["logs"]]
                lb = [(normalize(l["emitter"], addrs), [normalize(t, addrs) for t in l["topics"]], normalize(l["data"], addrs)) for l in cb["logs"]]
                if la != lb:
                    mismatches.append({"seq": s, "call": idx, "sig": sig, "args": argtxt, "value": value, "field": "logs",
                                       "solc": ca["logs"], "solar": cb["logs"]})
                    break
            sa = {k: normalize(v, addrs) for k, v in ca["storage"].items()}
            sb = {k: normalize(v, addrs) for k, v in cb["storage"].items()}
            if sa != sb:
                diff = {k: (sa.get(k), sb.get(k)) for k in sorted(set(sa) | set(sb)) if sa.get(k) != sb.get(k)}
                mismatches.append({"seq": s, "call": idx if idx != 1000000 else "constructor", "sig": sig, "args": argtxt,
                                   "field": "storage", "diff": {k: {"solc": v[0], "solar": v[1]} for k, v in list(diff.items())[:20]}})
                break

    (proj / "res/gas.txt").write_text("".join(
        f"[gas] seq {s} call {i} {sig} {argtxt} solc={ga} solar={gb} ratio={gb / ga if ga else 0:.2f}\n"
        for s, i, sig, argtxt, ga, gb in gas_rows))
    total_a = sum(g[4] for g in gas_rows)
    total_b = sum(g[5] for g in gas_rows)
    result.update(
        status="mismatch" if mismatches else "agree",
        sequences=len(sequences), calls=ncalls,
        deploy={"solc": [deploys.get((s, "solc"), (None,))[0] for s in range(len(sequences))],
                "solar": [deploys.get((s, "solar"), (None,))[0] for s in range(len(sequences))]},
        gas={"solc": total_a, "solar": total_b, "ratio": round(total_b / total_a, 3) if total_a else None},
        mismatches=mismatches[:10], elapsed=round(time.time() - t0, 1),
        forge_status=r.returncode,
    )
    if args.keep or mismatches or r.returncode != 0:
        result["project"] = str(proj)
    if args.keep:
        result["gas_calls"] = [{"seq": s, "call": i, "sig": sig, "args": argtxt, "solc": ga, "solar": gb}
                               for s, i, sig, argtxt, ga, gb in gas_rows]
    result["skipped_snapshots"] = skipped_snapshots
    if (r.returncode != 0 or aborted) and not mismatches:
        result["status"] = "error"
        fail = re.search(r"\[FAIL[^\]]*\][^\n]*", r.stdout)
        result["reason"] = "test aborted: " + (fail.group(0) if fail else (r.stdout + r.stderr)[-600:])
        result["project"] = str(proj)
    print(json.dumps(result, indent=1))
    if not (args.keep or mismatches or r.returncode != 0):
        shutil.rmtree(proj, ignore_errors=True)
    return 0 if result["status"] == "agree" else (1 if result["status"] == "mismatch" else 2)


if __name__ == "__main__":
    sys.exit(main())
