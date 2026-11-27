/-! EIP-8037 transaction initialization and gas accounting. Execution costs use
the execution pool exclusively. State costs consume the reservoir before
spilling into execution gas. Refills credit spilled execution gas before
the reservoir. Reverting a frame restores its net state charges.
https://eips.ethereum.org/EIPS/eip-8037 -/

namespace SolarTV.EVM.Transaction.Reservoir

structure Meter where
  execution : Nat
  reservoir : Nat
  -- A frame can undo a state creation charged by an earlier frame.
  stateSpent : Int := 0
  stateSpilled : Nat := 0
  deriving DecidableEq

def Meter.available (meter : Meter) : Nat := meter.execution + meter.reservoir

def begin (limit intrinsic : Nat) : Option Meter :=
  if intrinsic > min limit (2^24) || limit ≥ 2^32 then none else
  let gas := limit-intrinsic
  let bounded := min gas (2^24-intrinsic)
  some ⟨bounded,gas-bounded,0,0⟩

def Meter.spendExecution (meter : Meter) (cost : Nat) : Option Meter :=
  if meter.execution < cost then none else
    some {meter with execution := meter.execution-cost}

def Meter.spendState (meter : Meter) (cost : Nat) : Option Meter :=
  if meter.available < cost then none else
    let spill := cost-meter.reservoir
    some ⟨meter.execution-spill,meter.reservoir-cost,meter.stateSpent+cost,meter.stateSpilled+spill⟩

def Meter.refill (meter : Meter) (amount : Nat) : Meter :=
  let toExecution := min amount meter.stateSpilled
  ⟨meter.execution+toExecution,meter.reservoir+(amount-toExecution),
    meter.stateSpent-amount,meter.stateSpilled-toExecution⟩

def Meter.rollback (meter : Meter) : Meter :=
  ⟨meter.execution+meter.stateSpilled,
    ((meter.reservoir : Int)+meter.stateSpent-meter.stateSpilled).toNat,0,0⟩

inductive Charge where
  | execution (cost : Nat)
  | state (cost : Nat)
  | refill (amount : Nat)
  | rollback

def charge (meter : Meter) : Charge → Option Meter
  | .execution cost => meter.spendExecution cost
  | .state cost => meter.spendState cost
  | .refill amount => some (meter.refill amount)
  | .rollback => some meter.rollback

def trace (meter : Meter) : List Charge → List (Option Meter)
  | [] => [some meter]
  | debit :: rest => some meter :: match charge meter debit with
    | none => [none]
    | some next => trace next rest

theorem initialized (limit intrinsic : Nat) (meter : Meter)
    (ready : begin limit intrinsic = some meter) :
    meter.available = limit-intrinsic ∧ meter.execution ≤ 2^24-intrinsic := by
  simp only [begin] at ready
  split at ready
  · contradiction
  · simp only [Option.some.injEq] at ready
    subst meter
    exact ⟨Nat.add_sub_of_le (Nat.min_le_left _ _), Nat.min_le_right _ _⟩

theorem execution_cannot_draw_from_reservoir (meter : Meter) (cost : Nat)
    (short : meter.execution < cost) : meter.spendExecution cost = none := by
  simp [Meter.spendExecution,short]

theorem spend_state_balance (meter next : Meter) (cost : Nat)
    (paid : meter.spendState cost = some next) :
    next.available + cost = meter.available ∧ next.stateSpent = meter.stateSpent + cost := by
  simp only [Meter.spendState] at paid
  split at paid
  · contradiction
  · rename_i enough
    simp only [Option.some.injEq] at paid
    subst next
    simp only [Meter.available] at *
    constructor
    · omega
    · trivial

theorem reservoir_first (meter : Meter) (cost : Nat) (enough : cost ≤ meter.reservoir) :
    meter.spendState cost = some
      ⟨meter.execution,meter.reservoir-cost,meter.stateSpent+cost,meter.stateSpilled⟩ := by
  have available : ¬meter.available < cost := by unfold Meter.available; omega
  simp [Meter.spendState,available,Nat.sub_eq_zero_of_le enough]

/-- Rolling back a state charge restores its two debits, while execution
charges remain spent. The earlier ledger may itself contain state charges. -/
theorem rollback_state_charge (meter next : Meter) (cost : Nat)
    (paid : meter.spendState cost = some next) :
    next.rollback = meter.rollback := by
  simp only [Meter.spendState] at paid
  split at paid
  · contradiction
  · rename_i enough
    simp only [Option.some.injEq] at paid
    subst next
    simp only [Meter.rollback, Meter.available] at *
    congr 1
    · omega
    · apply congrArg Int.toNat
      omega

theorem refill_balance (meter : Meter) (amount : Nat) :
    (meter.refill amount).available = meter.available + amount ∧
      (meter.refill amount).stateSpent = meter.stateSpent - amount := by
  simp only [Meter.refill, Meter.available]
  constructor
  · have := Nat.min_le_left amount meter.stateSpilled
    omega
  · trivial

theorem refill_execution_first (meter : Meter) (amount : Nat)
    (fits : amount ≤ meter.stateSpilled) :
    meter.refill amount =
      ⟨meter.execution+amount,meter.reservoir,meter.stateSpent-amount,meter.stateSpilled-amount⟩ := by
  simp [Meter.refill, Nat.min_eq_left fits]

/-- A refill within a frame leaves its rollback checkpoint intact, including
when a prior frame paid for the state creation being undone. -/
theorem rollback_state_refill (meter : Meter) (amount : Nat) :
    (meter.refill amount).rollback = meter.rollback := by
  have toExecution_le_amount := Nat.min_le_left amount meter.stateSpilled
  have toExecution_le_spill := Nat.min_le_right amount meter.stateSpilled
  simp only [Meter.refill, Meter.rollback]
  congr 1
  · omega
  · apply congrArg Int.toNat
    omega

end SolarTV.EVM.Transaction.Reservoir
