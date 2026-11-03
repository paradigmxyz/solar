"""Run a command and report its exit code and peak RSS as JSON on a file descriptor.

Usage: measure.py FD COMMAND [ARG...]

A child forked from the large corpus process keeps that process's RSS peak across
`exec`, so the runner cannot measure small compilers itself. This small process
spawns the command instead; `wait4` then gives the largest RSS of the command and
the descendants it waited for. The numbers include this helper's own few
megabytes, about 11 MB on Linux.
"""

import json
import os
import signal
import sys


def main(fd, argv):
    # Keep the pipe out of the command, or reading it would wait for its descendants.
    os.set_inheritable(fd, False)
    try:
        # Python ignores SIGPIPE and SIGXFSZ; the command must not inherit that.
        pid = os.posix_spawnp(
            argv[0],
            argv,
            os.environ,
            setsigdef=[signal.SIGPIPE, signal.SIGXFSZ],
        )
    except OSError as error:
        # Keep the report below the pipe capacity, which the parent drains after the exit.
        report = {"returncode": None, "error": str(error)[:1000]}
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
