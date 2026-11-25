"""Link Morpho's paid nonce store to the signing record in emitted bytecode."""

from itertools import batched, pairwise
from pathlib import Path

from . import authorization_nonce, entry_proof, metered_prefix
from .callers import require
from .decoder_proof import Step
from .dispatch_proof import Instruction
from .market_proof import word, words

AUTHORIZER = ("load", 4)
NONCE = ("load", 100)
OLD = "(nonceValue env checkpoint)"
DIGEST = "(authorizationDigest env)"
FIELDS = [("load", offset) for offset in (4, 36, 68, 100, 132)]
MASK = 2**160 - 1
BINDERS = """(env : World.Environment) (checkpoint journal : Journal)
    (matched : readWord env.input 100 = nonceValue env checkpoint)
    (canonical : (nonceOwner env).toNat < 2^160)"""
ARGS = "env checkpoint journal matched canonical"
NAME = "MorphoSigningDigest"


def plan(runtime, decoded, catalog, layout, success, nonce, signing):
    # The captured nonce prefix establishes the exact scratch and live-in shape.
    _, stack, memory, _ = authorization_nonce.plan(
        runtime,
        decoded,
        catalog,
        layout,
        success,
        nonce,
    )
    stack = [
        "(nonceValue env checkpoint)" if v == authorization_nonce.OLD else v
        for v in stack[2:]
    ]
    points = [pc for pc in decoded if nonce < pc < signing]
    rows = []
    for pc, following in pairwise([*points, signing]):
        opcode = runtime[pc]
        meta = catalog[opcode]
        gas = meta["forks"]["cancun"]
        schema = (
            gas["available"],
            gas["base_gas"],
            gas["dynamic_gas"],
            *(meta["stack"] or (0, 0)),
        )
        inst = Instruction(pc, opcode, 0, schema, stack.copy(), None, None)
        row = Step(inst, memory.copy())
        rows.append(row)
        next_pc = pc + 1
        if 0x60 <= opcode <= 0x7F:
            inst.immediate = decoded[pc][1]
            stack.insert(0, inst.immediate)
            next_pc += opcode - 0x5F
        elif 0x80 <= opcode <= 0x8F:
            stack.insert(0, stack[opcode - 0x80])
        elif 0x90 <= opcode <= 0x9F:
            depth = opcode - 0x8F
            stack[0], stack[depth] = stack[depth], stack[0]
        elif opcode == 0x03 and stack[:2] == [NONCE, OLD]:
            stack[:2] = [0]
        elif opcode == 0x57 and stack[1] == 0:
            del stack[:2]
        elif opcode == 0x51 and stack[0] in (64, 128):
            stack[0] = memory[stack[0] // 32]
        elif opcode == 0x52:
            offset, value = stack[:2]
            del stack[:2]
            require(offset == 32 * len(memory), "wrong signing ABI store offset")
            index = len(memory)
            row.growth = 3 + (index + 1) ** 2 // 512 - index**2 // 512
            memory.append(value)
        elif opcode == 0x01 and all(isinstance(v, int) for v in stack[:2]):
            stack[:2] = [sum(stack[:2])]
        elif opcode == 0x16 and stack[:2] == [MASK, AUTHORIZER]:
            stack[:2] = [AUTHORIZER]
        elif opcode == 0x20 and stack[:2] == [288, 192] and memory[10:] == FIELDS:
            stack[:2] = [DIGEST]
        else:
            require(False, f"unsupported signing record opcode {opcode:#x} at PC {pc}")
        require(next_pc == following, "signing record is not sequential")
    require(
        memory[10:] == FIELDS and stack[0] == DIGEST, "missing authorization record"
    )
    return rows, Step(Instruction(signing, 0, 0, (), stack, None, None), memory)


def frame(row):
    return (
        f"decoderFrame env 0 {row.inst.pc} {words(row.inst.stack)} {words(row.memory)}"
    )


def charge(row):
    _, base, dynamic, _, _ = row.inst.schema
    return base + row.growth + (6 * dynamic if row.inst.opcode == 0x20 else 0)


def fragments(rows, end):
    typehash = end.memory[9]
    yield f"""
def authorizationDigest (env : World.Environment) : Word :=
  Keccak.hash (wordArena {words([typehash, *FIELDS])})

def signingFrame (env : World.Environment) (checkpoint : Journal) : EVM.State.Frame :=
  {frame(end)}
"""
    debits = []
    for i, (row, following) in enumerate(pairwise([*rows, end])):
        inst = row.inst
        opcode = inst.opcode
        fee = charge(row)
        name = f"recordStep{i}"
        next_frame = frame(following)
        debits.append(metered_prefix.Debit(name, inst.pc, opcode, fee, next_frame))
        out = metered_prefix.debit(name, BINDERS, frame(row), next_frame, opcode, fee)
        out += f"""
  have schema : EVM.State.available tvWorldConfig {opcode} =
    ⟨{", ".join(str(field).lower() for field in inst.schema)}⟩ := rfl
"""
        facts = ["dispatch_size", "schema", "unpaid"]
        if inst.immediate is not None:
            out += f"  have immediate : code.immediate {inst.pc} {opcode - 0x5F} = {inst.immediate} := by decide +kernel\n"
            facts.append("immediate")
        if opcode == 0x03:
            facts += ["matched", "Evm.sub"]
        if opcode == 0x16:
            out += """  have clean : Evm.and 1461501637330902918203684832716283019655932542975 (nonceOwner env) = nonceOwner env := by
    rw [nonceMask]
    apply BitVec.eq_of_toNat_eq
    simp [BitVec.toNat_setWidth,Nat.mod_eq_of_lt canonical,Nat.mod_eq_of_lt (nonceOwner env).isLt]
  simp [nonceOwner,Evm.and] at clean
"""
            facts += ["clean", "nonceOwner", "Evm.and"]
        if opcode in (0x20, 0x51, 0x52):
            offset = inst.stack[0]
            index = offset // 32
            arena = words(row.memory)
            facts += [
                "EVM.State.withMemory",
                "growth",
                "expanded",
                "contents",
                "Bind.bind",
                "Except.bind",
            ]
            if opcode == 0x52:
                value = word(inst.stack[1])
                out += f"""  have growth : EVM.State.memoryCharge (wordArena {arena}) {offset} 32 = {row.growth} := by
    change EVM.State.memoryCharge (wordArena {arena}) (32*{index}) 32 = _
    rw [EVM.State.memoryCharge,requiredWordStore,memoryWords,wordArena_length]
    simp only [List.length_cons,List.length_nil] <;> decide
  have expanded : expand (wordArena {arena}) {offset} 32 =
      .ok (wordArena (activeWords {arena} {index})) :=
    expandArena {arena} {index} (by change 32*max {len(row.memory)} ({index}+1) ≤ 65536; decide)
  have contents : writeBytes (wordArena (activeWords {arena} {index})) {offset} (wordBytes {value}) =
      wordArena (storeArena {arena} {index} {value}) :=
    writeArena {arena} {index} {value}
"""
            else:
                cells = 6 if opcode == 0x20 else 1
                count = cells * 32
                out += f"""  have within : {index}+{cells} ≤ ({arena} : List Word).length := by simp
  have growth : EVM.State.memoryCharge (wordArena {arena}) {offset} {count} = 0 :=
    arenaCharge {arena} {index} {cells} within
  have expanded : expand (wordArena {arena}) {offset} {count} = .ok (wordArena {arena}) :=
    expandArenaRange {arena} {index} {cells} within (by simp)
"""
                if opcode == 0x51:
                    out += f"""  have contents : readWord (wordArena {arena}) {offset} = {word(row.memory[index])} :=
    readArenaWord {arena} {index} _ rfl
"""
                else:
                    out += f"""  have contents : Keccak.hash (readBytes (wordArena {arena}) {offset} {count}) = authorizationDigest env := by
    change Keccak.hash (readBytes (wordArena {arena}) (32*{index}) (32*{cells})) = _
    rw [readArena _ _ _ within]
    rfl
"""
            out += "  simp at growth expanded contents\n"
        out += f"""  by_cases unpaid : balance < {fee} <;>
    simp [decoderFrame,EVM.State.instructionWithImage,EVM.State.next,EVM.State.pureOp,EVM.step,
      Evm.add,Fork.atLeast,Fork.ordinal,{",".join(facts)}] <;> rfl
"""
        yield out
    fee = sum(debit.gas for debit in debits)
    count = len(debits)
    proof = metered_prefix.certify(
        "metered_record",
        BINDERS,
        ARGS,
        fee,
        frame(rows[0]),
        "signingFrame env checkpoint",
        debits,
    )
    yield (
        proof
        + f"""
abbrev recordSteps : Nat := {count}
abbrev recordFee : Nat := {fee}

def nonceFrame (env : World.Environment) (checkpoint : Journal) (balance : Nat) :=
  {{nonceStoreFrame env checkpoint with gas := balance}}

def nonceWritePrice (env : World.Environment) (checkpoint : Journal) : SolarTV.Storage.Price :=
  MIR.StorageBoundary.quote tvWorldConfig env (nonceWarmed env checkpoint)
    (nonceKey env) (nonceNext env checkpoint)

def nonceWritten (env : World.Environment) (checkpoint : Journal) : Journal :=
  let warmed := nonceWarmed env checkpoint
  let old := account warmed env.address
  let store := putAccount (warmSlot warmed env.address (nonceKey env))
    {{old with storage := slotsWrite old.storage (nonceKey env) (nonceNext env checkpoint)}}
  {{store with refund := warmed.refund + (nonceWritePrice env checkpoint).refund}}

theorem noncePaid (env : World.Environment) (checkpoint : Journal) (balance : Nat)
    (writable : env.isStatic = false)
    (enough : MIR.StorageBoundary.exhausted tvWorldConfig (nonceFrame env checkpoint balance)
      (nonceWritePrice env checkpoint) = false) :
    MIR.StorageBoundary.store tvWorldConfig (nonceWarmed env checkpoint)
      (nonceFrame env checkpoint balance) (nonceKey env) (nonceNext env checkpoint)
      (nonceStoreRest env checkpoint) =
    .ok (.inr (nonceWritten env checkpoint,
      {{({frame(rows[0])}) with gas := balance-(nonceWritePrice env checkpoint).gas}})) := by
  have environment : (nonceStoreFrame env checkpoint).environment = env := rfl
  simp only [nonceFrame,nonceWritePrice,environment] at enough
  simp only [MIR.StorageBoundary.store,nonceFrame,environment,writable,enough,
    Bool.false_eq_true,ite_false,MIR.State.storageOperation,MIR.State.effect,nonceWritten,
    nonceWritePrice,Bind.bind,Except.bind,Pure.pure,Except.pure]
  rfl

theorem recordActive (env : World.Environment) (checkpoint : Journal) :
    MIR.PackingBoundary.Active (signingFrame env checkpoint).memory := by
  let arena : List Word := {words(end.memory)}
  have base : MIR.PackingBoundary.address (wordArena arena) = 288 :=
    readArenaWord arena 2 _ rfl
  change MIR.PackingBoundary.Active (wordArena arena)
  refine ⟨?_,?_,?_,?_⟩
  · simp [arena]
  · simp [arena]
  · rw [base]; decide
  · rw [base]; simp [arena]

theorem nonceToSigning (env : World.Environment) (checkpoint : Journal) (work balance : Nat)
    (matched : readWord env.input 100 = nonceValue env checkpoint)
    (canonical : (nonceOwner env).toNat < 2^160)
    (writable : env.isStatic = false)
    (canStore : MIR.StorageBoundary.exhausted tvWorldConfig (nonceFrame env checkpoint balance)
      (nonceWritePrice env checkpoint) = false)
    (canEncode : recordFee ≤ balance-(nonceWritePrice env checkpoint).gas) :
    EVM.State.run (work+recordSteps+1) tvWorldConfig (nonceWarmed env checkpoint)
      (nonceFrame env checkpoint balance) = (do
      let result ← EVM.State.run work tvWorldConfig (nonceWritten env checkpoint)
        {{signingFrame env checkpoint with gas := balance-(nonceWritePrice env checkpoint).gas-recordFee}}
      return {{result with work := result.work+(recordSteps+1)}}) := by
  change EVM.State.run _ tvWorldConfig (nonceWarmed env checkpoint)
      {{nonceStoreFrame env checkpoint with gas := balance}} = _
  rw [← nonceStoreMatches]
  simp only [MIR.StorageBoundary.resume,MIR.StorageBoundary.resumeWith]
  have stored := noncePaid env checkpoint balance writable canStore
  simp only [nonceFrame] at stored
  rw [stored]
  simp only [Bind.bind,Except.bind,Pure.pure,Except.pure]
  have encoded := (metered_record env checkpoint (nonceWritten env checkpoint) matched canonical).paid
    work (balance-(nonceWritePrice env checkpoint).gas) canEncode
  rw [encoded]
  cases EVM.State.run work tvWorldConfig (nonceWritten env checkpoint) _ <;>
    simp [Bind.bind,Except.bind,Pure.pure,Except.pure,Nat.add_assoc]

#print axioms nonceToSigning
#print axioms recordActive
"""
    )


def certify(
    kernel,
    entry,
    runtime,
    decoded,
    catalog,
    layout,
    success,
    nonce_pc,
    signing,
    timeout,
):
    rows, end = plan(runtime, decoded, catalog, layout, success, nonce_pc, signing)
    imports = [Path(entry).stem, "SolarTV.MIR.Transforms.PackingBoundary"]
    for i, group in enumerate(batched(fragments(rows, end), 7)):
        name = f"{NAME}{i}"
        body = "".join(group)
        source = (
            entry_proof.import_modules(imports)
            + entry_proof.HEADER
            + """
open SolarTV.Execution MIR.TransferFromABI MIR.TransferFromMarket
set_option linter.unusedVariables false
set_option linter.unusedSimpArgs false
"""
            + body
        )
        result = entry_proof.lean(
            kernel, name, source, timeout, module=True, namespace="AuthorizationEntry"
        )
        output = entry_proof.success(result, name)
        entry_proof.check_axioms(output.splitlines(), body.count("#print axioms"))
        imports = [name]
    return {
        "proof": f"kernel/{name}.lean",
        "pc": signing,
        "gas": sum(map(charge, rows)),
    }
