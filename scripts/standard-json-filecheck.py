#!/usr/bin/env python3
import os
import shutil
import subprocess
import sys
import tempfile


def main():
    solar = sys.argv[2]
    input_path = sys.argv[3]
    out = tempfile.NamedTemporaryFile(prefix="solar-standard-json.", delete=False)
    out_path = out.name
    out.close()

    try:
        with open(out_path, "wb") as stdout:
            status = subprocess.run(
                [solar, "--standard-json", "--pretty-json", "-Zui-testing", input_path],
                stdout=stdout,
            ).returncode
        with open(out_path, "rb") as stdout:
            output = stdout.read()
        sys.stdout.buffer.write(output)
        sys.stdout.buffer.flush()
        if status != 0:
            return status

        filecheck = (
            os.environ.get("FILECHECK") or shutil.which("FileCheck") or shutil.which("filecheck")
        )
        if filecheck is None:
            print("FileCheck not found", file=sys.stderr)
            return 1
        check_input = output.replace(b"\\\\", b"/") if os.name == "nt" else output
        return subprocess.run([filecheck, input_path], input=check_input).returncode
    finally:
        try:
            os.remove(out_path)
        except OSError:
            pass


if __name__ == "__main__":
    sys.exit(main())
