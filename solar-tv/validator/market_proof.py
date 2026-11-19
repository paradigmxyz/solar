"""Check the emitted MarketParams hash and first storage read after ABI decode."""

from dataclasses import dataclass
from itertools import pairwise

from . import decoder_proof
from .callers import require
from .dispatch_proof import Instruction

MARKET_ID = "(marketId env)"
MAPPING = "(marketMapping env)"
SLOT = "(marketSlot env)"
VALUE = "(marketValue env journal)"


@dataclass
class Market:
    instructions: list[decoder_proof.Step]
    pc: int
    stack: list
    memory: list
    gas_before_load: int
    mir_results: tuple[int, ...]

    @property
    def load_pc(self):
        return self.instructions[-1].inst.pc


def word(value):
    return value if isinstance(value, str) else decoder_proof.word(value)


def words(values):
    return "[" + ", ".join(map(word, values)) + "]"


def plan(runtime, decoder, pcs, catalog, mir_results=()):
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
            return Market(steps, next_pc, [VALUE, *stack[1:]], memory, gas, mir_results)
        else:
            require(opcode == 0x5B, f"unsupported market prefix opcode {opcode:#x}")
        require(following == next_pc, "market access path disagrees with raw replay")
        gas += base
    require(False, "missing first market SLOAD")


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
    if snapshot is not None:
        out += concrete_source(market, decoder, helper, fuel - width, pcs, snapshot)
    else:
        out += decoder_proof.entry_alias(
            helper, fuel + len(decoder.instructions), "decoderStart", "decoder_concrete"
        )
    return out


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


def mir_results(tables, function):
    results = []
    for table in tables:
        loads = [
            inst["result"]
            for block in table[function]["blocks"]
            for inst in block["instructions"]
            if inst["operation"]["kind"] == "opcode"
            and inst["operation"]["name"] == "sload"
        ]
        require(loads and loads[0] is not None, "missing first MIR market read")
        results.append(loads[0])
    return tuple(results)


def mir_source(decoder, results):
    out = "\nsection\nopen MIR.State\n"
    observation = """let cut := readMarket env journal surplus
    .ok (cut.frame.stack.headD 0,cut.frame.memory,cut.world)"""
    binders = decoder_proof.input_binders(decoder)
    market = " ".join(map(word, decoder.memory[4:]))
    for label, result in zip(("before", "after"), results, strict=True):
        out += f"""
theorem {label}_mir_market (env : World.Environment) (journal : Execution.Journal)
    (surplus : Nat) (rest : List Value) :
    (mirSteps {{mirCfg with environment := env}} {label}Functions {label}Entry journal (argumentWords env) rest
      >>= mirObservation {result}) =
    ({observation}) := by
  change _ = Except.ok (marketValue env journal,marketMemory (argumentWords env),
    Execution.warmSlot journal env.address (marketSlot env))
  have hashed (f : Frame) (mem : f.memory = wordArena ([0,0,288,0] ++ argumentWords env)) :
      operation {{mirCfg with environment := env}} f (.memory .hash) [128,160] =
        value f (marketId env) := hashMarket _ _ _ rfl mem
  have mapped (f : Frame) (mem : f.memory = wordArena ([0,0,288,0] ++ argumentWords env)) :
      operation {{mirCfg with environment := env}} f (.slot .mapping) [marketId env,3] =
        value {{f with memory := marketMemory (argumentWords env)}} (marketMapping env) :=
    mapMarket _ _ _ rfl mem
  have add (f : Frame) :
      operation {{mirCfg with environment := env}} f (.binary .add 256) [marketMapping env,2] =
        value f (marketSlot env) := by rfl
  have low (f : Frame) :
      operation {{mirCfg with environment := env}} f (.lowBits 256) [128] =
        value f 128 := by rfl
  simp [mirCfg,value] at low hashed mapped add
  simp (config := {{maxSteps := 150000}}) [mirSteps,mirStepsFrom,mirObservation,
    MIR.TransferFromMarket.mirStart,Execution.Trace.segment,
    transition,function,{label}Functions,{label}Entry,required,next,branch,advance,
    enter,MIR.State.read,readValue,assign,assignValue,instruction,put,
    Pure.pure,Except.pure,Except.bind,Bind.bind,Value.fits,Value.word,ValueType.width,ValueType.same,
    MIR.TransferFromEntry.scalar,Scalar.ofWord,
    low,hashed,mapped,add,readMarketStorage,value,effect,marketValue,mirCfg]

theorem {label}_abi_market {binders}
    (selected : MIR.TransferFromDispatch.selector env.input = selector) :
    (do
      let frame ← decode selector caller {{mirCfg with environment := env}} journal
      let cut ← mirStepsFrom {{mirCfg with environment := env}} {label}Functions
        ⟨{{function := {label}Entry}},[],frame⟩
      mirObservation {result} cut) =
    ({observation}) := by
  rw [typed_abi_frame env journal surplus room small offsetWord slice addresses mirCfg selected]
  rw [marketEntryFrame caller journal {market} _ _ _ (by decide)]
  exact {label}_mir_market env journal surplus _

#print axioms {label}_abi_market
"""
    return out + "\nend\n"
