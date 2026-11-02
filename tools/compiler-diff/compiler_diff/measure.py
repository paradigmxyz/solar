"""Run a command and report its exit code and peak RSS as JSON on a file descriptor.

Usage: measure.py FD COMMAND [ARG...]

A child forked from the large corpus process keeps that process's RSS peak across
`exec`, so the runner cannot measure small compilers itself. This small process
spawns the command instead; `wait4` then gives the largest RSS of the command and
the descendants it waited for.
"""

import json
import os
import sys


def main(fd, argv):
    try:
        pid = os.posix_spawnp(argv[0], argv, os.environ)
    except OSError as error:
        report = {"error": str(error)}
    else:
        _, status, usage = os.wait4(pid, 0)
        # Linux reports kilobytes, macOS bytes.
        scale = 1024 if sys.platform == "darwin" else 1
        report = {
            "returncode": os.waitstatus_to_exitcode(status),
            "max_rss_kb": usage.ru_maxrss // scale,
        }
    os.write(fd, json.dumps(report).encode())


if __name__ == "__main__":
    main(int(sys.argv[1]), sys.argv[2:])
