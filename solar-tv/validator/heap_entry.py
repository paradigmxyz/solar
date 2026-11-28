"""Certify an emitted entry's free-memory-pointer initialization."""

import argparse
import hashlib
import json
import os
import subprocess
import tempfile
from pathlib import Path

from . import audit, dispatch_proof, entry_proof, metered_prefix, native, world
from .common import Inconclusive, Program, Unsupported

MODULE = "SolarTV.EVMIR.Transforms.HeapEntry"
# The ABI route proof also imports this module, which the default Lean
# target does not build.
ABI_MODULE = "SolarTV.MIR.Transforms.TransferFromABI"


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


def routed_source(image, route, selector, minimum, floor, extent, configuration):
    source = f"""import {MODULE}
import {ABI_MODULE}
open SolarTV EvmRules Execution EVM.State
set_option autoImplicit false
set_option maxRecDepth 16384
set_option maxHeartbeats 4000000
set_option Elab.async false
set_option linter.unusedVariables false
set_option linter.unusedSimpArgs false
{configuration}
"""
    source += entry_proof.image_definitions(image)
    source += dispatch_proof.frame_definition(len(image))
    binders = f"""(env : World.Environment) (journal : Journal)
    (large : {minimum} ≤ env.input.length) (small : env.input.length < 2^256)
    (noValue : env.value = 0)
    (selected : MIR.TransferFromDispatch.selector env.input = {selector})"""
    args = "env journal large small noValue selected"
    for offset, bits in sorted(route.word_bounds().items()):
        binders += f"\n    (head{offset} : (Execution.readWord env.input {offset}).toNat < 2^{bits})"
        args += f" head{offset}"
    before_store = (
        f"dispatchFrame env 0 {route.pc} {dispatch_proof.stack_text(route.stack)}"
    )
    debits = []
    for i, step in enumerate(route.instructions):
        before = (
            f"dispatchFrame env 0 {step.pc} {dispatch_proof.stack_text(step.stack)}"
        )
        after = before_store
        if i + 1 < len(route.instructions):
            following = route.instructions[i + 1]
            after = f"dispatchFrame env 0 {following.pc} {dispatch_proof.stack_text(following.stack)}"
        name = f"route{i}"
        cost = step.schema[1]
        source += dispatch_proof.debit_source(
            name, step, binders, before, after, selector, cost
        )
        debits.append(metered_prefix.Debit(name, step.pc, step.opcode, cost, after))
    initial = "dispatchFrame env 0 0 []"
    source += metered_prefix.certify(
        "metered_dispatch", binders, args, route.gas, initial, before_store, debits
    )
    tail = dispatch_proof.stack_text(route.stack[2:])
    count, charge = len(debits) + 1, route.gas + 12
    return (
        source
        + f"""
def afterStore (env : World.Environment) : Frame :=
  {{({before_store}) with
    pc := {route.pc + 1}
    stack := {tail}
    memory := writeBytes (List.replicate 96 0) 64 (wordBytes {floor})}}

theorem abi_heap {binders} (caller : MIR.State.Frame) :
    MeteredPrefix tvWorldConfig journal ({initial}) journal (afterStore env) {count} {charge} ∧
    MIR.HeapCursor.Entry {{caller with memory := (afterStore env).memory}} {floor} ∧
    MIR.FreshHeap.Contract {{caller with memory := (afterStore env).memory}} {floor} {extent} := by
  have fetched : code.bytes[{route.pc}]? = some (0x52 : Byte) := by
    rw [← CodeChunks.get_eq]
    decide +kernel
  have step := EVM.HeapEntry.metered_store tvWorldConfig journal ({before_store}) caller {floor} {tail}
    (by rfl) fetched rfl (by simp) (by simp [dispatchFrame]) (by simp [dispatchFrame])
    (by decide) (by decide)
  have store : MeteredPrefix tvWorldConfig journal ({before_store}) journal (afterStore env) 1 12 ∧
      MIR.HeapCursor.Entry {{caller with memory := (afterStore env).memory}} {floor} := by
    simpa [dispatchFrame, afterStore, MIR.ContinuedCounter.padTo, memoryCharge, memoryCost,
      requiredWords, memoryWords] using step
  exact ⟨(metered_dispatch {args}).append store.1, store.2,
    MIR.FreshHeap.initialized {{caller with memory := List.replicate 96 0}} {floor} {extent}
      (by simp [MIR.Universal.MemoryValid]) (by simp) (by decide) (by decide) (by decide)⟩

theorem abi_heap_out_of_gas {binders} (caller : MIR.State.Frame) (work balance : Nat)
    (short : balance < {charge}) :
    ∃ spent ≤ {count},
      EVM.State.execute (work+{count}) tvWorldConfig journal
        (dispatchFrame env balance 0 []) = .ok ⟨.outOfGas,[],journal,0,spent⟩ := by
  exact (abi_heap {args} caller).1.rollbackOutOfGas work balance short
#print axioms abi_heap
#print axioms abi_heap_out_of_gas
"""
    )


def check_routed(
    image,
    floor,
    extent,
    selector,
    minimum,
    cut,
    solar,
    version,
    timeout,
    *,
    word_bounds=None,
):
    from .check import build_model

    if not (
        0 < cut < len(image) <= 65536
        and image[cut - 1] == 0x52
        and 0 <= selector < 2**32
        and 4 <= minimum <= 65536
        and 128 <= floor <= 65536
        and floor % 32 == 0
        and 0 <= extent <= 65536 - floor
    ):
        raise Unsupported("invalid ABI heap selector, cut, floor or input bound")
    bounds = word_bounds or {}
    if any(
        type(offset) is not int
        or type(bits) is not int
        or not 4 <= offset <= minimum - 32
        or not 1 <= bits < 256
        for offset, bits in bounds.items()
    ):
        raise Unsupported("invalid bounded ABI head word")
    executable = native.default(solar)
    catalog = {
        entry["opcode"]: entry for entry in native.catalog(executable, timeout)["evmir"]
    }
    with tempfile.TemporaryDirectory(prefix="solar-tv-heap-abi-") as folder:
        directory = Path(folder)
        path = directory / "world.json"
        sample = selector.to_bytes(4, "big") + bytes(minimum - 4)
        path.write_text(
            json.dumps(
                {"version": 1, "gas": 2_000_000, "calldata": "0x" + sample.hex()}
            )
        )
        fixture = world.read(path, solar, "exact")
        observed = observe(image, fixture, version, executable, timeout, pc=cut - 1)
        route = dispatch_proof.plan_prefix(
            image,
            selector,
            [*observed["pcs"], cut - 1],
            catalog,
            version,
            minimum,
            word_bounds=bounds,
        )
        if route.stack[:2] != [64, floor] or not all(
            isinstance(value, int) for value in route.stack
        ):
            raise Unsupported(
                "ABI path does not establish the requested concrete FMP store stack"
            )
        charge = route.gas + 12
        configuration = world.configuration(solar, version, fixture, timeout)
        source = routed_source(
            image, route, selector, minimum, floor, extent, configuration
        )
        for module in (MODULE, ABI_MODULE):
            entry_proof.success(build_model(timeout, module), module)
        output = entry_proof.success(
            entry_proof.lean(directory, "HeapABI", source, timeout), "ABI heap entry"
        )
        entry_proof.check_axioms(output.splitlines(), 2)
        paid = observe(
            image, {**fixture, "gas": charge}, version, executable, timeout, pc=cut
        )
        unpaid = observe(
            image, {**fixture, "gas": charge - 1}, version, executable, timeout
        )
        expected = bytes(64) + floor.to_bytes(32, "big")
        if (
            observed["stack"] != list(map(str, route.stack))
            or observed["memory"]
            or observed["gas"] != fixture["gas"] - route.gas
            or paid["gas"] != 0
            or paid["stack"] != list(map(str, route.stack[2:]))
            or paid["memory"] != list(expected)
            or paid["pcs"] != [*(step.pc for step in route.instructions), route.pc]
            or unpaid[:3] != ["out-of-gas", [], 0]
        ):
            raise Inconclusive(
                "native bytecode replay disagrees with the ABI heap certificate"
            )
    return {
        "version": 1,
        "status": "verified",
        "evidence": "kernel-proof",
        "semantics": audit.semantics(),
        "target": version,
        "image_sha256": hashlib.sha256(image).hexdigest(),
        "proof_sha256": hashlib.sha256(source.encode()).hexdigest(),
        "gas_policy": "exact-entry-prefix",
        "entry": 0,
        "selector": f"0x{selector:08x}",
        "min_calldata": minimum,
        **({"word_bounds": route.word_bounds()} if route.word_bounds() else {}),
        "cut": cut,
        "heap_floor": floor,
        "extent": extent,
        "instructions": len(route.instructions) + 1,
        "gas_cost": charge,
        "assumptions": [
            "zero call value, empty entry stack and memory, selected ABI route and calldata length between the stated minimum and 2^256 minus one",
            *(
                [
                    "each listed calldata word is below two to the power of its declared width"
                ]
                if route.word_bounds()
                else []
            ),
            "kernel-checked cursor and exact debit through the physical cut. MIR activation and suffix correspondence remain separate",
        ],
        "replay": {"funded": paid, "underfunded": unpaid[:3]},
    }


def word_bound(value):
    try:
        offset, bits = value.split(":", 1)
        return int(offset, 0), int(bits)
    except ValueError as error:
        raise argparse.ArgumentTypeError(
            "expected OFFSET:BITS for an ABI word"
        ) from error


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "image", type=Path, help="hex-encoded creation or runtime bytecode"
    )
    parser.add_argument("--floor", required=True, type=lambda value: int(value, 0))
    parser.add_argument("--extent", required=True, type=int)
    parser.add_argument(
        "--selector",
        type=lambda value: int(value, 0),
        help="ABI selector for a relocated heap store",
    )
    parser.add_argument("--min-calldata", type=int, help="minimum ABI calldata bytes")
    parser.add_argument(
        "--word-bound",
        type=word_bound,
        action="append",
        default=[],
        help="unsigned ABI head bound as OFFSET:BITS",
    )
    parser.add_argument(
        "--cut",
        type=lambda value: int(value, 0),
        help="emitted PC immediately after the FMP store",
    )
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
        route = (args.selector, args.min_calldata, args.cut)
        options = (args.solar, args.evm_version, args.timeout)
        bounds = dict(args.word_bound)
        if len(bounds) != len(args.word_bound):
            raise Unsupported("duplicate ABI word offset")
        if not bounds and all(value is None for value in route):
            evidence = check(image, args.floor, args.extent, *options)
        elif all(value is not None for value in route):
            evidence = check_routed(
                image, args.floor, args.extent, *route, *options, word_bounds=bounds
            )
        else:
            raise Unsupported(
                "an ABI heap proof requires --selector, --min-calldata and --cut together"
            )
        print(json.dumps(evidence))
        return 0
    except (
        OSError,
        ValueError,
        Inconclusive,
        Unsupported,
        subprocess.TimeoutExpired,
    ) as error:
        parser.exit(2, f"inconclusive: {error}\n")
