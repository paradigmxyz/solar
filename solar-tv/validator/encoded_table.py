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
ENTRY_MODULE = "SolarTV.EVMIR.Transforms.IndexedEntry"
FUEL = 64


class WrongDestination(Unsupported):
    def __init__(self, witness):
        self.witness = witness
        super().__init__(
            f"selector {witness['selector']} reaches PC {witness['actual_pc']}, "
            f"expected PC {witness['expected_pc']}"
        )


def start_image(image, catalog, version):
    return (
        f"import {MODULE}\n"
        + entry_proof.HEADER.replace(" MIR.TransferFromEntry", "")
        + "open EVM.EncodedTable\n"
        + entry_proof.image_definitions(image)
        + f"def cfg : EVM.State.Config := ⟨.{version}, {world.opcode_rows(catalog, version)}, 0, {{}}, none⟩\n"
    )


def guard_certificates(guards, entry, jumps):
    return "".join(
        f"""
def guard{i} := (EVM.IndexedGuard.read cfg code {guard_pc} {entry}).getD ⟨0,0,0,0,0⟩
theorem guard{i}Decoded : EVM.IndexedGuard.read cfg code {guard_pc} {entry} = some guard{i} :=
  by decide +kernel
theorem guard{i}Limit : guard{i}.limit = {limit}#256 := by decide +kernel
theorem guard{i}Miss (selector : Word) (tail : EVM.Stack) (gas : Nat)
    (outside : ({limit}#256).toNat ≤ selector.toNat) :
    EVM.IndexedGuard.run cfg code {guard_pc} {entry} {FUEL} {jumps} (selector :: tail) gas =
      if tail.length+3 ≤ 1024 && guard{i}.gas ≤ gas
      then some (.miss guard{i}.miss.toNat,selector::tail,gas-guard{i}.gas) else none :=
  EVM.IndexedGuard.misses cfg code {guard_pc} {entry} {FUEL} {jumps} guard{i}
    guard{i}Decoded (by decide +kernel) selector tail gas (by simpa only [guard{i}Limit] using outside)
theorem guard{i}Select (selector : Word) (tail : EVM.Stack) (gas : Nat)
    (inside : selector.toNat < ({limit}#256).toNat) :
    ∃ pc peak cost, EVM.IndexedJump.target targets selector = some pc ∧
      bounds[selector.toNat]? = some (peak,cost) ∧
      EVM.IndexedGuard.run cfg code {guard_pc} {entry} {FUEL} {jumps} (selector::tail) gas =
        if max 3 peak + tail.length ≤ 1024 && guard{i}.gas + guard{i}.markerGas + cost ≤ gas
        then some (.selected pc,tail,gas-(guard{i}.gas+guard{i}.markerGas+cost)) else none :=
  EVM.IndexedGuard.selects cfg code {guard_pc} {entry} {FUEL} {jumps} guard{i} guard{i}Decoded
    targets bounds certified (by decide +kernel) selector tail gas
    (by simpa only [guard{i}Limit] using inside)
#print axioms guard{i}Miss
#print axioms guard{i}Select
#eval IO.println ("range guard " ++ reprStr [{guard_pc}, guard{i}.limit.toNat,
  guard{i}.miss.toNat, guard{i}.gas, guard{i}.markerGas])
"""
        for i, (guard_pc, limit) in enumerate(guards)
    )


def certificate(
    image, pc, targets, jumps, catalog, version, mask_pc, binding, mask, guards
):
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
        matched_mask = (
            f"theorem sourceMask : proposed.1 = {mask}#256 := by decide +kernel\n"
            "#print axioms sourceMask\n"
            if mask is not None
            else ""
        )
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
{matched_mask}
#eval IO.println ("mask producer " ++ reprStr [proposed.1.toNat, proposed.2])
"""
    labels = f"def targets : List Nat := {targets}\n"
    if binding is not None:
        positions, logical = binding
        rows = ", ".join(f"({label},{offset})" for label, offset in positions)
        labels += f"""
def positions : List (Nat × Nat) := [{rows}]
def logical : List Nat := {logical}
theorem resolved : bindTargets positions logical = some targets := by decide +kernel
theorem logicalEdge (i label : Nat) (edge : logical[i]? = some label) :
    targets[i]? = positions.lookup label :=
  bound_target positions logical targets resolved i label edge
theorem checkedDestinations : validDestinations code targets = true := by decide +kernel
theorem destination (suffix : List Execution.Byte) (target : Word)
    (selected : target.toNat ∈ targets) :
    EVM.State.validJump (code.bytes ++ suffix) target = true :=
  destination_in_image code targets checkedDestinations suffix target selected
#print axioms logicalEdge
#print axioms destination
"""
    imports = "import SolarTV.EVMIR.Transforms.IndexedGuard\n" if guards else ""
    return (
        imports
        + start_image(image, catalog, version)
        + labels
        + f"""
def bounds := limits cfg code {pc} {FUEL} {jumps} targets.length
theorem certified : check cfg code {pc} {FUEL} {jumps} targets bounds = true := by decide +kernel

{proof}
{guard_certificates(guards, pc, jumps)}
#print axioms validated
#eval IO.println ("table bounds [" ++ String.intercalate "," (bounds.map fun (peak,gas) =>
  "[" ++ toString peak ++ "," ++ toString gas ++ "]") ++ "]")
"""
    )


def confirm_mismatch(directory, image, pc, targets, jumps, catalog, version, timeout):
    proof = entry_proof.lean(
        directory,
        "WrongDestination",
        start_image(image, catalog, version)
        + f"""
def targets : List Nat := {targets}
def proposal := mismatch cfg code {pc} {FUEL} {jumps} targets
theorem checkedCounterexample :
    (proposal.map fun (index, expected, actual, gas) =>
      targets[index]? == some expected && expected != actual &&
        execute cfg code {pc} {FUEL} {jumps} [BitVec.ofNat 256 index] gas == some (actual, [], 0)) =
    some true := by decide +kernel
#print axioms checkedCounterexample
#eval IO.println ("counterexample " ++ reprStr (match proposal with
  | some (index, expected, actual, gas) => [index, expected, actual, gas]
  | none => []))
""",
        timeout,
    )
    raise_witness(proof)


def confirm_guard_mismatches(directory, image, pc, catalog, version, timeout, guards):
    for start, limit in guards:
        proof = entry_proof.lean(
            directory,
            "WrongGuard",
            "import SolarTV.EVMIR.Transforms.IndexedGuard\n"
            + start_image(image, catalog, version)
            + f"""
def proposal := EVM.IndexedGuard.mismatch cfg code {start} {pc} ({limit}#256)
theorem checkedCounterexample : proposal.isSome = true := by decide +kernel
#print axioms checkedCounterexample
#eval IO.println ("counterexample " ++ reprStr (match proposal with
  | some (index, expected, actual, gas) => [index, expected, actual, gas]
  | none => []))
""",
            timeout,
        )
        raise_witness(proof, guard_pc=start)


def raise_witness(proof, **extra):
    if proof.returncode:
        return
    theorem, witness = proof.stdout.splitlines()
    entry_proof.check_axioms([theorem], 1)
    selector, expected, actual, gas = json.loads(
        witness.removeprefix("counterexample ")
    )
    raise WrongDestination(
        {
            "selector": selector,
            "expected_pc": expected,
            "actual_pc": actual,
            "gas": gas,
            **extra,
        }
    )


def check_entry(image, tables, catalog, version, timeout, *, final_image):
    producers = [
        (guard["pc"], table["entry_pc"])
        for table in tables
        for guard in table.get("range_guards", [])
    ]
    cuts = sorted(
        {start for start, _ in producers}
        | {table["producer"]["pc"] for table in tables if "producer" in table}
    )
    targets = sorted({edge["pc"] for table in tables for edge in table["entries"]})
    entries = [table["entry_pc"] for table in tables]
    pairs = ", ".join(f"({start},{pc})" for start, pc in producers)
    source = (
        f"import {ENTRY_MODULE}\n"
        + start_image(image, catalog, version)
        + f"""
open EVM.IndexedEntry
def cuts : List Nat := {cuts}
def producers : List (Nat × Nat) := [{pairs}]
def decoded := producers.map fun (start,entry) => EVM.IndexedGuard.read cfg code start entry
theorem completeGuards : decoded.all Option.isSome = true := by decide +kernel
def misses := decoded.filterMap fun guard => guard.map (·.miss.toNat)
def targets : List Nat := {targets}
def roots : List Point := (0 :: misses ++ targets).map fun pc => ⟨pc,none⟩
def entries : List Nat := {entries}
def finalImage : Bool := {str(final_image).lower()}
def seen := (candidates code finalImage cuts roots).getD []
theorem closed : Closed code finalImage cuts roots seen entries = true := by decide +kernel
theorem jumpInputs (opcode : Nat) (jump : isJump opcode = true) :
    0 < (EVM.State.available cfg opcode).inputs := by
  simp [isJump] at jump
  rcases jump with rfl | rfl <;> decide +kernel
theorem guardedEntry (data : List Execution.Byte) (frame : EVM.State.Frame)
    (finalSize : finalImage = true → data = [])
    (path : FramePrefix cfg (code.bytes ++ data) cuts roots frame) :
    frame.pc ∉ entries :=
  cannotBypass code cfg data finalImage cuts roots seen entries finalSize closed jumpInputs frame path
#print axioms guardedEntry
"""
    )
    try:
        entry_proof.success(
            build_model(timeout, ENTRY_MODULE), "indexed entry model build"
        )
        with tempfile.TemporaryDirectory(prefix="solar-tv-indexed-entry-") as directory:
            proof = entry_proof.lean(Path(directory), "GuardedEntry", source, timeout)
    except subprocess.TimeoutExpired:
        return {"status": "timeout", "cuts": cuts}
    if proof.returncode:
        return {
            "status": "unproven",
            "cuts": cuts,
            "diagnostic": (proof.stdout + proof.stderr)[-1200:],
        }
    entry_proof.check_axioms(proof.stdout.splitlines(), 1)
    return {"status": "verified", "cuts": cuts}


def check(
    image,
    pc,
    targets,
    jumps,
    replay,
    version,
    timeout,
    mask_pc=None,
    *,
    binding=None,
    mask=None,
    guards=(),
):
    if (
        not image
        or len(image) > 65536
        or not targets
        or len(targets) > 256
        or not 0 <= pc < len(image)
        or any(not 0 <= target < len(image) for target in targets)
        or jumps not in (1, 2)
        or (mask_pc is not None and not 0 <= mask_pc < pc)
        or any(
            not 0 <= start < len(image) or not 0 <= limit < 2**256
            for start, limit in guards
        )
    ):
        raise Unsupported("table image or target map is outside the supported bounds")
    catalog = native.catalog(replay, timeout)
    code = certificate(
        image, pc, targets, jumps, catalog, version, mask_pc, binding, mask, guards
    )
    module = "SolarTV.EVMIR.Transforms.IndexedGuard" if guards else MODULE
    entry_proof.success(build_model(timeout, module), "encoded table model build")
    with tempfile.TemporaryDirectory(prefix="solar-tv-table-") as directory:
        directory = Path(directory)
        proof = entry_proof.lean(directory, "Table", code, timeout)
        if proof.returncode and binding is not None:
            confirm_mismatch(
                directory, image, pc, targets, jumps, catalog, version, timeout
            )
            confirm_guard_mismatches(
                directory, image, pc, catalog, version, timeout, guards
            )
    if proof.returncode:
        raise Unsupported(
            "encoded table did not verify against the supplied map\n"
            + proof.stdout
            + proof.stderr
        )
    lines = proof.stdout.splitlines()
    entry_proof.check_axioms(
        [line for line in lines if "axioms" in line],
        1 + 2 * (binding is not None) + (mask is not None) + 2 * len(guards),
    )
    values = {}
    ranges = []
    for line in lines:
        label, _, value = line.partition(" [")
        if label in {"table bounds", "mask producer"}:
            values[label] = json.loads("[" + value)
        if label == "range guard":
            start, limit, miss, gas, marker_gas = json.loads("[" + value)
            ranges.append(
                {
                    "pc": start,
                    "limit": limit,
                    "miss_pc": miss,
                    "gas": gas,
                    "marker_gas": marker_gas,
                }
            )
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
    if guards:
        if len(ranges) != len(guards):
            raise Unsupported("encoded table proof omitted a range guard")
        result["range_guards"] = ranges
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
        if "producer" in result:
            result["entry_coverage"] = check_entry(
                image,
                [result],
                native.catalog(args.replay, args.timeout),
                args.evm_version,
                args.timeout,
                final_image=True,
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
