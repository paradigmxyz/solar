"""Validate Morpho's emitted CALL and returndata continuation against MIR."""

import argparse
import json
import re
import shutil
import subprocess
import sys
from dataclasses import asdict
from pathlib import Path
from tempfile import TemporaryDirectory

from . import (
    audit,
    builtin_lowering,
    call_tail,
    caller_replay,
    callers,
    evaluate,
    native,
    prune,
    regions,
)
from .check import PRELUDE, ROOT, build_model, run
from .common import Unsupported

FUNCTION = "safeTransferFrom"
SELECTOR = 0x23B872DD << 224
VERSION = "cancun"
TOKEN = 0x40000
ENTRY = ["return", "token", "from", "to", "value"]
WITNESS_GAS = 100000
CHILD_SAMPLES = (95572, 95569)
WARM_CHILD_SAMPLES = (98033, 98029)


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


def decode_runtime(code, catalog):
    """Read opcode boundaries and PUSH bytes from the final deployed image."""
    names = {definition["opcode"]: name for name, definition in catalog.items()}
    result = {}
    pc = 0
    while pc < len(code):
        opcode = code[pc]
        width = max(0, opcode - 0x5F) if 0x60 <= opcode <= 0x7F else 0
        next_pc = pc + 1 + width
        require(next_pc <= len(code), "truncated final bytecode PUSH")
        immediate = (
            int.from_bytes(code[pc + 1 : next_pc], "big")
            if width or opcode == 0x5F
            else None
        )
        result[pc] = (names.get(opcode, "unknown"), immediate)
        pc = next_pc
    return result


def extract_final(code, scheduled_setup, catalog):
    """Match the resolved byte range to the same typed transferFrom setup."""
    decoded = decode_runtime(code, catalog)
    expected = list(map(ir_operation, scheduled_setup))
    selector = expected.index(SELECTOR)
    expected[selector : selector + 1] = [SELECTOR >> 224, 224, "shl"]
    points = list(decoded)
    candidates = []
    for i, pc in enumerate(points):
        start = [decoded[p] for p in points[i : i + 6]]
        if (
            len(start) != 6
            or [op for op, _ in start[:4]]
            != ["jumpdest", "dup2", "extcodesize", "iszero"]
            or not start[4][0].startswith("push")
            or start[5] != ("jumpi", None)
        ):
            continue
        end = i + 6 + len(expected)
        tail = [decoded[p] for p in points[i + 6 : end]]
        if [op if value is None else value for op, value in tail] == expected:
            candidates.append((pc, points[end - 1], points[i + 4], start + tail))
    require(
        len(candidates) == 1, "cannot bind a unique final-bytecode transferFrom CALL"
    )
    entry, call, branch_push, ops = candidates[0]
    failure = ops[4][1]
    require(
        [decoded.get(failure + offset) for offset in (0, 1, 9, 11, 12)]
        == [
            ("jumpdest", None),
            ("push7", int.from_bytes(b"no code", "big")),
            ("push1", 200),
            ("shl", None),
            ("push1", 7),
        ],
        "final bytecode does not lead to the no-code exit",
    )
    return entry, call, branch_push, failure, ops


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
    # The checked guard temporarily pushes the token and its branch target.
    peak_depth = len(stack) + 2
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
        elif op == "shl":
            shift, value = stack[:2]
            require(
                isinstance(shift, int) and 0 <= shift < 256 and isinstance(value, int),
                "expected a constant selector shift",
            )
            stack[:2] = [(value << shift) % 2**256]
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
        peak_depth = max(peak_depth, len(stack))
    require(until_sample is not None, "missing scheduled GAS observation")
    # Cancun's warm access fees are also checked in CallBudget.warmCall.
    trace = guard, probe["base_gas"], 100, accesses, until_sample, pending, 100
    return trace, effects, peak_depth, stack[7:]


def location_expression(address):
    return f".fixed {address}" if isinstance(address, int) else f".heap {address[1]}"


def memory_expression(effects):
    def storage(value):
        if isinstance(value, int):
            return f".literal {value}"
        if isinstance(value, tuple):
            return f".heap {value[1]}"
        return {"from": ".sender", "to": ".receiver", "value": ".amount"}[value]

    def access(op, address, *args):
        if op == "load":
            return ".loadFmp"
        return f".store ({location_expression(address)}) ({storage(args[0])})"

    return "[" + ", ".join(access(*effect) for effect in effects) + "]"


def expression(trace):
    guard, cold, warm, accesses, sample, tail, call = trace
    rows = []
    for price, address, size in accesses:
        location = location_expression(address)
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
    with TemporaryDirectory(prefix="solar-tv-morpho-") as temporary:
        captures = Path(temporary)
        captured = subprocess.run(
            [
                solar,
                "--standard-json",
                "-j1",
                "-Zdump=evm-ir-runtime,evm-ir-runtime-scheduled",
                f"-Zsolar-tv={captures}",
                "-Zsolar-tv-capture",
            ],
            input=json.dumps(source),
            capture_output=True,
            text=True,
            check=True,
            timeout=timeout,
        )
        selected = [
            row["paths"]
            for row in evaluate.captures(captures)
            if evaluate.final_call_case(case, row)
        ]
        require(
            len(selected) == 1, "missing or ambiguous captured transferFrom MIR pair"
        )
        mir_pair = tuple(directory / f"{side}.mir" for side in ("before", "after"))
        for source_path, destination in zip(selected[0], mir_pair, strict=True):
            shutil.copyfile(source_path, destination)
    dump, artifact = captured.stdout.rsplit("\n{", 1)
    value = json.loads("{" + artifact)
    require(
        evaluate.digest(evaluate.stable(value["contracts"])) == original,
        "scheduled capture changed compiler output",
    )
    parts = re.split(r"(?m)^; === .*Morpho \((runtime|scheduled runtime)\) ===\n", dump)
    require(len(parts) == 5, "missing optimized or scheduled Morpho runtime")
    modules = {parts[i]: parts[i + 1].rstrip() + "\n" for i in (1, 3)}
    finalized, scheduled = modules["runtime"], modules["scheduled runtime"]
    (directory / "finalized.evmir").write_text(finalized)
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
    final = bytes.fromhex(
        value["contracts"][case["source"]][case["contract"]]["evm"]["deployedBytecode"][
            "object"
        ]
    )
    (directory / "runtime.bin").write_text(final.hex() + "\n")
    return scheduled, finalized, assembled.stdout, original, final, mir_pair


def prove(scheduled, trace, directory, timeout):
    module = "SolarTV.EVMIR.Transforms.CallBudget"
    model = build_model(timeout, module)
    require(model.returncode == 0, model.stdout + model.stderr)
    proof = directory / "CallBudget.lean"
    proof.write_text(
        PRELUDE.replace("import SolarTV\n", f"import {module}\n", 1)
        + f"""
open SolarTV.EVM.CallBudget
def schedule : Trace := {expression(scheduled)}
theorem scheduledShape : schedule = referenceTrace := by decide
def imported : Trace := {expression(trace)}
theorem finalShape : imported = referenceTrace 6 := by decide
theorem morphoBudget (entry : Entry) (heap : 128 ≤ entry.fmp) :
    run imported entry = reference entry 6 := by
  rw [finalShape]
  exact exact entry heap 6
theorem morphoFrame (cfg : EVM.State.Config) (journal : Execution.Journal)
    (frame : EVM.State.Frame) (token : Execution.Address) (input : List Execution.Byte)
    (fork : cfg.fork = .cancun)
    (heap : 128 ≤ (Entry.ofFrame journal frame token).fmp) :
    run imported (Entry.ofFrame journal frame token) = reference (Entry.ofFrame journal frame token) 6 ∧
    EVM.State.callBoundaryCharge cfg (Execution.warmAddress journal token)
      ⟨.call,token,0,input⟩ 0xf1 0 (Execution.memoryWords frame.memory)
        (Execution.memoryWords frame.memory) = 100 :=
  ⟨morphoBudget _ heap, warmCall _ _ _ _ _ fork⟩
#print axioms morphoFrame
theorem morphoForward (entry : Entry) (heap : 128 ≤ entry.fmp)
    (sample available child retained words elapsed : Nat)
    (reaches : run imported entry = .call sample available child retained words)
    (earlier : elapsed ≤ beforeCall entry 6) :
    Calls.forwardGas .cancun available (entry.gas-elapsed) false = some ⟨child,child⟩ :=
  earlierRequest entry 6 sample available child retained words elapsed
    ((morphoBudget entry heap).symm.trans reaches) earlier
-- The replay wrapper spends 43 gas and enters with the FMP word active.
theorem replayBudget :
    run imported ⟨{WITNESS_GAS - 43},128,3,false,true⟩ = .call 97191 97091 95574 1517 9 ∧
    run {{imported with afterSample := 4}} ⟨{WITNESS_GAS - 43},128,3,false,true⟩ =
      .call 97191 97087 95571 1516 9 := by decide
theorem warmReplayBudget :
    run imported ⟨{WITNESS_GAS - 43},128,3,true,true⟩ = .call 99691 99591 98035 1556 9 ∧
    run {{imported with afterSample := 4}} ⟨{WITNESS_GAS - 43},128,3,true,true⟩ =
      .call 99691 99587 98031 1556 9 := by decide
#print axioms warmReplayBudget
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
    for theorem in (
        "morphoBudget",
        "morphoFrame",
        "morphoForward",
        "replayBudget",
        "warmReplayBudget",
    ):
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


def replay_image(code, location, *, extra=b"", initialize=None, continuation=None):
    """Use the emitted bytes with a test trampoline and returndata observer.

    Preserve the conditional PUSH width and relocate its operand to a local
    revert. On the successful path only the optional mutation changes the
    prefix. The original scheduled and final images are never written here.
    """
    entry, call, branch, _, ops = location
    if initialize is None:
        initialize = bytes.fromhex("6080604052")
    if continuation is None:
        continuation = bytes.fromhex("f13d5f5f3e3d5ff3")
    trampoline = initialize + bytes.fromhex("60016202000162020000620400005f")
    trampoline += b"\x61" + (len(trampoline) + 4).to_bytes(2, "big") + b"\x56"
    prefix = bytearray(code[entry:call])
    failure = len(trampoline) + len(prefix) + len(extra) + len(continuation)
    width = int(ops[4][0][4:])
    require(
        width > 0 and failure < 256**width, "replay branch cannot keep its PUSH width"
    )
    offset = branch - entry + 1
    prefix[offset : offset + width] = failure.to_bytes(width, "big")
    return trampoline + prefix + extra + continuation + bytes.fromhex("5b5f5ffd")


def replay_pair(
    images,
    directory,
    solar,
    replay,
    timeout,
    *,
    warm=False,
    gas=False,
    token_code="5a5f52365f60203760845ff3",
    preserve_status=False,
):
    directory.mkdir(exist_ok=True)

    def wrapper(address):
        if preserve_status:
            observe = """  push 0
  mstore
  returndatasize
  push 0
  push 32
  returndatacopy
  returndatasize
  push 32
  add
  push 0
  return
"""
        else:
            observe = """  pop
  returndatasize
  push 0
  push 0
  returndatacopy
  returndatasize
  push 0
  return
"""
        return f"""@module TransferFromBudget
bb0:
  push 0
  push 0
  push 0
  push 0
  push 0
  push {address}
  push {WITNESS_GAS}
  call
{observe}"""

    left, right = directory / "before.evmir", directory / "broken.evmir"
    left.write_text(wrapper(0x50000))
    right.write_text(wrapper(0x50001))
    fixture = directory / "world.json"
    fixture.write_text(
        json.dumps(
            {
                "version": 1,
                "gas": 2 * WITNESS_GAS,
                "warm_addresses": [TOKEN] if warm else [],
                "accounts": [
                    {
                        "address": 0x50000,
                        "nonce": 1,
                        "code": "0x" + images[0].hex(),
                    },
                    {
                        "address": 0x50001,
                        "nonce": 1,
                        "code": "0x" + images[1].hex(),
                    },
                    {
                        "address": TOKEN,
                        "nonce": 1,
                        "code": "0x" + token_code,
                    },
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
            *(["--gas", "exact"] if gas else []),
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
        "missing independent CALL replay counterexample\n"
        + result.stdout
        + result.stderr,
    )
    record = json.loads(report.read_text())
    observed = record["witness"]
    require(
        record["evidence"] == "kernel-and-rust-replay"
        and observed["observations"] == observed["native_observations"],
        "CALL counterexample did not replay independently",
    )
    return observed


def witness(code, location, directory, solar, replay, timeout, *, warm=False):
    observed = replay_pair(
        [
            replay_image(code, location),
            replay_image(code, location, extra=bytes.fromhex("5f50")),
        ],
        directory,
        solar,
        replay,
        timeout,
        warm=warm,
        gas=True,
    )
    samples = []
    payload = (SELECTOR >> 224).to_bytes(4, "big") + b"".join(
        value.to_bytes(32, "big") for value in (0x20000, 0x20001, 1)
    )
    for status, data, *_ in observed["observations"]:
        require(
            status == "success" and len(data) == 132 and bytes(data[32:]) == payload,
            "child did not return its GAS and exact transferFrom calldata",
        )
        samples.append(int.from_bytes(bytes(data[:32]), "big"))
    # replayBudget predicts the forwarded budget. GAS itself costs two.
    require(
        tuple(samples) == (WARM_CHILD_SAMPLES if warm else CHILD_SAMPLES),
        "child GAS disagrees with the priced schedule",
    )
    return samples


def frame_witness(code, location, directory, solar, replay, timeout):
    """Observe the full active memory at CALL, including padding and caller bytes."""
    # Set FMP to 192, dirty the tail of its future object, and keep a live
    # caller sentinel beyond it. The prefix does not have to grow memory here.
    initialize = bytes.fromhex(
        "60c0604052"  # mstore(64, 192)
        "60775f52"  # mstore(0, 0x77)
        "5f1961012c52"  # mstore(300, ~0)
        "609961016052"  # mstore(352, 0x99)
    )
    continuation = bytes.fromhex("595ff3")  # return(0, msize())

    def image(runtime):
        return replay_image(
            runtime, location, initialize=initialize, continuation=continuation
        )

    start, end = location[:2]
    prefix = code[start:end]
    require(prefix.count(b"\x60\xa0") == 1, "cannot locate the FMP bump for replay")
    immediate = start + prefix.index(b"\x60\xa0") + 1
    broken = bytearray(code)
    broken[immediate] = 128
    witness = replay_pair(
        [image(code), image(broken)], directory, solar, replay, timeout
    )

    expected = bytearray(384)
    expected[31] = 0x77
    expected[95] = 0x60
    expected[94] = 1  # FMP is 192 + 160 = 352.
    expected[223] = 100
    expected[224:324] = (SELECTOR >> 224).to_bytes(4, "big") + b"".join(
        value.to_bytes(32, "big") for value in (0x20000, 0x20001, 1)
    )
    expected[324:332] = bytes([0xFF] * 8)
    expected[383] = 0x99
    reject = expected.copy()
    reject[95] = 0x40  # Mutant bumped by 128 instead of 160.
    for (status, data, *_), memory in zip(
        witness["observations"], [expected, reject], strict=True
    ):
        require(
            status == "success" and bytes(data) == memory,
            "independent replay disagrees with the prepared helper memory",
        )
    return {"memory_bytes": len(expected), "fmp": 352, "broken_fmp": 320}


def tail_witness(
    code,
    decoded,
    location,
    tail,
    resume_stack,
    catalog,
    directory,
    solar,
    replay,
    timeout,
):
    image, call = call_tail.replay_image(code, decoded, location, tail)
    relocated = call_tail.extract(
        decode_runtime(image, catalog), call + 1, resume_stack, catalog, VERSION
    )
    require(
        relocated.prices == tail.prices,
        "replay relocation changed the continuation price",
    )

    def variants(left, right):
        # The token selects an outcome using the caller address of each run.
        header = bytes.fromhex("33620500011461")
        target = len(header) + 3 + len(left)
        return (
            header + target.to_bytes(2, "big") + b"\x57" + left + b"\x5b" + right
        ).hex()

    def reason(text):
        return (
            bytes.fromhex("08c379a0")
            + word(32)
            + word(len(text))
            + text.ljust(32, b"\0")
        )

    def word(value):
        return value.to_bytes(32, "big")

    def memory(flag):
        start = 288
        length = 32 if flag else 0
        # The original 100-byte input object is followed by the copied result.
        expected = bytearray(start + 32 + length)
        expected[64:96] = word(len(expected))
        expected[128:160] = word(100)
        expected[160:260] = (SELECTOR >> 224).to_bytes(4, "big") + b"".join(
            map(word, (0x20000, 0x20001, 1))
        )
        expected[start : start + 32] = word(length)
        if flag:
            expected[-32:] = word(1)
        return bytes(expected)

    cases = (
        (
            "success-revert",
            bytes.fromhex("602a5f5560015f5260205ff3"),
            bytes.fromhex("602a5f5563deadbeef5f526004601cfd"),
            ((1, memory(True), 42), (0, reason(call_tail.FAILED), 0)),
        ),
        (
            "empty-malformed",
            bytes.fromhex("00"),
            bytes.fromhex("60025f5260205ff3"),
            ((1, memory(False), 0), (0, b"", 0)),
        ),
        (
            "short-false",
            bytes.fromhex("60015f526001601ff3"),
            bytes.fromhex("602a5f5560205ff3"),
            ((0, b"", 0), (0, reason(call_tail.FALSE), 0)),
        ),
    )
    directory.mkdir(exist_ok=True)
    for name, left, right, expected in cases:
        observed = replay_pair(
            [image, image],
            directory / name,
            solar,
            replay,
            timeout,
            token_code=variants(left, right),
            preserve_status=True,
            gas=True,
        )
        for side, (flag, data, slot) in zip(
            observed["observations"], expected, strict=True
        ):
            status, result, _, _, accounts, _ = side
            (token,) = (account for account in accounts if int(account[0]) == TOKEN)
            require(
                status == "success"
                and bytes(result) == word(flag) + data
                and token[4] == ([["0", str(slot)]] if slot else []),
                f"{name} replay disagrees on helper status, returndata or storage",
            )
    return [case[0] for case in cases]


def main(arguments):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--solar", default=str(ROOT.parent / "target/debug/solar"))
    parser.add_argument("--replay")
    parser.add_argument(
        "--mir-pair",
        nargs=2,
        type=Path,
        metavar=("BEFORE", "AFTER"),
        help="match an existing MIR pair against the new capture",
    )
    parser.add_argument("--timeout", type=int, default=60)
    args = parser.parse_args(arguments)
    solar = str(Path(shutil.which(args.solar) or args.solar).resolve())
    replay = str(Path(args.replay or native.default(solar)).resolve())
    directory = args.output.resolve()
    try:
        directory.mkdir()
        semantics = audit.semantics()
        scheduled, finalized, disassembly, bytecode, final, mir_pair = capture(
            solar, directory, args.timeout
        )
        if args.mir_pair:
            require(
                all(
                    prune.declarations(candidate.read_text())
                    == prune.declarations(captured.read_text())
                    for candidate, captured in zip(args.mir_pair, mir_pair, strict=True)
                ),
                "supplied MIR pair differs from the same-compile transferFrom capture",
            )
            mir_pair = args.mir_pair
        entry, failure, setup, ops = extract(scheduled, disassembly)
        catalog = {
            row["name"]: row for row in native.catalog(replay, args.timeout)["evmir"]
        }
        location = extract_final(final, setup, catalog)
        trace, effects, prefix_depth, resume_stack = project(location[-1], catalog)
        scheduled_trace, _, _, _ = project(ops, catalog)
        decoded = decode_runtime(final, catalog)
        tail = call_tail.extract(
            decoded, location[1] + 1, resume_stack, catalog, VERSION
        )
        sites = callers.extract(scheduled, finalized, decoded, entry, catalog, VERSION)
        extra_stack = max(prefix_depth, tail.peak_depth) - len(ENTRY)
        prove(scheduled_trace, trace, directory, args.timeout)
        child_samples = witness(final, location, directory, solar, replay, args.timeout)
        warm_samples = witness(
            final, location, directory / "warm", solar, replay, args.timeout, warm=True
        )
        frame = frame_witness(
            final, location, directory / "frame", solar, replay, args.timeout
        )
        tail_replays = tail_witness(
            final,
            decoded,
            location,
            tail,
            resume_stack,
            catalog,
            directory / "continuation",
            solar,
            replay,
            args.timeout,
        )
        caller_receipts = caller_replay.replay(
            final, directory / "callers", solar, replay, args.timeout
        )
        artifact = {
            "callers": [asdict(site) for site in sites],
            "runtime_sha256": evaluate.digest(final),
            "entry_pc": location[0],
            "call_pc": location[1],
            "additional_stack_slots": extra_stack,
            "continuation_pc": location[1] + 1,
        }
        mir = builtin_lowering.check(
            mir_pair,
            solar,
            replay,
            VERSION,
            args.timeout,
            FUNCTION,
            final_call={
                "callers": sites,
                "trace": expression(trace),
                "memory": memory_expression(effects),
                "tail_prices": "⟨"
                + ", ".join(map(str, [*tail.prices, tail.copy_gas]))
                + "⟩",
                "artifact": artifact,
                "proof": directory / "MIRCallBudget.lean",
            },
        )
        audit.write(
            directory / "mir.json",
            audit.record(
                mir,
                layer="mir",
                target=VERSION,
                policy=None,
                before=mir_pair[0],
                after=mir_pair[1],
                model=semantics,
            ),
        )
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
                "scope": "Cancun transferFrom entry through the five owning caller exits",
                "callers": artifact["callers"],
                "caller_replays": caller_receipts,
                "tail_fixed_gas": dict(zip(call_tail.ROUTES, tail.prices, strict=True)),
                "returndatacopy_word_gas": tail.copy_gas,
                "runtime_sha256": evaluate.digest(final),
                "entry_pc": location[0],
                "call_pc": location[1],
                "no_code_pc": location[3],
                "additional_stack_slots": extra_stack,
                "assumptions": [
                    "shared helper-entry arguments, environment, initial gas, journal, byte memory and returndata",
                    "the entry stack has room for the decoded prefix's peak temporary depth",
                    "a live FMP slot at 0x40, heap at least 128, and room for the 160-byte object in word-aligned memory within 64 KiB",
                    "Cancun, at most 30 million entry gas, and the token's code is available to the MIR model",
                    "final bytes independently decoded; operand-exact projection and scheduled function binding are trusted",
                ],
                "child_gas_samples": child_samples,
                "warm_child_gas_samples": warm_samples,
                "frame_replay": frame,
                "continuation_replays": tail_replays,
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
    print(f"verified Morpho {FUNCTION} through CALL and five caller exits")
    print(
        f"counterexample: child GAS {child_samples[0]} -> {child_samples[1]} (Rust replay matched)"
    )
    print(f"continuation replay: {', '.join(tail_replays)} (Lean and Rust matched)")
    print(
        f"counterexample: prepared FMP {frame['fmp']} -> {frame['broken_fmp']} "
        "(full memory replay matched)"
    )
    return 0
