#!/usr/bin/env python3
"""Apply seaport.spec to a copy of an unmodified Seaport checkout.

    apply_specs.py --seaport SEAPORT_DIR --out OUT_DIR [--spec seaport.spec] [--search DIR]...

The copy differs from the checkout only by NatSpec tags added in front of the
contracts and functions the spec file names. Every target must match exactly
one definition; anything else is an error, so a spec is never silently dropped.
"""
import argparse
import pathlib
import re
import shutil
import sys

HERE = pathlib.Path(__file__).resolve().parent
SEARCH = ["contracts", "lib/seaport-core/src", "lib/seaport-types/src"]


def die(msg):
    sys.exit(f"apply_specs: {msg}")


def parse(path):
    raw = path.read_text().splitlines()
    lines, buf = [], ""
    for ln in raw:
        if ln.endswith("\\"):
            buf += ln[:-1]
            continue
        lines.append(buf + ln)
        buf = ""
    macros, blocks, cur = {}, [], None
    for n, ln in enumerate(lines, 1):
        s = ln.strip()
        if not s or s.startswith("#"):
            continue
        if s.startswith("@define "):
            m = re.match(r"@define\s+(\w+)(\((\w+)\))?\s+(.*)$", s)
            if not m:
                die(f"{path}:{n}: bad @define")
            macros[m.group(1)] = (m.group(3), m.group(4).replace("\\n", "\n"))
            continue
        if s.startswith("== "):
            m = re.match(r"==\s+(contract|function)\s+(\w+)(?:\.(\w+)(\(.*\))?)?\s*$", s)
            if not m or (m.group(1) == "function") != bool(m.group(3)):
                die(f"{path}:{n}: bad header {s!r}")
            cur = {"kind": m.group(1), "contract": m.group(2), "fn": m.group(3),
                   "params": m.group(4), "tags": [], "line": n}
            blocks.append(cur)
            continue
        if cur is None:
            die(f"{path}:{n}: tag before any header")
        cur["tags"].append(s)
    return macros, blocks


def expand(text, macros):
    for _ in range(32):
        old = text
        for name, (param, body) in macros.items():
            if param:
                text = re.sub(rf"\b{name}\(([^()]*)\)",
                              lambda m: re.sub(rf"\b{param}\b", m.group(1), body), text)
            else:
                text = re.sub(rf"\b{name}\b", lambda m: body, text)
        if text == old:
            return text
    die("macro expansion does not terminate")


def norm_params(p):
    p = re.sub(r"\s+", " ", p.strip()[1:-1])
    out = []
    for part in filter(None, (x.strip() for x in p.split(","))):
        out.append(re.sub(r"\b(memory|calldata|storage)\b", "", part).split()[0])
    return out


CONTRACT_RE = r"^\s*(?:abstract\s+)?(?:contract|library|interface)\s+{}\b"


def find_contract(files, name):
    hits = [(f, i) for f, ls in files.items() for i, l in enumerate(ls)
            if re.match(CONTRACT_RE.format(name), l)]
    if len(hits) != 1:
        die(f"contract {name}: {len(hits)} definitions")
    return hits[0]


def contract_end(ls, start):
    depth, seen = 0, False
    for i in range(start, len(ls)):
        code = ls[i].split("//")[0]
        depth += code.count("{") - code.count("}")
        seen = seen or "{" in code
        if seen and depth == 0:
            return i
    return len(ls) - 1


def find_function(ls, start, end, fn, params):
    hits = []
    for i in range(start, end + 1):
        if re.match(rf"^\s*function\s+{fn}\s*\(", ls[i]):
            if params is not None:
                sig, j = "", i
                while ")" not in sig.split("(", 1)[-1] and j <= end:
                    sig += ls[j] + " "
                    j += 1
                got = sig[sig.index("(") : sig.index(")", sig.index("(")) + 1]
                if norm_params(got) != norm_params(params):
                    continue
            hits.append(i)
    return hits


def insert_tags(ls, at, tags):
    indent = re.match(r"\s*", ls[at]).group(0)
    j = at - 1
    while j >= 0 and not ls[j].strip():
        j -= 1
    if j >= 0 and ls[j].strip().endswith("*/") and "/**" in "".join(ls[: j + 1]):
        # inside the preceding /** ... */ block, before its closing line
        new = [f"{indent} * {t}" for t in tags]
        if ls[j].strip() == "*/":
            ls[j:j] = new
        else:
            body = ls[j].rstrip()[:-2].rstrip()
            ls[j : j + 1] = [body] + new + [f"{indent} */"]
    else:
        ls[at:at] = [f"{indent}/// {t}" for t in tags]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seaport", required=True, type=pathlib.Path)
    ap.add_argument("--out", required=True, type=pathlib.Path)
    ap.add_argument("--spec", type=pathlib.Path, default=HERE / "seaport.spec")
    ap.add_argument("--search", action="append", metavar="DIR",
                    help="directory, relative to --seaport, whose .sol files the targets resolve "
                         "in (repeatable; default: Seaport's own source roots). Use it for a "
                         "rewrite of Seaport that keeps the same contract and storage names.")
    a = ap.parse_args()
    macros, blocks = parse(a.spec)
    if a.out.exists():
        shutil.rmtree(a.out)
    shutil.copytree(a.seaport, a.out, symlinks=True,
                    ignore=shutil.ignore_patterns(".git", "node_modules", "out", "cache"))
    # Targets resolve in the sources Seaport.sol compiles from.
    files = {p: p.read_text().splitlines() for d in (a.search or SEARCH) for p in (a.out / d).rglob("*.sol")}
    # Resolve every target first, then insert bottom-up so line numbers hold.
    edits = {}
    for b in blocks:
        f, ci = find_contract(files, b["contract"])
        tags = [t for tag in b["tags"] for t in expand(tag, macros).split("\n")]
        bad = [t for t in tags if not re.match(r"@custom:[a-z-]+\s", t)]
        if bad:
            die(f"spec line {b['line']}: not a @custom tag after expansion: {bad[0]!r}")
        if b["kind"] == "contract":
            at = ci
        else:
            hits = find_function(files[f], ci, contract_end(files[f], ci), b["fn"], b["params"])
            if len(hits) != 1:
                die(f"function {b['contract']}.{b['fn']}{b['params'] or ''}: {len(hits)} definitions")
            at = hits[0]
        edits.setdefault(f, []).append((at, tags))
    for f, es in edits.items():
        ls = files[f]
        for at, tags in sorted(es, key=lambda e: -e[0]):
            insert_tags(ls, at, tags)
        f.write_text("\n".join(ls) + "\n")
        print(f"{f.relative_to(a.out)}: {sum(len(t) for _, t in es)} tags")


if __name__ == "__main__":
    main()
