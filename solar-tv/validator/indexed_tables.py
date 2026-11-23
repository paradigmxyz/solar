"""Bind finalized logical jump tables to measured destinations in emitted code."""

import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

from . import audit, encoded_table, native, world
from .check import ROOT
from .common import Unsupported

MAX_SOURCE = 1024 * 1024


def read(path, limit):
    if path.stat().st_size > limit:
        raise Unsupported(f"{path.name} exceeds the indexed-table audit input bound")
    return path.read_text()


def source_model(source, version, replay, timeout):
    request = {
        "version": 1,
        "text": source,
        "evm_version": version,
        "indexed_tables": True,
    }
    response = subprocess.run(
        [replay, "--evm-model"],
        input=json.dumps(request),
        text=True,
        capture_output=True,
        check=False,
        timeout=timeout,
    )
    value = json.loads(response.stdout)
    if response.returncode or value.get("version") != 1 or value.get("status") != "ok":
        raise Unsupported(
            "independent table import: " + value.get("reason", "invalid export")
        )
    return value["model"]


def validate_layout(layout, code_end, model):
    positions = layout["labels"]
    sites = layout["sites"]
    tables = model["tables"]
    order = {label: i for i, label in enumerate(model["blocks"])}
    if len(positions) != len(dict(positions)):
        raise Unsupported("emission map repeats a logical block label")
    last_block = last_pc = -1
    for label, pc in positions:
        if (
            label not in order
            or not last_block < order[label]
            or not last_pc < pc < code_end
        ):
            raise Unsupported("emission map does not follow the logical block order")
        last_block, last_pc = order[label], pc
    if not tables or [table["block"] for table in tables] != [
        site["block"] for site in sites
    ]:
        raise Unsupported("emitted table sites do not match the logical tables")
    by_block = dict(positions)
    last_pc = -1
    for site, table in zip(sites, tables, strict=True):
        pc = site["pc"]
        mask_pc = site["mask_pc"]
        if not last_pc < pc < code_end or (mask_pc is None) != (table["mask"] is None):
            raise Unsupported("invalid table entry or selector producer mark")
        block = by_block.get(site["block"], -1)
        following = next(
            (
                offset
                for label, offset in positions
                if order[label] > order[site["block"]]
            ),
            code_end,
        )
        if not block < (mask_pc if mask_pc is not None else pc) <= pc < following:
            raise Unsupported(
                "table recipe is outside its source block's emitted range"
            )
        logical = table["targets"]
        targets = [by_block[label] for label in logical]
        relevant = [(label, target) for label, target in positions if label in logical]
        yield site, targets, (relevant, logical), table["mask"]
        last_pc = pc


def check(manifest, replay, timeout):
    layout_text = read(manifest, MAX_SOURCE)
    layout = json.loads(layout_text)
    version = layout["evm_version"]
    if layout["version"] != 1 or version not in world.FORKS or timeout <= 0:
        raise Unsupported("unsupported indexed-table audit format, target or timeout")
    folder = manifest.parent
    source = read(folder / "source.evmir", MAX_SOURCE)
    image = bytes.fromhex(read(folder / "bytecode.hex", 2 * MAX_SOURCE))
    code_end = layout["code_end"]
    if type(code_end) is not int or not 0 < code_end <= len(image):
        raise Unsupported("invalid executable extent in emitted layout")
    code = image[:code_end]
    model = source_model(source, version, replay, timeout)
    tables = list(validate_layout(layout, code_end, model))
    record = {
        "version": 1,
        "status": "verified",
        "evidence": "kernel-checked-logical-indexed-tables",
        "module": layout["module"],
        "artifact": layout["artifact"],
        "evm_version": version,
        "semantics": audit.semantics(),
        "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
        "layout_sha256": hashlib.sha256(layout_text.encode()).hexdigest(),
        "bytecode_sha256": hashlib.sha256(image).hexdigest(),
        "code_end": code_end,
        "tables": [],
    }
    for site, targets, binding, mask in tables:
        try:
            receipt = encoded_table.check(
                code,
                site["pc"],
                targets,
                site["jumps"],
                replay,
                version,
                timeout,
                site["mask_pc"],
                binding=binding,
                mask=mask,
            )
        except encoded_table.WrongDestination as error:
            receipt = {
                "status": "counterexample",
                "diagnostic": str(error),
                "witness": error.witness,
            }
        except Unsupported as error:
            receipt = {"status": "unsupported", "diagnostic": str(error)}
        except subprocess.TimeoutExpired as error:
            receipt = {"status": "timeout", "diagnostic": str(error)}
        record["tables"].append(
            {
                "block": site["block"],
                "logical_targets": binding[1],
                "selector_scope": "from-mask" if mask is not None else "in-range",
                **receipt,
            }
        )
        if receipt["status"] == "counterexample":
            record.update(status="counterexample", diagnostic=receipt["diagnostic"])
            break
        if receipt["status"] == "timeout" or (
            receipt["status"] == "unsupported" and record["status"] == "verified"
        ):
            record.update(status=receipt["status"], diagnostic=receipt["diagnostic"])
    if record["semantics"] != audit.semantics():
        raise Unsupported("semantics changed during this check")
    return record


def main(argv):
    parser = argparse.ArgumentParser(
        prog="solar-tv indexed-tables",
        description="Check all indexed transfers in one compiler-emitted audit bundle.",
    )
    parser.add_argument(
        "layout", type=Path, help="layout.json from the indexed-tables directory"
    )
    parser.add_argument(
        "--replay",
        default=native.default(
            os.getenv("SOLAR") or str(ROOT.parent / "target/debug/solar")
        ),
    )
    parser.add_argument("--timeout", type=int, default=60)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args(argv)
    try:
        result = check(args.layout, args.replay, args.timeout)
    except subprocess.TimeoutExpired as error:
        result = {"version": 1, "status": "timeout", "diagnostic": str(error)}
    except Unsupported as error:
        result = {"version": 1, "status": "unsupported", "diagnostic": str(error)}
    except (KeyError, TypeError, ValueError, OSError) as error:
        result = {"version": 1, "status": "error", "diagnostic": str(error)}
    if args.report:
        audit.write(args.report, result)
    status = result["status"]
    if status == "verified":
        print("verified")
        return 0
    print(result["diagnostic"], file=sys.stderr)
    return {"counterexample": 1, "unsupported": 2, "timeout": 2, "error": 3}[status]
