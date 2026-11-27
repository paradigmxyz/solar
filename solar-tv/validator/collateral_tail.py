"""Certify the emitted SupplyCollateral path with empty callback data."""

from dataclasses import dataclass

from . import collateral_proof, decoder_proof, market_proof
from .callers import require
from .dispatch_proof import Instruction

LENGTH = ("load", 260)
CALLER = "(env.caller.setWidth 256)"
ADDRESS = "(env.address.setWidth 256)"
TOKEN = ("load", 36)
TOPIC = 0xA3B9472A1399E17E123F3C2E6586C23E504184D504DE59CDAA2B375E880C6184


@dataclass
class Tail:
    steps: list[decoder_proof.Step]
    helper: int
    stack: list
    memory: list
    gas: int


def plan(runtime, catalog, write, helper):
    stack, memory = write.stack.copy(), write.memory.copy()
    steps = []
    gas = 0
    pc = write.pc
    while pc != helper:
        require(len(steps) < 40, "collateral tail does not reach the helper")
        opcode = runtime[pc]
        spec = catalog[opcode]
        pricing = spec["forks"]["cancun"]
        schema = (
            pricing["available"],
            pricing["base_gas"],
            pricing["dynamic_gas"],
            *(spec["stack"] or (0, 0)),
        )
        require(schema[0] and len(stack) >= schema[3], "invalid collateral tail opcode")
        inst = Instruction(pc, opcode, gas, schema, stack.copy(), None, None)
        steps.append(decoder_proof.Step(inst, memory.copy()))
        gas += schema[1]
        next_pc = pc + 1
        if 0x60 <= opcode <= 0x7F:
            next_pc += opcode - 0x5F
            inst.immediate = int.from_bytes(runtime[pc + 1 : next_pc], "big")
            stack.insert(0, inst.immediate)
        elif opcode == 0x5F:
            stack.insert(0, 0)
        elif 0x80 <= opcode <= 0x8F:
            stack.insert(0, stack[opcode - 0x80])
        elif 0x90 <= opcode <= 0x9F:
            other = opcode - 0x8F
            stack[0], stack[other] = stack[other], stack[0]
        elif opcode in (0x30, 0x33):
            stack.insert(0, ADDRESS if opcode == 0x30 else CALLER)
        elif opcode == 0x52:
            require(stack[:2] == [0, collateral_proof.ASSETS], "unexpected event data")
            memory[0] = stack[1]
            del stack[:2]
        elif opcode == 0xA4:
            require(
                stack[:6]
                == [
                    0,
                    32,
                    TOPIC,
                    market_proof.MARKET_ID,
                    CALLER,
                    collateral_proof.OWNER,
                ],
                "unexpected collateral event",
            )
            gas += 32 * schema[2]
            del stack[:6]
        elif opcode == 0x15:
            require(stack[0] == LENGTH, "unexpected collateral callback condition")
            stack[0] = 1
        elif opcode == 0x57:
            target, condition = stack[:2]
            require(condition == 1, "expected the empty callback branch")
            inst.target = next_pc = target
            del stack[:2]
        elif opcode == 0x01:
            require(stack[:2] == [32, 128], "unexpected market field offset")
            stack[:2] = [160]
        elif opcode == 0x51:
            require(stack[0] == 160 and memory[5] == TOKEN, "unexpected token load")
            stack[0] = TOKEN
        elif opcode == 0x1B:
            require(stack[:2] == [96, TOKEN], "unexpected token alignment")
            stack[:2] = [f"(Evm.shl 96 {market_proof.word(TOKEN)})"]
        elif opcode == 0x1C:
            require(
                stack[:2] == [96, f"(Evm.shl 96 {market_proof.word(TOKEN)})"],
                "unexpected token cleanup",
            )
            stack[:2] = [TOKEN]
        elif opcode == 0x56:
            inst.target = next_pc = stack.pop(0)
        else:
            require(opcode == 0x5B, f"unexpected collateral tail opcode {opcode:#x}")
        pc = next_pc
    return Tail(steps, helper, stack, memory, gas)


ARGS = "env journal surplus empty tokenFits writable"
BINDERS = """(env : World.Environment) (journal : Execution.Journal) (surplus : Nat)
    (empty : Execution.readWord env.input 260 = 0)
    (tokenFits : (Execution.readWord env.input 36).toNat < 2^160)
    (writable : env.isStatic = false)"""


def fragments(tail: Tail, write, market, decoder, route):
    frame = market_proof.frame
    word, words = market_proof.word, market_proof.words
    stored = "(collateralWritten env journal)"
    logged = "(collateralLogged env journal)"
    yield f"""
def collateralLogged (env : World.Environment) (journal : Execution.Journal) : Execution.Journal :=
  let stored := collateralWritten env journal
  {{stored with logs := stored.logs ++
    [⟨env.address,[{TOPIC},marketId env,env.caller.setWidth 256,collateralOwner env],
      Execution.wordBytes (collateralAssets env)⟩]}}

def collateralAtHelper (env : World.Environment) (journal : Execution.Journal)
    (surplus : Nat) : ByteCut :=
  ⟨{frame(tail.helper, tail.stack, tail.memory, "surplus")}, {logged}, [], [],
    {[step.inst.pc for step in reversed(tail.steps)]} ++ (suppliedCollateral env journal surplus).pcs⟩

def collateralHelperCut (env : World.Environment) (journal : Execution.Journal)
    (surplus : Nat) : ByteCut :=
  let reached := collateralAtHelper env journal surplus
  {{reached with pcs := reached.pcs.reverse}}
"""
    world = stored
    pcs = "(suppliedCollateral env journal surplus).pcs"
    fuel = 2048 - (
        len(route.instructions)
        + len(decoder.instructions)
        + len(market.instructions)
        + len(market.guard.instructions)
        + len(write.steps)
    )
    for index, step in enumerate(tail.steps):
        inst = step.inst
        opcode = inst.opcode
        after_world = logged if opcode == 0xA4 else world
        source = frame(
            inst.pc, inst.stack, step.memory, f"surplus + {tail.gas - inst.gas}"
        )
        charge = inst.schema[1] + (32 * inst.schema[2] if opcode == 0xA4 else 0)
        left = tail.gas - inst.gas - charge
        next_stack = (
            tail.stack
            if index + 1 == len(tail.steps)
            else tail.steps[index + 1].inst.stack
        )
        next_mem = (
            tail.memory
            if index + 1 == len(tail.steps)
            else tail.steps[index + 1].memory
        )
        next_pc = (
            tail.helper
            if index + 1 == len(tail.steps)
            else tail.steps[index + 1].inst.pc
        )
        after = frame(next_pc, next_stack, next_mem, f"surplus + {left}")
        next_pcs = f"({inst.pc} :: {pcs})"
        lemma = (
            "ordinaryNext _ _ _ _ _ _ _ _ _"
            if opcode == 0xA4
            else "dispatchNext _ _ _ _ _ _ _ _"
        )
        out = f"""
theorem collateralTail{index} {BINDERS} :
    chunkedByteStep {tail.helper} tvWorldConfig code Keccak.hash {fuel - index - 1}
      ⟨{source}, {world}, [], [], {pcs}⟩ =
      .ok (.inr ⟨{after}, {after_world}, [], [], {next_pcs}⟩) := by
  refine {lemma} {opcode} ?_ ?_ (by decide) ?_
  · change {inst.pc} ≠ {tail.helper}
    decide
  · change code.get? {inst.pc} = some {opcode}
    decide +kernel
  have schema : EVM.State.available tvWorldConfig {opcode} =
    ⟨{", ".join(str(value).lower() for value in inst.schema)}⟩ := rfl
  have paid : ¬ surplus+{tail.gas - inst.gas} < {charge} := by omega
  have spent : surplus+{tail.gas - inst.gas}-{charge} = surplus+{left} := by omega
"""
        facts = ["schema", "paid", "spent"]
        if inst.immediate is not None:
            width = opcode - 0x5F
            out += f"  have immediate : code.immediate {inst.pc} {width} = {inst.immediate} := by decide +kernel\n"
            facts.append("immediate")
        if inst.target is not None:
            out += (
                f"  have target : {inst.target} ∈ code.jumps 0 := by decide +kernel\n"
            )
            facts.append("target")
        if opcode == 0x15:
            facts += ["Evm.iszero", "empty"]
        if opcode == 0x1C:
            out += f"""  have clean : Evm.shr 96 (Evm.shl 96 {word(TOKEN)}) = {word(TOKEN)} := by
    rw [MIR.ReentrantOwner.canon]
    exact scalarFits 160 _ tokenFits
"""
            facts.append("clean")
        if opcode in (0x51, 0x52, 0xA4):
            offset = inst.stack[0]
            cell = offset // 32
            arena = words(step.memory)
            out += f"""  have within : {cell}+1 ≤ ({arena} : List Word).length := by simp only [List.length_cons,List.length_nil] <;> decide
  have charge : EVM.State.memoryCharge (wordArena {arena}) {offset} 32 = 0 :=
    arenaCharge {arena} {cell} 1 within
  have expanded : Execution.expand (wordArena {arena}) {offset} 32 = .ok (wordArena {arena}) :=
    expandArenaRange {arena} {cell} 1 within (by simp only [List.length_cons,List.length_nil] <;> decide)
"""
            if opcode == 0x52:
                value = word(inst.stack[1])
                out += f"""  have data :
      Execution.writeBytes (wordArena {arena}) {offset} (Execution.wordBytes {value}) =
        wordArena {words(next_mem)} := writeActiveArena {arena} {cell} {value} within
"""
                facts += ["Execution.wordBytes_length"]
            else:
                value = word(step.memory[cell])
                data = (
                    "(Execution.readWord (wordArena " + arena + f") {offset})"
                    if opcode == 0x51
                    else (
                        "(Execution.readBytes (wordArena " + arena + f") {offset} 32)"
                    )
                )
                if opcode == 0x51:
                    out += f"""  have data : {data} = {value} := by
    change Execution.readWord (wordArena {arena}) (32*{cell}) = _
    exact readArenaWord {arena} {cell} {value} (by rfl)
"""
                else:
                    out += f"""  have data : {data} = Execution.wordBytes {value} := by
    change Execution.readBytes (wordArena {arena}) (32*{cell}) (32*1) = _
    rw [readArena _ _ _ within]
    simp [wordArena]
"""
            out += "  simp at charge expanded data\n"
            facts += [
                "EVM.State.withMemory",
                "charge",
                "expanded",
                "data",
                "Bind.bind",
                "Except.bind",
            ]
        if opcode == 0xA4:
            out += "  have fork : tvWorldConfig.fork = .cancun := rfl\n"
            facts += ["fork", "writable", "collateralLogged"]
        out += f"""  simp [decoderFrame, EVM.State.instructionWithImage, dispatch_size,
    EVM.State.next, EVM.step, EVM.Context.ofEnvironment, EVM.State.pureOp,
    Fork.atLeast, Fork.ordinal, {", ".join(facts)}] <;> first | exact clean | rfl
"""
        yield out
        world, pcs = after_world, next_pcs
    steps = len(tail.steps)
    out = f"""
theorem collateral_empty_callback {BINDERS} :
    Execution.Trace.run (chunkedByteStep {tail.helper} tvWorldConfig code) {fuel}
      (suppliedCollateral env journal (surplus+{tail.gas})) =
    .ok (collateralHelperCut env journal surplus) := by
  have reached : Execution.Trace.segment (chunkedByteStep {tail.helper} tvWorldConfig code)
      {steps} {fuel} (suppliedCollateral env journal (surplus+{tail.gas})) =
        .ok (.inr (collateralAtHelper env journal surplus)) := by
"""
    for index in range(steps):
        out += f"    apply Execution.Trace.segment_next {steps - index - 1} {fuel - index - 1} (collateralTail{index} {ARGS})\n"
    out += "    rfl\n"
    out += f"""  rw [Execution.Trace.run_segment {steps} {fuel} _ _ reached, Execution.Trace.resume, Execution.Trace.run]
  rfl

def collateralEntryGas (env : World.Environment) (journal : Execution.Journal)
    (surplus : Nat) : Nat :=
  surplus + {tail.gas} + (collateralWritePrice env journal).gas + collateralReadPrice env journal +
    {write.gas} + {market.guard.gas} + marketGas env journal + {decoder.gas} + {route.gas}

theorem abi_to_collateral_helper {decoder_proof.input_binders(decoder)}
    (empty : Execution.readWord env.input 260 = 0)
    {collateral_proof.PRECONDITIONS.replace("surplus + (collateralWritePrice", f"surplus + {tail.gas} + (collateralWritePrice")}
    (created : ¬ marketMissing (marketValue env journal))
    (noValue : env.value = 0)
    (selected : MIR.TransferFromDispatch.selector env.input = {route.stack[0]}) :
    runBytes {tail.helper} 2048 tvWorldConfig
      ⟨dispatchFrame env (collateralEntryGas env journal surplus) 0 [],journal,[],[],[]⟩ =
    .ok (collateralHelperCut env journal surplus) := by
  have tokenFits := addresses 36 (by decide)
  unfold collateralEntryGas
  rw [resume_abi_collateral_write env journal (surplus+{tail.gas})
    room small offsetWord slice addresses assetsNonzero ownerNonzero assetsFits sumFits
    writable sentry created noValue selected]
  exact collateral_empty_callback {ARGS}

#print axioms abi_to_collateral_helper
"""
    yield out
