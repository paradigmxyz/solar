#!/usr/bin/env python3
"""specmut for solverus: mutation testing of Solidity specifications.

A spec is weak when a mutant of the implementation that behaves differently still verifies.
Pipeline (after mutspec / MutDafny):
  1. coverage: mutate only functions that solverus verifies in the baseline and that some spec
     reaches (the function's own requires/ensures/asserts, its contract's invariants, or a
     caller's), via a textual call graph.
  2. mutants: relational, arithmetic, logical, constant and boolean replacement, and deletion of
     effect statements and revert guards; each operator is a knob.
  3. re-verify the mutated function and every function that can reach it; a mutant is KILLED only
     when solverus reports a failure with a counterexample (not a timeout).
  4. survivors: trivial compiler equivalence (optimised runtime bytecode identical => equivalent);
     the rest are reported for triage, each with its diff.
Triage records live in <spec>.equiv.json: {mutant_key: reason}, keys stable under spec edits.
"""

import argparse
import hashlib
import json
import os
import random
import re
import shutil
import subprocess
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import Any


# ---------------------------------------------------------------- lexing
def code_mask(src):
    """True at positions that are code (not comments, strings)."""
    n = len(src)
    m = [True] * n
    i = 0
    while i < n:
        c = src[i]
        if src.startswith("//", i):
            j = src.find("\n", i)
            j = n if j < 0 else j
            for k in range(i, j):
                m[k] = False
            i = j
        elif src.startswith("/*", i):
            j = src.find("*/", i + 2)
            j = n if j < 0 else j + 2
            for k in range(i, j):
                m[k] = False
            i = j
        elif c in "\"'":
            j = i + 1
            while j < n and src[j] != c:
                j += 2 if src[j] == "\\" else 1
            for k in range(i, min(j + 1, n)):
                m[k] = False
            i = j + 1
        else:
            i += 1
    return m


def match_brace(src, mask, i):
    d = 0
    for j in range(i, len(src)):
        if not mask[j]:
            continue
        if src[j] == "{":
            d += 1
        elif src[j] == "}":
            d -= 1
            if d == 0:
                return j
    return len(src) - 1


TOK = re.compile(
    r"0x[0-9a-fA-F_]+|\d[\d_]*(?:e\d+)?|[A-Za-z_$][\w$]*|<<=|>>=|\+\+|--|\+=|-=|\*=|/=|%=|&&|\|\||==|!=|<=|>=|=>|<<|>>|\*\*|[^\s]",
    re.DOTALL,
)


def tokens(src, mask, a, b):
    out = []
    for mt in TOK.finditer(src, a, b):
        if mask[mt.start()] and mask[mt.end() - 1]:
            out.append((mt.start(), mt.end(), mt.group()))
    return out


# ---------------------------------------------------------------- structure
def structure(src):
    mask = code_mask(src)
    masked = "".join(ch if mask[i] else " " for i, ch in enumerate(src))
    contracts: list[dict[str, Any]] = []
    for mt in re.finditer(
        r"\b(abstract\s+contract|contract|library|interface)\s+(\w+)[^{;]*\{", masked
    ):
        lb = mt.end() - 1
        rb = match_brace(src, mask, lb)
        doc = src[max(0, src.rfind("\n\n", 0, mt.start())) : mt.start()]
        contracts.append(
            {
                "kind": mt.group(1).split()[-1],
                "name": mt.group(2),
                "a": lb,
                "b": rb,
                "doc": doc,
            }
        )
    funcs: list[dict[str, Any]] = []
    for c in contracts:
        for mt in re.finditer(
            r"\b(function\s+(\w+)|constructor|modifier\s+(\w+)|receive|fallback)\s*\(",
            masked[c["a"] : c["b"]],
        ):
            s = c["a"] + mt.start()
            # skip nested (a function type in a parameter list etc.): must be at depth 1 of the contract
            depth = masked[c["a"] : s].count("{") - masked[c["a"] : s].count("}")
            if depth != 1:
                continue
            name = mt.group(2) or mt.group(3) or mt.group(1).split()[0]
            kind = "modifier" if mt.group(3) else "function"
            # header ends at the first top-level '{' or ';'
            j = c["a"] + mt.end()
            par = 1
            while j < c["b"] and par:
                if mask[j]:
                    par += masked[j] == "("
                    par -= masked[j] == ")"
                j += 1
            k = j
            while k < c["b"] and masked[k] not in "{;":
                k += 1
            if masked[k] != "{":
                continue
            e = match_brace(src, mask, k)
            # NatSpec block: comment lines directly above
            lines = src[:s].split("\n")
            docl = []
            for ln in reversed(lines[:-1]):
                if (
                    ln.strip().startswith("///")
                    or ln.strip().startswith("*")
                    or ln.strip().startswith("/**")
                ):
                    docl.append(ln)
                else:
                    break
            header = masked[s:k]
            mods = (
                re.findall(r"\b(\w+)\s*(?:\(|(?=\s|$))", header[header.find(")") + 1 :])
                if kind == "function"
                else []
            )
            funcs.append(
                {
                    "contract": c["name"],
                    "ckind": c["kind"],
                    "name": name,
                    "kind": kind,
                    "s": s,
                    "a": k,
                    "b": e,
                    "doc": "\n".join(reversed(docl)),
                    "mods": mods,
                    "cinv": "@custom:invariant" in c["doc"]
                    or "@custom:lock-stable" in c["doc"],
                }
            )
    return mask, masked, contracts, funcs


def has_spec(f, src):
    body = src[f["a"] : f["b"]]
    return (
        bool(re.search(r"@custom:(ensures|requires)", f["doc"]))
        or "assert(" in body
        or "@custom:assert" in body
        or (f["cinv"] and f["kind"] == "function")
    )


# ---------------------------------------------------------------- operators
ROR = {"<": ["<="], "<=": ["<"], ">": [">="], ">=": [">"], "==": ["!="], "!=": ["=="]}
AOR = {
    "+": ["-"],
    "-": ["+"],
    "*": ["/"],
    "/": ["*"],
    "%": ["*"],
    "+=": ["-="],
    "-=": ["+="],
    "++": ["--"],
    "--": ["++"],
}
LCR = {"&&": ["||"], "||": ["&&"]}
OPERAND_END = re.compile(r"[\w$)\]]$")


def mutants_of(src, mask, masked, f, ops):
    toks = tokens(src, mask, f["a"], f["b"] + 1)
    # skip inline assembly blocks
    skip = []
    for i, (s, e, t) in enumerate(toks):
        if t == "assembly":
            lb = masked.find("{", e)
            skip.append((s, match_brace(src, mask, lb)))
    toks = [t for t in toks if not any(a <= t[0] <= b for a, b in skip)]
    out = []

    def add(op, s, e, rep, orig):
        out.append({"op": op, "s": s, "e": e, "rep": rep, "orig": orig})

    for i, (s, e, t) in enumerate(toks):
        prev = toks[i - 1][2] if i else ""
        if "ROR" in ops and t in ROR:
            for r in ROR[t]:
                add("ROR", s, e, r, t)
        if "AOR" in ops and t in AOR:
            if t in ("+", "-", "*", "/", "%") and not OPERAND_END.search(prev):
                continue
            for r in AOR[t]:
                add("AOR", s, e, r, t)
        if "LCR" in ops and t in LCR:
            for r in LCR[t]:
                add("LCR", s, e, r, t)
        if "BOOL" in ops and t in ("true", "false"):
            add("BOOL", s, e, "false" if t == "true" else "true", t)
        if (
            "CRP" in ops
            and re.fullmatch(r"\d+", t)
            and prev not in ("[",)
            and not re.fullmatch(r"(u?int|bytes)\d*", prev)
        ):
            n = int(t)
            if n < 10**6:
                add("CRP", s, e, str(n + 1), t)
                if n > 0:
                    add("CRP", s, e, str(n - 1), t)
    if "SDL" in ops or "GDL" in ops:
        # statements: segments starting after '{', ';' or '}' in the body and ending at ';' at the same depth
        body_toks = toks
        pd, pdep = 0, []
        for _, _, t in body_toks:
            pdep.append(pd)
            pd += (t == "(") - (t == ")")
        pdep.append(pd)
        starts = [0] + [
            i + 1
            for i, (_, _, t) in enumerate(body_toks)
            if t in ("{", ";", "}") and pdep[i] == 0
        ]
        for st in starts:
            if st >= len(body_toks):
                continue
            t0 = body_toks[st][2]
            # find end ';' at same paren/brace depth
            d = 0
            end = None
            for j in range(st, len(body_toks)):
                tj = body_toks[j][2]
                if tj in "({[":
                    d += 1
                elif tj in ")}]":
                    d -= 1
                if d < 0:
                    break
                if tj == ";" and d == 0:
                    end = j
                    break
                if tj == "{" and t0 != "if":
                    break
            if end is None:
                continue
            seg = [x[2] for x in body_toks[st : end + 1]]
            s, e = body_toks[st][0], body_toks[end][1]
            if "GDL" in ops and t0 == "if" and "revert" in seg and "else" not in seg:
                add("GDL", s, e, "", src[s:e])
                continue
            if "SDL" not in ops:
                continue
            if t0 in (
                "return",
                "revert",
                "emit",
                "if",
                "for",
                "while",
                "unchecked",
                "assembly",
                "else",
                "try",
                "do",
                "_",
            ):
                continue
            # declarations: Type [loc] name ...
            if (
                len(seg) > 2
                and re.fullmatch(r"[A-Za-z_][\w.]*", seg[0])
                and re.fullmatch(r"[A-Za-z_]\w*", seg[1])
                and seg[1] not in ("memory", "storage", "calldata")
                and seg[0] not in ("delete",)
            ):
                continue
            if len(seg) > 3 and seg[1] in ("memory", "storage", "calldata"):
                continue
            if seg[0] in ("(",):
                continue  # tuple destructuring
            if seg[0] in (
                "mapping",
                "uint",
                "int",
                "address",
                "bool",
                "bytes",
                "string",
                "bytes32",
                "uint256",
            ):
                continue
            add("SDL", s, e, "", src[s:e])
    # stable key: function, operator, original, replacement, occurrence index among same (orig,rep)
    seen = {}
    for m in out:
        k = (m["op"], m["orig"], m["rep"])
        seen[k] = seen.get(k, 0) + 1
        m["key"] = (
            f"{f['contract']}.{f['name']}|{m['op']}|{m['orig'][:40]}->{m['rep']}|{seen[k]}"
        )
    return out


# ---------------------------------------------------------------- running
def run_sv(binp, file, only, timeout, wall):
    try:
        p = subprocess.run(
            [
                binp,
                "--json",
                "--jobs",
                "1",
                "--timeout",
                str(timeout),
                "--only",
                only,
                file,
            ],
            check=False,
            capture_output=True,
            text=True,
            timeout=wall,
        )
        o = re.sub(r"\x1b\[[0-9;]*m", "", p.stdout + p.stderr)
    except subprocess.TimeoutExpired:
        return "timeout", []
    mm = re.search(r"^JSON (.*)$", o, re.MULTILINE)
    if not mm:
        return "frontend-error", [l for l in o.splitlines() if "error" in l][:3]
    rows = json.loads(mm.group(1))
    if not rows:
        return "none", []
    st = [r["status"] for r in rows]
    for s in ("failed", "unsupported", "unknown"):
        if s in st:
            r = next(r for r in rows if r["status"] == s)
            return s, r["msgs"][:4]
    return "verified", []


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("file")
    ap.add_argument("--bin", default=os.environ.get("SV", "solverus"))
    ap.add_argument(
        "--baseline",
        help="solverus --json rows of the unmodified file (list or {rows:[..]})",
    )
    ap.add_argument("--ops", default="ROR,AOR,LCR,BOOL,CRP,SDL,GDL")
    ap.add_argument(
        "--per-fn",
        type=int,
        default=12,
        help="max mutants per function (stratified by operator)",
    )
    ap.add_argument("--only", default=None, help="regex on Contract.fn to mutate")
    ap.add_argument("--timeout", type=int, default=10000)
    ap.add_argument("--wall", type=int, default=600)
    ap.add_argument("-j", type=int, default=3)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--max-affected", type=int, default=8)
    ap.add_argument("--out", default=None)
    ap.add_argument(
        "--solc",
        default=os.environ.get("SOLC"),
        help="solc for TCE (default: $SOLC, else solc on PATH)",
    )
    ap.add_argument("--list", action="store_true")
    ap.add_argument(
        "--no-tce", action="store_true", help="skip trivial compiler equivalence"
    )
    ap.add_argument(
        "--tce-only",
        action="store_true",
        help="recompute TCE for recorded survivors (mutants regenerated from the current file and key); verify nothing",
    )
    ap.add_argument(
        "--rerun",
        action="store_true",
        help="re-run mutants recorded as survived or undecided (after strengthening specs)",
    )
    ap.add_argument(
        "--include-vendor",
        action="store_true",
        help="also mutate vendored code (lib/, node_modules)",
    )
    a = ap.parse_args()
    src = Path(a.file).read_text()
    mask, masked, _contracts, funcs = structure(src)
    out = a.out or a.file + ".specmut.jsonl"
    equiv_path = a.file + ".equiv.json"
    equiv = (
        json.loads(Path(equiv_path).read_text()) if os.path.exists(equiv_path) else {}
    )
    tmpd = os.path.join(os.path.dirname(os.path.abspath(a.file)), ".specmut")
    os.makedirs(tmpd, exist_ok=True)
    if a.tce_only:
        allr = {}
        if os.path.exists(out):
            for ln in Path(out).read_text().splitlines():
                r = json.loads(ln)
                allr[r["key"]] = r
        st = tce_pass(a, src, funcs, allr, equiv, tmpd)
        write_records(out, allr)
        print("TCE", json.dumps(st), flush=True)
        return
    # baseline
    if a.baseline:
        b = json.loads(Path(a.baseline).read_text())
        rows = b["rows"] if isinstance(b, dict) else b
    else:
        p = subprocess.run(
            [a.bin, "--json", "--jobs", str(a.j), "--timeout", str(a.timeout), a.file],
            check=False,
            capture_output=True,
            text=True,
        )
        mm = re.search(
            r"^JSON (.*)$", re.sub(r"\x1b\[[0-9;]*m", "", p.stdout), re.MULTILINE
        )
        rows = json.loads(mm.group(1)) if mm else []
    base = {r["fn"]: r["status"] for r in rows}
    verified = {k for k, v in base.items() if v == "verified"}
    byname = {}
    for f in funcs:
        byname.setdefault(f["name"], []).append(f)
    # textual call graph: who calls (or uses as modifier) name
    callers = {}
    for f in funcs:
        body = masked[f["a"] : f["b"]]
        for nm in set(re.findall(r"\b(\w+)\s*\(", body)) | set(f["mods"]):
            for g in byname.get(nm, []):
                callers.setdefault(id(g), set()).add(id(f))
    fid = {id(f): f for f in funcs}

    def affected(f):
        seen, stack = {id(f)}, [id(f)]
        while stack:
            for c in callers.get(stack.pop(), ()):
                if c not in seen:
                    seen.add(c)
                    stack.append(c)
        fs = [fid[i] for i in seen]
        names = [f"{g['contract']}.{g['name']}" for g in fs if g["kind"] == "function"]
        names = [n for n in dict.fromkeys(names) if n in verified]
        spec = [
            n
            for n in names
            if has_spec(next(g for g in fs if f"{g['contract']}.{g['name']}" == n), src)
        ]
        own = f"{f['contract']}.{f['name']}"
        order = ([own] if own in spec else []) + [n for n in spec if n != own]
        return order[: a.max_affected], bool(spec)

    # vendored contracts (the flattener's '// lib/...sol' path comments) are dependencies, not ours
    vendor, cur = set(), ""
    for line in src.split("\n"):
        mm = re.match(r"^// (\S+\.sol)\s*$", line)
        if mm:
            cur = mm.group(1)
        mc = re.match(
            r"^\s*(?:abstract\s+)?(?:contract|library|interface)\s+(\w+)", line
        )
        if mc and ("lib/" in cur or "node_modules" in cur):
            vendor.add(mc.group(1))
    rng = random.Random(a.seed)
    plan: list[dict[str, Any]] = []
    for f in funcs:
        if f["ckind"] == "interface":
            continue
        if f["contract"] in vendor and not a.include_vendor:
            continue
        fname = f"{f['contract']}.{f['name']}"
        if a.only and not re.search(a.only, fname):
            continue
        aff, cov = affected(f)
        ms = mutants_of(src, mask, masked, f, set(a.ops.split(",")))
        if not ms:
            continue
        if not cov:
            plan.append({"fn": fname, "nocov": len(ms)})
            continue
        byop = {}
        for m in ms:
            byop.setdefault(m["op"], []).append(m)
        for v in byop.values():
            rng.shuffle(v)
        pick = []
        while len(pick) < a.per_fn and any(byop.values()):
            for op in list(byop):
                if byop[op] and len(pick) < a.per_fn:
                    pick.append(byop[op].pop())
        for m in pick:
            m.update(fn=fname, affected=aff)
            plan.append(m)
    if a.list:
        for m in plan:
            print(json.dumps(m))
        return
    prev = {}
    if os.path.exists(out):
        for ln in Path(out).read_text().splitlines():
            r = json.loads(ln)
            prev[r["key"]] = r
    planned = {m["key"] for m in plan if "key" in m}
    # --rerun repeats only this plan's survived/undecided mutants; records outside the plan
    # (other functions under --only, mutants no longer generated) are kept as they are
    done = {
        k: r
        for k, r in prev.items()
        if not (a.rerun and k in planned and r["result"] in ("survived", "undecided"))
    }
    work = [m for m in plan if "key" in m and m["key"] not in done]
    nocov = sum(m.get("nocov", 0) for m in plan)
    print(
        f"{len([m for m in plan if 'key' in m])} mutants in {len({m['fn'] for m in plan if 'key' in m})} covered functions; {len(work)} to run; {nocov} mutation sites in code no spec reaches",
        flush=True,
    )
    if a.rerun and os.path.exists(out):
        keep = [json.dumps(r) for r in done.values()]
        Path(out).write_text("".join(k + "\n" for k in keep))

    def one(m):
        msrc = src[: m["s"]] + m["rep"] + src[m["e"] :]
        h = hashlib.sha1(m["key"].encode()).hexdigest()[:10]
        mf = os.path.join(tmpd, f"{h}-{os.path.basename(a.file)}")
        Path(mf).write_text(msrc)
        res, by, msgs = "survived", None, []
        t0 = time.time()
        for fn in m["affected"]:
            st, ms = run_sv(a.bin, mf, fn, a.timeout, a.wall)
            if st == "failed":
                res, by, msgs = "killed", fn, ms
                break
            if st in ("frontend-error",):
                res, msgs = "stillborn", ms
                break
            if st in ("unknown", "timeout", "unsupported", "none"):
                res, by, msgs = "undecided", fn, ms  # keep looking for a kill
        line = src.count("\n", 0, m["s"]) + 1
        r = {
            "key": m["key"],
            "fn": m["fn"],
            "op": m["op"],
            "line": line,
            "orig": m["orig"][:120],
            "rep": m["rep"],
            "result": res,
            "by": by,
            "msgs": [x[:300] for x in msgs],
            "secs": round(time.time() - t0),
            "file": mf,
        }
        if res != "survived":
            os.remove(mf)
        fh.write(json.dumps(r) + "\n")
        fh.flush()
        print(f"{res:10} {r['secs']:4}s {m['key']}", flush=True)
        return r

    with open(out, "a") as fh, ThreadPoolExecutor(a.j) as ex:
        list(ex.map(one, work))
    # summary + trivial compiler equivalence on survivors
    allr = {}
    for ln in Path(out).read_text().splitlines():
        r = json.loads(ln)
        allr[r["key"]] = r
    keys = {m["key"] for m in plan if "key" in m}
    allr = {k: v for k, v in allr.items() if k in keys}
    tce = {"tce": False}
    if not a.no_tce:
        allrec = {}
        for ln in Path(out).read_text().splitlines():
            r = json.loads(ln)
            allrec[r["key"]] = r
        tce = tce_pass(a, src, funcs, allr, equiv, tmpd)
        allrec.update(allr)
        write_records(out, allrec)
    from collections import Counter

    c = Counter(
        "equivalent"
        if (r["result"] == "survived" and r["key"] in equiv)
        else r["result"]
        for r in allr.values()
    )
    k, s = c["killed"], c["survived"]
    print(
        "SUMMARY",
        json.dumps(dict(c, tce=tce["tce"])),
        f"score={k}/{k + s}" + (f" = {100 * k / (k + s):.0f}%" if k + s else ""),
        flush=True,
    )
    for r in sorted(allr.values(), key=lambda r: (r["fn"], r["line"])):
        if r["result"] == "survived" and r["key"] not in equiv:
            note = {
                "dead": "  (not in any deployed bytecode)",
                "equivalent-in-context": "  (identical deployed bytecode: no caller reaches the difference)",
            }.get(r.get("tce"), "")
            print(
                f"SURVIVED {r['key']}  line {r['line']}: {r['orig']!r} -> {r['rep']!r}"
                + note
            )


# ---------------------------------------------------------------- trivial compiler equivalence
SOLC_FLAG_SETS = [
    ["--optimize", "--evm-version", "cancun"],
    ["--optimize", "--evm-version", "cancun", "--via-ir"],
]


def pick_solc(override=None):
    """The solc for the equivalence check: --solc (default $SOLC), else solc on PATH."""
    return override or shutil.which("solc")


class Tce:
    """Compiles a file and its mutants with one solc and one flag set and hashes every contract's
    creation and runtime bytecode (metadata hash off, so the source text and file name don't enter
    it). Results are cached in .specmut/tce-cache.json by solc, flags and source hash."""

    def __init__(self, solc, tmpd, name):
        import threading

        self.solc, self.tmpd, self.name = solc, tmpd, name
        self.flags: list[str] | None = None
        self.error: str | None = None
        self.lock = threading.Lock()
        self.cache_path = os.path.join(tmpd, "tce-cache.json")
        self.flags_path = os.path.join(tmpd, "tce-flags.json")
        self.cache = (
            json.loads(Path(self.cache_path).read_text())
            if os.path.exists(self.cache_path)
            else {}
        )
        self.dirty = 0

    def _compile(self, path, flags):
        p = subprocess.run(
            [self.solc]
            + flags
            + ["--metadata-hash", "none", "--combined-json", "bin,bin-runtime", path],
            check=False,
            capture_output=True,
            text=True,
        )
        if p.returncode != 0:
            lines = p.stderr.splitlines()
            i = next((i for i, l in enumerate(lines) if re.match(r"\w*Error", l)), None)
            if i is None:
                return None, (lines[:1] or ["solc failed"])[0][:300]
            loc = next(
                (
                    l.strip()[3:].strip()
                    for l in lines[i + 1 : i + 4]
                    if l.strip().startswith("-->")
                ),
                "",
            )
            loc = re.sub(r"^.*/", "", loc)
            return None, (lines[i] + (f" ({loc})" if loc else ""))[:300]
        cs = json.loads(p.stdout)["contracts"]
        blob = "|".join(
            f"{k.split(':')[-1]}={v.get('bin', '')}/{v.get('bin-runtime', '')}"
            for k, v in sorted(cs.items(), key=lambda kv: kv[0].split(":")[-1])
        )
        return hashlib.sha1(blob.encode()).hexdigest(), None

    def compile_src(self, text, path, flags=None):
        flags = flags or self.flags
        assert flags is not None, "compile_src before baseline"
        ck = hashlib.sha1(
            ("\0".join([self.solc] + flags) + "\0" + text).encode()
        ).hexdigest()
        with self.lock:
            if ck in self.cache:
                v = self.cache[ck]
                return (None, v[4:]) if v.startswith("ERR:") else (v, None)
        if not (os.path.exists(path) and Path(path).read_text() == text):
            Path(path).write_text(text)
        h, err = self._compile(path, flags)
        with self.lock:
            self.cache[ck] = h if h else "ERR:" + err
            self.dirty += 1
            if self.dirty >= 10:
                self.save()
        return h, err

    def save(self):
        tmp = self.cache_path + ".tmp"
        Path(tmp).write_text(json.dumps(self.cache))
        os.replace(tmp, self.cache_path)
        self.dirty = 0

    def baseline(self, src):
        """Hash of the unmodified file, trying the cached flags first, then each flag set."""
        known = (
            json.loads(Path(self.flags_path).read_text())
            if os.path.exists(self.flags_path)
            else {}
        )
        rec = known.get(self.name)
        sets = list(SOLC_FLAG_SETS)
        if rec and rec.get("solc") == self.solc and rec["flags"] in sets:
            sets.remove(rec["flags"])
            sets.insert(0, rec["flags"])
        path = os.path.join(self.tmpd, "base-" + self.name)
        first = None
        for fl in sets:
            h, err = self.compile_src(src, path, fl)
            if h:
                self.flags = fl
                known[self.name] = {"solc": self.solc, "flags": fl}
                Path(self.flags_path).write_text(
                    json.dumps(known, indent=1, sort_keys=True)
                )
                return h
            # stack too deep is the one error another flag set can fix
            first = first or err
            if "Stack too deep" not in (err or ""):
                break
        self.error = first
        return None


def tce_pass(a, src, funcs, allr, equiv, tmpd):
    """Trivial compiler equivalence for survivors: a mutant whose bytecode equals the original's
    is equivalent, provided the mutated function is live (a sentinel revert at the top of its body
    changes the bytecode). Mutant sources are regenerated from the current file and the key.
    Returns stats; updates equiv and each record's 'tce' field."""
    stats: dict[str, Any] = {
        "tce": False,
        "before": 0,
        "new": 0,
        "dropped": 0,
        "after": 0,
        "error": None,
    }
    surv = [r for r in allr.values() if r["result"] == "survived"]

    def triaged(r):
        return r["key"] in equiv and not equiv[r["key"]].startswith("TCE")

    stats["before"] = sum(1 for r in surv if r["key"] not in equiv)
    todo = [r for r in surv if not triaged(r)]
    solc = pick_solc(a.solc)
    name = os.path.basename(a.file)
    if not solc:
        stats["error"] = "no solc (pass --solc or set SOLC)"
    else:
        tce = Tce(solc, tmpd, name)
        ref = tce.baseline(src)
        if ref is None:
            stats["error"] = f"{os.path.basename(solc)}: {tce.error}"
        else:
            stats["tce"] = True
            stats["solc"] = os.path.basename(solc) + " " + " ".join(tce.flags or [])
    if not stats["tce"]:
        print(f"TCE unavailable: {stats['error']}", flush=True)
        for r in todo:
            r["tce"] = "unavailable"
    else:
        mask, masked = code_mask(src), None
        masked = "".join(ch if mask[i] else " " for i, ch in enumerate(src))
        allops = {"ROR", "AOR", "LCR", "BOOL", "CRP", "SDL", "GDL"}
        gen = {}
        for f in funcs:
            if f["ckind"] == "interface":
                continue
            for m in mutants_of(src, mask, masked, f, allops):
                m["f"] = f
                gen.setdefault(m["key"], []).append(m)

        def resolve(r):
            ms = gen.get(r["key"], [])
            if not ms:
                return None
            # overloads share a key: take the one nearest the recorded line
            return min(
                ms, key=lambda m: abs(src.count("\n", 0, m["s"]) + 1 - r["line"])
            )

        jobs, live_jobs = [], {}
        for r in todo:
            m = resolve(r)
            if m is None:
                r["tce"] = "stale"
                continue
            jobs.append((r, m))
            f = m["f"]
            live_jobs.setdefault(id(f), f)

        def sentinel(f):
            # an opaque guard (calldata length is unknown to the optimiser, and readable in pure
            # functions and constructors) keeps the rest of the body reachable, so immutables stay
            # assigned; a bare revert is the fallback
            sf = os.path.join(
                tmpd, f"sentinel-{f['contract']}.{f['name']}-{f['a']}-{name}"
            )
            alive = False
            for sent in (
                " if (msg.data.length == 0x5e471e15) revert(); ",
                " revert(); ",
            ):
                h, _ = tce.compile_src(src[: f["a"] + 1] + sent + src[f["a"] + 1 :], sf)
                if h is not None:
                    alive = h != ref
                    break
            try:
                os.remove(sf)
            except OSError:
                pass
            return id(f), alive

        def mutant(job):
            r, m = job
            msrc = src[: m["s"]] + m["rep"] + src[m["e"] :]
            mf = os.path.join(
                tmpd, f"{hashlib.sha1(r['key'].encode()).hexdigest()[:10]}-{name}"
            )
            h, err = tce.compile_src(msrc, mf)
            r["file"] = mf
            return r, h, err

        nj = max(1, min(a.j, 2))
        with ThreadPoolExecutor(nj) as ex:
            live = dict(ex.map(sentinel, live_jobs.values()))
            res = list(ex.map(mutant, jobs))
        tce.save()
        for (r, h, err), (_, m) in zip(res, jobs):
            r.pop("dead", None)
            if not live[id(m["f"])]:
                r["tce"] = "dead"
                r["dead"] = True
            elif h is None:
                r["tce"] = "compile-error"
            elif h == ref:
                # the deployed bytecode can't tell them apart, but an internal function's callers
                # may pin its arguments (constant folding after inlining), while its spec speaks
                # for every input: equivalent in this deployment only, so it stays a survivor
                f = m["f"]
                internal = f["kind"] == "modifier" or re.search(
                    r"\b(internal|private)\b", src[f["s"] : f["a"]]
                )
                r["tce"] = "equivalent-in-context" if internal else "equivalent"
                if internal:
                    stats["context"] = stats.get("context", 0) + 1
            else:
                r["tce"] = "different"
            was = r["key"] in equiv
            if r["tce"] == "equivalent":
                if not was:
                    stats["new"] += 1
                equiv[r["key"]] = (
                    "TCE: identical optimised bytecode (" + stats["solc"] + ")"
                )
            elif was:  # an earlier TCE verdict that no longer holds
                del equiv[r["key"]]
                stats["dropped"] += 1
        Path(a.file + ".equiv.json").write_text(
            json.dumps(equiv, indent=1, sort_keys=True)
        )
    stats["after"] = sum(1 for r in surv if r["key"] not in equiv)
    return stats


def write_records(out, allr_all):
    tmp = out + ".tmp"
    with open(tmp, "w") as fh:
        fh.writelines(json.dumps(r) + "\n" for r in allr_all.values())
    os.replace(tmp, out)


if __name__ == "__main__":
    main()
