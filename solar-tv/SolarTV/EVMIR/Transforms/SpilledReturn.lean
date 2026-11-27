import SolarTV.EVMIR.MemoryWrites
import SolarTV.EVMIR.State.MeteredPrefix

/-! Carry a concrete continuation through its private memory slot. The
caller supplies the decoded instructions and disjointness for intervening
writes. The load boundary compares both stack placement and physical gas. -/

namespace SolarTV.EVM.SpilledReturn
open EvmRules SolarTV.Execution

def saved (memory : List Byte) (slot continuation : Word) : Prop :=
  readWord memory slot.toNat = continuation

theorem preserve (memory grown bytes : List Byte) (slot continuation : Word)
    (destination count : Nat)
    (expanded : expand memory destination count = .ok grown)
    (disjoint : destination + bytes.length ≤ slot.toNat ∨ slot.toNat + 32 ≤ destination)
    (before : saved memory slot continuation) :
    saved (writeBytes grown destination bytes) slot continuation := by
  unfold saved at *
  rw [readWord_writeBytes_disjoint _ _ _ _ disjoint,
    Memory.Projection.load_expanded _ _ _ _ _ expanded]
  exact before

/-- The emitted MSTORE records the supplied return word in its selected
slot. An unpaid store produces the physical out-of-gas result. -/
theorem store (cfg : State.Config) (world : Journal) (frame : State.Frame)
    (grown : List Byte) (slot continuation : Word) (tail : Stack) (base dynamic : Nat)
    (opcode : State.available cfg 0x52 = ⟨true,base,dynamic,2,0⟩)
    (found : frame.code[frame.pc]? = some (0x52 : Byte))
    (stack : frame.stack = slot :: continuation :: tail)
    (fits : (slot :: continuation :: tail).length ≤ 1024)
    (expanded : expand frame.memory slot.toNat 32 = .ok grown) :
    let charge := base + State.memoryCharge frame.memory slot.toNat 32
    let memory := writeBytes grown slot.toNat (wordBytes continuation)
    State.instruction cfg world frame =
        (if frame.gas < charge then State.stepExhausted world
         else .ok (.next world {frame with
           pc := frame.pc+1, stack := tail, memory, gas := frame.gas-charge})) ∧
      saved memory slot continuation := by
  dsimp only
  constructor
  · rw [Memory.Projection.store_instruction cfg world frame slot continuation tail false base dynamic
      opcode found stack fits]
    simp [State.withMemory, expanded, State.next, Bind.bind, Except.bind]
  · exact readWord_writeWord _ _ _ (Memory.Projection.expanded_active _ _ _ _ (by decide) expanded)

/-- The emitted MLOAD reinstates the saved continuation at the stack top.
Both the byte address and alignment of the arena are taken from the cut. -/
theorem reload (cfg : State.Config) (world : Journal) (frame : State.Frame)
    (grown : List Byte) (slot continuation : Word) (tail : Stack) (base dynamic : Nat)
    (opcode : State.available cfg 0x51 = ⟨true,base,dynamic,1,1⟩)
    (found : frame.code[frame.pc]? = some (0x51 : Byte))
    (stack : frame.stack = slot :: tail) (fits : (slot :: tail).length ≤ 1024)
    (aligned : frame.memory.length % 32 = 0)
    (expanded : expand frame.memory slot.toNat 32 = .ok grown)
    (before : saved frame.memory slot continuation) :
    let charge := base + State.memoryCharge frame.memory slot.toNat 32
    State.instruction cfg world frame =
      if frame.gas < charge then State.stepExhausted world
      else .ok (.next world {frame with
        pc := frame.pc+1, stack := continuation :: tail, memory := grown, gas := frame.gas-charge}) := by
  have within : slot.toNat + 32 ≤ 65536 :=
    Nat.le_trans (Memory.Projection.expanded_active _ _ _ _ (by decide) expanded)
      (Memory.Projection.expanded_bound _ _ _ _ aligned expanded)
  rw [Memory.Projection.load_instruction 65536 cfg world frame slot tail base dynamic
    opcode found stack within fits]
  have value : readWord grown slot.toNat = continuation :=
    (Memory.Projection.load_expanded _ _ _ _ _ expanded).trans before
  simp [State.withMemory, expanded, State.next, Bind.bind, Except.bind,
    Memory.Projection.load_project, within, value]

/-- A restored continuation is consumed by the fetched JUMP. A real
instruction boundary supplies the destination check. -/
theorem jump (cfg : State.Config) (world : Journal) (frame : State.Frame)
    (continuation : Word) (tail : Stack) (base dynamic : Nat)
    (opcode : State.available cfg 0x56 = ⟨true,base,dynamic,1,0⟩)
    (found : frame.code[frame.pc]? = some (0x56 : Byte))
    (valid : State.validJump frame.code continuation = true)
    (stack : frame.stack = continuation :: tail) (fits : (continuation :: tail).length ≤ 1024) :
    State.instruction cfg world frame =
      if frame.gas < base then State.stepExhausted world
      else .ok (.next world {frame with
        pc := continuation.toNat, stack := tail, gas := frame.gas-base}) := by
  have present := (List.getElem?_eq_some_iff.mp found).1
  have height : ¬tail.length > 1024 := by simp only [List.length_cons] at fits; omega
  simp [State.instruction, found, State.instructionWithImage, Nat.not_le_of_gt present,
    opcode, stack, height, valid, State.pureOp]

/-- Bind a reloaded return address to the following JUMP and account for
both debits. The prefix supplies the ordinary kernel lemmas for all paid
and unpaid balances, including exhaustion after the load succeeds. -/
theorem resume (cfg : State.Config) (world : Journal) (frame : State.Frame)
    (grown : List Byte) (slot continuation : Word) (tail : Stack)
    (loadBase jumpBase loadDynamic jumpDynamic : Nat)
    (loadSchema : State.available cfg 0x51 = ⟨true,loadBase,loadDynamic,1,1⟩)
    (jumpSchema : State.available cfg 0x56 = ⟨true,jumpBase,jumpDynamic,1,0⟩)
    (loadByte : frame.code[frame.pc]? = some (0x51 : Byte))
    (jumpByte : frame.code[frame.pc+1]? = some (0x56 : Byte))
    (valid : State.validJump frame.code continuation = true)
    (stack : frame.stack = slot :: tail) (fits : (slot :: tail).length ≤ 1024)
    (aligned : frame.memory.length % 32 = 0)
    (expanded : expand frame.memory slot.toNat 32 = .ok grown)
    (before : saved frame.memory slot continuation) :
    State.MeteredPrefix cfg world frame world
      {frame with pc := continuation.toNat, stack := tail, memory := grown}
      2 (loadBase + State.memoryCharge frame.memory slot.toNat 32 + jumpBase) := by
  let loaded := {frame with pc := frame.pc+1, stack := continuation :: tail, memory := grown}
  have loadDebit (balance : Nat) := reload cfg world {frame with gas := balance}
    grown slot continuation tail loadBase loadDynamic loadSchema loadByte stack fits aligned
    expanded before
  have jumpDebit (balance : Nat) := jump cfg world {loaded with gas := balance}
    continuation tail jumpBase jumpDynamic jumpSchema jumpByte valid rfl fits
  have tailTrace : State.MeteredPrefix cfg world loaded world
      {frame with pc := continuation.toNat, stack := tail, memory := grown} 1 jumpBase := by
    simpa only [Nat.add_zero] using State.MeteredPrefix.step jumpDebit
      (State.MeteredPrefix.done world
        {frame with pc := continuation.toNat, stack := tail, memory := grown})
  exact State.MeteredPrefix.step loadDebit tailTrace

end SolarTV.EVM.SpilledReturn
