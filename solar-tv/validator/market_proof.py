"""Check the emitted MarketParams hash, first storage read and market guard."""

from dataclasses import dataclass
from itertools import pairwise

from . import decoder_proof, mir_state
from .callers import require
from .dispatch_proof import Instruction

MARKET_ID = "(marketId env)"
MAPPING = "(marketMapping env)"
SLOT = "(marketSlot env)"
VALUE = "(marketValue env journal)"
LOADED = "loaded"
MASKED = "(Evm.and (BitVec.ofNat 256 (2^128-1)) loaded)"
FLAG = "(marketGuardFlag loaded)"


@dataclass
class Guard:
    instructions: list[Instruction]
    missing: int
    present: int
    stack: list
    gas: int


@dataclass
class MirMarket:
    block: int
    predecessor: int
    locals: list[int]
    missing: int
    present: int
    blocks: dict[int, str]


@dataclass
class Market:
    instructions: list[decoder_proof.Step]
    pc: int
    stack: list
    memory: list
    gas_before_load: int
    mir: tuple[MirMarket, ...]
    guard: Guard

    @property
    def load_pc(self):
        return self.instructions[-1].inst.pc


def word(value):
    return value if isinstance(value, str) else decoder_proof.word(value)


def words(values):
    return "[" + ", ".join(map(word, values)) + "]"


def plan(runtime, decoder, pcs, catalog, mir=()):
    """Propose symbolic stacks along the independently reached physical path."""
    stack, memory = decoder.stack.copy(), decoder.memory.copy()
    gas, steps = 0, []
    start = pcs.index(decoder.pc)
    for pc, following in pairwise(pcs[start : start + 48]):
        opcode = runtime[pc]
        spec = catalog[opcode]
        pricing = spec["forks"]["cancun"]
        base = pricing["base_gas"]
        schema = (
            pricing["available"],
            base,
            pricing["dynamic_gas"],
            *(spec["stack"] or (0, 0)),
        )
        inst = Instruction(pc, opcode, gas, schema, stack.copy(), None, None)
        step = decoder_proof.Step(inst, memory.copy())
        steps.append(step)
        next_pc = pc + 1
        if 0x60 <= opcode <= 0x7F:
            next_pc += opcode - 0x5F
            inst.immediate = int.from_bytes(runtime[pc + 1 : next_pc], "big")
            stack.insert(0, inst.immediate)
        elif opcode == 0x5F:
            stack.insert(0, 0)
        elif 0x80 <= opcode <= 0x8F:
            stack.insert(0, stack[opcode - 0x80])
        elif opcode == 0x56:
            inst.target = next_pc = stack.pop(0)
            require(isinstance(next_pc, int), "unexpected market jump target")
        elif opcode == 0x20:
            offset, count = stack[:2]
            if (offset, count) == (128, 160):
                require(memory == decoder.memory, "changed MarketParams hash extent")
                result = MARKET_ID
            else:
                require(
                    (offset, count) == (0, 64) and memory[:2] == [MARKET_ID, 3],
                    "unexpected market mapping preimage",
                )
                result = MAPPING
            stack[:2] = [result]
            base += pricing["dynamic_gas"] * (count // 32)
        elif opcode == 0x52:
            offset, value = stack[:2]
            del stack[:2]
            require(offset in (0, 32), "unexpected market scratch store")
            memory[offset // 32] = value
        elif opcode == 0x19 and stack[0] == 0:
            stack[0] = 2**256 - 1
        elif opcode == 0x1C and stack[:2] == [128, 2**256 - 1]:
            stack[:2] = [2**128 - 1]
        elif opcode == 0x01 and stack[:2] == [2, MAPPING]:
            stack[:2] = [SLOT]
        elif opcode == 0x54:
            require(
                stack[0] == SLOT and following == next_pc,
                "unexpected first market SLOAD",
            )
            stack = [VALUE, *stack[1:]]
            guard = plan_guard(runtime, next_pc, stack, catalog)
            return Market(steps, next_pc, stack, memory, gas, mir, guard)
        else:
            require(opcode == 0x5B, f"unsupported market prefix opcode {opcode:#x}")
        require(following == next_pc, "market access path disagrees with raw replay")
        gas += base
    require(False, "missing first market SLOAD")


def plan_guard(runtime, pc, read_stack, catalog):
    """Follow the emitted stack operations to both successors of the market guard."""
    stack = [LOADED, *read_stack[1:]]
    steps, gas = [], 0
    for _ in range(16):
        require(0 <= pc < len(runtime), "invalid market guard PC")
        opcode = runtime[pc]
        spec = catalog[opcode]
        pricing = spec["forks"]["cancun"]
        base = pricing["base_gas"]
        schema = (
            pricing["available"],
            base,
            pricing["dynamic_gas"],
            *(spec["stack"] or (0, 0)),
        )
        inst = Instruction(pc, opcode, gas, schema, stack.copy(), None, None)
        steps.append(inst)
        gas += base
        next_pc = pc + 1
        if 0x60 <= opcode <= 0x7F:
            next_pc += opcode - 0x5F
            inst.immediate = int.from_bytes(runtime[pc + 1 : next_pc], "big")
            stack.insert(0, inst.immediate)
        elif 0x80 <= opcode <= 0x8F:
            stack.insert(0, stack[opcode - 0x80])
        elif 0x90 <= opcode <= 0x9F:
            depth = opcode - 0x8F
            stack[0], stack[depth] = stack[depth], stack[0]
        elif opcode == 0x16 and stack[:2] == [2**128 - 1, LOADED]:
            stack[:2] = [MASKED]
        elif opcode == 0x15 and stack[0] == MASKED:
            stack[0] = FLAG
        elif opcode == 0x56:
            inst.target = next_pc = stack.pop(0)
            require(isinstance(next_pc, int), "unexpected market guard jump target")
        elif opcode == 0x57 and stack[1] == FLAG:
            inst.target = stack[0]
            require(isinstance(inst.target, int), "unexpected missing market target")
            return Guard(steps, inst.target, next_pc, stack[2:], gas)
        else:
            require(opcode == 0x5B, f"unsupported market guard opcode {opcode:#x}")
        pc = next_pc
    require(False, "missing market guard branch")


def frame(pc, stack, memory, gas):
    return f"decoderFrame env ({gas}) {pc} {words(stack)} {words(memory)}"


def source(market, decoder, dispatch, helper, snapshot):
    fuel = 2048 - len(dispatch.instructions) - len(decoder.instructions)
    load = len(market.instructions) - 1
    pcs = [step.inst.pc for step in reversed(decoder.instructions)]
    pcs.extend(inst.pc for inst in reversed(dispatch.instructions))
    out = f"""
open MIR.TransferFromABI MIR.TransferFromMarket

def argumentWords (env : World.Environment) : List Word := {words(decoder.memory[4:])}
def marketId (env : World.Environment) : Word := marketHash (argumentWords env)
def marketMapping (env : World.Environment) : Word := mappingHash (argumentWords env)
def marketSlot (env : World.Environment) : Word := marketKey (argumentWords env)
def marketValue (env : World.Environment) (journal : Execution.Journal) : Word :=
  Execution.slotsRead (Execution.account journal env.address).storage (marketSlot env)
def marketPrice (env : World.Environment) (journal : Execution.Journal) : Nat :=
  SolarTV.Storage.readPrice .cancun (journal.warmSlots.contains (env.address,marketSlot env))

def marketGas (env : World.Environment) (journal : Execution.Journal) : Nat :=
  marketPrice env journal + {market.gas_before_load}
"""
    binders = "(env : World.Environment) (journal : Execution.Journal) (surplus : Nat)"
    for index, step in enumerate(market.instructions):
        inst = step.inst
        pending = market.gas_before_load - inst.gas
        before = frame(
            inst.pc,
            inst.stack,
            step.memory,
            f"surplus + marketPrice env journal + {pending}",
        )
        end = index == load
        if end:
            after = frame(market.pc, market.stack, market.memory, "surplus")
            new_world = "(Execution.warmSlot journal env.address (marketSlot env))"
        else:
            follow = market.instructions[index + 1]
            left = market.gas_before_load - follow.inst.gas
            after = frame(
                follow.inst.pc,
                follow.inst.stack,
                follow.memory,
                f"surplus + marketPrice env journal + {left}",
            )
            new_world = "journal"
        if end:
            out += f"""
def beforeMarketRead {binders} : ByteCut :=
  ⟨{before},journal,[],[],{pcs}⟩
"""
        next_pcs = [inst.pc, *pcs]
        proof_call = (
            "ordinaryNext _ _ _ _ _ _ _ _ _" if end else "dispatchNext _ _ _ _ _ _ _ _"
        )
        out += f"""
theorem marketStep{index} {binders} :
    chunkedByteStep {helper} tvWorldConfig code Keccak.hash {fuel - index - 1}
      ⟨{before},journal,[],[],{pcs}⟩ =
      .ok (.inr ⟨{after},{new_world},[],[],{next_pcs}⟩) := by
  refine {proof_call} {inst.opcode} ?_ ?_ (by decide) ?_
  · change {inst.pc} ≠ {helper}
    decide
  · change code.get? {inst.pc} = some {inst.opcode}
    decide +kernel
  have schema : EVM.State.available tvWorldConfig {inst.opcode} =
    ⟨{", ".join(str(value).lower() for value in inst.schema)}⟩ := by rfl
"""
        facts = ["schema"]
        if end:
            out += """  have paid : ¬ surplus + marketPrice env journal < marketPrice env journal := by omega
  have spent : surplus + marketPrice env journal - marketPrice env journal = surplus := by omega
  have cancun : tvWorldConfig.fork = .cancun := rfl
  have price : SolarTV.Storage.readPrice .cancun
      (decide ((env.address,marketSlot env) ∈ journal.warmSlots)) = marketPrice env journal := by
    simp [marketPrice]
"""
            facts += ["marketValue", "cancun", "price", "paid", "spent"]
        else:
            out += f"""  have paid : ¬ surplus + marketPrice env journal + {pending} < {pending - left} := by omega
  have spent : surplus + marketPrice env journal + {pending} - {pending - left} =
      surplus + marketPrice env journal + {left} := by omega
"""
            facts += ["paid", "spent"]
        if inst.immediate is not None:
            out += f"  have immediate : code.immediate {inst.pc} {inst.opcode - 0x5F} = {inst.immediate} := by decide +kernel\n"
            facts.append("immediate")
        if inst.target is not None:
            out += (
                f"  have target : {inst.target} ∈ code.jumps 0 := by decide +kernel\n"
            )
            facts.append("target")
        memory = words(step.memory)
        within = "(by simp only [List.length_cons,List.length_nil] <;> decide)"
        if inst.opcode == 0x20:
            offset, count = inst.stack[:2]
            hashed = word(follow.inst.stack[0])
            out += f"""  have charge : EVM.State.memoryCharge (wordArena {memory}) {offset} {count} = 0 := by
    exact arenaCharge {memory} {offset // 32} {count // 32} {within}
  have expanded : Execution.expand (wordArena {memory}) {offset} {count} = .ok (wordArena {memory}) :=
    expandArenaRange {memory} {offset // 32} {count // 32} {within} {within}
  have preimage : Keccak.hash (Execution.readBytes (wordArena {memory}) {offset} {count}) = {hashed} := by
    change Keccak.hash (Execution.readBytes (wordArena {memory}) (32*{offset // 32}) (32*{count // 32})) = _
    rw [readArena _ _ _ {within}]
    rfl
  simp at charge expanded preimage
"""
            facts += [
                "EVM.State.withMemory",
                "charge",
                "expanded",
                "preimage",
                "Except.bind",
                "Bind.bind",
            ]
        elif inst.opcode == 0x52:
            offset, value = inst.stack[:2]
            out += f"""  have charge : EVM.State.memoryCharge (wordArena {memory}) {offset} 32 = 0 :=
    arenaCharge {memory} {offset // 32} 1 {within}
  have expanded : Execution.expand (wordArena {memory}) {offset} 32 = .ok (wordArena {memory}) :=
    expandArenaRange {memory} {offset // 32} 1 {within} {within}
  have stored : Execution.writeBytes (wordArena {memory}) {offset} (Execution.wordBytes {word(value)}) =
      wordArena {words(follow.memory)} := by
    exact writeActiveArena {memory} {offset // 32} {word(value)} {within}
  simp at charge expanded stored
"""
            facts += [
                "EVM.State.withMemory",
                "charge",
                "expanded",
                "stored",
                "Except.bind",
                "Bind.bind",
            ]
        elif inst.opcode == 0x01:
            facts += [
                "marketSlot",
                "marketMapping",
                "marketKey",
                "Evm.add",
                "BitVec.add_comm",
            ]
        # Rewriting exact expansion and store equations keeps symbolic bytes opaque.
        out += f"""  simp [decoderFrame,EVM.State.instructionWithImage,dispatch_size,
    EVM.State.next,EVM.step,EVM.Context.ofEnvironment,EVM.State.pureOp,
    Fork.atLeast,Fork.ordinal,{",".join(facts)}] <;> rfl
"""
        pcs = next_pcs
    width = len(market.instructions)
    out += f"""
def readMarket {binders} : ByteCut :=
  ⟨{after},{new_world},[],[],{pcs}⟩

theorem underpaid_market (env : World.Environment) (journal : Execution.Journal)
    (gas : Nat) (unpaid : gas < marketPrice env journal) :
    chunkedByteExitStep tvWorldConfig code Keccak.hash {fuel - load - 1}
      {{beforeMarketRead env journal 0 with frame := {{(beforeMarketRead env journal 0).frame with gas}}}} =
    .ok (.inl ⟨⟨.outOfGas,[],journal,0,0⟩,[],[],none⟩) := by
  refine storageExhaustion _ _ _ _ _ _ ?_ ?_
  · change code.get? {market.load_pc} = some 0x54
    decide +kernel
  have schema : EVM.State.available tvWorldConfig 84 = ⟨true,2100,0,1,1⟩ := rfl
  have cancun : tvWorldConfig.fork = .cancun := rfl
  have price : SolarTV.Storage.readPrice .cancun
      (decide ((env.address,marketSlot env) ∈ journal.warmSlots)) = marketPrice env journal := by
    simp [marketPrice]
  simp [beforeMarketRead,decoderFrame,EVM.State.instructionWithImage,dispatch_size,
    EVM.State.pureOp,Fork.atLeast,Fork.ordinal,schema,cancun,price,unpaid]

theorem symbolic_market {binders} :
    Execution.Trace.segment (chunkedByteStep {helper} tvWorldConfig code) {width} {fuel}
      (decodedFrame env journal (surplus + marketGas env journal)) =
      .ok (.inr (readMarket env journal surplus)) := by
  rw [marketGas, ← Nat.add_assoc]
"""
    for index in range(width):
        out += f"  apply Execution.Trace.segment_next {width - index - 1} {fuel - index - 1} (marketStep{index} env journal surplus)\n"
    out += f"""  rfl

theorem resume_market {binders} :
    Execution.Trace.run (chunkedByteStep {helper} tvWorldConfig code) {fuel}
      (decodedFrame env journal (surplus + marketGas env journal)) =
    Execution.Trace.run (chunkedByteStep {helper} tvWorldConfig code) {fuel - width}
      (readMarket env journal surplus) :=
  Execution.Trace.run_segment {width} {fuel} _ _ (symbolic_market env journal surplus)

#print axioms resume_market
#print axioms underpaid_market
"""
    out += guard_source(market, decoder, dispatch, helper, fuel - width, pcs)
    if snapshot is not None:
        out += concrete_source(market, decoder, helper, fuel - width, pcs, snapshot)
    else:
        out += decoder_proof.entry_alias(
            helper, fuel + len(decoder.instructions), "decoderStart", "decoder_concrete"
        )
    return out


def guard_source(market, decoder, dispatch, helper, fuel, pcs):
    guard = market.guard
    width = len(guard.instructions)
    binders = "(env : World.Environment) (journal : Execution.Journal) (surplus : Nat) (loaded : Word)"
    args = "env journal surplus loaded"
    world = "(Execution.warmSlot journal env.address (marketSlot env))"
    target = f"if marketMissing loaded then {guard.missing} else {guard.present}"
    after_pcs = [*(inst.pc for inst in reversed(guard.instructions)), *pcs]
    out = f"""
def marketGuardCut {binders} : ByteCut :=
  ⟨{frame(f"({target})", guard.stack, market.memory, "surplus")},{world},[],[],{after_pcs}⟩
"""
    for index, inst in enumerate(guard.instructions):
        before = frame(
            inst.pc, inst.stack, market.memory, f"surplus + {guard.gas - inst.gas}"
        )
        last = index + 1 == width
        unfold = "  unfold marketGuardCut\n" if last else ""
        if last:
            result = f"marketGuardCut {args}"
        else:
            follow = guard.instructions[index + 1]
            after = frame(
                follow.pc,
                follow.stack,
                market.memory,
                f"surplus + {guard.gas - follow.gas}",
            )
            result = f"⟨{after},{world},[],[],{[inst.pc, *pcs]}⟩"
        out += f"""
theorem marketGuardStep{index} {binders} :
    chunkedByteStep {helper} tvWorldConfig code Keccak.hash {fuel - index - 1}
      ⟨{before},{world},[],[],{pcs}⟩ = .ok (.inr ({result})) := by
{unfold}  refine dispatchNext _ _ _ _ _ _ _ _ {inst.opcode} ?_ ?_ (by decide) ?_
  · change {inst.pc} ≠ {helper}
    decide
  · change code.get? {inst.pc} = some {inst.opcode}
    decide +kernel
  have schema : EVM.State.available tvWorldConfig {inst.opcode} =
    ⟨{", ".join(str(value).lower() for value in inst.schema)}⟩ := by rfl
  have paid : ¬ surplus + {guard.gas - inst.gas} < {inst.schema[1]} := by omega
"""
        facts = ["schema", "paid"]
        if inst.immediate is not None:
            out += f"  have immediate : code.immediate {inst.pc} {inst.opcode - 0x5F} = {inst.immediate} := by decide +kernel\n"
            facts.append("immediate")
        if inst.target is not None:
            out += (
                f"  have target : {inst.target} ∈ code.jumps 0 := by decide +kernel\n"
            )
            facts.append("target")
        if last:
            out += "  by_cases missing : marketMissing loaded <;>\n"
            facts.extend(("missing", "marketGuardFlag"))
        finish = "exact maskGuard loaded" if inst.opcode == 0x15 else "rfl"
        out += f"""  simp [decoderFrame,EVM.State.instructionWithImage,dispatch_size,
    EVM.State.next,EVM.step,EVM.Context.ofEnvironment,EVM.State.pureOp,
    Fork.atLeast,Fork.ordinal,{",".join(facts)}] <;> {finish}
"""
        pcs = [inst.pc, *pcs]
    out += f"""
theorem symbolic_market_guard (env : World.Environment) (journal : Execution.Journal) (surplus : Nat) :
    Execution.Trace.segment (chunkedByteStep {helper} tvWorldConfig code) {width} {fuel}
      (readMarket env journal (surplus + {guard.gas})) =
      .ok (.inr (marketGuardCut env journal surplus (marketValue env journal))) := by
"""
    for index in range(width):
        out += f"""  apply Execution.Trace.segment_next {width - index - 1} {fuel - index - 1}
    (marketGuardStep{index} env journal surplus (marketValue env journal))
"""
    read_fuel = fuel + len(market.instructions)
    return (
        out
        + f"""  rfl

theorem resume_market_guard (env : World.Environment) (journal : Execution.Journal) (surplus : Nat) :
    Execution.Trace.run (chunkedByteStep {helper} tvWorldConfig code) {read_fuel}
      (decodedFrame env journal (surplus + {guard.gas} + marketGas env journal)) =
    Execution.Trace.run (chunkedByteStep {helper} tvWorldConfig code) {fuel - width}
      (marketGuardCut env journal surplus (marketValue env journal)) := by
  rw [resume_market env journal (surplus + {guard.gas})]
  exact Execution.Trace.run_segment {width} {fuel} _ _ (symbolic_market_guard env journal surplus)

theorem resume_abi_market_guard {decoder_proof.input_binders(decoder)}
    (noValue : env.value = 0)
    (selected : MIR.TransferFromDispatch.selector env.input = {dispatch.stack[0]}) :
    runBytes {helper} 2048 tvWorldConfig
      ⟨dispatchFrame env
        (surplus + {guard.gas} + marketGas env journal + {decoder.gas} + {dispatch.gas}) 0 [],
        journal,[],[],[]⟩ =
    Execution.Trace.run (chunkedByteStep {helper} tvWorldConfig code) {fuel - width}
      (marketGuardCut env journal surplus (marketValue env journal)) := by
  rw [resume_abi_prefix env journal (surplus + {guard.gas} + marketGas env journal)
    room small offsetWord slice addresses noValue selected]
  exact resume_market_guard env journal surplus

#print axioms resume_abi_market_guard
"""
    )


def concrete_source(market, decoder, helper, fuel, pcs, snapshot):
    """Check the replayed checkpoint without reevaluating nested hashes later."""
    before, after = snapshot
    slot = int(before["stack"][0])
    memory = after["memory"]
    identity = decoder_proof.read_word(memory, 0)
    gas = after["gas"]
    stack = list(map(int, after["stack"]))
    stack_model = words(["(marketValue env checkpoint)", *market.stack[1:]])
    arena = [identity, 3, *decoder.memory[2:]]
    return f"""
theorem concreteMarketInput : wordArena (argumentWords env) = {memory[128:]} := by
  decide +kernel
theorem concreteMarketId : marketId env = {identity} := by
  rw [marketId,marketHash,concreteMarketInput]
  decide +kernel
theorem concreteMarketSlot : marketSlot env = {slot} := by
  change Keccak.hash (wordArena [marketId env,3]) + 2 = _
  rw [concreteMarketId]
  change Keccak.hash {memory[:64]} + 2 = _
  decide +kernel
theorem concreteMarketMemory : wordArena {words(arena)} = {memory} := by
  decide +kernel
theorem concreteMarketStack : {stack_model} =
    ({stack} : List Word) := by
  simp only [marketValue,concreteMarketSlot,concreteMarketId]
  decide +kernel

def physicalStart : ByteCut :=
  ⟨⟨env,code.bytes,{market.pc},{stack},{memory},[],{gas}⟩,
    Execution.warmSlot checkpoint env.address {slot},[],[],{pcs}⟩
theorem concreteMarketSnapshot : readMarket env checkpoint {gas} = physicalStart := by
  unfold readMarket decoderFrame physicalStart
  rw [concreteMarketStack,concreteMarketSlot,concreteMarketId,concreteMarketMemory]

theorem entry_concrete :
    runBytes {helper} 2048 tvWorldConfig initial =
      Execution.Trace.run (chunkedByteStep {helper} tvWorldConfig code) {fuel} physicalStart := by
  have priced : decoderStart.frame.gas = {gas} + marketGas env checkpoint := by
    rw [marketGas,marketPrice,concreteMarketSlot]
    decide +kernel
  rw [decoder_concrete]
  change Execution.Trace.run (chunkedByteStep {helper} tvWorldConfig code) _
    (decodedFrame env checkpoint decoderStart.frame.gas) = _
  rw [priced,resume_market env checkpoint {gas},concreteMarketSnapshot]
"""


def mir_markets(tables, function, catalog):
    definitions = {row["name"]: row for row in catalog.values()}
    results = []
    for table in tables:
        blocks = table[function]["blocks"]
        start = blocks[0]["terminator"]
        require(start["kind"] == "jump", "missing MIR market entry jump")
        predecessor = start["target"]
        hash_block = blocks[predecessor]
        tail = hash_block["terminator"]
        require(tail["kind"] == "jump", "missing MIR market hash jump")
        block = tail["target"]
        guard_block = blocks[block]
        term = guard_block["terminator"]
        locals_ = [
            inst["result"]
            for inst in (
                hash_block["instructions"]
                + guard_block["phis"]
                + guard_block["instructions"][:3]
            )
        ]
        require(
            len(locals_) == 6
            and all(local is not None for local in locals_)
            and term["kind"] == "branch",
            "unexpected MIR market guard shape",
        )
        used = {
            index: mir_state.block(blocks[index], definitions)
            for index in (0, predecessor, block)
        }
        results.append(
            MirMarket(block, predecessor, locals_, term["else"], term["then"], used)
        )
    return tuple(results)


def mir_source(decoder, results):
    out = "\nsection\nopen MIR.State\n"
    observation = """let cut := readMarket env journal surplus
    .ok (cut.frame.stack.headD 0,cut.frame.memory,cut.world)"""
    binders = decoder_proof.input_binders(decoder)
    market = " ".join(map(word, decoder.memory[4:]))
    hash_effects = """
  have hashed (f : Frame) (mem : f.memory = wordArena ([0,0,288,0] ++ argumentWords env)) :
      operation {mirCfg with environment := env} f (.memory .hash) [128,160] =
        value f (marketId env) := hashMarket _ _ _ rfl mem
  have low (f : Frame) :
      operation {mirCfg with environment := env} f (.lowBits 256) [128] =
        value f 128 := by rfl
  simp [mirCfg,value] at low hashed
"""
    load_effects = """
  have mapped (f : Frame) (mem : f.memory = wordArena ([0,0,288,0] ++ argumentWords env)) :
      operation {mirCfg with environment := env} f (.slot .mapping) [marketId env,3] =
        value {f with memory := marketMemory (argumentWords env)} (marketMapping env) :=
    mapMarket _ _ _ rfl mem
  have add (f : Frame) :
      operation {mirCfg with environment := env} f (.binary .add 256) [marketMapping env,2] =
        value f (marketSlot env) := by rfl
  simp [mirCfg,value] at mapped add
"""
    for label, mir in zip(("before", "after"), results, strict=True):
        lookups = [
            f"{label}MarketLookup",
            *(f"{label}MarketBlock{index}" for index in mir.blocks),
        ]
        steps = f"""    mirStepsFrom,Execution.Trace.segment,
    transition,{",".join(lookups)},required,next,branch,advance,
    enter,MIR.State.read,readValue,assign,assignValue,instruction,put,
    Pure.pure,Except.pure,Except.bind,Bind.bind,Value.fits,Value.word,ValueType.width,ValueType.same"""
        values = [128, MARKET_ID, MARKET_ID, MAPPING, SLOT, VALUE]
        bindings = [
            f"({local},MIR.TransferFromEntry.scalar 256 {value})"
            for local, value in zip(mir.locals, values, strict=True)
        ]
        hash_locals = ", ".join(reversed(bindings[:3]))
        load_locals = ", ".join(reversed(bindings))

        out += f"""
def {label}MarketBody : Function :=
  (MIR.State.function {label}Functions {label}Entry).toOption.getD ⟨[],[],[],none⟩

theorem {label}MarketLookup :
    MIR.State.function {label}Functions {label}Entry = .ok {label}MarketBody := rfl
"""
        for index, code in mir.blocks.items():
            out += f"""
theorem {label}MarketBlock{index} :
    {label}MarketBody.blocks[{index}]? = some {code} := rfl
"""
        out += f"""
def {label}HashMachine (env : World.Environment) (journal : Execution.Journal)
    (rest : List Value) : Machine :=
  ⟨{{function := {label}Entry, block := {mir.block}, entered := true,
    predecessor := some {mir.predecessor}}},[],
    ⟨journal,wordArena (0 :: 0 :: 288 :: 0 :: argumentWords env),
      MIR.TransferFromEntry.scalar 256 128 :: rest,[{hash_locals}],[],0,0,[]⟩⟩

theorem {label}_mir_hash (env : World.Environment) (journal : Execution.Journal) (rest : List Value) :
    mirSteps {{mirCfg with environment := env}} {label}Functions {label}Entry journal (argumentWords env) rest 7 =
      .ok (.inr ({label}HashMachine env journal rest)) := by
{hash_effects}  simp (config := {{maxSteps := 150000}}) [mirSteps,MIR.TransferFromMarket.mirStart,
{steps},MIR.TransferFromEntry.scalar,Scalar.ofWord,{label}HashMachine,
    low,hashed,value,effect,mirCfg,List.filter_cons] <;> rfl

def {label}MarketMachine (env : World.Environment) (journal : Execution.Journal)
    (rest : List Value) : Machine :=
  ⟨{{function := {label}Entry, block := {mir.block}, next := 3, entered := true,
    predecessor := some {mir.predecessor}}},[],
    ⟨Execution.warmSlot journal env.address (marketSlot env),marketMemory (argumentWords env),
      MIR.TransferFromEntry.scalar 256 128 :: rest,[{load_locals}],[],0,0,[]⟩⟩

theorem {label}_mir_load (env : World.Environment) (journal : Execution.Journal) (rest : List Value) :
    mirStepsFrom {{mirCfg with environment := env}} {label}Functions
      ({label}HashMachine env journal rest) 3 1017 =
      .ok (.inr ({label}MarketMachine env journal rest)) := by
{load_effects}  simp (config := {{maxSteps := 150000}}) [
{steps},MIR.TransferFromEntry.scalar,Scalar.ofWord,{label}HashMachine,{label}MarketMachine,
    mapped,add,readMarketStorage,value,effect,marketValue,mirCfg,List.filter_cons] <;> rfl

theorem {label}_mir_market (env : World.Environment) (journal : Execution.Journal) (rest : List Value) :
    mirSteps {{mirCfg with environment := env}} {label}Functions {label}Entry journal (argumentWords env) rest =
      .ok (.inr ({label}MarketMachine env journal rest)) := by
  unfold mirSteps
  exact (mirStepsContinue 7 3 1024 ({label}_mir_hash env journal rest)).trans
    ({label}_mir_load env journal rest)

theorem {label}_abi_market {binders}
    (selected : MIR.TransferFromDispatch.selector env.input = selector) :
    (do
      let frame ← decode selector caller {{mirCfg with environment := env}} journal
      let cut ← mirStepsFrom {{mirCfg with environment := env}} {label}Functions
        ⟨{{function := {label}Entry}},[],frame⟩
      mirObservation {mir.locals[-1]} cut) =
    ({observation}) := by
  rw [typed_abi_frame env journal surplus room small offsetWord slice addresses mirCfg selected]
  rw [marketEntryFrame caller journal {market} _ _ _ (by decide)]
  change (mirSteps {{mirCfg with environment := env}} {label}Functions {label}Entry journal
    (argumentWords env) _ >>= mirObservation {mir.locals[-1]}) = _
  rw [{label}_mir_market] <;> rfl

#print axioms {label}_abi_market
"""
        out += f"""
theorem {label}_mir_market_guard (env : World.Environment) (journal : Execution.Journal)
    (surplus : Nat) (rest : List Value) :
    (mirSteps {{mirCfg with environment := env}} {label}Functions {label}Entry journal (argumentWords env) rest 13
      >>= mirGuardObservation) =
    (let cut := marketGuardCut env journal surplus (marketValue env journal)
     .ok (if marketMissing (marketValue env journal) then {mir.missing} else {mir.present},
       cut.frame.memory,cut.world)) := by
  unfold mirSteps
  rw [mirStepsContinue 10 3 1024 ({label}_mir_market env journal rest)]
  change _ = Except.ok (if marketMissing (marketValue env journal) then {mir.missing} else {mir.present},
    marketMemory (argumentWords env),Execution.warmSlot journal env.address (marketSlot env))
  have truncate (f : Frame) (loaded : Word) :
      operation {{mirCfg with environment := env}} f (.lowBits 128) [loaded] =
        value f ((loaded.setWidth 128).setWidth 256) := by rfl
  simp [mirCfg,value] at truncate
  have kept (loaded : Word) :
      (Scalar.mk 128 (wordBits 128 ((loaded.setWidth 128).setWidth 256))).word =
        (loaded.setWidth 128).setWidth 256 := marketScalar loaded
  have zero : (Scalar.mk 128 (0#128)).word = 0#256 := rfl
  have test (loaded : Word) :
      (loaded.setWidth 128).setWidth 256 = 0#256 ↔ marketMissing loaded := marketWidenedZero loaded
  simp (config := {{maxSteps := 150000}}) [
{steps},{label}MarketMachine,MIR.TransferFromEntry.scalar,Scalar.ofWord,
    mirGuardObservation,readBits,effect,mirCfg,truncate,kept,zero,test]
  by_cases missing : (marketValue env journal).setWidth 128 = 0#128 <;>
    simp [marketMissing,missing]

theorem {label}_abi_market_guard {binders}
    (selected : MIR.TransferFromDispatch.selector env.input = selector) :
    (do
      let frame ← decode selector caller {{mirCfg with environment := env}} journal
      let cut ← mirStepsFrom {{mirCfg with environment := env}} {label}Functions
        ⟨{{function := {label}Entry}},[],frame⟩ 13
      mirGuardObservation cut) =
    (let cut := marketGuardCut env journal surplus (marketValue env journal)
     .ok (if marketMissing (marketValue env journal) then {mir.missing} else {mir.present},
       cut.frame.memory,cut.world)) := by
  rw [typed_abi_frame env journal surplus room small offsetWord slice addresses mirCfg selected]
  rw [marketEntryFrame caller journal {market} _ _ _ (by decide)]
  exact {label}_mir_market_guard env journal surplus _

#print axioms {label}_abi_market_guard
"""
    return out + "\nend\n"
