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

        filecheck = shutil.which("FileCheck") or shutil.which("filecheck")
        if filecheck is not None:
            return subprocess.run([filecheck, input_path], input=output).returncode
        return check_file(input_path, output.decode(errors="replace"))
    finally:
        try:
            os.remove(out_path)
        except OSError:
            pass


def check_file(input_path, output):
    checks = []
    with open(input_path, encoding="utf-8") as file:
        for line in file:
            line = line.strip()
            if line.startswith("// CHECK-NEXT:"):
                checks.append(("next", line.removeprefix("// CHECK-NEXT:").strip()))
            elif line.startswith("// CHECK-NOT:"):
                checks.append(("not", line.removeprefix("// CHECK-NOT:").strip()))
            elif line.startswith("// CHECK:"):
                checks.append(("check", line.removeprefix("// CHECK:").strip()))

    lines = output.splitlines()
    offset = 0
    matched = -1
    for kind, pattern in checks:
        if kind == "check":
            found = find_line(lines, pattern, offset)
            if found is None:
                print(f"CHECK failed: {pattern}", file=sys.stderr)
                return 1
            offset = found + 1
            matched = found
        elif kind == "next":
            next_line = matched + 1
            if next_line >= len(lines) or pattern not in lines[next_line]:
                print(f"CHECK-NEXT failed: {pattern}", file=sys.stderr)
                return 1
            offset = next_line + 1
            matched = next_line
        elif kind == "not":
            if find_line(lines, pattern, offset) is not None:
                print(f"CHECK-NOT failed: {pattern}", file=sys.stderr)
                return 1
    return 0


def find_line(lines, pattern, offset):
    pattern = normalize(pattern)
    for index in range(offset, len(lines)):
        if pattern in normalize(lines[index]):
            return index
    return None


def normalize(text):
    return text.replace("\\\\", "/").replace("\\", "/")


if __name__ == "__main__":
    sys.exit(main())
