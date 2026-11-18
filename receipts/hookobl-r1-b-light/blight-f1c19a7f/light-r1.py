"""Light acceptance harness (not part of the branch): runs generic.py's main from the hook checkout,
refusing only the final lake build of an Obl module (its peak RSS, 6.76 GB, is above the 6000 MB
light ceiling), and logging every command the hook runs with its exit status.

  python3 -I light.py CMDLOG -- <generic.py arguments>
"""

import json
import sys

sys.path.insert(0, "/home/hand/h2/r1/tools/certify")
import generic

log_path = sys.argv[1]
assert sys.argv[2] == "--"
real = generic.run


def run(cmd, *a, **kw):
    if cmd[:2] == ["lake", "build"] and any(c.endswith(".Obl") for c in cmd):
        rc, out, err = 97, "", "light acceptance: the Obl build is not run (peak RSS 6.76 GB > 6000 MB)"
    else:
        rc, out, err = real(cmd, *a, **kw)
    if cmd[0] in ("lake",) or str(cmd[0]).endswith("solar-verify") or "loop_equiv.py" in " ".join(map(str, cmd)):
        with open(log_path, "a") as f:
            f.write(json.dumps({"cmd": [str(c) for c in cmd], "rc": rc, "stderr_tail": err.strip()[-400:]}) + "\n")
    return rc, out, err


generic.run = run
sys.argv = ["generic.py", *sys.argv[3:]]
sys.exit(generic.main())
