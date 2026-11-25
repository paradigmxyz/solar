"""Bind typed signing-envelope allocation to the captured physical encoding."""

from dataclasses import dataclass
from itertools import batched

from . import entry_proof
from .callers import require
from .check import build_model

MODULE = "SolarTV.MIR.Transforms.PackingBoundary"
NAMESPACE = "MorphoPacking"
DOMAIN = "signingDomain"
SIGNING_TAG = 0x1901
BASE = "address mem"
DIGEST = "digest"
MARKER = "signingPrefix"


@dataclass
class Step:
    pc: int
    opcode: int
    immediate: int | None
    schema: tuple
    gas: int
    stack: list
    memory: str
    access: str | None


@dataclass
class Packing:
    steps: list[Step]
    next_pc: int
    stack: list
    gas: int
    domain: int


def plan(runtime, decoded, sample, catalog):
    points = [pc for pc in decoded if sample - 200 < pc < sample]
    starts = [
        points[i + 1]
        for i in range(len(points) - 3)
        if decoded[points[i]][0] == "keccak256"
        and decoded[points[i + 1]][0] == "push32"
        and decoded[points[i + 2]] == ("push1", 64)
        and decoded[points[i + 3]] == ("mload", None)
    ]
    require(len(starts) == 1, "expected a unique emitted signing domain relocation")
    start = starts[0]
    end = next(pc for pc in points if pc > start and decoded[pc] == ("keccak256", None))
    region = [pc for pc in points if start <= pc < end]
    expected = [
        "push32",
        "push1",
        "mload",
        "dup8",
        "dup2",
        "add",
        "push1",
        "mstore",
        "push1",
        "dup2",
        "mstore",
        "push1",
        "dup2",
        "add",
        "swap2",
        "push2",
        "push1",
        "shl",
        "dup4",
        "mstore",
        "push1",
        "dup3",
        "add",
        "mstore",
        "push1",
        "add",
        "swap2",
        "swap1",
        "swap2",
        "mstore",
        "push1",
        "swap1",
    ]
    require(
        [decoded[pc][0] for pc in region] == expected,
        "different emitted packing schedule",
    )
    domain = decoded[start][1]
    stack = [DIGEST, "x1", "x2", "x3", "x4", 128]
    memory = "mem"
    gas = 0
    rows = []
    for pc in region:
        op, imm = decoded[pc]
        price = catalog[op]["forks"]["cancun"]
        schema = (
            price["available"],
            price["base_gas"],
            price["dynamic_gas"],
            *(catalog[op]["stack"] or (0, 0)),
        )
        require(schema[0], "unavailable signing opcode")
        access = None
        row = Step(pc, runtime[pc], imm, schema, gas, stack.copy(), memory, None)
        rows.append(row)
        gas += price["base_gas"]
        if imm is not None:
            stack.insert(0, DOMAIN if pc == start else imm)
        elif op.startswith("dup"):
            stack.insert(0, stack[int(op[3:]) - 1])
        elif op.startswith("swap"):
            i = int(op[4:])
            stack[0], stack[i] = stack[i], stack[0]
        elif op == "mload":
            require(stack[0] == 64 and memory == "mem", "unexpected signing FMP load")
            stack[0] = BASE
            access = "64"
        elif op == "mstore":
            destination, value = stack[:2]
            if memory == "mem":
                require(
                    (destination, value) == (64, f"{BASE}+128"),
                    "wrong signing FMP bump",
                )
                memory, access = "bump mem", "64"
            elif memory == "bump mem":
                require((destination, value) == (BASE, 66), "wrong signing header")
                memory, access = "header mem", f"({BASE}).toNat"
            elif memory == "header mem":
                require(
                    (destination, value) == (f"{BASE}+32", MARKER),
                    "wrong signing marker",
                )
                memory, access = "marker mem", f"({BASE}).toNat+32"
            elif memory == "marker mem":
                require(
                    (destination, value) == (f"{BASE}+34", DOMAIN),
                    "wrong signing domain store",
                )
                memory, access = f"withDomain mem {DOMAIN}", f"({BASE}).toNat+34"
            else:
                require(
                    (destination, value) == (f"{BASE}+66", DIGEST), "wrong digest store"
                )
                memory, access = f"withDigest mem {DOMAIN} digest", f"({BASE}).toNat+66"
            del stack[:2]
        elif op == "add":
            left, right = stack[:2]
            offset = right if left == BASE else left
            require(
                BASE in (left, right) and isinstance(offset, int),
                "unexpected signing address",
            )
            stack[:2] = [f"{BASE}+{offset}"]
        elif op == "shl":
            require(stack[:2] == [240, SIGNING_TAG], "unexpected signing marker shift")
            stack[:2] = [MARKER]
        row.access = access
    require(
        stack[:2] == [f"{BASE}+32", 66] and memory.startswith("withDigest"),
        "wrong signing exit",
    )
    return Packing(rows, end, stack, gas, domain)


def lean_stack(values):
    return (
        "["
        + ", ".join(str(v) if isinstance(v, int) else f"({v})" for v in values)
        + "] ++ rest"
    )


def lean_frame(pc, gas, stack, memory):
    return f"packingFrame env ({gas}) {pc} ({lean_stack(stack)}) ({memory}) returndata"


BINDERS = """(env : World.Environment) (journal : Execution.Journal) (mem returndata : List Execution.Byte)
    (digest x1 x2 x3 x4 : Word) (rest : List Word) (surplus : Nat)
    (active : MIR.PackingBoundary.Active mem) (fits : rest.length ≤ {stack_room})"""
ARGS = "env journal mem returndata digest x1 x2 x3 x4 rest surplus active fits"


def fragments(layout):
    stack_room = 1024 - max(len(step.stack) for step in layout.steps)
    binders = BINDERS.format(stack_room=stack_room)
    yield f"""
def signingStart : Nat := {layout.steps[0].pc}
def signingExit : Nat := {layout.next_pc}
def signingDomain : Word := {layout.domain}
def signingSteps : Nat := {len(layout.steps)}
def signingFee : Nat := {layout.gas}

def packingFrame (env : World.Environment) (gas pc : Nat) (stack : List Word)
    (memory returndata : List Execution.Byte) : EVM.State.Frame :=
  ⟨env,code.bytes,pc,stack,memory,returndata,gas⟩
"""
    for index, row in enumerate(layout.steps):
        next_row = layout.steps[index + 1] if index + 1 < len(layout.steps) else None
        left = layout.gas - row.gas
        following = layout.gas - next_row.gas if next_row else 0
        before = lean_frame(row.pc, f"surplus+{left}", row.stack, row.memory)
        after = lean_frame(
            next_row.pc if next_row else layout.next_pc,
            f"surplus+{following}",
            next_row.stack if next_row else layout.stack,
            next_row.memory if next_row else f"withDigest mem {DOMAIN} digest",
        )
        schema = ", ".join(str(x).lower() for x in row.schema)
        out = f"""
theorem signingStep{index} {binders} :
    EVM.State.instruction tvWorldConfig journal ({before}) = .ok (.next journal ({after})) := by
  have opcode : code.get? {row.pc} = some {row.opcode} := by decide +kernel
  have reachable : {row.pc} < code.bytes.length := by
    rw [EVM.State.CodeChunks.size_eq]
    decide +kernel
  have schema : EVM.State.available tvWorldConfig {row.opcode} = ⟨{schema}⟩ := rfl
  have paid : ¬ surplus+{left} < {left - following} := by omega
  have spent : surplus+{left}-{left - following} = surplus+{following} := by omega
  have room : ¬ 1024 < rest.length+{len(row.stack) - row.schema[3] + row.schema[4]} := by omega
  have enough : ¬ rest.length+{len(row.stack)} < {row.schema[3]} := by omega
"""
        facts = [
            "schema",
            "paid",
            "spent",
            "room",
            "enough",
            "Bind.bind",
            "Except.bind",
        ]
        if row.immediate is not None:
            width = row.opcode - 0x5F
            if width:
                immediate = DOMAIN if index == 0 else row.immediate
                out += f"  have value : code.immediate {row.pc} {width} = {immediate} := by decide +kernel\n"
                facts.append("value")
        if row.access:
            out += f"""  have extent := active.extent
  have minimum := active.valid
  have memory := memoryWrite mem ({row.memory}) active (by simp) ({row.access}) (by omega)
  have expansion := memory.1
  have charge := memory.2
  try simp only [bump,header,marker,withDomain,withDigest] at expansion charge
"""
            facts += [
                "EVM.State.withMemory",
                "memory.1",
                "memory.2",
                "Execution.wordBytes_length",
            ]
            if row.opcode == 0x51:
                facts += ["address"]
            else:
                offset = row.access.rsplit("+", 1)[-1]
                if offset in ("32", "34", "66"):
                    out += f"""  have addr : ((address mem)+{offset}).toNat = (address mem).toNat+{offset} :=
    addAddress mem active {offset} (by decide)
  have within : (address mem).toNat+{offset} < 2^256 := by have bound := active.limit; omega
  have nowrap := Nat.mod_eq_of_lt within
  simp only [Nat.reducePow] at nowrap
"""
                    facts += ["addr", "nowrap"]

        out += f"""  simp only [packingFrame, EVM.State.instruction, ← EVM.State.CodeChunks.get_eq,
    opcode, Option.getD_some]
  unfold EVM.State.instructionWithImage
  rw [ite_eq_right (Nat.not_le_of_gt reachable)]
  simp [← EVM.State.CodeChunks.immediate_eq, EVM.State.pureOp, EVM.State.next,
    EVM.step, Evm.add, {", ".join(facts)}]
"""
        if row.access:
            out += """  all_goals simp [bump, header, marker, withDomain, withDigest, signingPrefix, charge, expansion, paid, spent]
"""
        out += "  all_goals first | omega | rfl | exact BitVec.add_comm _ _\n"
        yield out
    start = lean_frame(
        "signingStart",
        "surplus+signingFee",
        layout.steps[0].stack,
        layout.steps[0].memory,
    )
    end = lean_frame(
        "signingExit", "surplus", layout.stack, f"withDigest mem {DOMAIN} digest"
    )
    out = f"""
theorem signingRun {binders} (work : Nat) :
    EVM.State.run (work+signingSteps) tvWorldConfig journal ({start}) = (do
      let result ← EVM.State.run work tvWorldConfig journal ({end})
      return {{result with work := result.work+signingSteps}}) := by
  unfold signingStart signingExit signingSteps signingFee
  simp only [
"""
    for index in range(len(layout.steps)):
        left = len(layout.steps) - index - 1
        fuel = f"(work+{left})" if left else "work"
        out += f"    EVM.State.run_next {fuel} (signingStep{index} {ARGS}),\n"
    out += """    Bind.bind, Except.bind, Pure.pure, Except.pure]
  cases EVM.State.run work tvWorldConfig journal _ <;> rfl
"""
    yield out
    yield f"""
def envelope (lowered : Bool) (journal : Execution.Journal)
    (mem : List Execution.Byte) (digest : Word) : MIR.State.Step :=
  let typed : MIR.State.Frame := ⟨journal,mem,[],[],[],0,0,[]⟩
  if lowered then loweredSigningEnvelope typed signingDomain digest
  else MIR.State.abiPacked typed signingParts false [signingDomain,digest]

def packedWith (pack : Execution.Journal → List Execution.Byte → Word → MIR.State.Step) :
    EVM.State.Reentry.Route := fun budget proceed journal frame => do
  let digest :: x1 :: x2 :: x3 :: x4 :: 128 :: rest := frame.stack | none
  if h : signingSteps ≤ budget ∧ signingFee ≤ frame.gas ∧
      frame.code = code.bytes ∧ frame.pc = signingStart ∧ rest.length ≤ {stack_room} ∧
      frame.memory.length % 32 = 0 ∧ frame.memory.length ≤ 65536 ∧
      128 ≤ (address frame.memory).toNat ∧ (address frame.memory).toNat+128 ≤ frame.memory.length then
    let left := budget-signingSteps
    some (do
      let .returned (some base,typed) ← pack journal frame.memory digest
        | .error "expected signing envelope result"
      let result ← proceed left (by
        have positive : 0 < signingSteps := by decide
        omega) journal
          (packingFrame frame.environment (frame.gas-signingFee) signingExit
            ({lean_stack(layout.stack).replace(BASE, "base")}) typed.memory frame.returnData)
      return {{result with work := result.work+signingSteps}})
  else none

def packedRoute (lowered : Bool) : EVM.State.Reentry.Route := packedWith (envelope lowered)

def changedDomain : EVM.State.Reentry.Route :=
  packedWith fun journal mem digest =>
    loweredSigningEnvelope ⟨journal,mem,[],[],[],0,0,[]⟩ (signingDomain ^^^ 1) digest

theorem packedSound (lowered : Bool) : EVM.State.Reentry.Sound tvWorldConfig (packedRoute lowered) := by
  intro work proceed agrees journal frame result chosen
  unfold packedRoute packedWith at chosen
  split at chosen
  · rename_i digest x1 x2 x3 x4 rest stack
    split at chosen
    · rename_i ready
      obtain ⟨stepsPaid,gas,image,pc,fits,alignment,limits,base,extent⟩ := ready
      have active : Active frame.memory := ⟨alignment,limits,base,extent⟩
      let typed : MIR.State.Frame := ⟨journal,frame.memory,[],[],[],0,0,[]⟩
      have executed : envelope lowered journal frame.memory digest =
          .ok (.returned (some (address frame.memory),
            {{typed with memory := withDigest frame.memory signingDomain digest}})) := by
        cases lowered
        · unfold envelope
          simp only [Bool.false_eq_true,ite_false]
          rw [signingEnvelope]
          exact expanded typed active signingDomain digest
        · exact expanded typed active signingDomain digest
      cases chosen
      simp only [executed, Bind.bind, Except.bind, agrees]
      have paid : frame.gas-signingFee+signingFee = frame.gas := by omega
      have fuelPaid : work+1-signingSteps+signingSteps = work+1 := by omega
      have same :
          packingFrame frame.environment (frame.gas-signingFee+signingFee) signingStart
            ({lean_stack(layout.steps[0].stack)}) frame.memory frame.returnData = frame := by
        cases frame
        simp_all only [packingFrame, List.cons_append, List.nil_append]
        rfl
      let mem := frame.memory
      have execution := signingRun frame.environment journal mem frame.returnData digest x1 x2 x3 x4 rest
        (frame.gas-signingFee) active fits (work+1-signingSteps)
      change EVM.State.run (work+1-signingSteps+signingSteps) tvWorldConfig journal
          (packingFrame frame.environment (frame.gas-signingFee+signingFee) signingStart
            ({lean_stack(layout.steps[0].stack)}) frame.memory frame.returnData) = _ at execution
      rw [fuelPaid,same] at execution
      exact execution.symm
    · contradiction
  · contradiction

"""


def certify(directory, layout, configuration, timeout):
    entry_proof.success(build_model(timeout, MODULE), MODULE)
    imports = ["EntryImage", MODULE, "SolarTV.EVMIR.State.Reentry"]
    groups = list(batched(fragments(layout), 7))
    for index, group in enumerate(groups):
        name = f"MorphoPacking{index}"
        source = entry_proof.import_modules(imports) + entry_proof.HEADER
        if index == 0:
            source += configuration
        source += (
            "open MIR.PackingBoundary\n"
            "open MIR.LowerPacked (signingParts signingPrefix loweredSigningEnvelope signingEnvelope)\n"
            "set_option linter.unusedVariables false\n"
            "set_option linter.unusedSimpArgs false\n" + "".join(group)
        )
        last = index == len(groups) - 1
        if last:
            source += "#print axioms packedSound\n"
        result = entry_proof.lean(
            directory, name, source, timeout, module=True, namespace=NAMESPACE
        )
        output = entry_proof.success(result, name)
        if last:
            entry_proof.check_axioms(output.splitlines(), 1)
        imports = [name]
    return name
