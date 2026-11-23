import SolarTV.EVMIR.Transforms.MessageBoundary
import SolarTV.EVMIR.State.GasTrace
import SolarTV.State.MemoryLaws

/-! Exact Cancun gas at a GAS followed by a value-capable CALL with empty
input and output. Rejecting an unaffordable charge precedes child admission.
The split accounts for account warmth, value, a new account and the stipend. -/

namespace SolarTV.EVM.SampledCall
open EvmRules SolarTV.Execution
open State

/-- The arguments below the GAS result in the withdrawal suffix. -/
def arguments (target amount : Word) (rest : List Word) : List Word :=
  target :: amount :: 0 :: 0 :: 0 :: 0 :: rest

/-- Account and value debits, with no expansion for empty memory ranges. -/
def charge (cfg : Config) (world : Journal) (target : Address) (amount : Word) : Nat :=
  callBoundaryCharge cfg world ⟨.call,target,amount,[]⟩ 0xf1 0 0 0

def prepared (frame : Frame) (rest : List Word) (price : Nat) : Frame :=
  {frame with pc := frame.pc+2, stack := rest, gas := frame.gas-2-price}

theorem memory_empty (memory : List Byte) (offset : Nat) (limit : memory.length ≤ 65536) :
    expand memory offset 0 = .ok memory := by
  have words : 32 * (memory.length / 32) ≤ memory.length := by omega
  simp [expand, requiredWords, memoryWords, Nat.not_lt.mpr (Nat.le_trans words limit),
    Nat.sub_eq_zero_of_le words]

theorem decode (cfg : Config) (world : Journal) (frame : Frame)
    (target amount : Word) (rest : List Word)
    (fork : cfg.fork = .cancun) (schema : available cfg 0xf1 = ⟨true,2600,0,7,1⟩)
    (opcode : frame.code[frame.pc+1]? = some 0xf1)
    (stack : frame.stack = arguments target amount rest)
    (space : rest.length + 6 < 1024) (bounded : frame.memory.length ≤ 65536)
    (known : cfg.unknownCode ≠ some (target.setWidth 160)) :
    let address := target.setWidth 160
    instruction cfg world (MessageBoundary.afterGas frame) =
      if frame.environment.isStatic && amount != 0 then stepFailed world
      else if frame.gas - 2 < charge cfg world address amount then stepExhausted world
      else .ok (.call ⟨.call,address,amount,[]⟩ 0 0
        (BitVec.ofNat 256 (frame.gas-2)).toNat ⟨address,(account world address).code,false⟩
        (warmAddress world address) (prepared frame rest (charge cfg world address amount))) := by
  obtain ⟨within,_⟩ := List.getElem?_eq_some_iff.mp opcode
  simp [instruction, instructionWithImage, opcode, schema, Nat.not_le_of_gt within,
    stack, fork, Fork.atLeast, Fork.ordinal, pureOp, arguments, MessageBoundary.afterGas,
    show ¬ rest.length+1+1+1+1+1+1+1 < 7 by omega,
    show ¬ 1024 < rest.length+1 by omega, executionCode, delegationTarget, known,
    withMemory, memoryCharge, requiredWords, charge, memory_empty _ _ bounded, prepared,
    Bind.bind, Except.bind]

inductive Admission where
  | failed (reason : Stop)
  | rejected
  | entered (charged supplied : Nat)
  deriving DecidableEq

/-- Inspect the real instruction pair up to the child entry. Use the same
depth/funds rejection and EIP-150 calculation as the execution step. -/
def admit (cfg : Config) (world : Journal) (frame : Frame) : Except String Admission := do
  let sampled ← instruction cfg world frame
  match sampled with
  | .done result => return .failed result.stop
  | .next journal next =>
    let action ← instruction cfg journal next
    match action with
    | .done result => return .failed result.stop
    | .call .. =>
      if cfg.fork.atLeast .tangerineWhistle then
        -- The trace debit uses the interpreter's admission rule. Supply zero
        -- returned gas because this observation stops before child execution.
        return match GasTrace.callDebit cfg action 0 with
        | none => .rejected
        | some debit => .entered debit.charged debit.supplied
      else .error "expected EIP-150 call admission"
    | _ => .error "expected CALL"
  | _ => .error "expected GAS"

def expected (cfg : Config) (world : Journal) (frame : Frame) (target : Address)
    (amount : Word) : Admission :=
  let price := charge cfg world target amount
  if frame.gas < 2 then .failed .outOfGas
  else if frame.environment.isStatic && amount != 0 then .failed .exception
  else if frame.gas - 2 < price then .failed .outOfGas
  else if frame.environment.depth ≥ 1024 ||
      (account (warmAddress world target) frame.environment.address).balance.ult amount then .rejected
  else
    let left := frame.gas-2-price
    let cap := left-left/64
    .entered cap (cap + if amount != 0 then 2300 else 0)

/-- All gas budgets in the word domain. No supplied gas or requested gas
is assumed equal to a sample, since the actual GAS opcode produces it. -/
theorem exact (cfg : Config) (world : Journal) (frame : Frame)
    (target amount : Word) (rest : List Word)
    (fork : cfg.fork = .cancun)
    (gasSchema : available cfg 0x5a = ⟨true,2,0,0,1⟩)
    (callSchema : available cfg 0xf1 = ⟨true,2600,0,7,1⟩)
    (gasOp : frame.code[frame.pc]? = some 0x5a)
    (callOp : frame.code[frame.pc+1]? = some 0xf1)
    (stack : frame.stack = arguments target amount rest)
    (space : rest.length+6 < 1024) (bounded : frame.memory.length ≤ 65536)
    (known : cfg.unknownCode ≠ some (target.setWidth 160))
    (word : frame.gas < 2^256) :
    admit cfg world frame = .ok (expected cfg world frame (target.setWidth 160) amount) := by
  have live : frame.stack.length < 1024 := by
    simpa [stack, arguments, Nat.add_assoc] using space
  have request : (BitVec.ofNat 256 (frame.gas - 2)).toNat = frame.gas - 2 := by
    simp [Nat.mod_eq_of_lt (by omega : frame.gas - 2 < 2^256)]
  have cap : (frame.gas - 2 - charge cfg world (target.setWidth 160) amount) -
        (frame.gas - 2 - charge cfg world (target.setWidth 160) amount) / 64 ≤ frame.gas - 2 := by omega
  unfold admit expected
  rw [MessageBoundary.sample cfg world frame gasOp gasSchema live]
  split
  · rfl
  · simp only [Bind.bind, Except.bind]
    rw [decode cfg world frame target amount rest fork callSchema callOp stack space bounded known]
    by_cases prohibited : (frame.environment.isStatic && amount != 0) = true
    · simp only [prohibited, ite_true]
      rfl
    · by_cases short : frame.gas-2 < charge cfg world (target.setWidth 160) amount
      · simp only [prohibited, short, ite_true]
        rfl
      · simp only [prohibited, short, ite_false]
        simp [Pure.pure, Except.pure, prepared, request, Calls.hasValue, Calls.forwardGas,
          GasTrace.callDebit, fork, Fork.atLeast, Fork.ordinal, Nat.min_eq_right cap]
        by_cases rejected : 1024 ≤ frame.environment.depth ∨
          (account (warmAddress world (target.setWidth 160)) frame.environment.address).balance.ult amount = true
        · simp only [rejected, ite_true]
        · simp only [rejected, ite_false]

/-- The same live-in state at two resolved gas samples gives exactly the
same admission and child budget, even when the code images and PCs differ. -/
theorem paired (cfg : Config) (world : Journal) (frame : Frame)
    (before after : List Byte) (left right : Nat) (target amount : Word) (rest : List Word)
    (fork : cfg.fork = .cancun)
    (gasSchema : available cfg 0x5a = ⟨true,2,0,0,1⟩)
    (callSchema : available cfg 0xf1 = ⟨true,2600,0,7,1⟩)
    (leftGas : before[left]? = some 0x5a) (leftCall : before[left+1]? = some 0xf1)
    (rightGas : after[right]? = some 0x5a) (rightCall : after[right+1]? = some 0xf1)
    (stack : frame.stack = arguments target amount rest)
    (space : rest.length+6 < 1024) (bounded : frame.memory.length ≤ 65536)
    (known : cfg.unknownCode ≠ some (target.setWidth 160)) (word : frame.gas < 2^256) :
    admit cfg world {frame with code := before, pc := left} =
      admit cfg world {frame with code := after, pc := right} := by
  rw [exact cfg world {frame with code := before, pc := left} target amount rest
    fork gasSchema callSchema leftGas leftCall stack space bounded known word]
  rw [exact cfg world {frame with code := after, pc := right} target amount rest
    fork gasSchema callSchema rightGas rightCall stack space bounded known word]
  rfl

end SolarTV.EVM.SampledCall
