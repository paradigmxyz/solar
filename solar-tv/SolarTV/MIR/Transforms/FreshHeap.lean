import SolarTV.MIR.State.Ops
import SolarTV.MIR.Universal

/-! Direct-state obligations for replacing an initial FMP read with a heap floor.

The heap floor is supplied by a named placement binding, never guessed from
memory. Its equality to the live FMP is an explicit producer obligation. The
reservation must be infallible, uninitialized and nonwrapping. Memory contents,
the journal, return data and every other live frame field remain arbitrary.

The 64 KiB bound describes the direct interpreter's supported memory domain.
These lemmas neither establish entry initialization at callers nor certify
backend placement or physical gas. A complete imported SSA proof must preserve
the suffix and establish defined executions, rather than accepting equal errors.
-/

namespace SolarTV.MIR.FreshHeap
open EvmRules SolarTV.Execution

def Binding (layout : State.Layout) (name : String) (floor : Word) : Prop :=
  ∃ placement, (layout.functions.find? (·.1 == name)).map Prod.snd = some placement ∧
    placement.heapFloor = some floor

/-- This condition is checked in a theorem, not inferred for arbitrary callers. -/
def Contract (frame : State.Frame) (floor : Word) (extent : Nat) : Prop :=
  Universal.MemoryValid frame ∧ 96 ≤ frame.memory.length ∧
    readWord frame.memory 64 = floor ∧ 128 ≤ floor.toNat ∧
    floor.toNat % 32 = 0 ∧ floor.toNat + extent ≤ 65536

theorem active_slot (memory : List Byte) (bounded : memory.length ≤ 65536)
    (aligned : memory.length % 32 = 0) (active : 96 ≤ memory.length) :
    expand memory 64 32 = .ok memory := by
  have length : 32 * (memory.length / 32) = memory.length := by omega
  have enough : (64 + 32 + 31) / 32 ≤ memory.length / 32 := by omega
  simp [expand, requiredWords, memoryWords, Nat.max_eq_left enough, length,
    show ¬memory.length > 65536 by omega]

theorem query (cfg : State.Config) (frame : State.Frame) (name : String) (floor : Word)
    (binding : Binding cfg.layout name floor) :
    State.layoutOperation cfg frame (.heapFloor name) [] =
      .ok (.returned (some floor,frame)) := by
  obtain ⟨placement,found,value⟩ := binding
  simp [State.layoutOperation, State.placementBinding, State.required, found,value,
    Except.bind, Bind.bind]

theorem reservation (frame : State.Frame) (floor : Word) (extent : Nat)
    (valid : Contract frame floor extent) :
    State.allocate frame (BitVec.ofNat 256 extent) false false =
      .ok (.returned (some floor,
        {frame with memory := writeBytes frame.memory 64 (wordBytes (floor + BitVec.ofNat 256 extent))})) := by
  obtain ⟨⟨bounded,aligned⟩,active,cursor,lower,alignment,range⟩ := valid
  have slot := active_slot frame.memory bounded aligned active
  have extentSmall : extent < 2^256 := by omega
  have sumSmall : floor.toNat + extent < 2^256 := by omega
  have bump : (floor + BitVec.ofNat 256 extent).toNat = floor.toNat + extent := by
    simp [BitVec.toNat_add, BitVec.toNat_ofNat, Nat.mod_eq_of_lt extentSmall,
      Nat.mod_eq_of_lt sumSmall]
  simp only [State.allocate,slot,Except.bind,Bind.bind,cursor,bump]
  simp [show ¬floor.toNat + extent < floor.toNat by omega,
    show ¬floor.toNat + extent > 2^64 - 1 by omega, Pure.pure, Except.pure]

/-- An explicit entry write establishes the cursor condition; it is not an
implicit action performed by the validator or an arbitrary caller assumption. -/
theorem initialized (frame : State.Frame) (floor : Word) (extent : Nat)
    (memory : Universal.MemoryValid frame) (active : 96 ≤ frame.memory.length)
    (lower : 128 ≤ floor.toNat) (aligned : floor.toNat % 32 = 0)
    (range : floor.toNat + extent ≤ 65536) :
    Contract {frame with memory := writeBytes frame.memory 64 (wordBytes floor)} floor extent := by
  refine ⟨?_,?_,?_,lower,aligned,range⟩
  · simpa [Universal.MemoryValid] using memory
  · simpa using active
  · exact readWord_writeWord frame.memory 64 floor active

/-- Every admitted source extent has valid frames. This construction witnesses
non-vacuity only; the positive pair proof quantifies arbitrary live bytes. -/
theorem inhabited (frame : State.Frame) (extent : Nat) (bounded : extent ≤ 65536 - 128) :
    ∃ memory, Contract {frame with memory} (128 : Word) extent := by
  let initial := {frame with memory := List.replicate 96 (0 : Byte)}
  exact ⟨_, initialized initial 128 extent (by simp [Universal.MemoryValid,initial])
    (by simp [initial]) (by decide) (by decide) (by change 128 + extent ≤ 65536; omega)⟩

end SolarTV.MIR.FreshHeap
