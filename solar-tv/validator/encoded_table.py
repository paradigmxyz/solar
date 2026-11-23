"""Prove the local transfer of an indexed jump in emitted EVM bytecode."""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

from . import entry_proof, native, world
from .check import ROOT, build_model
from .common import Unsupported

MODULE = "SolarTV.EVMIR.Transforms.EncodedTable"
FUEL = 64


def certificate(image, pc, targets, jumps, catalog, version, mask_pc):
    if mask_pc is None:
        proof = f"""
theorem validated (selector : Word) (tail : EVM.Stack) (gas pc peak charge : Nat)
    (logical : EVM.IndexedJump.target targets selector = some pc)
    (row : bounds[selector.toNat]? = some (peak,charge)) :
    execute cfg code {pc} {FUEL} {jumps} (selector :: tail) gas =
      if peak + tail.length ≤ 1024 && charge ≤ gas
      then some (pc,tail,gas-charge) else none :=
  correspondence cfg code {pc} {FUEL} {jumps} targets bounds certified selector tail gas pc peak charge logical row
"""
    else:
        proof = f"""
def proposed := (maskPrefix cfg code {mask_pc} {pc}).getD (0,0)
theorem decoded : maskPrefix cfg code {mask_pc} {pc} = some proposed := by decide +kernel
theorem inRange : proposed.1.toNat < targets.length := by decide +kernel

theorem validated (input : Word) (tail : EVM.Stack) (gas : Nat) :
    ∃ pc peak charge, EVM.IndexedJump.target targets (Evm.and proposed.1 input) = some pc ∧
      bounds[(Evm.and proposed.1 input).toNat]? = some (peak,charge) ∧
      execute cfg code {mask_pc} ({FUEL}+2) {jumps} (input :: tail) gas =
        if max 2 peak + tail.length ≤ 1024 && proposed.2 + charge ≤ gas
        then some (pc,tail,gas-(proposed.2+charge)) else none :=
  masked_correspondence cfg code {mask_pc} {pc} {FUEL} {jumps - 1} targets bounds
    certified input proposed.1 tail gas proposed.2 decoded inRange

#eval IO.println ("mask producer " ++ reprStr [proposed.1.toNat, proposed.2])
"""
    return (
        f"import {MODULE}\n"
        + entry_proof.HEADER.replace(" MIR.TransferFromEntry", "")
        + "open EVM.EncodedTable\n"
        + entry_proof.image_definitions(image)
        + f"""
def cfg : EVM.State.Config := ⟨.{version}, {world.opcode_rows(catalog, version)}, 0, {{}}, none⟩
def targets : List Nat := {targets}
def bounds := limits cfg code {pc} {FUEL} {jumps} targets.length
theorem certified : check cfg code {pc} {FUEL} {jumps} targets bounds = true := by decide +kernel

{proof}
#print axioms validated
#eval IO.println ("table bounds [" ++ String.intercalate "," (bounds.map fun (peak,gas) =>
  "[" ++ toString peak ++ "," ++ toString gas ++ "]") ++ "]")
"""
    )


def check(image, pc, targets, jumps, replay, version, timeout, mask_pc=None):
    if (
        not image
        or len(image) > 65536
        or not targets
        or len(targets) > 256
        or not 0 <= pc < len(image)
        or any(not 0 <= target < len(image) for target in targets)
        or jumps not in (1, 2)
        or (mask_pc is not None and not 0 <= mask_pc < pc)
    ):
        raise Unsupported("table image or target map is outside the supported bounds")
    catalog = native.catalog(replay, timeout)
    code = certificate(image, pc, targets, jumps, catalog, version, mask_pc)
    entry_proof.success(build_model(timeout, MODULE), "encoded table model build")
    with tempfile.TemporaryDirectory(prefix="solar-tv-table-") as directory:
        proof = entry_proof.lean(Path(directory), "Table", code, timeout)
    if proof.returncode:
        raise Unsupported(
            "encoded table did not verify against the supplied map\n"
            + proof.stdout
            + proof.stderr
        )
    lines = proof.stdout.splitlines()
    entry_proof.check_axioms(
        [line for line in lines if line.startswith("'validated'")], 1
    )
    values = {}
    for line in lines:
        label, _, value = line.partition(" [")
        if label in {"table bounds", "mask producer"}:
            values[label] = json.loads("[" + value)
    if "table bounds" not in values or (
        mask_pc is not None and "mask producer" not in values
    ):
        raise Unsupported("encoded table proof omitted its measurements")
    result = {
        "version": 1,
        "status": "verified",
        "evidence": "kernel-checked-encoded-table",
        "evm_version": version,
        "runtime_sha256": hashlib.sha256(image).hexdigest(),
        "entry_pc": pc,
        "jumps": jumps,
        "entries": [
            {"pc": target, "peak": peak, "gas": gas}
            for target, (peak, gas) in zip(targets, values["table bounds"], strict=True)
        ],
    }
    if mask_pc is not None:
        mask, price = values["mask producer"]
        result["producer"] = {"pc": mask_pc, "mask": mask, "gas": price}
    return result


def main(argv):
    parser = argparse.ArgumentParser(
        prog="solar-tv encoded-table",
        description="Check an emitted runtime table against its ordered destination PCs.",
    )
    parser.add_argument("runtime", type=Path, help="runtime bytecode in hex")
    parser.add_argument(
        "--pc", type=int, required=True, help="table entry opcode offset"
    )
    parser.add_argument(
        "--targets", required=True, help="comma-separated destination PCs"
    )
    parser.add_argument(
        "--mask-pc", type=int, help="PC of the selector's preceding PUSH and AND"
    )
    parser.add_argument(
        "--jumps", type=int, choices=(1, 2), default=1, help="2 for outlined stubs"
    )
    parser.add_argument("--evm-version", choices=sorted(world.FORKS), default="cancun")
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
        if args.timeout <= 0 or args.runtime.stat().st_size > 131075:
            raise Unsupported("supply a positive timeout and a runtime up to 64 KiB")
        image = bytes.fromhex(args.runtime.read_text().strip().removeprefix("0x"))
        targets = [int(value) for value in args.targets.split(",")]
        result = check(
            image,
            args.pc,
            targets,
            args.jumps,
            args.replay,
            args.evm_version,
            args.timeout,
            args.mask_pc,
        )
    except (OSError, ValueError, Unsupported, subprocess.TimeoutExpired) as error:
        result = {"version": 1, "status": "unsupported", "diagnostic": str(error)}
    if args.report:
        args.report.write_text(json.dumps(result, indent=2) + "\n")
    if result["status"] == "verified":
        print(
            f"verified encoded table at pc {args.pc} ({len(result['entries'])} selectors)"
        )
        return 0
    print(result["diagnostic"], file=sys.stderr)
    return 2
