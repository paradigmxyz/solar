"""Check a compiled ABI error stub, including its exact revert bytes."""

from itertools import pairwise

from . import metered_prefix
from .callers import require
from .decoder_proof import Step
from .dispatch_proof import Instruction
from .market_proof import word, words


def plan(runtime, decoded, catalog, entry, stack, memory):
    rows, seen = [], set()
    stack, memory = stack.copy(), memory.copy()
    pc = entry
    while pc not in seen:
        seen.add(pc)
        opcode = runtime[pc]
        spec = catalog[opcode]
        price = spec["forks"]["cancun"]
        schema = (
            price["available"],
            price["base_gas"],
            price["dynamic_gas"],
            *(spec["stack"] or (0, 0)),
        )
        inst = Instruction(pc, opcode, 0, schema, stack.copy(), None, None)
        rows.append(Step(inst, memory.copy()))
        if opcode == 0xFD:
            return rows
        pc += 1
        if opcode == 0x5B:
            continue
        if 0x60 <= opcode <= 0x7F:
            end = pc + opcode - 0x5F
            inst.immediate = int.from_bytes(runtime[pc:end], "big")
            stack.insert(0, inst.immediate)
            pc = end
        elif opcode == 0x5F:
            stack.insert(0, 0)
        elif 0x80 <= opcode <= 0x8F:
            stack.insert(0, stack[opcode - 0x80])
        elif opcode == 0x1B:
            offset, value = stack[:2]
            require(
                isinstance(offset, int) and isinstance(value, int),
                "symbolic revert string",
            )
            stack[:2] = [(value << offset) % 2**256]
        elif opcode == 0x52:
            offset, value = stack[:2]
            require(
                isinstance(offset, int)
                and offset % 32 == 0
                and offset // 32 < len(memory),
                "revert stub writes outside its active memory",
            )
            memory[offset // 32] = value
            del stack[:2]
        elif opcode == 0x56:
            pc = stack.pop(0)
            require(
                pc in decoded and decoded[pc][0] == "jumpdest",
                "unexpected revert stub jump",
            )
            inst.target = pc
        else:
            require(False, "unexpected revert stub instruction")
    require(False, "cycle in revert stub")


def frame(row, balance=0):
    return f"decoderFrame env {balance} {row.inst.pc} {words(row.inst.stack)} {words(row.memory)}"


def fragments(rows, label, payload):
    expected = f"{label}Data"
    yield f"\ndef {expected} : List Byte := {list(payload)}\n"
    binders = "(env : World.Environment) (journal : Journal)"
    debits = []
    for row, follow in pairwise(rows):
        inst = row.inst
        cost = inst.schema[1]
        name = f"{label}Step{len(debits)}"
        debits.append(
            metered_prefix.Debit(name, inst.pc, inst.opcode, cost, frame(follow))
        )
        out = metered_prefix.debit(
            name, binders, frame(row), frame(follow), inst.opcode, cost
        )
        out += f"""
  have schema : EVM.State.available tvWorldConfig {inst.opcode} =
    ⟨{", ".join(str(x).lower() for x in inst.schema)}⟩ := rfl
"""
        facts = ["dispatch_size", "schema", "unpaid"]
        if inst.immediate is not None:
            out += f"  have immediate : code.immediate {inst.pc} {inst.opcode - 0x5F} = {inst.immediate} := by decide +kernel\n"
            facts.append("immediate")
        if inst.target is not None:
            out += f"  have destination : {inst.target} ∈ code.jumps 0 := by decide +kernel\n"
            facts.append("destination")
        if inst.opcode == 0x52:
            offset, stored = inst.stack[:2]
            arena = words(row.memory)
            index = offset // 32
            out += f"""  have within : {index}+1 ≤ ({arena} : List Word).length := by simp
  have expanded : expand (wordArena {arena}) {offset} 32 = .ok (wordArena {arena}) :=
    expandArenaRange {arena} {index} 1 within (by simp)
  have growth : EVM.State.memoryCharge (wordArena {arena}) {offset} 32 = 0 :=
    arenaCharge {arena} {index} 1 within
  have written : writeBytes (wordArena {arena}) {offset} (wordBytes {word(stored)}) =
      wordArena {words(follow.memory)} :=
    writeActiveArena {arena} {index} {word(stored)} within
  simp at expanded growth written
"""
            facts += [
                "EVM.State.withMemory",
                "expanded",
                "growth",
                "written",
                "Bind.bind",
                "Except.bind",
            ]
        out += f"""  by_cases unpaid : balance < {cost} <;>
    simp [decoderFrame,EVM.State.instructionWithImage,EVM.State.next,EVM.State.pureOp,
      EVM.step,Fork.atLeast,Fork.ordinal,{",".join(facts)}] <;> rfl
"""
        yield out
    last = rows[-1]
    offset, size = last.inst.stack[:2]
    end = offset + size
    covered = (end + 31) // 32
    constants = last.memory[:covered]
    tail = last.memory[covered:]
    require(all(isinstance(value, int) for value in constants), "symbolic revert bytes")
    arena = words(last.memory)
    charge = sum(debit.gas for debit in debits)
    yield metered_prefix.certify(
        label, binders, "env journal", charge, frame(rows[0]), frame(last), debits
    )
    yield f"""
theorem {label}Returns (env : World.Environment) (journal : Journal) (work balance : Nat) :
    EVM.State.run (work+1) tvWorldConfig journal ({frame(last, "balance")}) =
      .ok ⟨.reverted,{expected},journal,balance,1⟩ := by
  have wordCount : memoryWords (wordArena {arena}) = {len(last.memory)} := by simp [memoryWords]
  have needed : requiredWords (wordArena {arena}) {offset} {size} = {len(last.memory)} := by
    simp [requiredWords,memoryWords]
  have active : expand (wordArena {arena}) {offset} {size} = .ok (wordArena {arena}) := by
    rw [expand,needed]
    simp
  have growth : EVM.State.memoryCharge (wordArena {arena}) {offset} {size} = 0 := by
    rw [EVM.State.memoryCharge,needed,wordCount,Nat.sub_self]
  have data : readBytes (wordArena {arena}) {offset} {size} = {expected} := by
    change readBytes (wordArena ({words(constants)} ++ {words(tail)})) {offset} {size} = _
    rw [wordArena_append]
    have slice :
        readBytes (wordArena {words(constants)} ++ wordArena {words(tail)}) {offset} {size} =
        readBytes (wordArena {words(constants)}) {offset} {size} := by
      unfold readBytes
      apply List.map_congr_left
      intro i hi
      have lt := List.mem_range.mp hi
      have within : {offset}+i < (wordArena {words(constants)}).length := by simp; omega
      rw [List.getElem?_append_left within]
    rw [slice]
    decide +kernel
  have found : code.get? {last.inst.pc} = some 0xfd := by decide +kernel
  have schema : EVM.State.available tvWorldConfig 0xfd = ⟨true,0,0,2,0⟩ := rfl
  have done : EVM.State.instruction tvWorldConfig journal ({frame(last, "balance")}) =
      .ok (.done ⟨.reverted,{expected},journal,balance,0⟩) := by
    have step := code.instruction_eq tvWorldConfig journal ({frame(last, "balance")})
    simp only [decoderFrame] at step ⊢
    rw [step]
    simp at active growth data
    simp [decoderFrame, EVM.State.instructionWithImage,EVM.State.pureOp,EVM.State.withMemory,
      EVM.State.next,EVM.step,dispatch_size,found,schema,active,growth,data,
      Fork.atLeast,Fork.ordinal,Bind.bind,Except.bind] <;> rfl
  exact EVM.State.run_done work done

#print axioms {label}Returns
"""
