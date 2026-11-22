"""Check a semantic-to-lowered phase advance with identical executable MIR."""

from . import audit, type_prune
from .common import Unsupported


def check(texts, tables, version, timeout, executable):
    before, after = [text.splitlines() for text in texts]
    # Standalone IR output starts with a banner containing the input filename.
    # Keep the canonical module, including metadata and declaration order.
    for lines in (before, after):
        if lines and lines[0].startswith("; === "):
            lines.pop(0)
    if (
        len(after) < 2
        or after[1] != "@phase lowered"
        or not before
        or not before[0].startswith("@module ")
        or any(line.startswith("@phase ") for line in before[1:])
    ):
        raise Unsupported("phase certificate requires a semantic-to-lowered advance")
    del after[1]
    if before != after:
        raise Unsupported("phase advance changed executable or shared declarations")

    views = [
        {"code": lines, "functions": list(functions)}
        for lines, (_, functions) in zip((before, after), tables, strict=True)
    ]
    programs = type_prune.prove_identity(
        texts,
        views,
        version,
        timeout,
        executable,
        scope="phase-advance",
        evidence={"phases": ["semantic", "lowered"]},
    )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=programs
    )
