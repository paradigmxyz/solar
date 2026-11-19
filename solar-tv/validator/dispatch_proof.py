"""Prove the emitted ABI selector path with an arbitrary environment and journal."""

from dataclasses import dataclass
from itertools import pairwise

from .callers import require

LENGTH = "(BitVec.ofNat 256 env.input.length)"
HEAD = "(Execution.readWord env.input 0)"


@dataclass
class Instruction:
    pc: int
    opcode: int
    gas: int
    schema: tuple
    stack: list
    immediate: int | None
    target: int | None


@dataclass
class Dispatch:
    instructions: list[Instruction]
    pc: int
    stack: list[int]
    gas: int


def plan(runtime, site, pcs, catalog):
    """Propose a selector path and its stacks. Lean checks each transition."""
    end = pcs.index(site.abi_pc)
    require(0 < end <= 128 and pcs[0] == 0, "missing emitted ABI dispatcher path")
    stack = []
    path = []
    gas = 0
    for pc, following in pairwise(pcs[: end + 1]):
        opcode = runtime[pc]
        spec = catalog[opcode]
        pricing = spec["forks"]["cancun"]
        cost = pricing["base_gas"]
        schema = (
            pricing["available"],
            cost,
            pricing["dynamic_gas"],
            *(spec["stack"] or (0, 0)),
        )
        step = Instruction(pc, opcode, gas, schema, stack.copy(), None, None)
        path.append(step)
        gas += cost
        next_pc = pc + 1
        if 0x60 <= opcode <= 0x7F:
            width = opcode - 0x5F
            next_pc += width
            step.immediate = int.from_bytes(runtime[pc + 1 : next_pc], "big")
            stack.insert(0, step.immediate)
        elif opcode == 0x5F:
            stack.insert(0, 0)
        elif opcode == 0x36:
            stack.insert(0, LENGTH)
        elif opcode == 0x34:
            stack.insert(0, 0)
        elif opcode == 0x35 and stack[0] == 0:
            stack[0] = HEAD
        elif opcode == 0x1C and stack[:2] == [224, HEAD]:
            stack[:2] = [site.selector]
        elif opcode == 0x10 and stack[:2] == [LENGTH, 4]:
            stack[:2] = [0]
        elif opcode in {0x11, 0x14, 0x17}:
            a, b = stack[:2]
            result = (
                int(a > b)
                if opcode == 0x11
                else int(a == b)
                if opcode == 0x14
                else a | b
            )
            stack[:2] = [result]
        elif 0x80 <= opcode <= 0x8F:
            stack.insert(0, stack[opcode - 0x80])
        elif opcode == 0x57:
            target, condition = stack[:2]
            del stack[:2]
            if condition:
                step.target = target
                next_pc = target
        else:
            require(opcode == 0x5B, f"unmodeled dispatcher opcode {opcode:#x}")
        require(
            following == next_pc, "symbolic selector path disagrees with raw replay"
        )
    require(stack == [site.selector], "unexpected stack at ABI decoder")
    return Dispatch(path, site.abi_pc, stack, gas)


def frame(pc, stack, remaining):
    gas = f"(surplus + {remaining})" if remaining else "surplus"
    return f"dispatchFrame env {gas} {pc} {stack_text(stack)}"


def stack_text(stack):
    return "[" + ", ".join(map(str, stack)) + "]"


def source(route, helper, selector, runtime_size):
    binders = f"""(env : World.Environment) (journal : Execution.Journal) (surplus : Nat)
    (large : 4 ≤ env.input.length) (small : env.input.length < 2^256)
    (noValue : env.value = 0)
    (selected : MIR.TransferFromDispatch.selector env.input = {selector})"""
    out = f"""
def dispatchFrame (env : World.Environment) (gas pc : Nat) (stack : List Word) : EVM.State.Frame :=
  ⟨env,code.bytes,pc,stack,[],[],gas⟩

theorem dispatch_size : code.size = {runtime_size} := by decide +kernel

def dispatched (env : World.Environment) (journal : Execution.Journal) (surplus : Nat) : ByteCut :=
  ⟨{frame(route.pc, route.stack, 0)},
    journal,[],[],{[step.pc for step in reversed(route.instructions)]}⟩
"""
    for index, step in enumerate(route.instructions):
        after = (
            route.instructions[index + 1]
            if index + 1 < len(route.instructions)
            else None
        )
        next_frame = (
            frame(after.pc, after.stack, route.gas - after.gas)
            if after
            else f"dispatchFrame env surplus {route.pc} {stack_text(route.stack)}"
        )
        paid = route.gas - step.gas
        cost = paid - (route.gas - after.gas if after else 0)
        before = frame(step.pc, step.stack, paid)
        pcs = [earlier.pc for earlier in reversed(route.instructions[:index])]
        out += f"""
theorem dispatch{index} {binders} :
    chunkedByteStep {helper} tvWorldConfig code Keccak.hash {2047 - index} ⟨{before},journal,[],[],{pcs}⟩ =
      .ok (.inr ⟨{next_frame},journal,[],[],{[step.pc, *pcs]}⟩) := by
  refine dispatchNext _ _ _ _ _ _ _ _ {step.opcode} ?_ ?_ ?_ ?_
  · change {step.pc} ≠ {helper}
    decide
  · change code.get? {step.pc} = some {step.opcode}
    decide +kernel
  · decide
  have paid : ¬ surplus + {paid} < {cost} := by omega
  have schema : EVM.State.available tvWorldConfig {step.opcode} =
    ⟨{", ".join(str(value).lower() for value in step.schema)}⟩ := by rfl
"""
        facts = ["schema", "paid"]
        final = "rfl"
        if step.immediate is not None:
            out += f"  have immediate : code.immediate {step.pc} {step.opcode - 0x5F} = {step.immediate} := by decide +kernel\n"
            facts.append("immediate")
        if step.target is not None:
            out += (
                f"  have target : {step.target} ∈ code.jumps 0 := by decide +kernel\n"
            )
            facts.append("target")
        if step.opcode == 0x10:
            out += """  have enough : (BitVec.ofNat 256 env.input.length).ult 4 = false := by
    have length : (BitVec.ofNat 256 env.input.length).toNat = env.input.length :=
      by simp [Nat.mod_eq_of_lt small]
    simp [BitVec.ult_eq_decide, length, Nat.not_lt_of_ge large]
"""
            final = "exact enough"
        elif step.opcode == 0x34:
            facts.append("noValue")
        elif step.opcode == 0x1C:
            out += f"""  have word : Evm.shr 224 (Execution.readWord env.input 0) = {selector} := by
    change Execution.readWord env.input 0 >>> 224 = {selector}
    rw [MIR.TransferFromDispatch.selector_word, selected]
"""
            final = "exact word"
        out += f"""  simp [
    dispatchFrame, EVM.State.instructionWithImage, dispatch_size,
    EVM.State.next, EVM.step, EVM.Context.ofEnvironment, EVM.State.pureOp,
    Evm.lt, Evm.gt, Evm.eq, Evm.or, Fork.atLeast, Fork.ordinal, {", ".join(facts)}] <;> {final}
"""
    width = len(route.instructions)
    out += f"""
theorem symbolic_dispatch {binders} :
    Execution.Trace.segment (chunkedByteStep {helper} tvWorldConfig code) {width} 2048
      ⟨dispatchFrame env (surplus + {route.gas}) 0 [],journal,[],[],[]⟩ =
        .ok (.inr (dispatched env journal surplus)) := by
"""
    for index in range(width):
        out += f"  apply Execution.Trace.segment_next {width - index - 1} {2047 - index} (dispatch{index} env journal surplus large small noValue selected)\n"
    out += f"""  rfl

theorem resume_dispatch {binders} :
    runBytes {helper} 2048 tvWorldConfig
      ⟨dispatchFrame env (surplus + {route.gas}) 0 [],journal,[],[],[]⟩ =
    Execution.Trace.run (chunkedByteStep {helper} tvWorldConfig code) {2048 - width}
      (dispatched env journal surplus) := by
  unfold runBytes
  change Execution.Trace.run (byteStep {helper} tvWorldConfig code.bytes) 2048 _ = _
  rw [←chunkedByteStep_eq]
  exact Execution.Trace.run_segment {width} 2048 _ _
    (symbolic_dispatch env journal surplus large small noValue selected)

def dispatchStart := dispatched env checkpoint (initial.frame.gas - {route.gas})

theorem dispatch_concrete :
    runBytes {helper} 2048 tvWorldConfig initial =
      Execution.Trace.run (chunkedByteStep {helper} tvWorldConfig code) {2048 - width} dispatchStart :=
  resume_dispatch env checkpoint (initial.frame.gas - {route.gas})
    (by decide +kernel) (by decide +kernel) (by decide +kernel) (by decide +kernel)

#print axioms resume_dispatch
#print axioms dispatch_concrete
"""
    return out
