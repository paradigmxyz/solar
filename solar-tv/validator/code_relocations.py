"""Bind supported constructor code reads to the independent source export."""

import hashlib
import re

from . import world
from .common import Unsupported

_SIZE = "size-init-tail"
_READS = re.compile(r"copy-init-tail [0-9]+|copy-code-data [0-9]+ [0-9]+ [0-9]+")


def operation(name):
    return name == _SIZE or _READS.fullmatch(name) is not None


def assembled_reads(image, data_size, operations, catalog):
    from .call_budget import decode_runtime

    start = len(image) - data_size
    decoded = list(decode_runtime(image[:start], catalog).items())
    steps = [opcode if value is None else value for _, (opcode, value) in decoded]
    cursor = 0
    reads = []
    for operation in operations:
        name, *operands = operation.split()
        values = list(map(int, operands))
        if name == _SIZE:
            pattern = [len(image), "codesize", "sub"]
        elif name == "copy-init-tail":
            pattern = [
                len(image),
                "codesize",
                "sub",
                "dup1",
                len(image),
                values[0],
                "codecopy",
            ]
        else:
            offset, count, destination = values
            pattern = [count, "dup1", start + offset, destination, "codecopy"]
        for pc in range(cursor, len(steps) - len(pattern) + 1):
            if steps[pc : pc + len(pattern)] == pattern:
                reads.append(decoded[pc][0])
                cursor = pc + len(pattern)
                break
        else:
            raise Unsupported("assembled constructor code read has different operands")
    return reads


def bind(sources, texts, solar, version, timeout, catalog):
    suffixes = [source.get("data_suffix") for source in sources]
    if suffixes == [None, None]:
        return None
    if not suffixes[0] or suffixes[0] != suffixes[1]:
        raise Unsupported("constructor reads different declared data on the two sides")
    suffix = bytes.fromhex(suffixes[0])
    evidence = []
    for text, source in zip(texts, sources, strict=True):
        image = world.runtime(text, "evmir", solar, version, timeout)
        if not image.endswith(suffix):
            raise Unsupported(
                "declared bytes are absent from the end of the assembled code"
            )
        operations = [
            node["operation"]
            for row in source["nodes"]
            if operation((node := row["node"]).get("operation", ""))
        ]
        evidence.append(
            {
                "image_sha256": hashlib.sha256(image).hexdigest(),
                "declared_data_sha256": hashlib.sha256(suffix).hexdigest(),
                "declared_data_position": len(image) - len(suffix),
                "argument_position": len(image),
                "relocated_reads": sorted(set(operations)),
                "assembled_read_positions": assembled_reads(
                    image, len(suffix), operations, catalog
                ),
            }
        )
    return evidence
