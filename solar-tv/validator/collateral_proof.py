"""Propose and check the emitted SupplyCollateral path through its first write."""

from dataclasses import dataclass
from itertools import pairwise

from . import decoder_proof, market_proof
from .callers import require
from .dispatch_proof import Instruction

ASSETS = "(collateralAssets env)"
OWNER = "(collateralOwner env)"
MARKET = market_proof.MARKET_ID
POSITION_MAP = "(collateralMapping env)"
POSITION = "(collateralSlot env)"
CURRENT = "(collateralCurrent env journal)"
SUM = "(collateralSum env journal)"
PACKED = "(collateralWord env journal)"


@dataclass
class Path:
    steps: list[decoder_proof.Step]
    pc: int
    stack: list
    memory: list

    @property
    def store_pc(self):
        return self.steps[-1].inst.pc

    @property
    def load_pc(self):
        return next(step.inst.pc for step in self.steps if step.inst.opcode == 0x54)

    @property
    def gas(self):
        return sum(base_fee(step.inst) for step in self.steps)


def base_fee(inst):
    if inst.opcode in (0x54, 0x55):
        return 0
    base, dynamic = inst.schema[1:3]
    return base + (2 * dynamic if inst.opcode == 0x20 else 0)


def plan(runtime, pcs, catalog, market):
    """Use the reached trace only to propose control flow and operand order."""
    stack, memory = market.guard.stack.copy(), market.memory.copy()
    steps = []
    start = pcs.index(market.guard.present)
    for pc, following in pairwise(pcs[start : start + 80]):
        opcode = runtime[pc]
        spec = catalog[opcode]
        pricing = spec["forks"]["cancun"]
        schema = (
            pricing["available"],
            pricing["base_gas"],
            pricing["dynamic_gas"],
            *(spec["stack"] or (0, 0)),
        )
        require(schema[0] and len(stack) >= schema[3], "invalid collateral opcode")
        inst = Instruction(pc, opcode, 0, schema, stack.copy(), None, None)
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
        elif 0x90 <= opcode <= 0x9F:
            depth = opcode - 0x8F
            stack[0], stack[depth] = stack[depth], stack[0]
        elif opcode == 0x35:
            stack[0] = {164: ASSETS, 196: OWNER}[stack[0]]
        elif opcode == 0x15:
            require(stack[0] in (ASSETS, OWNER), "unknown collateral nonzero check")
            step.fact = "assetsNonzero" if stack[0] == ASSETS else "ownerNonzero"
            stack[0] = 0
        elif opcode == 0x1C:
            amount, value = stack[:2]
            require(amount == 128, "unknown collateral right shift")
            if value == CURRENT:
                stack[:2] = [f"(Evm.shr 128 {CURRENT})"]
            else:
                require(value in (ASSETS, SUM), "unknown collateral overflow check")
                step.fact = "assetsFits" if value == ASSETS else "sumFits"
                stack[:2] = [0]
        elif opcode == 0x01:
            a, b = stack[:2]
            if (a, b) == (1, POSITION_MAP):
                stack[:2] = [POSITION]
            else:
                require(
                    {a, b} == {ASSETS, f"(Evm.shr 128 {CURRENT})"},
                    "unknown collateral sum",
                )
                stack[:2] = [SUM]
        elif opcode == 0x16:
            require(set(stack[:2]) == {2**128 - 1, CURRENT}, "unknown collateral mask")
            stack[:2] = [f"(Evm.and {2**128 - 1} {CURRENT})"]
        elif opcode == 0x1B:
            require(stack[:2] == [128, SUM], "unknown collateral packing shift")
            stack[:2] = [f"(Evm.shl 128 {SUM})"]
        elif opcode == 0x17:
            require(
                set(stack[:2])
                == {f"(Evm.and {2**128 - 1} {CURRENT})", f"(Evm.shl 128 {SUM})"},
                "unknown collateral packed word",
            )
            stack[:2] = [PACKED]
        elif opcode == 0x52:
            offset, value = stack[:2]
            require(offset in (0, 32), "unexpected position hash scratch offset")
            memory[offset // 32] = value
            del stack[:2]
        elif opcode == 0x20:
            require(stack[:2] == [0, 64], "unexpected position hash range")
            if memory[:2] == [MARKET, 2]:
                stack[:2] = ["(positionMapping env)"]
            else:
                require(
                    memory[:2] == [OWNER, "(positionMapping env)"],
                    "unknown position hash input",
                )
                stack[:2] = [POSITION_MAP]
        elif opcode == 0x54:
            require(stack[0] == POSITION, "unexpected collateral storage load")
            stack[0] = CURRENT
        elif opcode == 0x55:
            require(
                stack[:2] == [POSITION, PACKED] and following == next_pc,
                "unexpected collateral storage write",
            )
            return Path(steps, next_pc, stack[2:], memory)
        elif opcode == 0x56:
            inst.target = next_pc = stack.pop(0)
        elif opcode == 0x57:
            _, condition = stack[:2]
            require(condition == 0, "unexpected taken branch on collateral write path")
            del stack[:2]
        else:
            require(opcode == 0x5B, f"unsupported collateral opcode {opcode:#x}")
        require(following == next_pc, "collateral path disagrees with raw replay")
    require(False, "missing first collateral SSTORE")


DEFINITIONS = """
open MIR.TransferFromABI MIR.TransferFromMarket

def collateralAssets (env : World.Environment) : Word := Execution.readWord env.input 164
def collateralOwner (env : World.Environment) : Word := Execution.readWord env.input 196
def positionMapping (env : World.Environment) : Word :=
  Keccak.hash (wordArena [marketId env,2])
def collateralMapping (env : World.Environment) : Word :=
  Keccak.hash (wordArena [collateralOwner env,positionMapping env])
def collateralSlot (env : World.Environment) : Word := collateralMapping env + 1
def collateralMarketWorld (env : World.Environment) (journal : Execution.Journal) :=
  Execution.warmSlot journal env.address (marketSlot env)
def collateralCurrent (env : World.Environment) (journal : Execution.Journal) : Word :=
  Execution.slotsRead (Execution.account (collateralMarketWorld env journal) env.address).storage
    (collateralSlot env)
def collateralSum (env : World.Environment) (journal : Execution.Journal) : Word :=
  collateralAssets env + Evm.shr 128 (collateralCurrent env journal)
def collateralWord (env : World.Environment) (journal : Execution.Journal) : Word :=
  Evm.or (Evm.shl 128 (collateralSum env journal))
    (Evm.and (BitVec.ofNat 256 (2^128-1)) (collateralCurrent env journal))
def collateralReadPrice (env : World.Environment) (journal : Execution.Journal) : Nat :=
  SolarTV.Storage.readPrice .cancun
    ((collateralMarketWorld env journal).warmSlots.contains (env.address,collateralSlot env))
def collateralWarmWorld (env : World.Environment) (journal : Execution.Journal) :=
  Execution.warmSlot (collateralMarketWorld env journal) env.address (collateralSlot env)
def collateralWritePrice (env : World.Environment) (journal : Execution.Journal) : SolarTV.Storage.Price :=
  let warmed := collateralWarmWorld env journal
  let original := (warmed.original.find? (·.address == env.address)).getD {address := env.address}
  SolarTV.Storage.writePrice .cancun true
    (Execution.slotsRead original.storage (collateralSlot env))
    (collateralCurrent env journal) (collateralWord env journal)
def collateralWritten (env : World.Environment) (journal : Execution.Journal) : Execution.Journal :=
  let warmed := collateralWarmWorld env journal
  let old := Execution.account warmed env.address
  Execution.putAccount {warmed with refund := warmed.refund + (collateralWritePrice env journal).refund}
    {old with storage := Execution.slotsWrite old.storage (collateralSlot env) (collateralWord env journal)}
"""

BINDERS = "(env : World.Environment) (journal : Execution.Journal) (surplus : Nat)"
PRECONDITIONS = """(assetsNonzero : collateralAssets env ≠ 0) (ownerNonzero : collateralOwner env ≠ 0)
    (assetsFits : (collateralAssets env).toNat < 2^128)
    (sumFits : (collateralSum env journal).toNat < 2^128)
    (writable : env.isStatic = false)
    (sentry : 2300 < surplus + (collateralWritePrice env journal).gas)"""
ARGS = (
    "env journal surplus assetsNonzero ownerNonzero assetsFits sumFits writable sentry"
)


def fragments(path, market, decoder, route, helper):
    pcs = [
        *(step.inst.pc for step in reversed(market.instructions)),
        *(step.inst.pc for step in reversed(decoder.instructions)),
        *(inst.pc for inst in reversed(route.instructions)),
    ]
    fuel = 2048 - len(pcs)
    guard = market.guard
    fuel -= len(guard.instructions)
    pcs = [*(inst.pc for inst in reversed(guard.instructions)), *pcs]
    frame, word, words = market_proof.frame, market_proof.word, market_proof.words
    read_price = "collateralReadPrice env journal"
    write_price = "(collateralWritePrice env journal).gas"
    world = "collateralMarketWorld env journal"
    fees = [
        read_price
        if s.inst.opcode == 0x54
        else write_price
        if s.inst.opcode == 0x55
        else base_fee(s.inst)
        for s in path.steps
    ]
    budgets = ["surplus"]
    fixed = 0
    pending = ["surplus"]
    for fee in reversed(fees):
        if isinstance(fee, int):
            fixed += fee
        else:
            pending.append(fee)
        budgets.append(" + ".join([*pending, str(fixed)] if fixed else pending))
    budgets.reverse()
    yield (
        DEFINITIONS
        + f"""
def suppliedCollateral {BINDERS} : ByteCut :=
  ⟨{frame(path.pc, path.stack, path.memory, "surplus")},
    collateralWritten env journal,[],[],{[*(s.inst.pc for s in reversed(path.steps)), *pcs]}⟩
"""
    )
    for index, step in enumerate(path.steps):
        inst = step.inst
        before = frame(inst.pc, inst.stack, step.memory, budgets[index])
        after = "(suppliedCollateral env journal surplus)"
        next_world = "collateralWarmWorld env journal" if inst.opcode == 0x54 else world
        if inst.opcode != 0x55:
            follow = path.steps[index + 1]
            after_frame = frame(
                follow.inst.pc, follow.inst.stack, follow.memory, budgets[index + 1]
            )
            after = f"⟨{after_frame},{next_world},[],[],{[inst.pc, *pcs]}⟩"
        proof = (
            "ordinaryNext _ _ _ _ _ _ _ _ _"
            if inst.opcode in (0x54, 0x55)
            else "dispatchNext _ _ _ _ _ _ _ _"
        )
        width = inst.opcode - 0x5F
        charge = fees[index]
        out = f"""
theorem collateralStep{index} {BINDERS} {PRECONDITIONS} :
    chunkedByteStep {helper} tvWorldConfig code Keccak.hash {fuel - index - 1}
      ⟨{before},{world},[],[],{pcs}⟩ = .ok (.inr {after}) := by
  refine {proof} {inst.opcode} ?_ ?_ (by decide) ?_
  · change {inst.pc} ≠ {helper}
    decide
  · change code.get? {inst.pc} = some {inst.opcode}
    decide +kernel
  have schema : EVM.State.available tvWorldConfig {inst.opcode} =
    ⟨{", ".join(str(value).lower() for value in inst.schema)}⟩ := rfl
  have paid : ¬ ({budgets[index]}) < {charge} := by omega
  have spent : ({budgets[index]}) - {charge} = {budgets[index + 1]} := by omega
"""
        facts = ["schema", "paid", "spent"]
        finish = "rfl"
        store = ""
        if inst.immediate is not None:
            out += f"  have immediate : code.immediate {inst.pc} {width} = {inst.immediate} := by decide +kernel\n"
            facts.append("immediate")
        if inst.target is not None:
            out += (
                f"  have target : {inst.target} ∈ code.jumps 0 := by decide +kernel\n"
            )
            facts.append("target")
        if inst.opcode == 0x35:
            facts += ["collateralAssets", "collateralOwner"]
        elif inst.opcode == 0x15:
            facts += ["Evm.iszero"]
            finish = f"exact {step.fact}"
        elif inst.opcode == 0x1C and step.fact:
            out += f"  have shifted : Evm.shr 128 {word(inst.stack[1])} = 0 := shrFits _ 128 (by decide) {step.fact}\n"
            facts.append("shifted")
            finish = "exact shifted"
        elif inst.opcode == 0x01:
            facts += ["collateralSlot", "collateralSum", "Evm.add", "BitVec.add_comm"]
        elif inst.opcode in (0x20, 0x52):
            arena = words(step.memory)
            offset = inst.stack[0]
            count = 64 if inst.opcode == 0x20 else 32
            idx, n = offset // 32, count // 32
            within = "(by simp only [List.length_cons,List.length_nil] <;> decide)"
            out += f"""  have charge : EVM.State.memoryCharge (wordArena {arena}) {offset} {count} = 0 :=
    arenaCharge {arena} {idx} {n} {within}
  have expanded : Execution.expand (wordArena {arena}) {offset} {count} = .ok (wordArena {arena}) :=
    expandArenaRange {arena} {idx} {n} {within} {within}
"""
            if inst.opcode == 0x20:
                out += f"""  have changed : Keccak.hash (Execution.readBytes (wordArena {arena}) {offset} {count}) =
      {word(follow.inst.stack[0])} := by
    change Keccak.hash (Execution.readBytes (wordArena {arena}) (32*{idx}) (32*{n})) = _
    rw [readArena _ _ _ {within}]
    rfl
"""
            else:
                value = word(inst.stack[1])
                out += f"""  have changed : Execution.writeBytes (wordArena {arena}) {offset} (Execution.wordBytes {value}) =
      wordArena {words(follow.memory)} :=
    writeActiveArena {arena} {idx} {value} {within}
"""
            out += "  simp at charge expanded changed\n"
            facts += [
                "EVM.State.withMemory",
                "charge",
                "expanded",
                "changed",
                "Except.bind",
                "Bind.bind",
            ]
        elif inst.opcode in (0x54, 0x55):
            out += "  have cancun : tvWorldConfig.fork = .cancun := rfl\n"
            facts += (
                ["collateralReadPrice", "collateralCurrent", "collateralWarmWorld"]
                if inst.opcode == 0x54
                else [
                    "collateralWritten",
                    "collateralWritePrice",
                    "collateralCurrent",
                    "collateralWarmWorld",
                    "collateralMarketWorld",
                    "writable",
                    "warmAgain",
                ]
            )
            if inst.opcode == 0x55:
                out += """  have sentryPaid := Nat.not_le_of_lt sentry
  simp only [collateralWritePrice,collateralCurrent,collateralWarmWorld,collateralMarketWorld,
    warmAccount,warmOriginal] at paid spent sentryPaid
"""
                facts.append("sentryPaid")
            else:
                out += f"  simp [{','.join(facts[3:])}] at paid spent\n"
            facts.append("cancun")
            finish = "first | rfl | assumption | omega"
            if inst.opcode == 0x55:
                store = """
  all_goals
    split
    · rename_i h
      exact (Nat.not_le_of_lt sentry (of_decide_eq_true h)).elim
    · first
      | rfl
      | split
        · rename_i h
          exact (paid h).elim
        · rfl"""
        elif inst.opcode == 0x17:
            facts += ["collateralWord", "Evm.or", "BitVec.or_comm"]
        out += f"""  simp [suppliedCollateral,decoderFrame,EVM.State.instructionWithImage,dispatch_size,
    EVM.State.next,EVM.step,EVM.Context.ofEnvironment,EVM.State.pureOp,
    Fork.atLeast,Fork.ordinal,{",".join(facts)}]"""
        out += (store if store else f" <;> {finish}") + "\n"
        yield out
        world = next_world
        pcs = [inst.pc, *pcs]
    width = len(path.steps)
    out = f"""
theorem symbolic_collateral_write {BINDERS} {PRECONDITIONS}
    (created : ¬ marketMissing (marketValue env journal)) :
    Execution.Trace.segment (chunkedByteStep {helper} tvWorldConfig code) {width} {fuel}
      (marketGuardCut env journal ({budgets[0]}) (marketValue env journal)) =
      .ok (.inr (suppliedCollateral env journal surplus)) := by
  simp only [marketGuardCut,created,ite_false]
"""
    for index in range(width):
        out += f"  apply Execution.Trace.segment_next {width - index - 1} {fuel - index - 1} (collateralStep{index} {ARGS})\n"
    out += "  rfl\n\n#print axioms symbolic_collateral_write\n"
    out += f"""
theorem resume_abi_collateral_write {decoder_proof.input_binders(decoder)}
    {PRECONDITIONS}
    (created : ¬ marketMissing (marketValue env journal))
    (noValue : env.value = 0)
    (selected : MIR.TransferFromDispatch.selector env.input = {route.stack[0]}) :
    runBytes {helper} 2048 tvWorldConfig
      ⟨dispatchFrame env
          ({budgets[0]} + {guard.gas} + marketGas env journal + {decoder.gas} + {route.gas}) 0 [],
        journal,[],[],[]⟩ =
    Execution.Trace.run (chunkedByteStep {helper} tvWorldConfig code) {fuel - width}
      (suppliedCollateral env journal surplus) := by
  rw [resume_abi_prefix env journal ({budgets[0]} + {guard.gas} + marketGas env journal)
    room small offsetWord slice addresses noValue selected,
    resume_market_guard env journal ({budgets[0]})]
  exact Execution.Trace.run_segment {width} {fuel} _ _ (symbolic_collateral_write {ARGS} created)

#print axioms resume_abi_collateral_write
"""
    yield out
