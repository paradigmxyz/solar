"""Price the pinned Morpho transferFrom CALL from its scheduled runtime IR."""

import argparse
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

from . import audit, evaluate, native, regions
from .check import PRELUDE, ROOT, build_model, run
from .common import Unsupported

FUNCTION = "safeTransferFrom"
SELECTOR = 0x23B872DD << 224
VERSION = "cancun"
TOKEN = 0x40000
ENTRY = ["return", "token", "from", "to", "value"]
WITNESS_GAS = 100000
CHILD_SAMPLES = (95578, 95574)


def require(condition, reason):
    if not condition:
        raise Unsupported(reason)


def assembled_operation(line):
    op, _, operand = line.lower().partition(" ")
    if op.startswith("push"):
        require(
            op == "push0" or re.fullmatch(r"push(?:[1-9]|[12][0-9]|3[0-2])", op),
            "invalid assembled push",
        )
        return op, 0 if op == "push0" else int(operand, 16)
    return op, None


def ir_operation(instruction):
    op, _, operand = instruction.partition(" ")
    if op == "push":
        return int(operand, 0)
    return op + operand


def extract(scheduled, disassembly):
    """Bind the selected helper to the physically priced fallthrough.

    The assembler's disassembly supplies the real PUSH widths, JUMPDEST and
    fallthrough choice. Keep the conditional branch and verify its no-code
    destination. Only the continuation after the CALL is outside this region.
    """
    entries = re.findall(
        rf"(?m)^(bb\d+) \[ret=0, inputs=5, outputs=0\]:\s*; function {FUNCTION}\(",
        scheduled,
    )
    require(len(entries) == 1, "cannot select the scheduled transferFrom helper")
    entry = entries[0]
    blocks = regions.blocks(scheduled)
    guard = blocks[entry]
    require(
        len(guard) == 6
        and guard[:3] == ["dup 2", "extcodesize", "iszero"]
        and guard[4] == "jumpi"
        and re.fullmatch(r"push bb\d+", guard[3])
        and re.fullmatch(r"jump bb\d+", guard[5]),
        "unexpected transferFrom guard",
    )
    failure, success = guard[3][5:], guard[5][5:]
    require(success in blocks and failure in blocks, "missing transferFrom successor")
    setup = blocks[success]
    require(setup.count("call") == 1, "cannot select the transferFrom CALL")
    setup = setup[: setup.index("call") + 1]

    instructions = [
        op for line in disassembly.splitlines() if (op := line.split(";", 1)[0].strip())
    ]
    anchors = [
        i
        for i, line in enumerate(instructions)
        if line.lower() == f"push32 0x{SELECTOR:064x}"
    ]
    require(len(anchors) == 1, "cannot bind the assembled transferFrom selector")
    anchor = anchors[0]
    start = max((i for i in range(anchor) if instructions[i] == "JUMPDEST"), default=-1)
    require(start >= 0, "assembled transferFrom entry lacks JUMPDEST")
    rows = instructions[start : start + 6 + len(setup)]
    ops = [assembled_operation(row) for row in rows]
    require(
        len(ops) == 6 + len(setup)
        and [op for op, _ in ops[:6]][:4]
        == ["jumpdest", "dup2", "extcodesize", "iszero"]
        and ops[4][0].startswith("push")
        and ops[4][1] != 0
        and ops[5] == ("jumpi", None),
        "assembled transferFrom guard or layout changed",
    )
    actual = [op if value is None else value for op, value in ops[6:]]
    require(
        actual == list(map(ir_operation, setup)), "scheduled CALL and assembly disagree"
    )

    # Check the assembled taken target. The disassembler labels jumps and
    # prints their destinations with the resolved physical program counter.
    pc = 0
    offsets = {}
    for row in instructions:
        op, arg = assembled_operation(row)
        offsets[pc] = (op, arg)
        pc += 1 + (int(op[4:]) if op.startswith("push") else 0)
    failure_pc = ops[4][1]
    fallback = blocks[failure]
    require(
        len(fallback) == 3
        and fallback[0].startswith("push ")
        and fallback[1] == "push 7"
        and fallback[2].startswith("jump ")
        and offsets.get(failure_pc) == ("jumpdest", None)
        and offsets.get(failure_pc + 1) == ("push32", ir_operation(fallback[0]))
        and offsets.get(failure_pc + 34) == ("push1", 7),
        "assembled no-code exit changed",
    )
    return entry, failure, setup, ops


def add(left, right):
    if isinstance(left, int):
        left, right = right, left
    require(
        left[0] == "heap" and isinstance(right, int),
        "unexpected CALL address arithmetic",
    )
    return "heap", left[1] + right


def project(ops, catalog):
    """Trace symbolic stack operands and keep every physical gas charge."""
    guard = 0
    for opcode, _ in ops[:6]:
        if opcode != "extcodesize":
            guard += catalog[opcode]["forks"][VERSION]["base_gas"]
    probe = catalog["extcodesize"]["forks"][VERSION]
    stack = ENTRY.copy()
    accesses, effects = [], []
    pending, until_sample = 0, None
    for op, immediate in ops[6:]:
        definition = catalog[op]["forks"][VERSION]
        require(definition["available"], "scheduled CALL opcode unavailable")
        if op != "call":
            pending += definition["base_gas"]
        if immediate is not None:
            stack.insert(0, immediate)
        elif op.startswith("dup"):
            stack.insert(0, stack[int(op[3:]) - 1])
        elif op.startswith("swap"):
            depth = int(op[4:])
            stack[0], stack[depth] = stack[depth], stack[0]
        elif op == "pop":
            stack.pop(0)
        elif op == "add":
            stack[:2] = [add(*stack[:2])]
        elif op in {"mload", "mstore"}:
            require(until_sample is None, "memory effect after GAS is unmodeled")
            address = stack.pop(0)
            accesses.append((pending, address, 32))
            pending = 0
            if op == "mload":
                require(address == 64, "expected FMP load at 0x40")
                effects.append(("load", address))
                stack.insert(0, ("heap", 0))
            else:
                effects.append(("store", address, stack.pop(0)))
        elif op == "gas":
            require(until_sample is None, "unexpected second GAS")
            until_sample, pending = pending, 0
            stack.insert(0, "sample")
        elif op == "call":
            require(
                stack[:7] == ["sample", "token", 0, ("heap", 32), 100, 0, 0],
                "scheduled CALL does not match the typed message",
            )
        else:
            raise Unsupported(f"unmodeled CALL setup instruction {op}")
    require(until_sample is not None, "missing scheduled GAS observation")
    require(
        effects
        == [
            ("load", 64),
            ("store", 64, ("heap", 160)),
            ("store", ("heap", 0), 100),
            ("store", ("heap", 32), SELECTOR),
            ("store", ("heap", 36), "from"),
            ("store", ("heap", 68), "to"),
            ("store", ("heap", 100), "value"),
        ],
        "scheduled memory writes do not encode transferFrom",
    )
    # Cancun's warm access fees are also checked in CallBudget.warmCall.
    return guard, probe["base_gas"], 100, accesses, until_sample, pending, 100


def expression(trace):
    guard, cold, warm, accesses, sample, tail, call = trace
    rows = []
    for price, address, size in accesses:
        location = (
            f".fixed {address}" if isinstance(address, int) else f".heap {address[1]}"
        )
        rows.append(f"⟨{price}, {location}, {size}⟩")
    return f"⟨{guard}, {cold}, {warm}, [{', '.join(rows)}], {sample}, {tail}, {call}⟩"


def capture(solar, directory, timeout):
    case = next(
        row
        for row in json.loads((ROOT / "evaluation.json").read_text())["cases"]
        if row["name"] == "morpho-blue"
    )
    source = evaluate.inputs(case)
    original, _ = evaluate.compile_input(solar, source, directory, timeout=timeout)
    captured = subprocess.run(
        [solar, "--standard-json", "-j1", "-Zdump=evm-ir-runtime-scheduled"],
        input=json.dumps(source),
        capture_output=True,
        text=True,
        check=True,
        timeout=timeout,
    )
    dump, artifact = captured.stdout.rsplit("\n{", 1)
    value = json.loads("{" + artifact)
    require(
        evaluate.digest(evaluate.stable(value["contracts"])) == original,
        "scheduled capture changed compiler output",
    )
    scheduled = dump.split("@module Morpho_runtime\n", 1)[1]
    scheduled = "@module Morpho_runtime\n" + scheduled.rstrip() + "\n"
    path = directory / "scheduled.evmir"
    path.write_text(scheduled)
    assembled = run(
        [
            solar,
            path,
            "-j1",
            "-Onone",
            f"--evm-version={VERSION}",
            "-Zevm-ir-pipeline=none",
            "-Zvalidate-ir=true",
            "-Zdump=disasm-runtime",
            "--allow=2264",
        ],
        ROOT,
        timeout,
    )
    require(assembled.returncode == 0, "scheduled assembly failed\n" + assembled.stderr)
    (directory / "scheduled.disasm").write_text(assembled.stdout)
    return scheduled, assembled.stdout, original


def prove(trace, directory, timeout):
    module = "SolarTV.EVMIR.Transforms.CallBudget"
    model = build_model(timeout, module)
    require(model.returncode == 0, model.stdout + model.stderr)
    proof = directory / "CallBudget.lean"
    proof.write_text(
        PRELUDE.replace("import SolarTV\n", f"import {module}\n", 1)
        + f"""
open SolarTV.EVM.CallBudget
def imported : Trace := {expression(trace)}
theorem morphoBudget (entry : Entry) (heap : 128 ≤ entry.fmp) :
    run imported entry = reference entry := by
  have shape : imported = referenceTrace := by decide
  rw [shape]
  exact exact entry heap
theorem morphoFrame (cfg : EVM.State.Config) (journal : Execution.Journal)
    (frame : EVM.State.Frame) (token : Execution.Address) (input : List Execution.Byte)
    (fork : cfg.fork = .cancun)
    (heap : 128 ≤ (Entry.ofFrame journal frame token).fmp) :
    run imported (Entry.ofFrame journal frame token) = reference (Entry.ofFrame journal frame token) ∧
    EVM.State.callBoundaryCharge cfg (Execution.warmAddress journal token)
      ⟨.call,token,0,input⟩ 0xf1 0 (Execution.memoryWords frame.memory)
        (Execution.memoryWords frame.memory) = 100 :=
  ⟨morphoBudget _ heap, warmCall _ _ _ _ _ fork⟩
#print axioms morphoFrame
theorem morphoForward (entry : Entry) (heap : 128 ≤ entry.fmp)
    (sample available child retained words elapsed : Nat)
    (reaches : run imported entry = .call sample available child retained words)
    (earlier : elapsed ≤ beforeCall entry) :
    Calls.forwardGas .cancun available (entry.gas-elapsed) false = some ⟨child,child⟩ :=
  earlierRequest entry sample available child retained words elapsed
    ((morphoBudget entry heap).symm.trans reaches) earlier
-- The replay wrapper spends 43 gas and enters with the FMP word active.
theorem replayBudget :
    run imported ⟨{WITNESS_GAS - 43},128,3,false,true⟩ = .call 97197 97097 95580 1517 9 ∧
    run {{imported with afterSample := 4}} ⟨{WITNESS_GAS - 43},128,3,false,true⟩ =
      .call 97197 97093 95576 1517 9 := by decide
#print axioms replayBudget
#print axioms morphoForward
#print axioms morphoBudget
"""
    )
    result = run(["lake", "env", "lean", proof.resolve()], ROOT, timeout)
    require(
        result.returncode == 0,
        "exact scheduled CALL proof failed\n" + result.stdout + result.stderr,
    )
    for theorem in ("morphoBudget", "morphoFrame", "morphoForward", "replayBudget"):
        axioms = re.search(rf"'{theorem}' depends on axioms: \[(.*?)\]", result.stdout)
        require(
            f"'{theorem}' does not depend on any axioms" in result.stdout
            or axioms
            and not set(axioms[1].split(", "))
            - {"propext", "Quot.sound", "Classical.choice"},
            "unexpected axioms in scheduled CALL proof\n"
            + result.stdout
            + result.stderr,
        )


def witness(setup, directory, solar, replay, timeout):
    prologue = f"""@module TransferFromBudget
bb0:
  push 128
  push 64
  mstore
  push 1
  push 0x20001
  push 0x20000
  push {TOKEN}
  push 0
  jump bb1
bb90:
  invalid
bb1:
  dup 2
  extcodesize
  iszero
  push bb3
  jumpi
  jump bb2
bb2:
"""
    continuation = """
  returndatasize
  push 0
  push 0
  returndatacopy
  returndatasize
  push 0
  return
bb3:
  push 0
  push 0
  revert
"""
    before = prologue + "\n".join(f"  {op}" for op in setup) + continuation
    require(
        before.count("  gas\n  call") == 1,
        "cannot place the extra charged instructions",
    )
    after = before.replace("  gas\n  call", "  gas\n  push 0\n  pop\n  call")
    left, right = directory / "before.evmir", directory / "broken.evmir"
    left.write_text(before)
    right.write_text(after)
    fixture = directory / "world.json"
    # The child returns its gas sample, so the witness checks forwarding as
    # well as the caller's exact remaining gas.
    fixture.write_text(
        json.dumps(
            {
                "version": 1,
                "gas": WITNESS_GAS,
                "accounts": [
                    {"address": TOKEN, "nonce": 1, "code": "0x5a5f5260205ff3"}
                ],
            }
        )
    )
    report = directory / "counterexample.json"
    result = run(
        [
            ROOT / "solar-tv",
            "evmir",
            left.resolve(),
            right.resolve(),
            "--world",
            fixture.resolve(),
            "--gas",
            "exact",
            "--evm-version",
            VERSION,
            "--solar",
            solar,
            "--replay",
            replay,
            "--timeout",
            str(timeout),
            "--report",
            report.resolve(),
        ],
        ROOT,
        timeout * 4,
    )
    require(
        result.returncode == 1,
        "missing independent CALL counterexample\n" + result.stdout + result.stderr,
    )
    record = json.loads(report.read_text())
    observed = record["witness"]
    require(
        record["evidence"] == "kernel-and-rust-replay"
        and observed["observations"] == observed["native_observations"],
        "CALL counterexample did not replay independently",
    )
    samples = []
    for status, data, *_ in observed["observations"]:
        require(status == "success" and len(data) == 32, "child did not return its GAS")
        samples.append(int.from_bytes(bytes(data), "big"))
    # replayBudget predicts the forwarded budget. GAS itself costs two.
    require(
        tuple(samples) == CHILD_SAMPLES, "child GAS disagrees with the priced schedule"
    )
    return samples


def main(arguments):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--solar", default=str(ROOT.parent / "target/debug/solar"))
    parser.add_argument("--replay")
    parser.add_argument("--timeout", type=int, default=60)
    args = parser.parse_args(arguments)
    solar = str(Path(shutil.which(args.solar) or args.solar).resolve())
    replay = str(Path(args.replay or native.default(solar)).resolve())
    directory = args.output.resolve()
    try:
        directory.mkdir()
        semantics = audit.semantics()
        scheduled, disassembly, bytecode = capture(solar, directory, args.timeout)
        entry, failure, setup, ops = extract(scheduled, disassembly)
        catalog = {
            row["name"]: row for row in native.catalog(replay, args.timeout)["evmir"]
        }
        trace = project(ops, catalog)
        prove(trace, directory, args.timeout)
        child_samples = witness(setup, directory, solar, replay, args.timeout)
        require(
            semantics == audit.semantics(),
            "validator semantics changed during the proof",
        )
        audit.write(
            directory / "result.json",
            {
                "version": 1,
                "status": "verified",
                "semantics": semantics,
                "schedule_sha256": evaluate.digest(scheduled.encode()),
                "disassembly_sha256": evaluate.digest(disassembly.encode()),
                "compiler_output_sha256": bytecode,
                "function": FUNCTION,
                "entry": entry,
                "no_code_exit": failure,
                "gas_policy": "exact",
                "scope": "Cancun scheduled function entry to CALL or no-code transfer",
                "assumptions": [
                    "the same well-formed five-word function-entry stack, initial gas, journal and byte memory",
                    "aligned FMP in the word at 0x40, at least 128, with room for the 160-byte object in the model's 64 KiB domain",
                    "gas fits in an EVM word and memory length is measured in active words",
                    "trusted compiler disassembly and operand-exact projection of the scheduled, assembled prefix",
                ],
                "child_gas_samples": child_samples,
            },
        )
    except (
        OSError,
        ValueError,
        KeyError,
        IndexError,
        subprocess.SubprocessError,
    ) as error:
        print(f"inconclusive: {error}", file=sys.stderr)
        return 2
    print(f"verified Morpho {FUNCTION} exact CALL budget")
    print(
        f"counterexample: child GAS {child_samples[0]} -> {child_samples[1]} (Rust replay matched)"
    )
    return 0
