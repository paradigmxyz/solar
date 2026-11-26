"""Check the pinned FlashLoan helper's emitted returndata paths."""

import argparse
import hashlib
import itertools
import json
import subprocess
from pathlib import Path

from validator import audit, entry_proof, heap_entry, metered_prefix, native, world
from validator.call_tail import MASK

ENTRY = 3600
CONTINUATION = 6396
TOKEN = 13107
VERSION = "cancun"
CASES = {
    "Empty": ("00", 1, b"", CONTINUATION, 107, b""),
    "True": ("602a5f5560015f5260205ff3", 1, (1).to_bytes(32), CONTINUATION, 205, b""),
    "False": ("602a5f5560205ff3", 1, bytes(32), 302, 262, b"transfer returned false"),
    "Malformed": ("60025f5260205ff3", 1, (2).to_bytes(32), 116, 162, b""),
    "Short": ("60015f526001601ff3", 1, b"\x01", 116, 134, b""),
    "Reverted": (
        "602a5f5563deadbeef5f526004601cfd",
        0,
        bytes.fromhex("deadbeef"),
        302,
        149,
        b"transfer reverted",
    ),
}

COMMON = """
open EVM.State.ReturnBuffer

def flashTailLength (buffer : List Byte) : Word := BitVec.ofNat 256 buffer.length

def flashTailFrame (env : World.Environment) (buffer : List Byte) (pc : Nat)
    (stack : List Word) (memory : List Byte) : Frame :=
  {(decoderFrame env 0 pc stack []) with memory := memory, returnData := buffer}

theorem flashTailRound (buffer : List Byte) (fits : buffer.length ≤ 65248) :
    Evm.and (~~~(31 : Word)) (flashTailLength buffer + 63) =
      BitVec.ofNat 256 (32+32*words buffer.length) := by
  change (~~~(31 : Word) &&& (flashTailLength buffer + 63)) = _
  rw [BitVec.and_comm, flashTailLength, MIR.LowerReturndata.roundedMask _ (by omega)]
  congr 1
  unfold words
  omega

theorem flashTailBump (buffer : List Byte) :
    Evm.add 256 (BitVec.ofNat 256 (32+32*words buffer.length)) = cursorWord 256 buffer.length := by
  change BitVec.ofNat 256 256 + BitVec.ofNat 256 (32+32*words buffer.length) = _
  rw [← BitVec.ofNat_add]
  unfold cursorWord
  apply congrArg (BitVec.ofNat 256)
  omega

theorem flashTailCopyCovered (env : World.Environment) (buffer : List Byte)
    (fits : buffer.length ≤ 65248) :
    expand (header (flashMem5 env) 256 buffer.length) 288 buffer.length =
      .ok (active (flashMem5 env) 256 buffer.length) := by
  apply expand_at_cursor
  · simp
  · decide
  · unfold words
    omega

theorem flashTailCopyCharge (env : World.Environment) (buffer : List Byte) :
    memoryCharge (header (flashMem5 env) 256 buffer.length) 288 buffer.length =
      memoryCost (9 + words buffer.length) - memoryCost 9 := by
  rw [charge_at_cursor _ _ _ (by simp) (by decide)]

theorem flashTailHeader (env : World.Environment) (buffer : List Byte) :
    readWord (copied (flashMem5 env) buffer 256) 256 = flashTailLength buffer :=
  read_header _ _ _ (by simp)

theorem flashTailPayload (env : World.Environment) (buffer : List Byte)
    (full : 32 ≤ buffer.length) :
    readWord (copied (flashMem5 env) buffer 256) 288 = readWord buffer 0 :=
  read_payload_word _ _ _ (by simp) full

theorem flashTailLengthNat (buffer : List Byte) (fits : buffer.length ≤ 65248) :
    (flashTailLength buffer).toNat = buffer.length := by
  simp [flashTailLength, Nat.mod_eq_of_lt (show buffer.length < 2^256 by omega)]
"""

DYNAMIC = (
    "3*((buffer.length+31)/32)+(memoryCost (9+(buffer.length+31)/32)-memoryCost 9)"
)
LENGTH = "(flashTailLength buffer)"
FIRST = "(readWord buffer 0)"
FIT = "(fits : buffer.length ≤ 65248)"
PREMISES = {
    "Empty": [("empty", f"{LENGTH} = 0")],
    "Short": [
        ("nonempty", f"{LENGTH} ≠ 0"),
        ("short", f"Evm.sgt 32 {LENGTH} = 1"),
    ],
    "Reverted": [],
}
for route in ("True", "False", "Malformed"):
    PREMISES[route] = [
        ("nonempty", f"{LENGTH} ≠ 0"),
        ("long", f"Evm.sgt 32 {LENGTH} = 0"),
        ("full", "32 ≤ buffer.length"),
        ("canonical", f"Evm.lt 1 {FIRST} = {int(route == 'Malformed')}"),
    ]
    if route != "Malformed":
        PREMISES[route].append(
            ("boolean", f"Evm.lt 0 {FIRST} = {int(route == 'True')}")
        )


def frame(pc, stack, memory):
    return f"flashTailFrame env buffer {pc} [{', '.join(map(str, stack))}] ({memory})"


def op_result(op, a, b, route):
    if op == "add":
        if isinstance(a, int) and isinstance(b, int):
            return (a + b) & MASK, None
        if a == LENGTH or b == LENGTH:
            assert {a, b} == {LENGTH, 63}
            return f"({LENGTH}+63)", None
        assert {a, b} == {"BitVec.ofNat 256 (32+32*words buffer.length)", 256}
        return "(cursorWord 256 buffer.length)", "flashTailBump buffer"
    if op == "and":
        assert {a, b} == {f"({LENGTH}+63)", MASK - 31}
        return (
            "BitVec.ofNat 256 (32+32*words buffer.length)",
            "flashTailRound buffer fits",
        )
    if op == "shl":
        return (b << a) & MASK, None
    if op == "lt":
        assert b == FIRST and int(a) in (0, 1)
        if a == 1:
            return int(route == "Malformed"), "canonical"
        return int(route == "True"), "boolean"
    assert op == "sgt" and a == 32 and b == LENGTH
    return int(route == "Short"), "short" if route == "Short" else "long"


def setup_debit(image, catalog, route, pc, following, stack, memory, binder):
    before = frame(pc, stack, memory)
    opcode = image[pc]
    row = catalog[opcode]
    op = row["name"]
    schema = row["forks"][VERSION]
    charge = schema["base_gas"]
    name = f"flash{route}Step{pc}"
    after_memory = memory
    definitions = ""
    extra = ""
    facts = ["schema", "unpaid"]
    finish = "(first | rfl | assumption)"
    if 0x60 <= opcode <= 0x7F:
        width = opcode - 0x5F
        immediate = int.from_bytes(image[pc + 1 : pc + 1 + width], "big")
        stack.insert(0, immediate)
        extra += f"  have literal : code.immediate {pc} {width} = {immediate} := by decide +kernel\n"
        facts.append("literal")
    elif op == "push0":
        stack.insert(0, 0)
    elif op.startswith("dup"):
        stack.insert(0, stack[int(op[3:]) - 1])
    elif op.startswith("swap"):
        depth = int(op[4:])
        stack[0], stack[depth] = stack[depth], stack[0]
    elif op == "pop":
        stack.pop(0)
    elif op == "returndatasize":
        stack.insert(0, LENGTH)
    elif op == "iszero":
        assert isinstance(stack[0], int)
        stack[0] = int(stack[0] == 0)
    elif op == "not":
        stack[0] ^= MASK
    elif op in {"add", "and", "shl", "lt", "sgt"}:
        result, fact = op_result(op, *stack[:2], route)
        stack[:2] = [result]
        if fact:
            facts.append(fact)
            finish = f"(first | rfl | exact {fact})"
    elif op in {"jump", "jumpi"}:
        target = stack.pop(0)
        if op == "jumpi":
            condition = stack.pop(0)
            if condition == LENGTH:
                assert route != "Reverted"
                facts.extend(
                    ["empty" if route == "Empty" else "nonempty", "beq_iff_eq"]
                )
            else:
                assert bool(condition) == (following == target)
        if following == target:
            extra += f"  have valid : {target} ∈ code.jumps 0 := by decide +kernel\n"
            facts.append("valid")
    elif op in {"mload", "mstore", "returndatacopy"}:
        address = stack.pop(0)
        count = "buffer.length" if op == "returndatacopy" else 32
        if op == "returndatacopy":
            assert stack[:2] == [0, LENGTH] and address == 288
            del stack[:2]
            assert memory.startswith("header ")
            extra += "  have expanded := flashTailCopyCovered env buffer fits\n"
            extra += "  have growth := flashTailCopyCharge env buffer\n"
            after_memory = "copied (flashMem5 env) buffer 256"
            charge = f"{charge}+{DYNAMIC}"
            facts.extend(
                [
                    "flashTailLengthNat buffer fits",
                    "readBytes_entire",
                    "copied",
                    "words",
                ]
            )
        elif op == "mstore" and address == 256:
            assert stack.pop(0) == LENGTH and memory.startswith("bumped ")
            after_memory = "header (flashMem5 env) 256 buffer.length"
            extra += """  have expanded : expand (bumped (flashMem5 env) 256 buffer.length) 256 32 =
      .ok (bumped (flashMem5 env) 256 buffer.length ++ List.replicate 32 0) :=
    expand_at_cursor _ _ _ (by simp) (by decide) (by decide)
  have growth : memoryCharge (bumped (flashMem5 env) 256 buffer.length) 256 32 = 3 := by
    rw [charge_at_cursor _ _ _ (by simp) (by decide)]
    rfl
"""
            charge += 3
            facts.extend(["header", "flashTailLength"])
        else:
            mem_length = f"({memory}).length"
            extra += f"""  have aligned : {mem_length} % 32 = 0 := by simp <;> omega
  have limit : {mem_length} ≤ 65536 := by simp [words] <;> omega
  have covered : {address}+{count} ≤ {mem_length} := by simp [words] <;> omega
  have expanded := expand_covered ({memory}) {address} {count} aligned limit covered
  have growth := charge_covered ({memory}) {address} {count} aligned covered
"""
            if op == "mload":
                if address == 256:
                    stack.insert(0, LENGTH)
                    facts.append("flashTailHeader")
                else:
                    assert address == 288
                    stack.insert(0, FIRST)
                    facts.append("flashTailPayload env buffer full")
            else:
                word = stack.pop(0)
                if memory == "flashMem5 env":
                    assert address == 64 and word == "(cursorWord 256 buffer.length)"
                    after_memory = "bumped (flashMem5 env) 256 buffer.length"
                    facts.append("bumped")
                else:
                    scratch = f"flash{route}Scratch{pc}"
                    after_memory = f"{scratch} env buffer"
                    definitions = f"""
def {scratch} (env : World.Environment) (buffer : List Byte) : List Byte :=
  writeBytes ({memory}) {address} (wordBytes {word})

@[simp] theorem {scratch}Length (env : World.Environment) (buffer : List Byte) :
    ({after_memory}).length = ({memory}).length := by
  simp only [{scratch}, writeBytes_length]
"""
                    facts.append(scratch)
        facts.extend(["expanded", "growth", "withMemory", "Bind.bind", "Except.bind"])
    else:
        assert op == "jumpdest", (pc, op)

    after = frame(following, stack, after_memory)
    output = definitions + metered_prefix.debit(
        name, binder, before, after, opcode, charge
    )
    fields = (
        schema["available"],
        schema["base_gas"],
        schema["dynamic_gas"],
        *row["stack"],
    )
    schema = ",".join(str(item).lower() for item in fields)
    output += f"  have schema : available tvWorldConfig {opcode} = ⟨{schema}⟩ := rfl\n"
    output += extra
    output += f"""  by_cases unpaid : balance < ({charge}) <;>
    simp [flashTailFrame, decoderFrame, instructionWithImage, dispatch_size, next,
      EVM.step, EVM.Context.ofEnvironment, pureOp, Evm.iszero,
      Fork.atLeast, Fork.ordinal, {",".join(facts)}] <;> {finish}
"""
    return output, metered_prefix.Debit(name, pc, opcode, charge, after), after_memory


def route_source(image, catalog, route, pcs, flag, base, reason):
    guards = PREMISES[route]
    binders = " ".join(f"({name} : {guard})" for name, guard in guards)
    binder = f"(env : World.Environment) (journal : Journal) (buffer : List Byte) {FIT} {binders}"
    args = "env journal buffer fits " + " ".join(name for name, _ in guards)
    stack = [
        flag,
        6396,
        "(readWord env.input 4)",
        256,
        160,
        "(readWord env.input 4)",
        132,
        "(readWord env.input 100)",
        "(readWord env.input 36)",
        3760401218,
    ]
    memory = "flashMem5 env"
    begin = frame(ENTRY, stack, memory)
    source, debits, scratch = [], [], []
    for pc, following in itertools.pairwise(pcs):
        step, debit, memory = setup_debit(
            image, catalog, route, pc, following, stack, memory, binder
        )
        if (
            memory.startswith(f"flash{route}Scratch")
            and memory.split()[0] not in scratch
        ):
            scratch.append(memory.split()[0])
        source.append(step)
        debits.append(debit)
    ending = frame(pcs[-1], stack, memory)
    charge = f"{base + 3}+{DYNAMIC}"
    prefix = f"flash{route}Tail"
    source.append(
        metered_prefix.certify(prefix, binder, args, charge, begin, ending, debits)
    )
    source.append(f"""
theorem {prefix}Frame (env : World.Environment) (sample : Nat)
    (child : MessageOutcome) (flag : child.value = {flag}) :
    flashResume env sample child = {{({begin.replace("env buffer", "env child.data")}) with
      gas := flashAvailable sample / 64 + child.gas}} := by
  have actual : flashResume env sample child = {{
    ({begin.replace("env buffer", "env child.data").replace(f"[{flag},", "[child.value,")}) with
      gas := flashAvailable sample / 64 + child.gas}} := rfl
  rw [actual, flag]

theorem {prefix}Unpaid {binder} (work balance : Nat) (shortGas : balance < ({charge})) :
    ∃ result, run (work+{len(debits)}) tvWorldConfig journal
      {{({begin}) with gas := balance}} = .ok result ∧
      result.stop = .outOfGas ∧ result.data = [] ∧ result.gas = 0 ∧ result.work ≤ {len(debits)} :=
  ({prefix} {args}).unpaid work balance shortGas

#print axioms {prefix}Unpaid
""")
    if pcs[-1] != CONTINUATION:
        offset, count = stack[:2]
        assert (offset, count) == ((28, 100) if reason else (0, 0))
        error = (
            f'MIR.TransferFromReturn.revertError (copied (flashMem5 env) buffer 256) "{reason.decode()}"'
            if reason
            else "[]"
        )
        if reason:
            message = reason.decode()
            reason_word = int.from_bytes(reason.ljust(32, b"\0"), "big")
            binding = f"""  have length : "{message}".utf8ByteSize = {len(reason)} := by decide +kernel
  have contents : MIR.TransferFromReturn.reasonWord "{message}" = {reason_word} := by decide +kernel
  have payload : readBytes ({memory}) {offset} {count} = {error} := by
    simp only [MIR.TransferFromReturn.revertError, length, contents, {", ".join(scratch)}] <;> rfl
"""
            bytes_fact = "payload"
        else:
            binding = ""
            bytes_fact = "readBytes_zero"
        source.append(f"""
set_option maxHeartbeats 800000 in
theorem {prefix}Halt {binder} (balance : Nat) :
    instruction tvWorldConfig journal {{({ending}) with gas := balance}} =
      .ok (.done ⟨.reverted,{error},journal,balance,0⟩) := by
  have found : code.get? {pcs[-1]} = some 253 := by decide +kernel
  have schema : available tvWorldConfig 253 = ⟨true,0,0,2,0⟩ := rfl
  have aligned : ({memory}).length % 32 = 0 := by simp <;> omega
  have limit : ({memory}).length ≤ 65536 := by simp [words] <;> omega
  have covered : {offset}+{count} ≤ ({memory}).length := by simp <;> omega
  have expanded := expand_covered ({memory}) {offset} {count} aligned limit covered
  have growth := charge_covered ({memory}) {offset} {count} aligned covered
{binding}  have step := code.instruction_eq tvWorldConfig journal {{({ending}) with gas := balance}}
  simp only [flashTailFrame, decoderFrame, found, Option.getD_some] at step ⊢
  rw [step]
  simp [instructionWithImage, dispatch_size, schema, pureOp, withMemory, expanded, growth,
    Bind.bind, Except.bind, {bytes_fact}] <;> rfl

theorem {prefix}Reverts {binder} (work balance : Nat) (enough : ({charge}) ≤ balance) :
    run (work+1+{len(debits)}) tvWorldConfig journal {{({begin}) with gas := balance}} =
      .ok ⟨.reverted,{error},journal,balance-({charge}),1+{len(debits)}⟩ :=
  ({prefix} {args}).paid_halt (fun gas => ⟨.reverted,{error},journal,gas,0⟩)
    ({prefix}Halt {args}) work balance enough

#print axioms {prefix}Reverts
""")
    else:
        source.append(f"""
theorem {prefix}Returns {binder} (work balance : Nat) (enough : ({charge}) ≤ balance) :
    run (work+{len(debits)}) tvWorldConfig journal {{({begin}) with gas := balance}} = (do
      let outcome ← run work tvWorldConfig journal {{({ending}) with gas := balance-({charge})}}
      return {{outcome with work := outcome.work+{len(debits)}}}) :=
  ({prefix} {args}).paid work balance enough

#print axioms {prefix}Returns
""")
    child_args = args.replace("env journal buffer", "env child.world child.data")
    child_binders = (
        f"(env : World.Environment) (sample : Nat) (child : MessageOutcome) "
        f"(flag : child.value = {flag}) (data : child.data = buffer) {FIT} {binders}"
    )
    balance = "(flashAvailable sample / 64 + child.gas)"
    if pcs[-1] != CONTINUATION:
        paid_work = f"work+1+{len(debits)}"
        observation = (
            f".ok ⟨.reverted,{error},child.world,{balance}-({charge}),1+{len(debits)}⟩"
        )
        theorem = f"{prefix}Reverts"
    else:
        paid_work = f"work+{len(debits)}"
        observation = f"""(do
      let outcome ← run work tvWorldConfig child.world
        {{({ending}) with gas := {balance}-({charge})}}
      return {{outcome with work := outcome.work+{len(debits)}}})"""
        theorem = f"{prefix}Returns"
    source.append(f"""
theorem {prefix}FromChild (buffer : List Byte) {child_binders} (work : Nat)
    (enough : ({charge}) ≤ {balance}) :
    run ({paid_work}) tvWorldConfig child.world (flashResume env sample child) = {observation} := by
  subst buffer
  rw [{prefix}Frame env sample child flag]
  exact {theorem} {child_args} work {balance} enough

#print axioms {prefix}FromChild
""")
    return "".join(source), len(debits), ending


def token_account(fixture, code):
    return {
        **fixture,
        "accounts": [
            {**a, "code": list(bytes.fromhex(code))}
            if int(a["address"]) == TOKEN
            else a
            for a in fixture["accounts"]
        ],
    }


def expected_error(reason):
    if not reason:
        return b""
    return (
        bytes.fromhex("08c379a0")
        + (32).to_bytes(32)
        + len(reason).to_bytes(32)
        + reason.ljust(32, b"\0")
    )


def digest(blob):
    return hashlib.sha256(blob).hexdigest()


def import_source(base):
    return (
        f"""import {base}
import SolarTV.EVMIR.Transforms.ReturnBuffer
import SolarTV.MIR.Transforms.TransferFromReturn
open SolarTV EvmRules Execution EVM.State
open MIR.TransferFromABI
set_option autoImplicit false
set_option maxRecDepth 16384
set_option maxHeartbeats 4000000
set_option Elab.async false
set_option linter.unusedVariables false
set_option linter.unusedSimpArgs false
"""
        + COMMON
    )


def kernel(directory, name, source, count, *, module=False):
    log = directory / f"{name}.log"
    try:
        result = entry_proof.lean(directory, name, source, 180, module=module)
    except subprocess.TimeoutExpired as error:
        log.write_text((error.stdout or "") + (error.stderr or ""))
        raise
    log.write_text(result.stdout + result.stderr)
    output = entry_proof.success(result, name)
    entry_proof.check_axioms(output.splitlines(), count)
    return digest(source.encode())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--world", type=Path, required=True)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--replay", type=Path, required=True)
    parser.add_argument("--prior-proof", type=Path, required=True)
    parser.add_argument("--prior-receipt", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--route", choices=CASES)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    baseline = json.loads(args.baseline.read_text())["contracts"]["src/Morpho.sol"][
        "Morpho"
    ]
    image = bytes.fromhex(baseline["evm"]["deployedBytecode"]["object"])
    prior = json.loads(args.prior_receipt.read_text())
    assert prior["status"] == "verified"
    assert prior["runtime_sha256"] == digest(image)
    assert prior["source_sha256"] == digest(args.prior_proof.read_bytes())
    replay = str(args.replay.resolve())
    assert prior["replay_sha256"] == digest(args.replay.read_bytes())
    catalog = {op["opcode"]: op for op in native.catalog(replay, 30)["evmir"]}
    fixture = world.read(args.world, str(args.compiler.resolve()), "exact")
    assert prior["fixture_sha256"] == digest(args.world.read_bytes())
    directory = args.output.resolve()
    prior_source = args.prior_proof.read_text()
    base = "CheckedFlashLoanCall"
    print("Checking the first CALL theorem in Lean", flush=True)
    base_hash = kernel(directory, base, prior_source, 16, module=True)
    common_source = import_source(base)
    evidence, route_sources = {}, {}
    for name, (code, flag, buffer, stop, fixed_cost, reason) in CASES.items():
        if args.route is not None and name != args.route:
            continue
        state = token_account(fixture, code)
        resumed = heap_entry.observe(image, state, VERSION, replay, 30, pc=ENTRY)
        reached = heap_entry.observe(image, state, VERSION, replay, 30, pc=stop)
        assert (
            int(resumed["stack"][0]) == flag and bytes(resumed["returndata"]) == buffer
        )
        words = (len(buffer) + 31) // 32
        tail_cost = (
            fixed_cost
            + 3
            + 3 * words
            + (3 * (9 + words) + (9 + words) ** 2 // 512 - 27)
        )
        assert resumed["gas"] - reached["gas"] == tail_cost
        full = heap_entry.observe(image, state, VERSION, replay, 30)
        if stop == CONTINUATION:
            assert full[:2] == ["success", []] and len(full[5]) == 1
            assert reached["stack"] == ["13107", "132", "0", "1", "3760401218"]
            assert int.from_bytes(bytes(reached["memory"][64:96])) == 288 + 32 * words
            assert bytes(reached["memory"][288 : 288 + len(buffer)]) == buffer
        else:
            assert full[:3] == ["revert", list(expected_error(reason)), reached["gas"]]
            assert not full[5]
        token = next(account for account in full[4] if int(account[0]) == TOKEN)
        assert token[4] == ([["0", "42"]] if name == "True" else [])

        # Isolate the tail's budget from a state write's EIP-150 forwarding.
        affordable = token_account(fixture, code.removeprefix("602a5f55"))
        funded = heap_entry.observe(image, affordable, VERSION, replay, 30, pc=stop)
        minimum = affordable["gas"] - funded["gas"]
        exact = heap_entry.observe(
            image, {**affordable, "gas": minimum}, VERSION, replay, 30, pc=stop
        )
        assert exact["gas"] == 0 and exact["memory"] == funded["memory"]
        unpaid = heap_entry.observe(
            image, {**affordable, "gas": minimum - 1}, VERSION, replay, 30
        )
        assert unpaid[:3] == ["out-of-gas", [], 0] and not unpaid[5]
        pcs = reached["pcs"][reached["pcs"].index(ENTRY) :] + [stop]
        generated, steps, _ = route_source(
            image, catalog, name, pcs, flag, fixed_cost, reason
        )
        print(f"Checking the {name.lower()} continuation in Lean", flush=True)
        route_source_hash = kernel(
            directory, f"FlashLoanTransferReturn{name}", common_source + generated, 3
        )
        route_sources[name] = generated
        evidence[name] = {
            "source_sha256": route_source_hash,
            "steps": steps,
            "cost": tail_cost,
            "resumed": resumed,
            "reached": reached,
            "full": full,
            "minimum": minimum,
            "exact": exact,
            "unpaid": unpaid,
        }
    name = "FlashLoanTransferReturn" + (args.route or "")
    mutation = None
    maximum = None
    if args.route is None:
        length = 65248
        large_token = f"60015f5261{length:04x}5ff3"
        state = token_account(fixture, large_token)
        after_child = heap_entry.observe(image, state, VERSION, replay, 30, pc=ENTRY)
        after_return = heap_entry.observe(
            image, state, VERSION, replay, 30, pc=CONTINUATION
        )
        words = (length + 31) // 32
        copy_and_growth = 3 * words + (3 * (9 + words) + (9 + words) ** 2 // 512 - 27)
        assert len(after_child["returndata"]) == length
        assert len(after_return["memory"]) == 65536
        assert after_child["gas"] - after_return["gas"] == 208 + copy_and_growth
        assert after_return["memory"][288:] == after_child["returndata"]
        maximum = {"length": length, "tail_cost": 208 + copy_and_growth}

        print("Checking the changed bool guard in Lean", flush=True)
        offset = 8120
        assert image[offset - 1 : offset + 1] == bytes.fromhex("6001")
        changed = image[:offset] + b"\x02" + image[offset + 1 :]
        malformed = token_account(fixture, CASES["Malformed"][0])
        accepted = heap_entry.observe(
            changed, malformed, VERSION, replay, 30, pc=CONTINUATION
        )
        assert accepted["stack"] == evidence["True"]["reached"]["stack"]
        changed_prior = prior_source.replace(
            entry_proof.image_definitions(image),
            entry_proof.image_definitions(changed),
            1,
        )
        assert changed_prior != prior_source
        changed_base = base + "WrongBool"
        changed_base_hash = kernel(
            directory, changed_base, changed_prior, 16, module=True
        )
        changed_source = import_source(changed_base) + route_sources["Malformed"]
        wrong = entry_proof.lean(directory, name + "WrongBool", changed_source, 180)
        (directory / f"{name}WrongBool.log").write_text(wrong.stdout + wrong.stderr)
        assert wrong.returncode != 0 and "code.immediate 8119 1 = 1" in wrong.stdout
        mutation = {
            "offset": offset,
            "base_sha256": changed_base_hash,
            "runtime_sha256": digest(changed),
            "source_sha256": digest(changed_source.encode()),
            "accepted": accepted,
        }

    (args.output / f"{name}.json").write_text(
        json.dumps(
            {
                "status": "verified",
                "runtime_sha256": digest(image),
                "call_proof_sha256": base_hash,
                "replay_sha256": digest(args.replay.read_bytes()),
                "semantics": audit.semantics(),
                "driver_sha256": digest(Path(__file__).read_bytes()),
                "routes": evidence,
                "maximum_returndata": maximum,
                "mutation": mutation,
            },
            indent=2,
        )
        + "\n"
    )
    print(
        f"{name}: kernel and independent Rust replay passed for {len(evidence)} routes",
        flush=True,
    )


if __name__ == "__main__":
    main()
