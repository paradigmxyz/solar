"""Box harness (not part of the branch): runs generic.py's main from the r1 checkout and logs every
command the loop cut hook runs with its exit status. Nothing is refused.

  python3 -I full.py CMDLOG -- <generic.py arguments>
"""

import json
import sys

sys.path.insert(0, "/home/hand/h2/hsrc/tools/certify")
import generic

log_path = sys.argv[1]
assert sys.argv[2] == "--"
real = generic.run


def run(cmd, *a, **kw):
    rc, out, err = real(cmd, *a, **kw)
    s = " ".join(map(str, cmd))
    if cmd[0] == "lake" or str(cmd[0]).endswith("solar-verify") or "loop_equiv.py" in s:
        with open(log_path, "a") as f:
            f.write(json.dumps({"cmd": [str(c) for c in cmd], "rc": rc, "stderr_tail": err.strip()[-600:], "stdout_errors": [l for l in out.splitlines() if "error" in l][:40]}) + "\n")
    return rc, out, err


generic.run = run
sys.argv = ["generic.py", *sys.argv[3:]]
sys.exit(generic.main())
