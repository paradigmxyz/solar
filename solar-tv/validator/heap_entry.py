"""Certify an emitted entry's free-memory-pointer initialization."""

import argparse
import hashlib
import json
import os
import subprocess
import tempfile
from pathlib import Path

from . import audit, entry_proof, native, world
from .common import Inconclusive, Program, Unsupported

MODULE = "SolarTV.EVMIR.Transforms.HeapEntry"


def observe(image, fixture, version, executable, timeout, pc=None):
    state = world.with_runtime(fixture, image)
    program = Program(
        ("world-entry", "evmir"), [], "", "", world=state, evm_version=version
    )
    result = native.replay(
        program,
        program,
        [],
        executable=executable,
        timeout=timeout,
        probe_pc=pc,
        raw_runtime=pc is None,
    )
    left, right = result["observations"]
    if left != right:
        raise Inconclusive("independent bytecode replay is unstable")
    return json.loads(left)


def check(image, floor, extent, solar, version, timeout):
    from .check import build_model

    if not 5 < len(image) <= 65536:
        raise Unsupported("heap entry requires a bytecode image of 6 to 65536 bytes")
    if not 128 <= floor <= 255 or floor % 32 or not 0 <= extent <= 65536 - floor:
        raise Unsupported(
            "entry heap floor or allocation extent is outside the supported range"
        )
    executable = native.default(solar)
    with tempfile.TemporaryDirectory(prefix="solar-tv-heap-entry-") as folder:
        directory = Path(folder)
        fixture_path = directory / "world.json"
        fixture_path.write_text('{"version":1,"gas":18}')
        fixture = world.read(fixture_path, solar, "exact")
        configuration = world.configuration(solar, version, fixture, timeout)
        source = f"""import {MODULE}
open SolarTV EvmRules Execution EVM.State EVM.HeapEntry
set_option autoImplicit false
{configuration}
def emittedPrefix : List Byte := {list(image[:5])}
def floor : Byte := {floor}
theorem binding : emittedPrefix = [0x60,floor,0x60,0x40,0x52] := by decide
theorem certificate (env : World.Environment) (journal : Journal) (tail : List Byte) :
    MeteredPrefix tvWorldConfig journal ⟨env,emittedPrefix ++ tail,0,[],[],[],0⟩
      journal (ready env floor tail) 3 18 := by
  rw [binding]
  exact metered tvWorldConfig journal env floor tail (by decide) (by decide)
theorem producer (frame : MIR.State.Frame) (env : World.Environment) (tail : List Byte) :
    MIR.FreshHeap.Contract {{frame with memory := (ready env floor tail).memory}}
      (floor.setWidth 256) {extent} :=
  floor_contract frame env floor tail {extent} (by decide) (by decide) (by decide)
#print axioms certificate
#print axioms producer
#print axioms MeteredPrefix.rollbackOutOfGas
"""
        entry_proof.success(build_model(timeout, MODULE), MODULE)
        output = entry_proof.success(
            entry_proof.lean(directory, "Heap", source, timeout), "heap entry"
        )
        entry_proof.check_axioms(output.splitlines(), 3)
        paid = observe(image, fixture, version, executable, timeout, pc=5)
        unpaid = observe(image, {**fixture, "gas": 17}, version, executable, timeout)
        if (paid["gas"], paid["stack"], paid["memory"], paid["pcs"]) != (
            0,
            [],
            [0] * 95 + [floor],
            [0, 2, 4],
        ) or unpaid[:3] != ["out-of-gas", [], 0]:
            raise Inconclusive(
                "native bytecode replay disagrees with the entry certificate"
            )

    return {
        "version": 1,
        "status": "verified",
        "evidence": "kernel-proof",
        "semantics": audit.semantics(),
        "target": version,
        "image_sha256": hashlib.sha256(image).hexdigest(),
        "gas_policy": "exact-entry-prefix",
        "entry": 0,
        "cut": 5,
        "heap_floor": floor,
        "extent": extent,
        "gas_cost": 18,
        "replay": {"funded": paid, "underfunded": unpaid[:3]},
    }


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "image", type=Path, help="hex-encoded creation or runtime bytecode"
    )
    parser.add_argument("--floor", required=True, type=lambda value: int(value, 0))
    parser.add_argument("--extent", required=True, type=int)
    parser.add_argument("--evm-version", choices=world.FORKS, default="cancun")
    parser.add_argument("--timeout", type=int, default=30)
    parser.add_argument(
        "--solar",
        default=os.getenv("SOLAR")
        or str(Path(__file__).resolve().parents[2] / "target/debug/solar"),
    )
    args = parser.parse_args(argv)
    try:
        if args.image.stat().st_size > 132 * 1024:
            raise Unsupported("heap entry hex image exceeds 132 KiB")
        image = bytes.fromhex(args.image.read_text().strip().removeprefix("0x"))
        print(
            json.dumps(
                check(
                    image,
                    args.floor,
                    args.extent,
                    args.solar,
                    args.evm_version,
                    args.timeout,
                )
            )
        )
        return 0
    except (
        OSError,
        ValueError,
        Inconclusive,
        Unsupported,
        subprocess.TimeoutExpired,
    ) as error:
        parser.exit(2, f"inconclusive: {error}\n")
