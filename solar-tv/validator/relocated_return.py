"""Check a stateful ABI path whose internal return address moves during layout."""

import hashlib
import json
import tempfile
from dataclasses import replace
from functools import partial
from pathlib import Path

from . import audit, entry_proof, native, report, world
from .call_budget import decode_runtime
from .common import Inconclusive, Program, Unsupported

HEADER = """open EvmRules SolarTV EVM.RelocatedReturn MIR.TransferFromEntry
set_option autoImplicit false
set_option maxRecDepth 16384
set_option maxHeartbeats 4000000
set_option Elab.async false
"""


def caller_push(image, selector, catalog):
    """Find the dispatch edge and the mint ABI entry's continuation producer."""
    decoded = decode_runtime(image, catalog)
    pcs = list(decoded)
    ops = list(decoded.values())
    entries = [
        entry[1]
        for tag, comparison, entry, jump in zip(ops, ops[1:], ops[2:], ops[3:])
        if tag == ("push4", selector)
        and comparison == ("eq", None)
        and entry[1] is not None
        and jump == ("jumpi", None)
    ]
    if len(entries) != 1 or decoded.get(entries[0]) != ("jumpdest", None):
        raise Unsupported("cannot identify the ABI dispatch destination")
    start = pcs.index(entries[0])
    end = next(
        (i for i in range(start + 1, len(pcs)) if ops[i][0] == "jumpdest"),
        len(pcs),
    )
    for i in range(start, end - 4):
        if ops[i : i + 4] == [
            ("push1", 36),
            ("calldataload", None),
            ("swap1", None),
            ("push0", 0),
        ]:
            target = ops[i + 4][1]
            if decoded.get(target) == ("jumpdest", None):
                return pcs[i + 4], target
    raise Unsupported("cannot identify the ABI caller's continuation PUSH")


def actions(image, pcs, producer, decoded, catalog):
    """Propose a common route. Lean decodes both byte images independently."""
    result = []
    remaining = iter(enumerate(pcs[:-1]))
    for index, pc in remaining:
        op, immediate = decoded[pc]
        if pc == producer:
            result.append(".saveReturn")
        elif immediate is not None:
            next_pc = pc + 1 + max(0, image[pc] - 0x5F)
            next_op = decoded[next_pc][0]
            if next_pc != pcs[index + 1] or next_op not in {"jumpi", "jump"}:
                result.append(f".push {immediate}")
            else:
                next(remaining)
                if next_op == "jumpi":
                    taken = pcs[index + 2] == immediate
                    result.append(f".guard {str(taken).lower()}")
        elif op == "jump":
            result.append(".resume")
        elif op.startswith("dup"):
            result.append(f".dup {image[pc] - 0x80}")
        elif op.startswith("swap"):
            result.append(f".swap {image[pc] - 0x8F}")
        elif op != "jumpdest":
            inputs, outputs = catalog[op]["stack"]
            result.append(f".op {image[pc]} {inputs} {outputs}")
    return "[" + ", ".join([*result, ".stop"]) + "]"


def replay_side(program, image, pc, executable, timeout):
    fixture = program.world
    assert fixture is not None
    raw = replace(program, world=world.with_runtime(fixture, image))
    options = {"executable": executable, "timeout": timeout}
    cut = json.loads(
        native.replay(raw, raw, [], probe_pc=pc, **options)["observations"][0]
    )
    receipt = json.loads(
        native.replay(raw, raw, [], raw_runtime=True, **options)["observations"][0]
    )
    if cut.get("pc") != pc or not cut.get("pcs") or receipt[0] != "success":
        raise Unsupported(
            "funded ABI scenario did not return to the encoded continuation"
        )
    return cut, receipt


def observation_expression(receipt):
    _, data, gas, refund, accounts, logs = receipt
    accounts = [
        f"⟨{address}, {nonce}, {balance}, {code}, "
        f"[{', '.join(f'({slot}, {value})' for slot, value in slots)}], [], false, false⟩"
        for address, nonce, balance, code, slots in accounts
    ]
    logs = [
        f"⟨{address}, [{', '.join(topics)}], {data}⟩" for address, topics, data in logs
    ]
    return f"⟨.success, {data}, {gas}, {refund}, [{', '.join(accounts)}], [{', '.join(logs)}]⟩"


def prove_side(prove, name, image, fixture, anchor, cut, receipt, timeout):
    producer, target = anchor
    jump = cut["pcs"][-1]
    source = (
        f"import {entry_proof.MODULE}\nimport SolarTV.EVMIR.Transforms.RelocatedReturn\n{entry_proof.HEADER}\n"
        + entry_proof.image_definitions(image)
        + fixture["configuration"]
        + f"""
def seed : List Execution.Account := {world.accounts_expression(fixture)}
def accounts := seed.map fun a =>
  if a.address = {fixture["address"]} then {{a with code := image}} else a
def checkpoint : Execution.Journal :=
  ⟨accounts, accounts, [{", ".join(world.warm_addresses(fixture, fixture["target"]))}],
    [{", ".join(f"({address},{key})" for address, key in fixture["warm_slots"])}], [], 0⟩
def env : World.Environment := {world.environment_expression(fixture)}
def physicalStart : ByteCut :=
  ⟨⟨env, image, 0, [], [], [], {fixture["gas"]}⟩, checkpoint, [], [], []⟩
def physicalWith (hash : List Execution.Byte → Word) :=
  chunkedByteStep {target} tvWorldConfig code hash
"""
    )
    entry_proof.success(prove(name, source, timeout, module=True), name)
    _, reached, _ = entry_proof.phase(
        prove, name, (("physical", "ByteCut", "ByteCut", fixture["fuel"]),), timeout
    )
    source = f"""import {reached[0]}
{entry_proof.HEADER}
theorem reachedInBytecode :
    Execution.Trace.run (byteStep {target} tvWorldConfig code.bytes) {fixture["fuel"]} physicalStart =
      .ok physicalCut := by
  rw [← chunkedByteStep_eq]
  simpa only [physicalStep, physicalWith, {name}Memo.hashChecked_eq] using physicalReached
def exit := Execution.Trace.run (chunkedByteExitStep tvWorldConfig code) 32 (byteExitStart physicalCut)
def observation := exit.toOption.map fun r =>
  EVM.State.observe tvWorldConfig .effects (finishByte checkpoint r.result)
theorem exitInBytecode : exit = runByteExit tvWorldConfig 32 physicalCut := by
  simp only [exit, runByteExit, chunkedByteExitStep_eq]
  rfl
def stack := physicalCut.frame.stack
def memory := physicalCut.frame.memory
def returnData := physicalCut.frame.returnData
def visited := physicalCut.pcs
def liveJournal := EVM.RelocatedReturn.withoutRootCode tvWorldConfig.root physicalCut.world
theorem returnAddress :
    (EVM.RelocatedReturn.decode code {producer}).bind (·.immediate) = some {target} ∧
    visited.contains {producer} = true ∧ visited.getLast? = some {jump} ∧
    code.get? {jump} = some 0x56 ∧ physicalCut.frame.pc = {target} := by decide +kernel
theorem matchedRustCut :
    stack = [{", ".join(cut["stack"])}] ∧ memory = {cut["memory"]} ∧
    returnData = {cut["returndata"]} ∧ visited = {cut["pcs"]} := by decide +kernel
theorem matchedRustExit : observation = some {observation_expression(receipt)} := by decide +kernel
#print axioms reachedInBytecode
#print axioms exitInBytecode
#print axioms returnAddress
#print axioms matchedRustCut
#print axioms matchedRustExit
"""
    module = f"{name}Exit"
    checked = entry_proof.success(prove(module, source, timeout, module=True), module)
    entry_proof.check_axioms(checked.splitlines(), 5)
    return module


def prove_pair(
    directory, pair, images, anchors, cuts, receipts, selector, catalog, timeout
):
    modules = []
    for name, program, image, anchor, cut, receipt in zip(
        ("Old", "New"), pair, images, anchors, cuts, receipts, strict=True
    ):
        prove = partial(entry_proof.lean, directory, namespace=name)
        modules.append(
            prove_side(
                prove,
                name + "Return",
                image,
                program.world,
                anchor,
                cut,
                receipt,
                timeout,
            )
        )
    before, after = [
        cut["pcs"] + [target, target + 1]
        for cut, (_, target) in zip(cuts, anchors, strict=True)
    ]
    old_push, old_return = anchors[0]
    new_push, new_return = anchors[1]
    source = (
        "import SolarTV.EVMIR.Transforms.RelocatedReturn\n"
        + entry_proof.import_modules(modules)
        + HEADER
    )
    if receipts[0] == receipts[1]:
        proposal = actions(
            images[0], before, old_push, decode_runtime(images[0], catalog), catalog
        )
        source += f"""
def cfg := Old.tvWorldConfig
def actions : List Action := {proposal}
theorem oldRoute : physicalRoute cfg Old.code {old_push} {before} = some ⟨{old_return}, actions⟩ := by decide +kernel
theorem newRoute : physicalRoute cfg New.code {new_push} {after} = some ⟨{new_return}, actions⟩ := by decide +kernel
theorem roles : usesReturn actions [] false false = true := by decide +kernel
theorem statefulRoute (env : World.Environment) (live : Live) :
    (run cfg Old.code {old_push} {before} (fundedHandler cfg env) live).map Prod.snd =
    (run cfg New.code {new_push} {after} (fundedHandler cfg env) live).map Prod.snd :=
  correspondence _ _ _ _ _ _ _ _ _ _ oldRoute newRoute roles _ _
theorem sameCut : Old.stack = New.stack ∧ Old.memory = New.memory ∧
    Old.returnData = New.returnData ∧ Old.liveJournal = New.liveJournal := by decide +kernel
theorem sameExit : Old.observation = New.observation := by decide +kernel
#print axioms statefulRoute
#print axioms sameCut
#print axioms sameExit
"""
        count = 3
    else:
        source += """
theorem differentExit : Old.observation ≠ New.observation := by decide +kernel
#print axioms differentExit
"""
        count = 1
    checked = entry_proof.success(
        entry_proof.lean(directory, "Compare", source, timeout),
        f"{selector} return comparison",
    )
    entry_proof.check_axioms(checked.splitlines(), count)


def check(paths, solar, executable, version, timeout, selector, world_path, replay):
    # This ABI producer is specific to the ERC20Mock mint path and both
    # continuations must be physically reached by independent execution.
    if selector.lower() != "0x40c10f19" or version != "cancun":
        raise Unsupported("--return-path supports the Cancun ERC20 mint ABI path")
    fixture = world.read(world_path, solar, None)
    if (
        fixture["symbols"]
        or bytes(fixture["calldata"][:4]).hex() != selector[2:].lower()
    ):
        raise Unsupported(
            "return-path replay needs concrete calldata for mint(address,uint256)"
        )
    fixture["configuration"] = world.configuration(
        solar, version, fixture, timeout, executable
    )
    fixture["target"] = version
    catalog = {op["name"]: op for op in native.catalog(executable, timeout)["evmir"]}
    if any(
        path.suffix != ".evmir" or path.stat().st_size > 1024 * 1024 for path in paths
    ):
        raise Unsupported("return path requires .evmir files of at most 1024 KiB")
    sources = [path.read_text() for path in paths]
    images = [
        world.runtime(source, "evmir", solar, version, timeout) for source in sources
    ]
    anchors = [caller_push(image, int(selector, 16), catalog) for image in images]
    pair = tuple(
        Program(
            ("world-entry", "evmir"),
            [],
            "",
            "",
            canonical_ir=source,
            selector=selector,
            evm_version=version,
            scope="return-path",
            world={**fixture, "image_sha256": hashlib.sha256(image).hexdigest()},
        )
        for source, image in zip(sources, images, strict=True)
    )
    cuts, receipts = zip(
        *(
            replay_side(program, image, target, executable, timeout)
            for program, image, (_, target) in zip(pair, images, anchors, strict=True)
        ),
        strict=True,
    )
    for program, (push, target), cut in zip(pair, anchors, cuts, strict=True):
        program.import_evidence = {
            "kind": "bytecode-return-path",
            "caller_push_pc": push,
            "continuation_pc": target,
            "indirect_jump_pc": cut["pcs"][-1],
            "executed_instructions": len(cut["pcs"]),
        }
    with tempfile.TemporaryDirectory(prefix="solar-tv-relocated-return-") as folder:
        try:
            prove_pair(
                Path(folder),
                pair,
                images,
                anchors,
                cuts,
                receipts,
                selector,
                catalog,
                timeout,
            )
        except (Unsupported, KeyError, IndexError, StopIteration) as error:
            raise Inconclusive(f"return-path certificate rejected: {error}") from error
    targets = f"continuation PC {anchors[0][1]} → {anchors[1][1]}"
    if receipts[0] != receipts[1]:
        before, after = receipts
        reason = report.difference(json.dumps(before), json.dumps(after))
        if before[1] != after[1]:
            reason += f" ({len(before[1])} → {len(after[1])} bytes)"
        return audit.Result(
            "counterexample",
            stderr=(
                f"counterexample: {selector} ({targets})\n"
                f"  {reason}\n"
                f"  Lean kernel and Rust replay agree\nreplay: {replay}\n"
            ),
            program=pair[0],
            pair=pair,
            witness={
                "calldata": bytes(fixture["calldata"]).hex(),
                "observations": list(receipts),
                "cuts": list(cuts),
            },
        )
    return audit.Result(
        "verified",
        stdout=f"verified: {selector} returns with the same live data and effects ({targets})\n",
        program=pair[0],
        pair=pair,
    )
