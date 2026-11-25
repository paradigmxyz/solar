import SolarTV.EVMIR.State.Transactions.Settlement

/-! Amsterdam keeps the block's execution and state gas counters separately.
Receipt gas tracks cumulative post-refund charges, with the calldata floor.
The block header records the larger of the two block counters. -/

namespace SolarTV.EVM.Transaction.Settlement

structure BlockGas where
  execution : Nat := 0
  state : Nat := 0
  receipts : Nat := 0
  deriving DecidableEq

def BlockGas.header (used : BlockGas) : Nat := max used.execution used.state

def BlockGas.CanInclude (used : BlockGas) (blockLimit txLimit : Nat) : Prop :=
  min txLimit (2^24) ≤ blockLimit - used.execution ∧ txLimit ≤ blockLimit - used.state

instance (used : BlockGas) (blockLimit txLimit : Nat) :
    Decidable (used.CanInclude blockLimit txLimit) := by
  unfold BlockGas.CanInclude
  infer_instance

def BlockGas.record (used : BlockGas) (gas : Gas) : BlockGas :=
  ⟨used.execution + gas.execution,used.state + gas.stateSpent,used.receipts + gas.receipt⟩

theorem included_stays_within_block (used : BlockGas) (gas : Gas) (blockLimit txLimit : Nat)
    (valid : used.header ≤ blockLimit) (admitted : used.CanInclude blockLimit txLimit)
    (exec : gas.execution ≤ min txLimit (2^24)) (state : gas.stateSpent ≤ txLimit) :
    (used.record gas).header ≤ blockLimit := by
  have execution := Nat.le_trans (Nat.le_max_left used.execution used.state) valid
  have stateUsed := Nat.le_trans (Nat.le_max_right used.execution used.state) valid
  obtain ⟨mayExecute,mayWrite⟩ := admitted
  have executed := Nat.le_trans exec mayExecute
  have written := Nat.le_trans state mayWrite
  exact Nat.max_le.mpr ⟨Nat.add_le_of_le_sub' execution executed,
    Nat.add_le_of_le_sub' stateUsed written⟩

theorem state_capacity_cannot_pay_execution (used : BlockGas) (blockLimit txLimit : Nat)
    (short : blockLimit - used.execution < min txLimit (2^24)) :
    ¬ used.CanInclude blockLimit txLimit :=
  fun admitted => Nat.not_le_of_gt short admitted.1

theorem execution_capacity_cannot_pay_state (used : BlockGas) (blockLimit txLimit : Nat)
    (short : blockLimit - used.state < txLimit) :
    ¬ used.CanInclude blockLimit txLimit :=
  fun admitted => Nat.not_le_of_gt short admitted.2

end SolarTV.EVM.Transaction.Settlement
