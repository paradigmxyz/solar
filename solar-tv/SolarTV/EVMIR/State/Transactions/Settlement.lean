import SolarTV.EVMIR.State.Transactions.Reservoir
import SolarTV.State.Journal

/-! Transaction gas settlement after the interpreter has committed or rolled back
the message frame. The caller supplies the final gas tracker and the unconditional
upfront state debit and authorization state credit. -/

namespace SolarTV.EVM.Transaction.Settlement
open EvmRules Execution Reservoir

structure Gas where
  spent : Nat
  stateSpent : Nat
  refund : Nat
  floor : Nat
  deriving DecidableEq

def Gas.receipt (gas : Gas) : Nat :=
  max (gas.spent - gas.refund) gas.floor

def Gas.execution (gas : Gas) : Nat :=
  max (gas.spent - gas.stateSpent) gas.floor

def finish (limit floor : Nat) (meter : Meter) (refund : Int)
    (initialState stateRefund : Nat) : Gas :=
  let spent := limit - meter.execution - meter.reservoir
  let stateSpent := min ((meter.stateSpent + (initialState : Int)).toNat) (2^63 - 1)
  ⟨spent, stateSpent - stateRefund, min refund.toNat (spent / 5), floor⟩

theorem receipt_bounded (limit floor : Nat) (meter : Meter) (refund : Int)
    (initialState stateRefund : Nat) (validFloor : floor ≤ limit) :
    (finish limit floor meter refund initialState stateRefund).receipt ≤ limit := by
  simp only [Gas.receipt, finish]
  omega

theorem refund_excludes_reservoir (limit floor : Nat) (meter : Meter)
    (refund : Int) (initialState stateRefund : Nat) :
    (finish limit floor meter refund initialState stateRefund).refund ≤
      (limit - meter.execution - meter.reservoir) / 5 := by
  exact Nat.min_le_right _ _

theorem state_credit_can_clear_charge (limit floor : Nat) (meter : Meter)
    (refund : Int) (initialState stateRefund : Nat)
    (settled : meter.stateSpent + (initialState : Int) ≤ 0) :
    (finish limit floor meter refund initialState stateRefund).stateSpent = 0 := by
  have zero : (meter.stateSpent + (initialState : Int)).toNat = 0 := by omega
  simp [finish, zero]

def credit (journal : Journal) (address : Address) (delta : Word) : Journal :=
  let old := account journal address
  if delta == 0 then journal else
    putAccount journal {old with address, balance := old.balance + delta}

/-- The supplied journal already includes the upfront gas purchase. A
beneficiary that was also the sender sees the reimbursement before the tip. -/
def fees (journal : Journal) (sender beneficiary : Address) (gas : Gas)
    (limit : Nat) (price baseFee : Word) : Journal :=
  let returned := BitVec.ofNat 256 (limit - gas.receipt) * price
  let tip := BitVec.ofNat 256 gas.receipt * BitVec.ofNat 256 (price.toNat - baseFee.toNat)
  credit (credit journal sender returned) beneficiary tip

theorem credited_balance (world : Journal) (address : Address) (delta : Word) :
    (account (credit world address delta) address).balance =
      (account world address).balance + delta := by
  by_cases h : delta = 0#256
  · simp [credit, h]
  · simp [credit, h, account, putAccount]

theorem sender_is_beneficiary (world : Journal) (owner : Address)
    (gas : Gas) (limit : Nat) (price baseFee : Word) :
    (account (fees world owner owner gas limit price baseFee) owner).balance =
      (account world owner).balance +
        BitVec.ofNat 256 (limit - gas.receipt) * price +
        BitVec.ofNat 256 gas.receipt * BitVec.ofNat 256 (price.toNat - baseFee.toNat) := by
  simp [fees, credited_balance]

end SolarTV.EVM.Transaction.Settlement
