"""Relate the authorization nonce slot and increment to the emitted runtime."""

from . import metered_prefix
from .callers import require
from .decoder_proof import Step
from .dispatch_proof import Instruction
from .market_proof import word, words

AUTHOR = ("load", 4)
MASK = 2**160 - 1
KEY = "(nonceKey env)"
OLD = "(nonceValue env journal)"
NEXT = "(nonceNext env journal)"


def plan(runtime, decoded, catalog, layout, start, stop):
    points = list(decoded)
    segment = points[points.index(start) : points.index(stop)]
    stack = layout.stack.copy()
    memory = layout.memory.copy()
    rows = []
    fee = 0
    for i, pc in enumerate(segment):
        opcode = runtime[pc]
        spec = catalog[opcode]
        pricing = spec["forks"]["cancun"]
        schema = (
            pricing["available"],
            pricing["base_gas"],
            pricing["dynamic_gas"],
            *(spec["stack"] or (0, 0)),
        )
        inst = Instruction(pc, opcode, 0, schema, stack.copy(), None, None)
        row = Step(inst, memory.copy())
        rows.append(row)
        if opcode != 0x54:
            fee += schema[1]
        following = segment[i + 1] if i + 1 < len(segment) else stop
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
        elif opcode == 0x19 and stack[0] == 0:
            stack[0] = 2**256 - 1
        elif opcode == 0x1C and stack[:2] == [96, 2**256 - 1]:
            stack[:2] = [MASK]
        elif opcode == 0x16 and stack[:2] == [MASK, AUTHOR]:
            stack[:2] = [AUTHOR]
        elif opcode == 0x51 and stack[0] == 128:
            stack[0] = memory[4]
        elif opcode == 0x52 and stack[0] in (0, 32):
            memory[stack[0] // 32] = stack[1]
            del stack[:2]
        elif opcode == 0x20 and stack[:2] == [0, 64] and memory[:2] == [AUTHOR, 7]:
            stack[:2] = [KEY]
            fee += schema[2] * 2
        elif opcode == 0x54 and stack[0] == KEY:
            stack[0] = OLD
        elif opcode == 0x01 and stack[:2] == [OLD, 1]:
            stack[:2] = [NEXT]
        elif opcode == 0x15 and stack[0] == NEXT:
            stack[0] = 0
        elif opcode == 0x57 and stack[1] == 0:
            del stack[:2]
        else:
            require(False, "unexpected authorization nonce instruction")
        require(next_pc == following, "authorization nonce path is not sequential")
    require(
        stack[:3] == [KEY, NEXT, OLD] and runtime[stop] == 0x55,
        "unexpected authorization nonce store",
    )
    return rows, stack, memory, fee


DEFINITIONS = """
open SolarTV.Execution MIR.TransferFromABI MIR.TransferFromMarket

def nonceOwner (env : World.Environment) : Word := readWord env.input 4
def nonceKey (env : World.Environment) : Word :=
  Keccak.hash (wordArena [nonceOwner env,7])
def nonceValue (env : World.Environment) (journal : Journal) : Word :=
  slotsRead (account journal env.address).storage (nonceKey env)
def nonceNext (env : World.Environment) (journal : Journal) : Word :=
  nonceValue env journal + 1

def noncePrice (env : World.Environment) (journal : Journal) : Nat :=
  SolarTV.Storage.readPrice .cancun (journal.warmSlots.contains (env.address,nonceKey env))
def nonceWarmed (env : World.Environment) (journal : Journal) : Journal :=
  warmSlot journal env.address (nonceKey env)

theorem nonceMask (owner : Word) :
    Evm.and 1461501637330902918203684832716283019655932542975 owner = (owner.setWidth 160).setWidth 256 := by
  apply BitVec.eq_of_toNat_eq
  rw [Evm.and, BitVec.toNat_and, Nat.and_comm, show
    (1461501637330902918203684832716283019655932542975 : Word).toNat = 2^160-1 by decide,
    Nat.and_two_pow_sub_one_eq_mod, BitVec.toNat_setWidth, BitVec.toNat_setWidth]
  exact (Nat.mod_eq_of_lt
    (Nat.lt_trans (Nat.mod_lt _ (by decide : 0 < 2^160)) (by decide : 2^160 < 2^256))).symm
"""

BINDERS = """(env : World.Environment) (journal : Journal)
    (canonical : (nonceOwner env).toNat < 2^160) (noOverflow : nonceNext env journal ≠ 0#256)"""
ARGS = "env journal canonical noOverflow"


def frame(pc, stack, memory):
    return f"decoderFrame env 0 {pc} {words(stack)} {words(memory)}"


def fragments(runtime, layout, start, stop, rows, stack, memory, fixed_gas):
    yield DEFINITIONS
    debits = []
    warmed = False
    for i, row in enumerate(rows):
        inst = row.inst
        opcode = inst.opcode
        following = rows[i + 1] if i + 1 < len(rows) else None
        after = (
            frame(following.inst.pc, following.inst.stack, following.memory)
            if following is not None
            else frame(stop, stack, memory)
        )
        before = frame(inst.pc, inst.stack, row.memory)
        read = opcode == 0x54
        current = "(nonceWarmed env journal)" if warmed else "journal"
        after_world = "(nonceWarmed env journal)" if read else current
        charge = (
            "noncePrice env journal"
            if read
            else str(inst.schema[1] + (inst.schema[2] * 2 if opcode == 0x20 else 0))
        )
        debits.append(
            metered_prefix.Debit(
                f"nonceStep{i}", inst.pc, opcode, charge, after, after_world
            )
        )
        out = metered_prefix.debit(
            f"nonceStep{i}",
            BINDERS,
            before,
            after,
            opcode,
            f"({charge})",
            current=current,
            next_world=after_world,
        )
        out += f"""
  have pc : code.size = {len(runtime)} := dispatch_size
  have schema : EVM.State.available tvWorldConfig {opcode} =
    ⟨{", ".join(str(x).lower() for x in inst.schema)}⟩ := rfl
"""
        facts = ["pc", "schema", "unpaid"]
        last = "rfl"
        if inst.immediate is not None:
            out += f"  have immediate : code.immediate {inst.pc} {opcode - 0x5F} = {inst.immediate} := by decide +kernel\n"
            facts.append("immediate")
        if opcode == 0x16:
            out += """  have clean : Evm.and 1461501637330902918203684832716283019655932542975 (nonceOwner env) = nonceOwner env := by
    rw [nonceMask]
    apply BitVec.eq_of_toNat_eq
    have wide : (nonceOwner env).toNat < 2^256 := (nonceOwner env).isLt
    simp [BitVec.toNat_setWidth, Nat.mod_eq_of_lt canonical, Nat.mod_eq_of_lt wide]
"""
            facts += ["clean", "nonceOwner", "Evm.and"]
            out += "  simp [nonceOwner, Evm.and] at clean\n"
        if opcode == 0x15:
            out += """  have nonzero : Evm.iszero (nonceNext env journal) = 0#256 := by
    simp [Evm.iszero, noOverflow]
"""
            facts += ["nonzero"]
        if opcode == 0x01:
            facts += ["nonceNext", "Evm.add", "BitVec.add_comm"]
        if read:
            facts += ["noncePrice", "nonceValue", "nonceWarmed"]
            out += """  have fork : tvWorldConfig.fork = .cancun := rfl
"""
            facts.append("fork")
        if opcode in (0x51, 0x52, 0x20):
            count = 2 if opcode == 0x20 else 1
            offset = inst.stack[0]
            arena = words(row.memory)
            index = offset // 32
            out += f"""  have within : {index}+{count} ≤ ({arena} : List Word).length := by
    simp only [List.length_cons,List.length_nil] <;> decide
  have expandOk : expand (wordArena {arena}) {offset} {32 * count} = .ok (wordArena {arena}) :=
    expandArenaRange {arena} {index} {count} within (by simp only [List.length_cons,List.length_nil] <;> decide)
  have growth : EVM.State.memoryCharge (wordArena {arena}) {offset} {32 * count} = 0 :=
    arenaCharge {arena} {index} {count} within
"""
            facts += ["expandOk", "growth", "EVM.State.withMemory"]
            if opcode == 0x51:
                out += f"""  have contents : readWord (wordArena {arena}) {offset} = {word(row.memory[index])} :=
    readArenaWord {arena} {index} _ rfl
"""
            elif opcode == 0x52:
                out += f"""  have contents :
      writeBytes (wordArena {arena}) {offset} (wordBytes {word(inst.stack[1])}) =
      wordArena {words(following.memory)} :=
    writeActiveArena {arena} {index} _ (by omega)
"""
            else:
                out += f"""  have contents : Keccak.hash (readBytes (wordArena {arena}) 0 64) = nonceKey env := by
    rw [show 0 = 32*0 by rfl, show 64 = 32*2 by rfl, readArena _ 0 2 within]
    rfl
"""
            out += "  simp at growth expandOk contents\n"
            facts += ["contents", "Bind.bind", "Except.bind"]
        out += f"""  by_cases unpaid : balance < ({charge}) <;>
    simp [decoderFrame,EVM.State.instructionWithImage,EVM.State.next,EVM.State.pureOp,EVM.step,
      EVM.Context.ofEnvironment,Fork.atLeast,Fork.ordinal,{", ".join(facts)}] <;> {last}
"""
        yield out
        warmed |= read

    total = f"(noncePrice env journal + {fixed_gas})"
    out = f"""
open SolarTV.Execution

def nonceStoreRest (env : World.Environment) (journal : Journal) : List Word :=
  {words(stack[2:])}

def nonceStoreFrame (env : World.Environment) (journal : Journal) : EVM.State.Frame :=
  decoderFrame env 0 {stop} (nonceKey env :: nonceNext env journal :: nonceStoreRest env journal) {words(memory)}

"""
    out += metered_prefix.certify(
        "metered_nonce",
        BINDERS,
        ARGS,
        total,
        frame(start, layout.stack, layout.memory),
        "nonceStoreFrame env journal",
        debits,
    )
    yield out + "\n#print axioms metered_nonce\n"


def source(
    runtime,
    decoded,
    catalog,
    layout,
    start,
    stop,
    abi_gas,
    abi_steps,
    guard_gas,
    binders,
    args,
):
    rows, stack, memory, fixed_gas = plan(
        runtime, decoded, catalog, layout, start, stop
    )
    pieces = fragments(runtime, layout, start, stop, rows, stack, memory, fixed_gas)
    gas = f"{abi_gas} + ({guard_gas} + (noncePrice env journal + {fixed_gas}))"
    steps = abi_steps + 5 + len(rows)
    link = f"""
abbrev noncePrefixSteps : Nat := {steps}
abbrev noncePrefixCharge (env : World.Environment) (journal : Journal) : Nat := {gas}

theorem authorizationToNonce {binders}
    (noValue : env.value = 0)
    (selected : MIR.TransferFromDispatch.selector env.input = {layout.selector})
    (live : expiry env = 0#256)
    (noOverflow : nonceNext env journal ≠ 0#256) :
    EVM.State.MeteredPrefix tvWorldConfig journal (dispatchFrame env 0 0 [])
      (nonceWarmed env journal) (nonceStoreFrame env journal) {steps} ({gas}) := by
  have deadline := metered_deadline env journal
  simp only [afterDeadline, show expiry env = (0 : Word) from live, ite_true] at deadline
  exact (metered_abi {args} noValue selected).append (deadline.append
    (metered_nonce env journal (addresses 4 (by decide)) noOverflow))

theorem authorizationNonceOutOfGas {binders}
    (noValue : env.value = 0)
    (selected : MIR.TransferFromDispatch.selector env.input = {layout.selector})
    (live : expiry env = 0#256)
    (noOverflow : nonceNext env journal ≠ 0#256)
    (work balance : Nat) (unpaid : balance < {gas}) :
    ∃ spent ≤ {steps},
      EVM.State.execute (work+{steps}) tvWorldConfig journal (dispatchFrame env balance 0 []) =
        .ok ⟨.outOfGas,[],journal,0,spent⟩ :=
  (authorizationToNonce {args} noValue selected live noOverflow).rollbackOutOfGas work balance unpaid

theorem nonceStoreMatches (env : World.Environment) (journal : Journal) (surplus work : Nat) :
    MIR.StorageBoundary.resume tvWorldConfig (nonceWarmed env journal)
      {{nonceStoreFrame env journal with gas := surplus}} (nonceKey env)
      (nonceNext env journal) (nonceStoreRest env journal) work =
    EVM.State.run (work+1) tvWorldConfig (nonceWarmed env journal)
      {{nonceStoreFrame env journal with gas := surplus}} := by
  apply MIR.StorageBoundary.resume_eq _ _ _ _ _ _ _
  · change code.bytes[{stop}]? = some 0x55
    rw [← EVM.State.CodeChunks.get_eq]
    decide +kernel
  · rfl
  · rfl
  · rfl
  · simp [nonceStoreRest]

theorem resumeAuthorizationNonce {binders}
    (noValue : env.value = 0)
    (selected : MIR.TransferFromDispatch.selector env.input = {layout.selector})
    (live : expiry env = 0#256)
    (noOverflow : nonceNext env journal ≠ 0#256)
    (work balance : Nat) (paid : {gas} ≤ balance) :
    EVM.State.run (work+1+{steps}) tvWorldConfig journal (dispatchFrame env balance 0 []) = (do
      let result ← MIR.StorageBoundary.resume tvWorldConfig (nonceWarmed env journal)
        {{nonceStoreFrame env journal with gas := balance-({gas})}} (nonceKey env)
        (nonceNext env journal) (nonceStoreRest env journal) work
      return {{result with work := result.work+{steps}}}) := by
  rw [nonceStoreMatches]
  exact (authorizationToNonce {args} noValue selected live noOverflow).paid (work+1) balance paid

#print axioms authorizationNonceOutOfGas
#print axioms resumeAuthorizationNonce
"""
    return pieces, link, fixed_gas
