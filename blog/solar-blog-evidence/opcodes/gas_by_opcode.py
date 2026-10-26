#!/usr/bin/env python3
"""Attribute runtime-benchmark execution gas to opcodes and opcode kinds with an EVM step tracer.

`collect` runs the runtime benchmark's own cases and gas calls (benches/runtime/benchmark.py,
`--gas-profile hot`) for one compiler configuration against a private anvil node and traces every
gas call with `debug_traceTransaction` and `tracer.js`. `report` merges collected runs into a
Markdown report and a JSON summary.

    uv run --no-project --python 3.14 gas_by_opcode.py collect \
        --repo /path/to/solar --compiler solar --solar /path/to/solar-bin \
        --objective gas --port 8601 --output runs/solar-gas.json
    uv run --no-project --python 3.14 gas_by_opcode.py report runs/*.json --output report.md
"""

from __future__ import annotations

import argparse
import json
import os
import stat
import subprocess
import sys
import tempfile
import time
import urllib.request
from collections import defaultdict
from pathlib import Path
from typing import Any

HERE = Path(__file__).resolve().parent
TRACER = (HERE / "tracer.js").read_text()

KIND_ORDER = [
    "stack",
    "push",
    "recipe",
    "control",
    "guard",
    "arith",
    "compare",
    "bitwise",
    "memory",
    "fmp",
    "mem_expansion",
    "storage",
    "transient",
    "calldata",
    "returndata",
    "keccak",
    "call",
    "precompile",
    "log",
    "env",
    "halt",
    "other",
]
KIND_LABEL = {
    "stack": "stack shuffling (DUP/SWAP/POP)",
    "push": "constants: PUSH",
    "recipe": "constants: recipes (pure ops on in-block constants)",
    "control": "control flow (JUMP/JUMPI/JUMPDEST)",
    "guard": "revert guards (JUMPI to reverting code + its condition chain and target push)",
    "arith": "arithmetic",
    "compare": "comparisons (non-guard)",
    "bitwise": "bitwise/shifts",
    "memory": "memory ops (static + copy words)",
    "fmp": "free-memory-pointer bookkeeping (mload/mstore 0x40 + push + bump add)",
    "mem_expansion": "memory expansion (dynamic)",
    "storage": "storage (SLOAD/SSTORE incl. cold/set)",
    "transient": "transient storage (TLOAD/TSTORE)",
    "calldata": "calldata",
    "returndata": "returndata",
    "keccak": "keccak",
    "call": "calls/creates (own overhead)",
    "precompile": "precompile execution",
    "log": "logs",
    "env": "environment/code (CALLER, GAS, CODECOPY, ...)",
    "halt": "halts (STOP/RETURN/REVERT static)",
    "other": "other",
}
OVERHEAD_KINDS = ["stack", "push", "recipe", "control", "guard", "fmp"]
CONFIGS = {
    "gas": ["solar", "solc-legacy", "solc-viair"],
    "size": ["solar", "solc-legacy", "solc-viair"],
}


def rpc(url: str, method: str, params: list[Any], timeout: float = 3600) -> Any:
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    req = urllib.request.Request(url, body, {"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        data = json.loads(resp.read())
    if "error" in data:
        raise RuntimeError(f"{method}: {data['error']}")
    return data["result"]


def solc_wrapper(directory: Path, solc: str, via_ir: bool) -> Path:
    path = directory / f"solc-{'viair' if via_ir else 'legacy'}"
    path.write_text(
        f"""#!/usr/bin/env python3
import json, subprocess, sys
payload = json.load(sys.stdin)
payload.setdefault("settings", {{}})["viaIR"] = {via_ir!r}
proc = subprocess.run([{solc!r}, "--standard-json"], input=json.dumps(payload), text=True, capture_output=True)
sys.stdout.write(proc.stdout)
sys.stderr.write(proc.stderr)
sys.exit(proc.returncode)
"""
    )
    path.chmod(path.stat().st_mode | stat.S_IEXEC)
    return path


def collect(args: argparse.Namespace) -> int:
    sys.path.insert(0, str(Path(args.repo) / "benches" / "runtime"))
    import benchmark  # type: ignore
    import cases  # type: ignore
    from common import parse_receipt_int  # type: ignore

    rpc_url = f"http://127.0.0.1:{args.port}"
    tmp = Path(tempfile.mkdtemp(prefix="gas-by-opcode-"))
    if args.compiler == "solar":
        spec = benchmark.CompilerSpec("solar", "solar", Path(args.solar), "solar")
    else:
        wrapper = solc_wrapper(tmp, args.solc, args.compiler == "solc-viair")
        spec = benchmark.CompilerSpec(args.compiler, args.compiler, wrapper, "solc", ())
    runs = 1 if args.objective == "size" else None

    state: dict[str, Any] = {"traces": [], "runtime": ""}
    orig_compile = benchmark.compile_case

    def compile_case(*a: Any, **k: Any) -> dict[str, Any]:
        res = orig_compile(*a, **k)
        state["runtime"] = res.get("runtime_bytecode", "")
        return res

    orig_send_call = benchmark.send_call

    def send_call(address, signature, call_args, rpc_url_, private_key):
        gas, tx_hash, error = orig_send_call(address, signature, call_args, rpc_url_, private_key)
        if gas is None or tx_hash is None:
            return gas, tx_hash, error
        if state.get("tracing", True):
            started = time.monotonic()
            trace = rpc(rpc_url_, "debug_traceTransaction", [tx_hash, {"tracer": TRACER, "timeout": "3600s"}])
            trace["trace_seconds"] = time.monotonic() - started
            trace["receipt_gas"] = gas
            trace["signature"] = signature
            trace["args"] = list(call_args)
            state["traces"].append(trace)
        return gas, tx_hash, error

    benchmark.compile_case = compile_case
    benchmark.send_call = send_call

    tests = benchmark.select_tests(["runtime"], "all")
    if args.tests:
        tests = [t for t in tests if t.test_id in set(args.tests)]
    out: dict[str, Any] = {"compiler": args.compiler, "objective": args.objective, "cases": {}}
    anvil = benchmark.start_anvil(rpc_url)
    try:
        for test in tests:
            if not cases.gas_calls(test, "hot"):
                continue
            state["traces"] = []
            started = time.monotonic()
            entry = benchmark.run_test_case(
                test, [spec], True, "hot", rpc_url, benchmark.DEFAULT_PRIVATE_KEY,
                optimizer_runs=runs,
            )
            data = entry["compilers"][spec.compiler_id]
            traces = list(state["traces"])
            calls = []
            pcs: dict[str, dict[str, list[int]]] = {}
            for result in data.get("gas_results", []):
                if result.get("gas") is None:
                    calls.append({"label": result["label"], "error": result.get("error", "")})
                    continue
                # Setup transactions (such as the Zones proxy setup) also go through call_contract.
                trace = traces.pop(0)
                while trace["signature"] != result["call"] or trace["receipt_gas"] != result["gas"]:
                    trace = traces.pop(0)
                # Per-pc gas covers only calls the runtime bench compares.
                pcs_map = trace.pop("pcs")
                for addr, m in ({} if result.get("comparison_exclusion_reason") else pcs_map).items():
                    agg = pcs.setdefault(addr, {})
                    for pc, v in m.items():
                        e = agg.setdefault(pc, [0] * len(v))
                        for i, x in enumerate(v):
                            e[i] += x
                trace.pop("signature")
                trace.pop("args")
                calls.append({
                    "label": result["label"],
                    "call": result["call"],
                    "excluded": result.get("comparison_exclusion_reason"),
                    **trace,
                })
            codes = {}
            for addr in pcs:
                try:
                    codes[addr] = rpc(rpc_url, "eth_getCode", [addr, "latest"])
                except Exception as exc:  # noqa: BLE001
                    codes[addr] = f"error: {exc}"
            out["cases"][test.test_id] = {
                "status": data.get("status"),
                "error": data.get("error") or data.get("deploy_error") or "",
                "address": data.get("address"),
                "runtime_size": data.get("runtime_size"),
                "runtime_bytecode": state["runtime"],
                "calls": calls,
                "pcs": pcs,
                "codes": codes,
            }
            n_ok = sum(1 for c in calls if "exec" in c)
            print(
                f"[{args.compiler}/{args.objective}] {test.test_id}: {data.get('status')} "
                f"{n_ok}/{len(calls)} calls traced in {time.monotonic() - started:.1f}s",
                flush=True,
            )
            Path(args.output).write_text(json.dumps(out))
    finally:
        benchmark.stop_anvil(anvil)
    Path(args.output).write_text(json.dumps(out))
    return 0


OPNAMES: dict[int, str] = {}


def opname(op: int) -> str:
    if not OPNAMES:
        names = {
            0x00: "STOP", 0x01: "ADD", 0x02: "MUL", 0x03: "SUB", 0x04: "DIV", 0x05: "SDIV", 0x06: "MOD",
            0x07: "SMOD", 0x08: "ADDMOD", 0x09: "MULMOD", 0x0A: "EXP", 0x0B: "SIGNEXTEND", 0x10: "LT",
            0x11: "GT", 0x12: "SLT", 0x13: "SGT", 0x14: "EQ", 0x15: "ISZERO", 0x16: "AND", 0x17: "OR",
            0x18: "XOR", 0x19: "NOT", 0x1A: "BYTE", 0x1B: "SHL", 0x1C: "SHR", 0x1D: "SAR", 0x1E: "CLZ",
            0x20: "KECCAK256", 0x30: "ADDRESS", 0x31: "BALANCE", 0x32: "ORIGIN", 0x33: "CALLER",
            0x34: "CALLVALUE", 0x35: "CALLDATALOAD", 0x36: "CALLDATASIZE", 0x37: "CALLDATACOPY",
            0x38: "CODESIZE", 0x39: "CODECOPY", 0x3A: "GASPRICE", 0x3B: "EXTCODESIZE", 0x3C: "EXTCODECOPY",
            0x3D: "RETURNDATASIZE", 0x3E: "RETURNDATACOPY", 0x3F: "EXTCODEHASH", 0x40: "BLOCKHASH",
            0x41: "COINBASE", 0x42: "TIMESTAMP", 0x43: "NUMBER", 0x44: "PREVRANDAO", 0x45: "GASLIMIT",
            0x46: "CHAINID", 0x47: "SELFBALANCE", 0x48: "BASEFEE", 0x49: "BLOBHASH", 0x4A: "BLOBBASEFEE",
            0x50: "POP", 0x51: "MLOAD", 0x52: "MSTORE", 0x53: "MSTORE8", 0x54: "SLOAD", 0x55: "SSTORE",
            0x56: "JUMP", 0x57: "JUMPI", 0x58: "PC", 0x59: "MSIZE", 0x5A: "GAS", 0x5B: "JUMPDEST",
            0x5C: "TLOAD", 0x5D: "TSTORE", 0x5E: "MCOPY", 0x5F: "PUSH0", 0xF0: "CREATE", 0xF1: "CALL",
            0xF2: "CALLCODE", 0xF3: "RETURN", 0xF4: "DELEGATECALL", 0xF5: "CREATE2", 0xFA: "STATICCALL",
            0xFD: "REVERT", 0xFE: "INVALID", 0xFF: "SELFDESTRUCT",
        }
        for i in range(1, 33):
            names[0x5F + i] = f"PUSH{i}"
        for i in range(1, 17):
            names[0x7F + i] = f"DUP{i}"
            names[0x8F + i] = f"SWAP{i}"
        for i in range(5):
            names[0xA0 + i] = f"LOG{i}"
        OPNAMES.update(names)
    return OPNAMES.get(op, f"0x{op:02x}")


def disassemble(code_hex: str) -> list[tuple[int, str, str]]:
    code = bytes.fromhex(code_hex.removeprefix("0x"))
    out, pc = [], 0
    while pc < len(code):
        op = code[pc]
        if 0x60 <= op <= 0x7F:
            n = op - 0x5F
            out.append((pc, opname(op), "0x" + code[pc + 1 : pc + 1 + n].hex()))
            pc += n + 1
        else:
            out.append((pc, opname(op), ""))
            pc += 1
    return out


def hot_blocks(code_hex: str, pcs: dict[str, list[int]], top: int) -> list[dict[str, Any]]:
    """Group executed pcs into straight-line blocks and return the costliest ones."""
    if not code_hex or code_hex.startswith("error"):
        return []
    blocks, cur = [], []
    for ins in disassemble(code_hex):
        if ins[1] == "JUMPDEST" and cur:
            blocks.append(cur)
            cur = []
        cur.append(ins)
        if ins[1] in ("JUMP", "JUMPI", "STOP", "RETURN", "REVERT", "INVALID", "SELFDESTRUCT"):
            blocks.append(cur)
            cur = []
    if cur:
        blocks.append(cur)
    res = []
    for b in blocks:
        gas = sum(pcs.get(str(pc), [0, 0])[1] for pc, _, _ in b)
        if gas == 0:
            continue
        count = max(pcs.get(str(pc), [0, 0])[0] for pc, _, _ in b)
        res.append({"start": b[0][0], "gas": gas, "count": count, "ins": [
            (pc, name, imm, pcs.get(str(pc), [0, 0])[0], (pcs.get(str(pc), [0, 0, 0]) + [0])[2] > 0)
            for pc, name, imm in b
        ]})
    res.sort(key=lambda x: -x["gas"])
    return res[:top]


def add_into(dst: dict, src: dict) -> None:
    for k, v in src.items():
        if isinstance(v, dict):
            add_into(dst.setdefault(k, {}), v)
        elif isinstance(v, list):
            e = dst.setdefault(k, [0] * len(v))
            for i, x in enumerate(v):
                e[i] += x
        else:
            dst[k] = dst.get(k, 0) + v


def fmt(n: float) -> str:
    return f"{n:,.0f}"


def pct(a: float, b: float) -> str:
    return f"{100 * a / b:.1f}%" if b else "-"


def delta(a: float, b: float) -> str:
    if not b:
        return "-"
    d = a - b
    return f"{'+' if d >= 0 else ''}{fmt(d)} ({'+' if d >= 0 else ''}{100 * d / b:.1f}%)"


def report(args: argparse.Namespace) -> int:
    runs: dict[tuple[str, str], dict[str, Any]] = {}
    for path in args.inputs:
        data = json.loads(Path(path).read_text())
        runs[(data["objective"], data["compiler"])] = data
    md: list[str] = []
    summary: dict[str, Any] = {}
    w = md.append
    w("# Execution gas by opcode kind\n")
    w(
        "Every unit of execution gas in every hot-profile runtime benchmark call, attributed by an "
        "opcode step tracer (`tracer.js` through anvil `debug_traceTransaction`). Execution gas is "
        "receipt gas minus intrinsic gas plus applied refund; it is what codegen controls. Calls "
        "where the EIP-7623 calldata floor sets receipt gas still report their real execution gas. "
        "Only calls that succeeded under all three compilers of an objective, and that the runtime "
        "bench does not exclude from comparisons, enter the comparisons.\n"
    )
    for objective in ("gas", "size"):
        comps = [c for c in CONFIGS[objective] if (objective, c) in runs]
        if not comps:
            continue
        label = "-Ogas / default runs" if objective == "gas" else "-Osize / optimizer runs 1"
        w(f"\n## {objective}: {label}\n")
        cases = sorted(set().union(*(runs[(objective, c)]["cases"].keys() for c in comps)))
        agg = {c: {} for c in comps}
        per_case: dict[str, dict[str, dict]] = {}
        coverage = []
        invariant_bad = 0
        floors = 0
        for case in cases:
            datas = {c: runs[(objective, c)]["cases"].get(case) for c in comps}
            calls_by = {}
            for c, d in datas.items():
                calls_by[c] = {x["label"]: x for x in (d or {}).get("calls", []) if "exec" in x}
                for x in calls_by[c].values():
                    intr = x["gasLimit"] - x["firstGas"]
                    tot = x["exec"] + intr
                    comp = tot - min(x["refund"], tot // 5)
                    if x["receipt_gas"] < comp or sum(x["kinds"].values()) != x["exec"]:
                        invariant_bad += 1
                    elif x["receipt_gas"] > comp:
                        floors += 1
            labels = None
            for c in comps:
                ls = {k for k, v in calls_by[c].items() if not v.get("excluded")}
                labels = ls if labels is None else labels & ls
            labels = labels or set()
            status = {c: (datas[c] or {}).get("status", "missing") for c in comps}
            total_calls = max(len((datas[c] or {}).get("calls", [])) for c in comps)
            coverage.append((case, status, len(labels), total_calls, {c: (datas[c] or {}).get("error", "") for c in comps}))
            per_case[case] = {}
            for c in comps:
                acc: dict[str, Any] = {}
                for lab in labels:
                    x = calls_by[c][lab]
                    add_into(acc, {"exec": x["exec"], "steps": x["steps"], "kinds": x["kinds"], "ko": x.get("ko", {}), "ops": x["ops"], "dyn": x["dyn"], "red": x["red"], "calls": 1})
                per_case[case][c] = acc
                add_into(agg[c], acc)
        summary[objective] = {"aggregate": agg, "per_case": per_case}

        w("### Coverage\n")
        w(f"Compared calls: {sum(x[2] for x in coverage)} across {sum(1 for x in coverage if x[2])} cases. "
          f"Attribution check (sum of step gas + intrinsic - refund = receipt gas, or receipt is the calldata floor): "
          f"{invariant_bad} mismatches; {floors} calls billed at the calldata floor.\n")
        bad = [x for x in coverage if x[2] < x[3] or any(s != "ok" for s in x[1].values())]
        if bad:
            w("| case | compared/total calls | " + " | ".join(comps) + " |")
            w("| --- | ---: | " + " | ".join("---" for _ in comps) + " |")
            for case, status, n, total, errs in bad:
                cells = [status[c] + (f": {errs[c][:80]}".replace("|", "/").replace("\n", " ") if errs[c] else "") for c in comps]
                w(f"| {case} | {n}/{total} | " + " | ".join(cells) + " |")
            w("")

        totals = {c: agg[c].get("exec", 0) for c in comps}
        w("### Totals and kinds (sum over compared calls)\n")
        w("| kind | " + " | ".join(f"{c} gas | share" for c in comps) + " | solar vs legacy | solar vs via-IR |")
        w("| --- | " + " | ".join("---: | ---:" for _ in comps) + " | ---: | ---: |")
        for kind in KIND_ORDER:
            vals = {c: agg[c].get("kinds", {}).get(kind, 0) for c in comps}
            if not any(vals.values()):
                continue
            cells = " | ".join(f"{fmt(vals[c])} | {pct(vals[c], totals[c])}" for c in comps)
            w(f"| {kind} | {cells} | {delta(vals['solar'], vals.get('solc-legacy', 0))} | {delta(vals['solar'], vals.get('solc-viair', 0))} |")
        cells = " | ".join(f"**{fmt(totals[c])}** | 100%" for c in comps)
        w(f"| **total** | {cells} | {delta(totals['solar'], totals.get('solc-legacy', 0))} | {delta(totals['solar'], totals.get('solc-viair', 0))} |")
        steps = {c: agg[c].get("steps", 0) for c in comps}
        w("| steps | " + " | ".join(f"{fmt(steps[c])} | " for c in comps) + f" | {delta(steps['solar'], steps.get('solc-legacy', 0))} | {delta(steps['solar'], steps.get('solc-viair', 0))} |")
        w("")
        over = {c: sum(agg[c].get("kinds", {}).get(k, 0) for k in OVERHEAD_KINDS) for c in comps}
        w("Overhead kinds (" + ", ".join(OVERHEAD_KINDS) + "): " + "; ".join(
            f"{c} {fmt(over[c])} ({pct(over[c], totals[c])})" for c in comps) + ".\n")

        w("### Dynamic gas components\n")
        dyn_keys = sorted(set().union(*(agg[c].get("dyn", {}).keys() for c in comps)))
        w("| component | " + " | ".join(comps) + " | top opcodes (solar) |")
        w("| --- | " + " | ".join("---:" for _ in comps) + " | --- |")
        for k in dyn_keys:
            vals = {c: sum(agg[c].get("dyn", {}).get(k, {}).values()) for c in comps}
            top = sorted(agg["solar"].get("dyn", {}).get(k, {}).items(), key=lambda x: -x[1])[:4]
            w(f"| {k} | " + " | ".join(fmt(vals[c]) for c in comps) + " | " + ", ".join(f"{o} {fmt(g)}" for o, g in top) + " |")
        static = {c: sum(v[2] for v in agg[c].get("ops", {}).values()) for c in comps}
        w("| **static (all opcodes)** | " + " | ".join(fmt(static[c]) for c in comps) + " | |")
        w("")

        w("### Memory and storage redundancy (dynamic detection)\n")
        w("Counts and the static gas of the redundant instruction only (MSTORE/MLOAD 3, SLOAD 100); "
          "the instructions that compute addresses and values are not included. `same_block` means no "
          "JUMPDEST, jump, or call between the two accesses. Dead stores: a 32-byte MSTORE fully "
          "overwritten before any read; unread at exit: never read before the frame ended.\n")
        red_keys = sorted(set().union(*(agg[c].get("red", {}).keys() for c in comps)))
        w("| metric | " + " | ".join(f"{c} count | gas" for c in comps) + " |")
        w("| --- | " + " | ".join("---: | ---:" for _ in comps) + " |")
        for k in red_keys:
            w(f"| {k} | " + " | ".join(f"{fmt(agg[c].get('red', {}).get(k, [0, 0])[0])} | {fmt(agg[c].get('red', {}).get(k, [0, 0])[1])}" for c in comps) + " |")
        w("")

        w("### Kinds by opcode (top contributors)\n")
        w("| kind | " + " | ".join(comps) + " |")
        w("| --- | " + " | ".join("---" for _ in comps) + " |")
        for kind in KIND_ORDER:
            cells = []
            for c in comps:
                items = [(k.split("|")[1], v) for k, v in agg[c].get("ko", {}).items() if k.split("|")[0] == kind and v]
                items.sort(key=lambda x: -x[1])
                cells.append(", ".join(f"{o} {fmt(v)}" for o, v in items[:5]))
            if any(cells):
                w(f"| {kind} | " + " | ".join(cells) + " |")
        w("")

        w("### Opcodes (top 40 by largest gas in any compiler)\n")
        ops = set().union(*(agg[c].get("ops", {}).keys() for c in comps))
        rows = sorted(ops, key=lambda o: -max(agg[c].get("ops", {}).get(o, [0, 0, 0])[1] for c in comps))[:40]
        w("| opcode | " + " | ".join(f"{c} count | gas | static" for c in comps) + " |")
        w("| --- | " + " | ".join("---: | ---: | ---:" for _ in comps) + " |")
        for o in rows:
            w(f"| {o} | " + " | ".join(
                " | ".join(fmt(x) for x in agg[c].get("ops", {}).get(o, [0, 0, 0])) for c in comps) + " |")
        w("")

        w("### Per case: execution gas and the kinds that differ most\n")
        w("Solar minus the cheaper solc pipeline (per kind, positive = Solar spends more).\n")
        w("| case | calls | solar | legacy | via-IR | solar vs best | biggest Solar excess kinds | biggest Solar savings kinds |")
        w("| --- | ---: | ---: | ---: | ---: | ---: | --- | --- |")
        for case in cases:
            pc_ = per_case[case]
            if not pc_["solar"].get("calls"):
                continue
            ex = {c: pc_[c].get("exec", 0) for c in comps}
            refs = [c for c in comps if c != "solar"]
            best = min(refs, key=lambda c: ex[c])
            kd = {k: pc_["solar"].get("kinds", {}).get(k, 0) - pc_[best].get("kinds", {}).get(k, 0) for k in KIND_ORDER}
            plus = sorted(((k, v) for k, v in kd.items() if v > 0), key=lambda x: -x[1])[:3]
            minus = sorted(((k, v) for k, v in kd.items() if v < 0), key=lambda x: x[1])[:3]
            w(f"| {case} | {pc_['solar'].get('calls', 0)} | {fmt(ex['solar'])} | {fmt(ex.get('solc-legacy', 0))} | {fmt(ex.get('solc-viair', 0))} | "
              f"{delta(ex['solar'], ex[best])} vs {best.removeprefix('solc-')} | "
              + ", ".join(f"{k} +{fmt(v)}" for k, v in plus) + " | " + ", ".join(f"{k} {fmt(v)}" for k, v in minus) + " |")
        w("")

        w("### Per case and kind: Solar minus via-IR (gas)\n")
        if "solc-viair" in comps:
            kinds_present = [k for k in KIND_ORDER if any(per_case[c_]["solar"].get("kinds", {}).get(k, 0) or per_case[c_]["solc-viair"].get("kinds", {}).get(k, 0) for c_ in cases)]
            w("| case | " + " | ".join(kinds_present) + " |")
            w("| --- | " + " | ".join("---:" for _ in kinds_present) + " |")
            for case in cases:
                pc_ = per_case[case]
                if not pc_["solar"].get("calls"):
                    continue
                w(f"| {case} | " + " | ".join(
                    fmt(pc_["solar"].get("kinds", {}).get(k, 0) - pc_["solc-viair"].get("kinds", {}).get(k, 0)) for k in kinds_present) + " |")
            w("")

        w("### Solar's costliest blocks across all compared calls\n")
        w("Straight-line blocks (split at JUMPDEST and after jumps and halts) of the benchmarked "
          "contracts, by gas over the case's compared calls. Overhead = stack, PUSH, JUMP/JUMPI/JUMPDEST "
          "gas inside the block. Full listings are in the hot-blocks file.\n")
        w("| case | block | entered | gas | overhead | share of case | shape |")
        w("| --- | ---: | ---: | ---: | ---: | ---: | --- |")
        ranked = []
        for case in cases:
            d = runs[(objective, "solar")]["cases"].get(case)
            if not d or not per_case[case]["solar"].get("calls"):
                continue
            for addr, pcs in d.get("pcs", {}).items():
                for b in hot_blocks(d.get("codes", {}).get(addr, ""), pcs, 10):
                    ov = sum(pcs.get(str(pc), [0, 0])[1] for pc, name, _, _, _ in b["ins"]
                             if name.startswith(("DUP", "SWAP", "PUSH", "JUMP")) or name == "POP")
                    total = sum(x["exec"] for x in d.get("calls", []) if "exec" in x and not x.get("excluded"))
                    ranked.append((b["gas"], case, b, ov, total))
        ranked.sort(key=lambda x: -x[0])
        for gas, case, b, ov, total in ranked[:25]:
            names = [n for _, n, _, _, _ in b["ins"]]
            shape = " ".join(names[:14]) + (" ..." if len(names) > 14 else "")
            w(f"| {case} | @{b['start']} | {fmt(b['count'])} | {fmt(gas)} | {fmt(ov)} ({pct(ov, gas)}) | {pct(gas, total)} | `{shape}` |")
        w("")

        if args.hotspots:
            hs: list[str] = [f"# Hot blocks, {objective}\n"]
            for case in cases:
                hs.append(f"\n## {case}\n")
                for c in comps:
                    d = runs[(objective, c)]["cases"].get(case)
                    if not d:
                        continue
                    blocks = []
                    for addr, pcs in d.get("pcs", {}).items():
                        for b in hot_blocks(d.get("codes", {}).get(addr, ""), pcs, 8):
                            blocks.append((addr, b))
                    blocks.sort(key=lambda x: -x[1]["gas"])
                    total = sum(x["exec"] for x in d.get("calls", []) if "exec" in x)
                    hs.append(f"\n### {c} (exec {fmt(total)})\n")
                    hs.append("Columns: pc, executions, `*` when the step's gas was reattributed to guard, fmp, or recipe.\n")
                    for addr, b in blocks[:6]:
                        hs.append(f"- `{addr[:10]}` block @{b['start']}: {fmt(b['gas'])} gas ({pct(b['gas'], total)}), entered {fmt(b['count'])}x")
                        hs.append("  ```")
                        for pc, name, imm, n, re in b["ins"]:
                            hs.append(f"  {pc:6d} {n:8d} {'*' if re else ' '} {name} {imm}".rstrip())
                        hs.append("  ```")
            Path(args.hotspots.replace("{objective}", objective)).write_text("\n".join(hs) + "\n")

    Path(args.output).write_text("\n".join(md) + "\n")
    if args.json_output:
        Path(args.json_output).write_text(json.dumps(summary))
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("collect")
    c.add_argument("--repo", required=True)
    c.add_argument("--compiler", choices=["solar", "solc-legacy", "solc-viair"], required=True)
    c.add_argument("--solar")
    c.add_argument("--solc")
    c.add_argument("--objective", choices=["gas", "size"], required=True)
    c.add_argument("--port", type=int, default=8601)
    c.add_argument("--tests", nargs="*")
    c.add_argument("--output", required=True)
    r = sub.add_parser("report")
    r.add_argument("inputs", nargs="+")
    r.add_argument("--output", required=True)
    r.add_argument("--json-output")
    r.add_argument("--hotspots", help="Markdown path for hot blocks; `{objective}` expands")
    args = parser.parse_args()
    if args.cmd == "collect":
        return collect(args)
    return report(args)


if __name__ == "__main__":
    sys.exit(main())
