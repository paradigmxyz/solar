"""Prove the emitted ABI entry for the authorization callback up to TIMESTAMP."""

from dataclasses import dataclass

from . import (
    authorization_deadline,
    decoder_proof,
    dispatch_proof,
    entry_proof,
    mir_state,
)
from .callers import require
from .check import build_model
from .decoder_proof import Step, words
from .dispatch_proof import Instruction

SELECTOR = 0x8069218F
AUTH_WORDS = (4, 36, 68, 100, 132)
V_WORD = 164
AUTH_END = 260


@dataclass
class AuthDecoder:
    instructions: list[Step]
    abi_pc: int
    selector: int
    pc: int
    stack: list
    memory: list
    gas: int


def plan(runtime, decoded, catalog):
    rows = list(decoded.values())
    labels = [
        dest
        for selector, compare, (push, dest), branch in zip(
            rows, rows[1:], rows[2:], rows[3:], strict=False
        )
        if selector == ("push4", SELECTOR)
        and compare == ("eq", None)
        and push.startswith("push")
        and branch == ("jumpi", None)
    ]
    require(len(labels) == 1 and labels[0] in decoded, "missing authorization entry")
    start = labels[0]
    stack, memory, rows = [SELECTOR], [], []
    gas = 0
    pc = start
    while runtime[pc] != 0x42:
        require(len(rows) < 60, "did not reach the authorization deadline")
        opcode = runtime[pc]
        spec = catalog[opcode]["forks"]["cancun"]
        schema = (
            spec["available"],
            spec["base_gas"],
            spec["dynamic_gas"],
            *catalog[opcode]["stack"],
        )
        row = Instruction(pc, opcode, gas, schema, stack.copy(), None, None)
        step = Step(row, memory.copy())
        rows.append(step)
        next_pc = pc + 1
        if 0x60 <= opcode <= 0x7F:
            next_pc += opcode - 0x5F
            row.immediate = int.from_bytes(runtime[pc + 1 : next_pc], "big")
            stack.insert(0, row.immediate)
        elif 0x80 <= opcode <= 0x8F:
            stack.insert(0, stack[opcode - 0x80])
        elif opcode == 0x36:
            stack.insert(0, ("size", 0))
        elif opcode == 0x35:
            require(stack[0] in AUTH_WORDS, "unexpected authorization head word")
            stack[0] = ("load", stack[0])
        elif opcode == 0x10:
            require(stack[:2] == [("size", 0), AUTH_END], "unexpected ABI length check")
            stack[:2] = [0]
            step.fact = f"ltSize env.input.length {AUTH_END} small room"
        elif opcode == 0x1C:
            count, (_, offset) = stack[:2]
            require(
                count == 160 and offset in (4, 36),
                "unexpected authorization address check",
            )
            stack[:2] = [0]
            step.fact = f"addressShift (Execution.readWord env.input {offset}) (addresses {offset} (by decide))"
        elif opcode == 0x11:
            require(
                stack[:2] == [("load", 68), 1], "unexpected authorization bool check"
            )
            stack[:2] = [0]
            step.fact = (
                "fitsSlice (Execution.readWord env.input 68) 1 (by decide) boolean"
            )
        elif opcode == 0x57:
            require(stack[1] == 0, "unexpected authorization ABI branch")
            del stack[:2]
        elif opcode == 0x01:
            x, y = stack[:2]
            require(
                isinstance(x, int) and isinstance(y, int), "nonconstant ABI pointer"
            )
            stack[:2] = [x + y]
        elif opcode == 0x52:
            target, value = stack[:2]
            require(
                isinstance(target, int) and target % 32 == 0,
                "nonaligned authorization ABI store",
            )
            del stack[:2]
            index = target // 32
            grow = max(0, index + 1 - len(memory))
            memory.extend([0] * grow)
            memory[index] = value
            step.fact = index
            step.growth = 3 * grow
        else:
            require(opcode == 0x5B, "unexpected ABI decoder opcode")
        gas += schema[1] + step.growth
        pc = next_pc
    require(
        memory == [0, 0, 288, 0, *(("load", i) for i in AUTH_WORDS)],
        "wrong authorization arena",
    )
    return AuthDecoder(rows, start, SELECTOR, pc, stack, memory, gas)


BINDINGS = """(env : World.Environment) (journal : Execution.Journal) (surplus : Nat)
    (room : 260 ≤ env.input.length) (small : env.input.length < 2^256)
    (addresses : ∀ i, i ∈ ([4,36] : List Nat) → (Execution.readWord env.input i).toNat < 2^160)
    (boolean : (Execution.readWord env.input 68).toNat ≤ 1)"""
ARGS = "env journal surplus room small addresses boolean"


def fragments(decoder, route, stop, functions):
    source, pcs = decoder_proof.steps_source(decoder, route, stop, BINDINGS)
    yield source
    width = len(decoder.instructions)
    budget = 2048 - len(route.instructions)
    yield (
        f"""
def signatureArgs : List MIR.State.Value :=
  [scalar 256 128,.aggregate [scalar 256 {V_WORD},scalar 256 96]]

def authArena (env : World.Environment) : List Execution.Byte :=
  wordArena {words(decoder.memory)}

def authorizationFrame (env : World.Environment) (journal : Execution.Journal) : MIR.State.Frame :=
  ⟨journal,authArena env,signatureArgs,[],[],0,0,[]⟩

def authCut (env : World.Environment) (journal : Execution.Journal) (surplus : Nat) : ByteCut :=
  ⟨decoderFrame env surplus {decoder.pc} {words(decoder.stack)} {words(decoder.memory)},
    journal,[],[],{pcs}⟩

theorem authorization_decoder {BINDINGS} :
    Execution.Trace.segment (chunkedByteStep {stop} tvWorldConfig code) {width} {budget}
      (dispatched env journal (surplus+{decoder.gas})) = .ok (.inr (authCut env journal surplus)) := by
"""
        + "".join(
            f"  apply Execution.Trace.segment_next {width - i - 1} {budget - i - 1} (decodeStep{i} {ARGS})\n"
            for i in range(width)
        )
        + f"""  rfl

theorem authorization_entry {BINDINGS}
    (noValue : env.value = 0)
    (selected : MIR.TransferFromDispatch.selector env.input = {SELECTOR}) :
    runBytes {stop} 2048 tvWorldConfig
      ⟨dispatchFrame env (surplus+{decoder.gas}+{route.gas}) 0 [],journal,[],[],[]⟩ =
    .ok ⟨(authCut env journal surplus).frame,journal,[],[],(authCut env journal surplus).pcs.reverse⟩ := by
  rw [resume_dispatch env journal (surplus+{decoder.gas}) (by omega) small noValue selected,
    Execution.Trace.run_segment {width} {budget} _ _ (authorization_decoder {ARGS}),
    Execution.Trace.resume,Execution.Trace.run]
  rfl

#print axioms authorization_entry
"""
    )

    for side, (entry, body) in zip(("before", "after"), functions, strict=True):
        yield f"""
def {side}AuthFunctions : MIR.State.Functions := [{body}]

theorem {side}_authorization_ready {BINDINGS}
    (noValue : env.value = 0)
    (selected : MIR.TransferFromDispatch.selector env.input = {SELECTOR}) :
    ∃ physical : ByteCut,
      runBytes {stop} 2048 tvWorldConfig
        ⟨dispatchFrame env (surplus+{decoder.gas}+{route.gas}) 0 [],journal,[],[],[]⟩ =
        .ok physical ∧
      MIR.State.start {side}AuthFunctions {entry} signatureArgs (authorizationFrame env journal) =
        .ok (authorizationFrame env journal) ∧
      physical.frame.memory = (authorizationFrame env journal).memory ∧
      physical.frame.gas = surplus ∧
      physical.world = (authorizationFrame env journal).world ∧
      {V_WORD} + 96 ≤ env.input.length := by
  refine ⟨_,authorization_entry {ARGS} noValue selected,rfl,rfl,rfl,rfl,?_⟩
  exact room

#print axioms {side}_authorization_ready
"""


def certify(
    directory, runtime, decoded, layout, route, config, tables, catalog, timeout
):
    module = "SolarTV.MIR.Transforms.TransferFromMarket"
    entry_proof.success(build_model(timeout, module), module)
    functions = []
    bodies = []
    opcodes = {opcode["name"]: opcode for opcode in catalog.values()}
    for table in tables:
        body = table["setAuthorizationWithSig"]
        functions.append((body["id"], mir_state.function_body(body, opcodes)))
        bodies.append(body)
    guard, success, expired, guard_gas = authorization_deadline.source(
        runtime, decoded, catalog, layout
    )
    fragments_ = [
        config
        + dispatch_proof.source(
            route, layout.pc, SELECTOR, len(runtime), concrete=False
        ),
        *fragments(layout, route, layout.pc, functions),
        guard,
        authorization_deadline.MIR_DEFINITIONS,
        *authorization_deadline.mir_fragments(
            *bodies, opcodes, success=success, failure=expired
        ),
    ]
    imports = ["EntryImage", module]
    for index, body in enumerate(fragments_):
        name = f"MorphoAuthorizationEntry{index}"
        source = (
            entry_proof.import_modules(imports)
            + entry_proof.HEADER
            + "open MIR.TransferFromABI\n"
            + "set_option linter.unusedVariables false\n"
            + "set_option linter.unusedSimpArgs false\n"
            + body
        )
        result = entry_proof.lean(
            directory,
            name,
            source,
            timeout,
            module=True,
            namespace="AuthorizationEntry",
        )
        output = entry_proof.success(result, name)
        entry_proof.check_axioms(output.splitlines(), body.count("#print axioms"))
        imports = [name]
    return {
        "proof": f"kernel/{name}.lean",
        "body_pc": layout.pc,
        "gas": route.gas + layout.gas,
        "deadline_gas": guard_gas,
        "deadline_success_pc": success,
        "deadline_expired_pc": expired,
    }
