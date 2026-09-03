#!/usr/bin/env python3
"""Campaign runner: enumerate contracts/functions and run solsymdiff on each."""
from __future__ import annotations

import argparse, json, os, pathlib, random, re, shutil, subprocess, sys, time
from concurrent.futures import ThreadPoolExecutor, as_completed

ROOT = pathlib.Path(__file__).resolve().parents[2]
SOLSYMDIFF = ROOT / "fuzz" / "bin" / "solsymdiff"
OUT = ROOT / "target" / "symaudit"
VARIANT = os.environ.get("SYMAUDIT_VARIANT", "")
EXTRA = os.environ.get("SYMAUDIT_EXTRA", "").split()
RESULTS = OUT / f"results{VARIANT}.jsonl"
DONE = OUT / f"done{VARIANT}.txt"

SKIP_PATTERNS = [
    r"address\(this\)", r"\bthis\.", r"\bnew\s+[A-Z]", r"\bcreate2?\(", r"gasleft\(",
    r"\bgas\(\)", r"selfdestruct", r"codesize", r"extcode", r"\.code\b", r"codecopy",
    r"\bimport\b", r"==== Source", r"compileViaYul: false", r"bytecodeFormat",
    r"EVMVersion: [<=]", r"\.creationCode", r"\.runtimeCode", r"\bthis\b\)", r"\bthis\b,",
    r"msg\.value", r"payable", r"\.transfer\(", r"\.send\(", r"\.call\{", r"\.call\(",
    r"\.delegatecall", r"\.staticcall", r"blobhash", r"blockhash", r"balance\b",
    r"revertStrings",
]
SKIP_RE = re.compile("|".join(SKIP_PATTERNS))
MIN_SKIP_RE = re.compile("|".join([r"\bimport\b", r"==== Source", r"compileViaYul: false", r"bytecodeFormat", r"EVMVersion: [<=]", r"revertStrings"]))
ONLY_SKIPPED = bool(os.environ.get("SYMAUDIT_ONLY_PREVIOUSLY_SKIPPED"))

def candidates(seed: int, solc_dirs: list[str] | None) -> list[pathlib.Path]:
    files = []
    for p in (ROOT / "tests" / "ui" / "codegen").rglob("*.sol"):
        if "auxiliary" in p.parts:
            continue
        files.append(p)
    for extra in os.environ.get("SYMAUDIT_DIRS", "").split(":"):
        if not extra:
            continue
        if os.environ.get("SYMAUDIT_MULTISRC"):
            for rootfile in (ROOT / extra).rglob("ROOT.txt"):
                files.append(rootfile.parent / rootfile.read_text().strip())
            continue
        for p in (ROOT / extra).rglob("*.sol"):
            if "auxiliary" in p.parts or "out" in p.parts or "lib" in p.parts or "node_modules" in p.parts:
                continue
            files.append(p)
    if os.environ.get("SYMAUDIT_NO_DEFAULT"):
        files = [f for f in files if "tests/ui/codegen" not in str(f)]
    sem = ROOT / "testdata" / "solidity" / "test" / "libsolidity" / "semanticTests"
    for p in sem.rglob("*.sol"):
        if os.environ.get("SYMAUDIT_NO_DEFAULT"):
            break
        if solc_dirs and p.parent.name not in solc_dirs and p.parts[-2] not in solc_dirs:
            continue
        files.append(p)
    rng = random.Random(seed)
    rng.shuffle(files)
    return files

def solc_abis(path: pathlib.Path, timeout: float) -> dict[str, list]:
    evm = EXTRA[EXTRA.index("--evm-version") + 1] if "--evm-version" in EXTRA else "osaka"
    cmd = ["solc", "--abi", "--evm-version", evm, "--via-ir", "--optimize", str(path)]
    r = subprocess.run(cmd, capture_output=True, text=True, errors="replace", timeout=timeout, cwd=path.parent)
    if r.returncode != 0:
        return {}
    out = {}
    cur = None
    lines = r.stdout.splitlines()
    for i, line in enumerate(lines):
        m = re.match(r"^=======\s+(.*):(\w+)\s+=======$", line)
        if m:
            cur = m.group(2)
            continue
        if line.startswith("[") and cur is not None:
            try:
                out[cur] = json.loads(line)
            except json.JSONDecodeError:
                pass
            cur = None
    return out

def canonical(item):
    t = item["type"]
    if not t.startswith("tuple"):
        return t
    comps = ",".join(canonical(c) for c in item["components"])
    return f"({comps}){t[5:]}"

def sig(entry):
    return f"{entry['name']}({','.join(canonical(i) for i in entry['inputs'])})"

def run_one(path, contract, signature, mutability, timeout, prefixes=()):
    cmd = [sys.executable, str(SOLSYMDIFF), "--source", str(path), "--contract", contract,
           "--signature", signature, "--include-view", "--include-stateful",
           "--timeout", str(timeout), "--symbolic-timeout", "20", "--max-paths", "512"] + EXTRA
    if os.environ.get("SYMAUDIT_MULTISRC"):
        cmd += ["--project-root", str(path.parent)]
    if mutability != "pure" and os.environ.get("SYMAUDIT_PREFIX"):
        cmd += ["--prefix-calldata", os.environ["SYMAUDIT_PREFIX"]]
    for pfx in prefixes:
        cmd += ["--prefix-calldata", pfx]
    t0 = time.time()
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout + 30, cwd=ROOT)
        try:
            res = json.loads(r.stdout)
        except json.JSONDecodeError:
            res = {"status": "error", "reason": (r.stderr or r.stdout)[-2000:]}
    except subprocess.TimeoutExpired:
        res = {"status": "error", "reason": "campaign timeout"}
    res["elapsed"] = round(time.time() - t0, 1)
    rec = {"file": str(path.relative_to(ROOT)), "contract": contract, "signature": signature,
           "mutability": mutability, "prefixes": list(prefixes), "status": res.get("status"), "reason": res.get("reason"),
           "counterexample": res.get("counterexample"), "project": res.get("project"),
           "elapsed": res["elapsed"]}
    if rec["status"] != "mismatch" and rec.get("project"):
        shutil.rmtree(rec["project"], ignore_errors=True)
    return rec

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--jobs", type=int, default=6)
    ap.add_argument("--timeout", type=float, default=90)
    ap.add_argument("--max-files", type=int, default=100000)
    ap.add_argument("--deadline-minutes", type=float, default=110)
    ap.add_argument("--solc-dirs", nargs="*", default=None)
    ap.add_argument("--only", default=None, help="regex on file path")
    args = ap.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    done = set()
    if DONE.exists():
        done = set(DONE.read_text().split("\n"))
    deadline = time.time() + args.deadline_minutes * 60
    files = candidates(args.seed, args.solc_dirs)
    if args.only:
        files = [f for f in files if re.search(args.only, str(f))]
    tasks = []
    nfiles = 0
    for path in files:
        if nfiles >= args.max_files:
            break
        try:
            text = path.read_text(errors="replace")
        except OSError:
            continue
        if os.environ.get("SYMAUDIT_MULTISRC"):
            pass
        elif ONLY_SKIPPED:
            if MIN_SKIP_RE.search(text) or not SKIP_RE.search(text):
                continue
        elif SKIP_RE.search(text):
            continue
        key = str(path.relative_to(ROOT))
        if key in done:
            continue
        abis = solc_abis(path, 30)
        if not abis:
            continue
        nfiles += 1
        for contract, abi in abis.items():
            if any(e.get("type") == "constructor" and e.get("inputs") for e in abi):
                continue
            for e in abi:
                if e.get("type") != "function":
                    continue
                if e.get("stateMutability") not in ("pure", "view", "nonpayable"):
                    continue
                tasks.append((path, contract, sig(e), e["stateMutability"]))
    if os.environ.get("SYMAUDIT_TASKS"):
        tasks = []
        for line in pathlib.Path(os.environ["SYMAUDIT_TASKS"]).read_text().splitlines():
            parts = line.split("|")
            f, c, sg, m = parts[:4]
            prefixes = parts[4].split(",") if len(parts) > 4 and parts[4] else []
            tasks.append((ROOT / f, c, sg, m, prefixes))
    print(f"{len(tasks)} tasks over {nfiles} files", flush=True)
    mismatches = []
    stats = {}
    with ThreadPoolExecutor(args.jobs) as ex, RESULTS.open("a") as out, DONE.open("a") as donef:
        futs = {}
        it = iter(tasks)
        def submit_next():
            try:
                t = next(it)
            except StopIteration:
                return False
            futs[ex.submit(run_one, *t[:4], args.timeout, *t[4:])] = t
            return True
        for _ in range(args.jobs):
            submit_next()
        while futs:
            if time.time() > deadline:
                print("deadline reached", flush=True)
                for f in futs: f.cancel()
                break
            for fut in as_completed(list(futs)):
                t = futs.pop(fut)
                try:
                    rec = fut.result()
                except Exception as err:  # noqa
                    rec = {"file": str(t[0].relative_to(ROOT)), "contract": t[1], "signature": t[2],
                           "status": "error", "reason": repr(err)}
                out.write(json.dumps(rec) + "\n"); out.flush()
                donef.write(rec["file"] + "\n"); donef.flush()
                stats[rec["status"]] = stats.get(rec["status"], 0) + 1
                tag = rec["status"]
                if tag == "mismatch":
                    mismatches.append(rec)
                    print(f"!!! MISMATCH {rec['file']} {rec['contract']} {rec['signature']} -> {rec['project']}", flush=True)
                elif tag in ("incomplete", "error"):
                    print(f"[{tag}] {rec['file']} {rec['contract']}.{rec['signature']}: {str(rec.get('reason'))[:160]}", flush=True)
                else:
                    print(f"[{tag}] {rec['file']} {rec['contract']}.{rec['signature']} ({rec.get('elapsed')}s)", flush=True)
                submit_next()
                break
    print("stats", stats, flush=True)
    print(f"{len(mismatches)} mismatches", flush=True)

if __name__ == "__main__":
    main()
