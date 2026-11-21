import SolarTV.MIR.State.Messages
import SolarTV.EVMIR.Transforms.CallBudget

/-! A shared resource certificate for a CALL boundary. EVM debits the input
and output ranges separately. MIR debits the final active-memory extent once.
Both consume the incoming budget before forwarding gas to the child. -/

namespace SolarTV.MIR.CallDebit
open EvmRules SolarTV.Execution MIR.State
open EVM.CallBudget (charged charged_add)

private theorem required_ge (memory : List Byte) (offset size : Nat) :
    memoryWords memory ≤ requiredWords memory offset size := by
  unfold requiredWords
  split
  · exact Nat.le_refl _
  · exact Nat.le_max_left _ _

private theorem expanded_words (memory expanded : List Byte) (offset size : Nat)
    (ok : expand memory offset size = .ok expanded) :
    memoryWords expanded = requiredWords memory offset size := by
  unfold expand at ok
  dsimp only at ok
  split at ok
  · contradiction
  · cases ok
    have sufficient := required_ge memory offset size
    simp only [memoryWords, List.length_append, List.length_replicate] at *
    omega

private theorem cost_mono {a b : Nat} (h : a ≤ b) :
    EVM.State.memoryCost a ≤ EVM.State.memoryCost b := by
  unfold EVM.State.memoryCost
  exact Nat.add_le_add (Nat.mul_le_mul_left _ h)
    (Nat.div_le_div_right (Nat.mul_le_mul h h))

theorem memoryCharge_zero (memory : List Byte) (offset size : Nat)
    (ok : expand memory offset size = .ok memory) :
    EVM.State.memoryCharge memory offset size = 0 := by
  unfold EVM.State.memoryCharge
  rw [← expanded_words _ _ _ _ ok, Nat.sub_self]

theorem charge_eq (ext : EVM.State.Config) (world : Journal) (message : Calls.Message)
    (opcode resolution : Nat) (memory input active : List Byte)
    (inputOffset inputSize outOffset outSize : Nat)
    (inputOk : expand memory inputOffset inputSize = .ok input)
    (outputOk : expand input outOffset outSize = .ok active) :
    EVM.State.callBoundaryCharge ext world message opcode resolution
        (memoryWords memory) (memoryWords active) =
      (EVM.State.callBoundaryCharge ext world message opcode resolution 0 0 +
        EVM.State.memoryCharge memory inputOffset inputSize) +
        EVM.State.memoryCharge input outOffset outSize := by
  have mid := expanded_words _ _ _ _ inputOk
  have last := expanded_words _ _ _ _ outputOk
  have growIn := cost_mono (required_ge memory inputOffset inputSize)
  have growOut := cost_mono (required_ge input outOffset outSize)
  unfold EVM.State.memoryCharge EVM.State.callBoundaryCharge
  rw [← mid, ← last]
  rw [← mid] at growIn
  rw [← last] at growOut
  simp only [EVM.State.memoryCost, Nat.mul_zero, Nat.zero_div, Nat.add_zero, Nat.sub_zero] at *
  omega

/-- Check the actual two EVM memory debits, including either unpaid leg. -/
theorem prepare_eq (ext : EVM.State.Config) (world : Journal) (frame : EVM.State.Frame)
    (message : Calls.Message) (opcode resolution : Nat) (input active : List Byte)
    (inputOffset inputSize outOffset outSize : Nat)
    (inputOk : expand frame.memory inputOffset inputSize = .ok input)
    (outputOk : expand input outOffset outSize = .ok active)
    (next : EVM.State.Frame → EVM.State.Action) :
    EVM.State.withMemory world frame
        (EVM.State.callBoundaryCharge ext world message opcode resolution 0 0)
        inputOffset inputSize (fun prepared => EVM.State.withMemory world prepared
          0 outOffset outSize next) =
      match charged frame.gas (EVM.State.callBoundaryCharge ext world message opcode resolution
          (memoryWords frame.memory) (memoryWords active)) with
      | none => EVM.State.stepExhausted world
      | some gas => next {frame with memory := active, gas} := by
  rw [charge_eq ext world message opcode resolution frame.memory input active
    inputOffset inputSize outOffset outSize inputOk outputOk, ← charged_add]
  simp only [EVM.State.withMemory, charged]
  split
  · rename_i unpaid
    simp [Nat.not_le_of_gt unpaid]
  · rename_i paid
    simp only [inputOk, Bind.bind, Except.bind, Nat.zero_add, Nat.le_of_not_gt paid,
      ite_true, Option.bind_some]
    split
    · rename_i unpaid
      simp [Nat.not_le_of_gt unpaid]
    · rename_i paid
      simp [outputOk, Nat.le_of_not_gt paid]

/-- Relate MIR's message ledger to the balance left by EVM's actual memory
preparation. The same fork and requested gas then give the same EIP-150 split.
The theorem takes the incoming gas as an input to the boundary. -/
theorem budget_eq (cfg : Config) (ext : EVM.State.Config) (frame : Frame)
    (message : Calls.Message) (opcode resolution : Nat) (input active : List Byte)
    (inputOffset inputSize outOffset outSize : Nat) (budgets : List Nat)
    (requested : Word) (incoming : Nat)
    (ledger : cfg.messageGas = some budgets)
    (selected : budgets[frame.messageGas.length]? = some incoming)
    (fork : cfg.fork = ext.fork)
    (inputOk : expand frame.memory inputOffset inputSize = .ok input)
    (outputOk : expand input outOffset outSize = .ok active) :
    callBudget cfg frame ext active message opcode resolution (some requested) =
      .ok ((charged incoming (EVM.State.callBoundaryCharge ext frame.world message opcode resolution
          0 0 + EVM.State.memoryCharge frame.memory inputOffset inputSize)).bind
        (fun left => charged left (EVM.State.memoryCharge input outOffset outSize)) |>.bind
        (fun available =>
          (Calls.forwardGas ext.fork available requested.toNat (Calls.hasValue message)).map
            (fun budget => (budget.supplied,available-budget.charged)))) := by
  rw [charged_add, ← charge_eq ext frame.world message opcode resolution frame.memory input active
    inputOffset inputSize outOffset outSize inputOk outputOk]
  simp only [callBudget, ledger, selected, required, Bind.bind, Except.bind,
    Option.map_some, Option.getD_some, fork]
  unfold charged
  split
  · rename_i unpaid
    simp [Nat.not_le_of_gt unpaid] <;> rfl
  · rename_i paid
    simp [Nat.le_of_not_gt paid] <;> rfl

theorem warmTwice (world : Journal) (address : Address) :
    warmAddress (warmAddress world address) address = warmAddress world address := by
  by_cases warm : address ∈ world.warmAddresses <;> simp [warmAddress, warm]

end SolarTV.MIR.CallDebit
