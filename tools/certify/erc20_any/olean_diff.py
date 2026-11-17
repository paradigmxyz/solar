#!/usr/bin/env python3
"""Compare two replay plans' module olean sha256s (replay.py plan lines "replay MODULE SHA256 ...").

    olean_diff.py OLD_PLAN NEW_PLAN OUT

Writes OUT with each plan's sorted "module sha256" inventory digest and every module that is in
only one plan or whose olean differs. Data only: it binds nothing to the old plan's receipt.
"""
import hashlib
import sys


def load(p):
    return {l.split()[1]: l.split()[2] for l in open(p) if l.startswith("replay ")}


def digest(m):
    return hashlib.sha256("".join(f"{k} {m[k]}\n" for k in sorted(m)).encode()).hexdigest()


old, new = load(sys.argv[1]), load(sys.argv[2])
both = old.keys() & new.keys()
same = sorted(k for k in both if old[k] == new[k])
diff = sorted(k for k in both if old[k] != new[k])
lines = [f"old {len(old)} modules inventory {digest(old)}", f"new {len(new)} modules inventory {digest(new)}",
         f"identical {len(same)} differing {len(diff)} only_old {len(old.keys() - new.keys())} "
         f"only_new {len(new.keys() - old.keys())}"]
lines += [f"differs {k} {old[k]} {new[k]}" for k in diff]
lines += [f"only_old {k}" for k in sorted(old.keys() - new.keys())]
lines += [f"only_new {k}" for k in sorted(new.keys() - old.keys())]
open(sys.argv[3], "w").write("\n".join(lines) + "\n")
print("\n".join(lines[:3]))
