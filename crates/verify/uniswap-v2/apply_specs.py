#!/usr/bin/env python3
"""Apply a spec file to a copy of an unmodified Solidity source tree.

    apply_specs.py --src SRC_DIR --out OUT_DIR [--spec FILE] [--search DIR]...
        [--define NAME=BODY]... [--rename FROM=TO]...

The copy differs from the source only by NatSpec comments: tags added in front
of the contracts, functions and statements the spec file names. Every target
must match exactly one definition (or one line, for a statement); anything else
is an error, so a spec is never silently dropped.

Spec file format: a header line names the target, and the lines after it (up to
the next header) are tags written without the leading "///".

    == contract <Contract>
    == function <Contract>.<name>[(<parameter types>)]
    == statement <Contract>.<name>[(<parameter types>)] /<regex>/

A statement target is the one line of the function's body that the Python
regex matches; its tags (such as "@custom:ghost x = e") go right before it.
Lines starting with "#" are comments, and "\" at the end of a line joins it with
the next one. "@define NAME body" and "@define NAME(x, y) body" declare
shorthands, expanded textually (repeatedly) before the tags are written; "\n" in
a body starts a new tag.
"""

import argparse
import pathlib
import re
import shutil
import sys


def die(msg):
    sys.exit(f"apply_specs: {msg}")


HEADER = re.compile(
    r"==\s+(contract|function|statement)\s+(\w+)(?:\.(\w+)(\([^)]*\))?)?(?:\s+/(.*)/)?\s*$"
)


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
            m = re.match(r"@define\s+(\w+)(\(\s*\w+(?:\s*,\s*\w+)*\s*\))?\s+(.*)$", s)
            if not m:
                die(f"{path}:{n}: bad @define")
            params = re.findall(r"\w+", m.group(2)) if m.group(2) else None
            macros[m.group(1)] = (params, m.group(3).replace("\\n", "\n"))
            continue
        if s.startswith("== "):
            m = HEADER.match(s)
            kind = m and m.group(1)
            ok = m and (
                (kind == "contract" and not m.group(3) and not m.group(5))
                or (kind == "function" and m.group(3) and not m.group(5))
                or (kind == "statement" and m.group(3) and m.group(5))
            )
            if not ok:
                die(f"{path}:{n}: bad header {s!r}")
            cur = {
                "kind": kind,
                "contract": m.group(2),
                "fn": m.group(3),
                "params": m.group(4),
                "regex": m.group(5),
                "tags": [],
                "line": n,
            }
            blocks.append(cur)
            continue
        if cur is None:
            die(f"{path}:{n}: tag before any header")
        cur["tags"].append(s)
    return macros, blocks


def split_args(text):
    """Arguments of a call whose text starts after its "(": (args, end index)."""
    args, depth, start = [], 0, 0
    for i, ch in enumerate(text):
        if ch in "([":
            depth += 1
        elif ch in ")]":
            if depth == 0:
                args.append(text[start:i].strip())
                return args, i
            depth -= 1
        elif ch == "," and depth == 0:
            args.append(text[start:i].strip())
            start = i + 1
    die(f"unbalanced macro call in {text!r}")


def expand_once(text, name, params, body):
    if not params:
        return re.sub(rf"\b{name}\b", lambda m: body, text)
    out, pos = "", 0
    for m in re.finditer(rf"\b{name}\(", text):
        if m.start() < pos:
            continue
        args, end = split_args(text[m.end() :])
        if len(args) != len(params):
            die(f"{name} takes {len(params)} arguments, got {len(args)}")
        sub = body
        for p, a in zip(params, args, strict=True):
            sub = re.sub(rf"\b{p}\b", lambda _m, a=a: a, sub)
        out += text[pos : m.start()] + f"({sub})"
        pos = m.end() + end + 1
    return out + text[pos:]


def expand(text, macros):
    for _ in range(32):
        old = text
        for name, (params, body) in macros.items():
            text = expand_once(text, name, params, body)
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
    hits = [
        (f, i)
        for f, ls in files.items()
        for i, line in enumerate(ls)
        if re.match(CONTRACT_RE.format(name), line)
    ]
    if len(hits) != 1:
        die(f"contract {name}: {len(hits)} definitions")
    return hits[0]


def block_end(ls, start):
    """The line closing the brace block that opens at or after line start."""
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


def insert_tags(ls, at, tags, doc_block=True):
    indent = re.match(r"\s*", ls[at]).group(0)
    j = at - 1
    while j >= 0 and not ls[j].strip():
        j -= 1
    if (
        doc_block
        and j >= 0
        and ls[j].strip().endswith("*/")
        and "/**" in "".join(ls[: j + 1])
    ):
        # inside the preceding /** ... */ block, before its closing line
        new = [f"{indent} * {t}" for t in tags]
        if ls[j].strip() == "*/":
            ls[j:j] = new
        else:
            body = ls[j].rstrip()[:-2].rstrip()
            ls[j : j + 1] = [body, *new, f"{indent} */"]
    else:
        ls[at:at] = [f"{indent}/// {t}" for t in tags]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", required=True, type=pathlib.Path)
    ap.add_argument("--out", required=True, type=pathlib.Path)
    ap.add_argument(
        "--spec",
        type=pathlib.Path,
        default=pathlib.Path(__file__).resolve().parent / "uniswap-v2.spec",
    )
    ap.add_argument(
        "--search",
        action="append",
        metavar="DIR",
        help="directory, relative to --src, whose .sol files the targets resolve in "
        "(repeatable; default: all of --src)",
    )
    ap.add_argument(
        "--define",
        action="append",
        default=[],
        metavar="NAME=BODY",
        help="override (or add) a shorthand of the spec file (repeatable)",
    )
    ap.add_argument(
        "--rename",
        action="append",
        default=[],
        metavar="FROM=TO",
        help="resolve the spec file's contract FROM as contract TO (repeatable)",
    )
    a = ap.parse_args()
    macros, blocks = parse(a.spec)
    for d in a.define:
        name, eq, body = d.partition("=")
        m = re.match(r"(\w+)(\(\s*\w+(?:\s*,\s*\w+)*\s*\))?$", name.strip())
        if not eq or not m:
            die(f"--define {d!r}: expected NAME=BODY or NAME(x, ...)=BODY")
        params = re.findall(r"\w+", m.group(2)) if m.group(2) else None
        macros[m.group(1)] = (params, body.strip())
    renames = {}
    for r in a.rename:
        old_name, eq, new_name = r.partition("=")
        if (
            not eq
            or not old_name.strip().isidentifier()
            or not new_name.strip().isidentifier()
        ):
            die(f"--rename {r!r}: expected FROM=TO")
        renames[old_name.strip()] = new_name.strip()
    if a.out.exists():
        shutil.rmtree(a.out)
    shutil.copytree(
        a.src,
        a.out,
        symlinks=True,
        ignore=shutil.ignore_patterns(".git", "node_modules", "out", "cache"),
    )
    files = {
        p: p.read_text().splitlines()
        for d in (a.search or ["."])
        for p in (a.out / d).rglob("*.sol")
    }
    # Resolve every target first, then insert bottom-up so line numbers hold.
    edits = {}
    for b in blocks:
        f, ci = find_contract(files, renames.get(b["contract"], b["contract"]))
        ls = files[f]
        tags = [t for tag in b["tags"] for t in expand(tag, macros).split("\n")]
        bad = [t for t in tags if not re.match(r"@custom:[a-z-]+\s", t)]
        if bad:
            die(f"spec line {b['line']}: not a @custom tag after expansion: {bad[0]!r}")
        name = f"{b['contract']}.{b['fn']}{b['params'] or ''}"
        if b["kind"] == "contract":
            at, doc = ci, True
        else:
            hits = find_function(ls, ci, block_end(ls, ci), b["fn"], b["params"])
            if len(hits) != 1:
                die(f"function {name}: {len(hits)} definitions")
            at, doc = hits[0], True
            if b["kind"] == "statement":
                body = range(at, block_end(ls, at) + 1)
                rx = re.compile(b["regex"])
                lines = [i for i in body if i != at and rx.search(ls[i])]
                if len(lines) != 1:
                    die(
                        f"statement /{b['regex']}/ in {name}: {len(lines)} matching lines"
                    )
                at, doc = lines[0], False
        edits.setdefault(f, []).append((at, tags, doc))
    for f, es in edits.items():
        ls = files[f]
        for at, tags, doc in sorted(es, key=lambda e: -e[0]):
            insert_tags(ls, at, tags, doc)
        f.write_text("\n".join(ls) + "\n")
        print(f"{f.relative_to(a.out)}: {sum(len(e[1]) for e in es)} tags")


if __name__ == "__main__":
    main()
