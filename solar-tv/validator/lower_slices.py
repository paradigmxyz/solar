"""Normalize projections from a locally constructed, fully defined slice."""

import subprocess
import sys
from copy import deepcopy

from . import dead_ssa, mir_cfg, mir_state, prune
from .common import Unsupported


def materialize(body):
    body = deepcopy(body)
    definitions = {}
    for block in body["blocks"]:
        for inst in block["instructions"]:
            op = inst["operation"]
            if op["kind"] != "make_slice":
                continue
            ty = inst["type"]
            pointer, size = op["args"]
            place = ty.get("location")
            if (
                inst["result"] is None
                or ty.get("kind") != "slice"
                or op["location"] != place
                or pointer["type"]
                != {
                    "kind": "pointer",
                    "location": place,
                    "bits": 256,
                    "name": f"ptr<{place}>",
                }
                or size["type"] != {"kind": "word", "bits": 256, "name": "i256"}
                or any(operand["kind"] == "undefined" for operand in op["args"])
            ):
                raise Unsupported("slice projection requires a defined typed pair")
            mir_state.value_type(ty)
            definitions[inst["result"]] = (ty, pointer, size)
    aliases = {}
    for block in body["blocks"]:
        keep = []
        for inst in block["instructions"]:
            op = inst["operation"]
            if op["kind"] == "extract_value":
                (value,) = op["args"]
                if value["kind"] == "local" and value["index"] in definitions:
                    ty, pointer, size = definitions[value["index"]]
                    if (
                        value["type"] != ty
                        or op["aggregate_type"] != ty
                        or op["index"] not in (0, 1)
                    ):
                        raise Unsupported("slice projection changed its declared field")
                    field = (pointer, size)[op["index"]]
                    if inst["result"] is None or inst["type"] != field["type"]:
                        raise Unsupported("slice projection changed its carrier")
                    aliases[inst["result"]] = field
                    continue
            keep.append(inst)
        block["instructions"] = keep

    def resolve(value):
        if isinstance(value, list):
            return [resolve(item) for item in value]
        if not isinstance(value, dict):
            return value
        old = set()
        while value.get("kind") == "local" and value["index"] in aliases:
            index = value["index"]
            if index in old:
                raise Unsupported("slice projection has an ungrounded alias")
            old.add(index)
            value = aliases[index]
        return {key: resolve(item) for key, item in value.items()}

    body["blocks"] = resolve(body["blocks"])
    uses = set(dead_ssa.reads(body["blocks"], reject_undefined=False))
    for block in body["blocks"]:
        block["instructions"] = [
            inst
            for inst in block["instructions"]
            if inst["result"] not in definitions or inst["result"] in uses
        ]
    return body, len(aliases)


SOURCE_OPERATION = "= extract_value slice<"


def check(paths, solar, replay, version, timeout, function, *, verbose=False):
    try:
        if any(
            path.suffix != ".mir" or path.stat().st_size > 1024 * 1024 for path in paths
        ):
            return None
        tables = [prune.declarations(path.read_text())[1] for path in paths]
        if any(function not in table for table in tables):
            return None
        before, after = ("\n".join(table[function]["text"]) for table in tables)
        if before.count(SOURCE_OPERATION) <= after.count(SOURCE_OPERATION):
            return None
        return mir_cfg.check(
            paths, solar, replay, version, timeout, function, lowering="lower-slices"
        )
    except (Unsupported, OSError, subprocess.TimeoutExpired) as error:
        if verbose:
            print(f"Slice projection proof: {error}", file=sys.stderr)
        return None
