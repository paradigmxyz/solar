"""Select complete regions only after the compiler validates the full module.

The original text remains the independent interpreter's replay input. Projections
remove unrelated bodies, never operations from the selected body. EVM successors
are uninterpreted boundaries named by their original stable labels.
"""

import re

from .common import Unsupported


def function(text: str, name: str) -> str:
    functions = {}
    current = None
    phase = ""
    for line in text.splitlines():
        if line.startswith("@phase "):
            phase = line + "\n"
        if match := re.match(r"fn @([^ (]+)\(", line):
            current = match[1]
            if current in functions:
                raise Unsupported("ambiguous function name")
            functions[current] = []
        if current is not None:
            functions[current].append(line)
            if line == "}":
                current = None
    if name not in functions:
        raise Unsupported(f"function @{name} is absent")
    if "backend-memory" in text or "backend-stack" in text:
        raise Unsupported("backend memory records")
    return "@module Function\n" + phase + "\n".join(functions[name]) + "\n"


def blocks(text: str) -> dict[str, list[str]]:
    blocks = {}
    current = None
    for line in text.splitlines():
        plain = line.split(";", 1)[0].strip()
        if match := re.match(r"(bb\d+)(?: \[.*\])?(?: !metadata\(.*\))?:$", plain):
            current = match[1]
            blocks[current] = []
        elif current and plain:
            blocks[current].append(plain)
    return blocks


def window(text: str, name: str, height: int) -> tuple[str, str | None]:
    bodies = blocks(text)
    if name not in bodies or not 0 <= height <= 1024:
        raise Unsupported("missing window or invalid stack height")
    labels = list(bodies)
    index = labels.index(name)
    following = labels[index + 1] if index + 1 < len(labels) else None
    lines = ["@module Window", f"{name} [stack={height}]:", *bodies[name]]
    targets = set(re.findall(r"\bbb\d+\b", " ".join(bodies[name])))
    for label in sorted(targets, key=lambda label: int(label[2:])):
        if label != name:
            if label not in bodies:
                raise Unsupported("missing window successor")
            lines.extend([f"{label}:", "stop"])
    return "\n".join(lines) + "\n", following


def compatible_windows(before: str, after: str, name: str) -> None:
    old, new = blocks(before), blocks(after)
    if list(old) != list(new):
        raise Unsupported("window label correspondence changed")
    neighbors = set()
    for bodies in (old, new):
        for label, body in bodies.items():
            targets = set(re.findall(r"\bbb\d+\b", " ".join(body)))
            if label == name:
                neighbors.update(targets)
            if name in targets:
                neighbors.add(label)
    if any(old.get(label) != new.get(label) for label in neighbors - {name}):
        raise Unsupported("neighboring window changed; stack correspondence is unknown")
