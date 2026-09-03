#!/usr/bin/env python3
"""Sweep directories of Solidity files with the stateful differential (statediff.py).

usage: sdcampaign.py DIR... --out results.jsonl [--root DIR] [--no-skip] [--extra "..."] [--jobs N]
                     [--timeout S] [--only REGEX] [--seed N] [--calls N] [--seqs N] [--limit N]

For every .sol file, every deployable contract is compiled with both compilers and run through
statediff.py with random call sequences. Constructor arguments come from a solc semantic-test
expectation line `// constructor(): a, b ->` (applied to the last contract in the file, as
solc's test framework does); contracts whose constructor needs arguments that no such line
provides are skipped. Files that require libraries (`// library:`), an EVM version outside the
selected one (`// EVMVersion:`), legacy codegen, or EOF are skipped. The default skip list drops
files that observe their own address or code, create contracts, or move value, since those
differ for reasons reviewed as non-bugs; `--no-skip` keeps them.

Every result is appended as one JSON line to --out with the statediff status, the mismatch
list, gas totals, and a `self` flag (true when the file matches the non-bug skip patterns), so
`self=False` mismatches are the candidates to triage.
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
from concurrent.futures import ThreadPoolExecutor, as_completed

ROOT = pathlib.Path(__file__).resolve().parents[2]
STATEDIFF = pathlib.Path(__file__).resolve().parent / "statediff.py"
EVM_VERSIONS = [
    "homestead", "tangerineWhistle", "spuriousDragon", "byzantium", "constantinople",
    "petersburg", "istanbul", "berlin", "london", "paris", "shanghai", "cancun", "prague", "osaka",
]
SELF_RE = re.compile("|".join([
    r"address\(this\)", r"\bthis\b", r"\bnew\s+[A-Z]", r"\bcreate2?\(", r"selfdestruct", r"codesize",
    r"extcode", r"\.code\b", r"codecopy", r"\.creationCode", r"\.runtimeCode", r"msg\.value",
    r"payable", r"\.transfer\(", r"\.send\(", r"\.call\{", r"\.call\(", r"\.delegatecall", r"\.staticcall",
    r"blobhash", r"blockhash", r"balance\b", r"gasleft\(", r"\bgas\(\)", r"mload\(0x40\)", r"mload\(64\)",
    r"block\.", r"tx\.", r"chainid", r"msg\.sender", r"\bthis\)", r"coinbase", r"timestamp", r"difficulty",
    r"prevrandao", r"basefee", r"blobbasefee", r"origin",
]))
HARD_SKIP_RE = re.compile("|".join([
    r"// compileViaYul: false", r"// bytecodeFormat: *>=EOFv1", r"// revertStrings", r"// library:",
    r"==== Source", r"^import\b|\nimport\b", r"// compileToEwasm", r"// allowNonExistingFunctions",
]))
LITERAL_RE = re.compile(r"^(-?\d+|0x[0-9a-fA-F]+|true|false|\"[^\"]*\"|hex\"[0-9a-fA-F]*\"|left\(0x[0-9a-fA-F]+\)|right\(0x[0-9a-fA-F]+\))$")


def evm_ok(text: str, evm: str) -> bool:
    for m in re.finditer(r"// EVMVersion: *([<>=]+) *(\w+)", text):
        op, ver = m.group(1), m.group(2)
        if ver not in EVM_VERSIONS:
            return False
        a, b = EVM_VERSIONS.index(evm), EVM_VERSIONS.index(ver)
        ok = {"<": a < b, "<=": a <= b, ">": a > b, ">=": a >= b, "=": a == b, "==": a == b}.get(op, True)
        if not ok:
            return False
    return True


def ctor_args(text: str) -> str | None:
    """Return the constructor arguments from the expectation section, '' for none, None if unusable."""
    m = re.search(r"^// constructor\(([^)]*)\)(?:, *([^:]+))?: *(.*?) *->", text, re.M)
    if not m:
        return ""
    if m.group(2):
        return None  # value transfer on deployment
    args = m.group(3).strip()
    if not args:
        return ""
    parts = [p.strip() for p in args.split(",")]
    out = []
    for p in parts:
        if not LITERAL_RE.match(p):
            return None
        if p.startswith("left("):
            out.append("0x" + p[7:-1].ljust(64, "0"))
        elif p.startswith("right("):
            out.append(p[6:-1])
        elif p.startswith("hex"):
            out.append(p)
        else:
            out.append(p)
    return " ".join(out)


def solc_contracts(path: pathlib.Path, evm: str, root: pathlib.Path | None) -> list[tuple[str, bool]]:
    """Return (contract name, has constructor args) for every deployable contract, in source order."""
    cmd = ["solc", "--combined-json", "abi,bin", "--evm-version", evm, "--via-ir", "--optimize", str(path)]
    if root:
        cmd += ["--base-path", str(root)]
    r = subprocess.run(cmd, capture_output=True, text=True, errors="replace", timeout=120, cwd=path.parent)
    if r.returncode != 0:
        return []
    try:
        out = json.loads(r.stdout)
    except json.JSONDecodeError:
        return []
    result = []
    for key, art in out.get("contracts", {}).items():
        name = key.rsplit(":", 1)[1]
        if not art.get("bin"):
            continue
        abi = art["abi"]
        if isinstance(abi, str):
            abi = json.loads(abi)
        ctor = next((e for e in abi if e.get("type") == "constructor"), None)
        funcs = [e for e in abi if e.get("type") == "function"]
        if not funcs:
            continue
        result.append((name, bool(ctor and ctor.get("inputs"))))
    return result


def run_one(path: pathlib.Path, contract: str, ctor: str, args, extra: list[str]) -> dict:
    cmd = [sys.executable, str(STATEDIFF), str(path), contract, "--seed", str(args.seed), "--calls", str(args.calls),
           "--seqs", str(args.seqs)] + extra
    if ctor:
        cmd += ["--ctor", ctor]
    if args.root:
        cmd += ["--project-root", str(args.root)]
    t0 = time.time()
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=args.timeout, cwd=ROOT)
        try:
            res = json.loads(r.stdout)
        except json.JSONDecodeError:
            res = {"status": "error", "reason": (r.stderr or r.stdout)[-1500:]}
    except subprocess.TimeoutExpired:
        res = {"status": "error", "reason": "campaign timeout"}
    res["elapsed"] = round(time.time() - t0, 1)
    return res


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("dirs", nargs="+")
    ap.add_argument("--out", required=True)
    ap.add_argument("--root", type=pathlib.Path)
    ap.add_argument("--no-skip", action="store_true")
    ap.add_argument("--extra", default="")
    ap.add_argument("--jobs", type=int, default=6)
    ap.add_argument("--timeout", type=float, default=600)
    ap.add_argument("--only", default=None)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--calls", type=int, default=25)
    ap.add_argument("--seqs", type=int, default=2)
    ap.add_argument("--limit", type=int, default=10 ** 9)
    args = ap.parse_args()
    extra = args.extra.split()
    evm = extra[extra.index("--evm-version") + 1] if "--evm-version" in extra else "osaka"
    out_path = pathlib.Path(args.out)
    done = set()
    if out_path.exists():
        for line in out_path.read_text().splitlines():
            try:
                rec = json.loads(line)
                done.add((rec["file"], rec["contract"]))
            except Exception:  # noqa: BLE001
                pass
    files = []
    for d in args.dirs:
        p = ROOT / d if not pathlib.Path(d).is_absolute() else pathlib.Path(d)
        files.extend(sorted(x for x in p.rglob("*.sol") if not {"out", "lib", "node_modules", "auxiliary"} & set(x.parts)))
    if args.only:
        files = [f for f in files if re.search(args.only, str(f))]
    tasks = []
    skipped = {"hard": 0, "evm": 0, "self": 0, "ctor": 0, "nocontract": 0}
    for path in files:
        text = path.read_text(errors="replace")
        if HARD_SKIP_RE.search(text):
            skipped["hard"] += 1
            continue
        if not evm_ok(text, evm):
            skipped["evm"] += 1
            continue
        is_self = bool(SELF_RE.search(text))
        if is_self and not args.no_skip:
            skipped["self"] += 1
            continue
        contracts = solc_contracts(path, evm, args.root)
        if not contracts:
            skipped["nocontract"] += 1
            continue
        ctor = ctor_args(text)
        for i, (name, needs_args) in enumerate(contracts):
            last = i == len(contracts) - 1
            if needs_args and not (last and ctor):
                skipped["ctor"] += 1
                continue
            rel = str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path)
            if (rel, name) in done:
                continue
            tasks.append((path, rel, name, ctor if (last and needs_args) else "", is_self))
        if len(tasks) >= args.limit:
            break
    print(f"{len(tasks)} tasks over {len(files)} files, skipped {skipped}", flush=True)
    stats: dict[str, int] = {}
    with ThreadPoolExecutor(args.jobs) as ex, out_path.open("a") as out:
        futs = {ex.submit(run_one, t[0], t[2], t[3], args, extra): t for t in tasks}
        for fut in as_completed(futs):
            path, rel, name, ctor, is_self = futs[fut]
            try:
                res = fut.result()
            except Exception as err:  # noqa: BLE001
                res = {"status": "error", "reason": repr(err)}
            rec = {"file": rel, "contract": name, "ctor": ctor, "self": is_self, "evm": evm, "extra": args.extra,
                   "status": res.get("status"), "reason": (res.get("reason") or "")[:800],
                   "mismatches": res.get("mismatches"), "gas": res.get("gas"), "calls": res.get("calls"),
                   "deploy": res.get("deploy"), "project": res.get("project"), "elapsed": res.get("elapsed")}
            out.write(json.dumps(rec) + "\n")
            out.flush()
            stats[rec["status"]] = stats.get(rec["status"], 0) + 1
            if rec["status"] == "mismatch":
                m = (rec["mismatches"] or [{}])[0]
                print(f"!!! MISMATCH self={is_self} {rel} {name} {m.get('sig')} {m.get('field')} -> {rec['project']}", flush=True)
            elif rec["status"] == "error":
                print(f"[error] {rel} {name}: {rec['reason'][:200]}", flush=True)
            else:
                print(f"[agree] {rel} {name} ({rec['calls']} calls, gas ratio {(rec['gas'] or {}).get('ratio')})", flush=True)
    print("stats", stats, flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
