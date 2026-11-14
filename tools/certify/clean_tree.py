#!/usr/bin/env python3
"""Is a git checkout exactly its HEAD commit? Used by scripts/certify for its provenance.

  clean_tree.py DIR [--allow REGEX] [--paths FILE]

Prints one line per difference between DIR's working tree and the tree of HEAD and exits 1 when
there is any. It reads git with replacement objects off (GIT_NO_REPLACE_OBJECTS=1) and refuses
replace refs and grafts outright, and it never trusts the index: every file HEAD's tree records is
hashed from the working tree (git hash-object --no-filters) and compared with the blob id in the
tree, so skip-worktree and assume-unchanged entries, clean and smudge filters and a stale index
cannot hide a change, and the flags themselves are reported. Untracked files count too, ignored
ones included (outside the .lake/ build directories, which the driver treats on its own). Paths matching --allow (a Python regex over the path relative to
DIR) are the outputs the driver writes and are not reported. With --paths FILE (one path relative
to DIR per line) only those paths are checked, and with --prefix P (repeatable) only paths under P, each of which must be a committed, unchanged file.
"""
import os, re, subprocess, sys, traceback


# Exit 0 clean, 1 with the differences listed, 2 on any error (an uncaught exception included),
# so that the driver never reads a failure as a clean tree.
def fail(msg):
    print(msg, file=sys.stderr)
    sys.exit(2)


def _crash(kind, value, tb):
    traceback.print_exception(kind, value, tb)
    sys.stderr.flush()
    os._exit(2)


sys.excepthook = _crash

args = sys.argv[1:]
if not args:
    fail("usage: clean_tree.py DIR [--allow REGEX] [--paths FILE]")
top, allow, paths, prefixes = args[0], None, None, []
i = 1
while i < len(args):
    if args[i] == "--allow":
        allow = re.compile(args[i + 1]); i += 2
    elif args[i] == "--prefix":
        prefixes.append(args[i + 1]); i += 2
    elif args[i] == "--paths":
        paths = [p for p in open(args[i + 1]).read().splitlines() if p]; i += 2
    else:
        fail("clean_tree.py: unknown argument " + args[i])
env = dict(os.environ, GIT_NO_REPLACE_OBJECTS="1", GIT_OPTIONAL_LOCKS="0")
for k in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES",
          "GIT_REPLACE_REF_BASE", "GIT_GRAFT_FILE", "GIT_CONFIG_PARAMETERS", "GIT_CONFIG_COUNT"):
    env.pop(k, None)

def git(*a, inp=None):
    return subprocess.run(["git", "-C", top, *a], env=env, input=inp, capture_output=True, check=True).stdout

bad = []
top = os.path.realpath(top)
if os.path.realpath(git("rev-parse", "--show-toplevel").decode().strip()) != top:
    fail("clean_tree.py: %s is not the top of a git checkout" % top)
if git("for-each-ref", "refs/replace/").strip():
    bad.append("replace refs: " + " ".join(l.split()[-1] for l in git("for-each-ref", "refs/replace/").decode().splitlines()))
common = git("rev-parse", "--git-common-dir").decode().strip()
common = common if os.path.isabs(common) else os.path.join(top, common)
for g in (os.path.join(common, "info", "grafts"),):
    if os.path.exists(g):
        bad.append("grafts file " + g)

def ok(p):
    return allow is not None and allow.search(p)

# HEAD's tree: path -> (mode, blob).
tree = {}
for rec in git("ls-tree", "-r", "-z", "--full-tree", "HEAD").split(b"\0"):
    if not rec:
        continue
    meta, path = rec.split(b"\t", 1)
    mode, kind, oid = meta.decode().split()
    tree[path.decode()] = (mode, oid)
def inside(p):
    return not prefixes or any(p == q or p.startswith(q.rstrip("/") + "/") for q in prefixes)

want = paths if paths is not None else sorted(p for p in tree if inside(p))
# Index flags: assume-unchanged (lowercase tag) and skip-worktree (S).
for rec in git("ls-files", "-v", "-z").split(b"\0"):
    if rec and (rec[:1].islower() or rec[:1] == b"S"):
        p = rec[2:].decode()
        if (p in paths if paths is not None else inside(p)) and not ok(p):
            bad.append("index flag %s: %s" % (rec[:1].decode(), p))
files = []
for p in want:
    if p not in tree:
        bad.append("not committed: " + p); continue
    if ok(p) and paths is None:
        continue
    mode, oid = tree[p]
    full = os.path.join(top, p)
    if mode == "160000":
        try:
            sub = subprocess.run(["git", "-C", full, "rev-parse", "HEAD"], env=env, capture_output=True, check=True).stdout.decode().strip()
        except subprocess.CalledProcessError:
            sub = ""
        if sub != oid:
            bad.append("submodule not at its commit: " + p)
        continue
    if mode == "120000":
        if not os.path.islink(full) or git("hash-object", "--stdin", inp=os.readlink(full).encode()).decode().strip() != oid:
            bad.append("changed: " + p)
        continue
    if os.path.islink(full) or not os.path.isfile(full):
        bad.append("missing or not a regular file: " + p); continue
    if (os.stat(full).st_mode & 0o111 != 0) != (mode == "100755"):
        bad.append("mode changed: " + p)
    files.append(p)
if files:
    got = git("hash-object", "--no-filters", "--stdin-paths", inp="\n".join(os.path.join(top, p) for p in files).encode()).decode().split()
    for p, h in zip(files, got):
        if h != tree[p][1]:
            bad.append("changed: " + p)
if paths is None:
    for rec in git("ls-files", "--others", "-z").split(b"\0"):
        p = rec.decode()
        if p and inside(p) and "/.lake/" not in "/" + p and "/__pycache__/" not in "/" + p and not ok(p):
            bad.append("untracked or ignored: " + p)
for b in bad:
    print(b)
sys.exit(1 if bad else 0)
