#!/usr/bin/env python3
"""Print FILE:LINE:TEXT for every line of the .lean files under the given paths whose code,
outside comments and string literals, matches the banned pattern (argv[1]). Comments are
skipped so that prose naming a construct is not mistaken for its use; string literals are kept
as code, so they can only add hits."""
import pathlib
import re
import sys

banned = re.compile(sys.argv[1])


def code_lines(text):
    """The text with comments blanked, line by line (nested block comments, line comments)."""
    out, depth, i, n, in_str = [], 0, 0, len(text), False
    while i < n:
        c = text[i]
        if depth == 0 and in_str:
            out.append(c)
            if c == "\\" and i + 1 < n:
                out.append(text[i + 1])
                i += 2
                continue
            if c == '"':
                in_str = False
            i += 1
        elif text.startswith("/-", i):
            depth += 1
            i += 2
        elif depth > 0 and text.startswith("-/", i):
            depth -= 1
            i += 2
        elif depth > 0:
            out.append("\n" if c == "\n" else " ")
            i += 1
        elif text.startswith("--", i):
            j = text.find("\n", i)
            i = n if j < 0 else j
        else:
            if c == '"':
                in_str = True
            out.append(c)
            i += 1
    return "".join(out).split("\n")


hit = False
for root in sys.argv[2:]:
    p = pathlib.Path(root)
    if not p.exists():
        print(f"scan_code.py: no such path: {root}", file=sys.stderr)
        sys.exit(2)
    files = [p] if p.is_file() else sorted(p.rglob("*.lean"))
    for f in files:
        if f.suffix != ".lean":
            continue
        for k, line in enumerate(code_lines(f.read_text(errors="replace")), 1):
            if banned.search(line):
                print(f"{f}:{k}:{line.strip()}")
                hit = True
sys.exit(0 if hit else 1)

