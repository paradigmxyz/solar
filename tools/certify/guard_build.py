#!/usr/bin/env python3
"""Snapshot and check of the package build directory around a sandboxed step.

    guard_build.py allow ROOT OUT TARGET... [-- MINUS... [--deny-statements]]
        write to OUT the package modules the step may rebuild: those reachable from the TARGETs by
        the header imports of the package's sources under ROOT, less those reachable from MINUS,
        and with --deny-statements less every module any Certified/*/Statement.lean reaches and
        every statement and final module but the TARGETs
    guard_build.py packages ROOT TARGET...
        print, one per line and sorted, the modules outside the package (its lake packages', not
        the toolchain's Init, Std, Lean or Lake) that the package modules reachable from the
        TARGETs import: what the driver builds before any sandboxed step, since the sandbox may
        write only the package's own build directory
    guard_build.py snap LIBDIR OUT
        record the SHA-256 of every file under LIBDIR in OUT
    guard_build.py check LIBDIR SNAP ALLOW
        fail (exit 1) unless every file under LIBDIR that is new, changed or gone since SNAP belongs
        to a module listed in ALLOW

The allowed modules are computed by the driver before the step, never read from its output. On
failure the changed files are listed and removed with their module's .trace and .hash files, so
that the next build compiles those modules again from their sources instead of trusting what an
untrusted module wrote.
"""
import hashlib
import os
import re
import sys

SRCDIRS = (".", "Certified", "Certified/w1", "tv")
_SRCDIRS = {}


def srcdirs(root):
    """The package's source directories: the defaults above and every srcDir its lakefile.lean
    names (such as vendor/evm-rules), so that no module of the package is missed."""
    if root not in _SRCDIRS:
        dirs = list(SRCDIRS)
        lf = os.path.join(root, "lakefile.lean")
        if os.path.isfile(lf):
            for d in re.findall(r'srcDir\s*:=\s*"([^"]+)"', open(lf).read()):
                if d not in dirs:
                    dirs.append(d)
        _SRCDIRS[root] = dirs
    return _SRCDIRS[root]
IMPORT = re.compile(r"^\s*(?:(?:public|meta|private)\s+)*import\s+(.*)$")


def header_imports(path):
    """The modules a Lean file imports: its header only (import lines before the first other
    command, after comments), as Lean reads it, so a comment or string elsewhere names nothing."""
    text = open(path, errors="replace").read()
    i, n, out = 0, len(text), []
    while i < n:
        if text[i].isspace():
            i += 1
        elif text.startswith("--", i):
            j = text.find("\n", i)
            i = n if j < 0 else j + 1
        elif text.startswith("/-", i):
            depth, i = 1, i + 2
            while i < n and depth:
                if text.startswith("/-", i):
                    depth, i = depth + 1, i + 2
                elif text.startswith("-/", i):
                    depth, i = depth - 1, i + 2
                else:
                    i += 1
        else:
            # "module" and "prelude" may open the header; each import command names the modules
            # on its own line ("import A B"), so a following command is never read as a module.
            m = re.match(r"(?:module|prelude)(?![\w.'?!])", text[i:])
            if m:
                i += m.end()
                continue
            m = re.match(r"(?:(?:public|meta|private)[ \t]+)*import[ \t]+(?:all[ \t]+)?([^\n]*)", text[i:])
            if not m:
                break
            line = m.group(1).split("--")[0]
            out += [x for x in line.split() if re.fullmatch(r"[A-Za-z_][\w.'?!]*", x)]
            i += m.end()
    return out


def closure(root, target):
    seen, todo = set(), [target]
    while todo:
        m = todo.pop()
        if m in seen:
            continue
        path = next((p for p in (os.path.join(root, d, *m.split(".")) + ".lean" for d in srcdirs(root))
                     if os.path.isfile(p)), None)
        if path is None:
            continue
        seen.add(m)
        todo.extend(header_imports(path))
    return seen


def statement_closures(root):
    """Every module any published statement of the package imports, and every statement and
    final module: what no proof build may change."""
    out = set()
    cert = os.path.join(root, "Certified")
    for name in sorted(os.listdir(cert)):
        if os.path.isfile(os.path.join(cert, name, "Statement.lean")):
            out |= closure(root, f"{name}.Statement")
            out |= {f"{name}.Statement", f"{name}.Final"}
    return out


def scan(lib):
    out = {}
    for root, _, files in os.walk(lib):
        for f in files:
            p = os.path.join(root, f)
            if os.path.islink(p):
                out[p] = "link:" + os.readlink(p)
                continue
            h = hashlib.sha256()
            with open(p, "rb") as fh:
                for chunk in iter(lambda: fh.read(1 << 20), b""):
                    h.update(chunk)
            out[p] = h.hexdigest()
    return out


def module_of(lib, path):
    rel = os.path.relpath(path, lib)
    d, f = os.path.split(rel)
    return os.path.join(d, f.split(".", 1)[0]).replace(os.sep, ".")


cmd = sys.argv[1]
if cmd == "allow":
    root, out, *rest = sys.argv[2:]
    targets, minus = (rest[: rest.index("--")], rest[rest.index("--") + 1:]) if "--" in rest else (rest, [])
    deny = "--deny-statements" in minus
    minus = [m for m in minus if m != "--deny-statements"]
    allowed = set()
    for t in targets:
        # "=M" allows exactly the module M.
        allowed |= {t[1:]} if t.startswith("=") else closure(root, t)
    for m in minus:
        allowed -= closure(root, m)
    if deny:
        allowed -= statement_closures(root) - set(targets)
    with open(out, "w") as fh:
        fh.write("".join(m + "\n" for m in sorted(allowed)))
elif cmd == "order":
    # The modules reachable from TARGET less those reachable from the MINUS modules, each after the
    # modules it imports, so that building them one at a time builds only the module named.
    root, out, target, *rest = sys.argv[2:]
    minus = set()
    for m in rest:
        if m != "--":
            minus |= closure(root, m)
    want = closure(root, target) - minus
    order, done = [], set()

    def visit(m, stack=()):
        if m in done or m not in want:
            return
        if m in stack:
            sys.exit(f"import cycle at {m}")
        path = next(p for p in (os.path.join(root, d, *m.split(".")) + ".lean" for d in srcdirs(root))
                    if os.path.isfile(p))
        for d in header_imports(path):
            visit(d, stack + (m,))
        done.add(m)
        order.append(m)

    visit(target)
    with open(out, "w") as fh:
        fh.write("".join(m + "\n" for m in order))
elif cmd == "packages":
    root, *targets = sys.argv[2:]
    if not targets:
        sys.exit("guard_build.py packages: no targets")
    inside, outside = set(), set()
    for t in targets:
        reach = closure(root, t)
        if not reach:
            sys.exit(f"guard_build.py packages: {t} is not a module of the package under {root}")
        inside |= reach
    for m in inside:
        path = next(p for p in (os.path.join(root, d, *m.split(".")) + ".lean" for d in srcdirs(root))
                    if os.path.isfile(p))
        outside |= {d for d in header_imports(path) if d not in inside}
    toolchain = ("Init", "Std", "Lean", "Lake")
    print("\n".join(sorted(m for m in outside if m.split(".", 1)[0] not in toolchain and m not in toolchain)))
elif cmd == "snap":
    lib, out = sys.argv[2:]
    with open(out, "w") as fh:
        for p, h in sorted(scan(lib).items()):
            fh.write(f"{h} {p}\n")
elif cmd == "check":
    lib, snap, allow = sys.argv[2:]
    before = {}
    for line in open(snap):
        h, p = line.rstrip("\n").split(" ", 1)
        before[p] = h
    after = scan(lib)
    allowed = set(open(allow).read().split())
    bad = [p for p in sorted(set(before) | set(after))
           if before.get(p) != after.get(p)
           and (module_of(lib, p) not in allowed or str(after.get(p, "")).startswith("link:"))]
    if bad:
        for p in bad[:20]:
            print(f"guard_build: changed outside the modules this step may build: {p}", file=sys.stderr)
        for p in bad:
            stem = os.path.join(os.path.dirname(p), os.path.basename(p).split(".", 1)[0])
            for ext in (".olean", ".ilean", ".trace", ".c", ".olean.hash", ".ilean.hash", ".c.hash"):
                try:
                    os.remove(stem + ext)
                except OSError:
                    pass
        sys.exit(1)
else:
    sys.exit(f"guard_build.py: unknown command {cmd}")
