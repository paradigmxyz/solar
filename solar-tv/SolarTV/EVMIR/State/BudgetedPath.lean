import SolarTV.EVMIR.State.MeteredPrefix

/-! Instruction paths with distinct admission and debit amounts. SSTORE can
require more gas than it spends. The path theorem applies to every incoming
balance and rolls all frame effects back on exhaustion. -/

namespace SolarTV.EVM.State.BudgetedPath
open EvmRules SolarTV.Execution

structure Debit where
  minimum : Nat
  cost : Nat
  deriving DecidableEq

def required : List Debit → Nat
  | [] => 0
  | d :: rest => max d.minimum (d.cost + required rest)

def charged (path : List Debit) : Nat := (path.map Debit.cost).sum

theorem required_exact (cost : Nat) (rest : List Debit) :
    required (⟨cost,cost⟩ :: rest) = cost + required rest := by
  change max cost (cost + required rest) = cost + required rest
  omega

theorem required_sentry (sentry cost : Nat) (rest : List Debit) :
    required (⟨max sentry cost,cost⟩ :: rest) = max sentry (cost + required rest) := by
  change max (max sentry cost) (cost + required rest) = max sentry (cost + required rest)
  omega

theorem required_nil : required [] = 0 := rfl

structure Exit where
  stop : Stop
  data : List Byte
  world : Journal
  deriving DecidableEq

def Exit.result (exit : Exit) (gas : Nat) : Result :=
  ⟨exit.stop,exit.data,exit.world,gas,0⟩

def expected (path : List Debit) (exit : Exit) (world : Journal) (gas : Nat) : Result :=
  if gas < required path then ⟨.outOfGas,[],world,0,0⟩ else
    exit.result (gas - charged path)

inductive Checked (cfg : Config) : Journal → Frame → List Debit → Exit → Prop where
  | halt {world frame exit fee}
      (bounded : fee.cost ≤ fee.minimum)
      (checks : ∀ balance, instruction cfg world {frame with gas := balance} =
        if balance < fee.minimum then stepExhausted world
        else .ok (.done (exit.result (balance-fee.cost)))) :
      Checked cfg world frame [fee] exit
  | next {world frame newWorld newFrame fee debits exit}
      (bounded : fee.cost ≤ fee.minimum)
      (checks : ∀ balance, instruction cfg world {frame with gas := balance} =
        if balance < fee.minimum then stepExhausted world
        else .ok (.next newWorld {newFrame with gas := balance-fee.cost}))
      (tail : Checked cfg newWorld newFrame debits exit) :
      Checked cfg world frame (fee :: debits) exit

theorem Checked.runs {cfg : Config} {world frame debits exit}
    (path : Checked cfg world frame debits exit) (fuel gas : Nat) :
    ∃ result, run (fuel+debits.length) cfg world {frame with gas := gas} = .ok result ∧
      {result with work := 0} = expected debits exit result.world gas ∧
      (gas < required debits → result.stop = .outOfGas) := by
  induction path generalizing gas with
  | @halt world frame exit fee bounded check =>
    simp only [List.length_cons, List.length_nil, Nat.zero_add, required, Nat.add_zero,
      Nat.max_eq_left bounded]
    by_cases paid : gas < fee.minimum
    · rw [run_done fuel (by simpa [paid, stepExhausted] using check gas)]
      exact ⟨_,rfl,by simp [expected,required,Nat.max_eq_left bounded,paid],fun _ => rfl⟩
    · rw [run_done fuel (by simpa [paid] using check gas)]
      exact ⟨_,rfl,by simp [expected,required,Nat.max_eq_left bounded,paid,charged,Exit.result],
        fun h => (paid h).elim⟩
  | @next world frame newWorld newFrame fee rest exit bounded check tail ih =>
    simp only [List.length_cons, Nat.add_succ, required]
    by_cases first : gas < fee.minimum
    · rw [run_done (fuel+rest.length) (by simpa [first, stepExhausted] using check gas)]
      have short : gas < max fee.minimum (fee.cost + required rest) := by omega
      exact ⟨_,rfl,by simp [expected,required,short],fun _ => rfl⟩
    · rw [run_next (fuel+rest.length) (by simpa [first] using check gas)]
      obtain ⟨result,steps,shape,stopped⟩ := ih (gas-fee.cost)
      rw [steps]
      simp only [Except.bind,Bind.bind,Except.pure,Pure.pure]
      -- A debit's own charge and the remainder's admission must both fit.
      by_cases enough : fee.cost + required rest ≤ gas
      · have restPaid : ¬ gas-fee.cost < required rest := by omega
        have totalPaid : ¬ gas < max fee.minimum (fee.cost + required rest) := by omega
        refine ⟨_,rfl,?_,fun h => (totalPaid h).elim⟩
        simpa [expected,required,restPaid,totalPaid,charged,Exit.result,
          Nat.sub_sub,Nat.add_assoc] using shape
      · have paidCost : fee.cost ≤ gas := by omega
        have restShort : gas-fee.cost < required rest := by omega
        have totalShort : gas < max fee.minimum (fee.cost + required rest) := by omega
        exact ⟨_,rfl,by simpa [expected,required,restShort,totalShort] using shape,
          fun _ => stopped restShort⟩

def settle (initial : Journal) (result : Result) : Result :=
  if result.stop = .success then {result with world := finalize result.world, work := 0}
  else
    let gas := if result.stop = .exception || result.stop = .outOfGas then 0 else result.gas
    {result with world := initial, work := 0, gas := gas}

theorem Checked.execute {cfg : Config} {world frame debits exit}
    (path : Checked cfg world frame debits exit) (fuel gas : Nat) :
    (State.execute (fuel+debits.length) cfg world {frame with gas := gas}).map
        (fun r => {r with work := 0}) =
      .ok (settle world (expected debits exit world gas)) := by
  obtain ⟨result,runs,shape,stopped⟩ := path.runs fuel gas
  have post :
      (State.execute (fuel+debits.length) cfg world {frame with gas := gas}).map
        (fun r => {r with work := 0}) = .ok (settle world result) := by
    unfold State.execute settle
    simp only [runs, Bind.bind, Except.bind]
    split <;> rfl
  rw [post]
  by_cases short : gas < required debits
  · have stop := stopped short
    have data : result.data = [] := by
      have field := congrArg Result.data shape
      simpa [expected,short] using field
    simp [stop, data, expected, short, settle]
  · have eq : {result with work := 0} = exit.result (gas - charged debits) := by
      simpa only [expected,short,ite_false] using shape
    have clearWork : settle world result = settle world {result with work := 0} := by
      unfold settle
      split <;> rfl
    rw [clearWork, eq]
    simp only [expected,short,ite_false]

/-- Extend an exact-charge prefix with a path whose admission checks can
exceed the corresponding charges. Settlement uses the original journal. -/
theorem Checked.executeAfterPrefix {cfg : Config} {initial middle : Journal}
    {start cut : Frame} {count price : Nat} {suffix : List Debit} {exit : Exit}
    (tail : Checked cfg middle cut suffix exit)
    (head : MeteredPrefix cfg initial start middle cut count price) (fuel gas : Nat) :
    (State.execute (fuel + (count + suffix.length)) cfg initial {start with gas := gas}).map
        (fun r => {r with work := 0}) =
      .ok (if gas < price + required suffix then ⟨.outOfGas,[],initial,0,0⟩
        else settle initial (exit.result (gas - (price + charged suffix)))) := by
  have joined : ∃ path, Checked cfg initial start path exit ∧
      path.length = count + suffix.length ∧ required path = price + required suffix ∧
      charged path = price + charged suffix := by
    induction head with
    | done => exact ⟨suffix, tail, by simp, by simp, by simp⟩
    | @step _ _ _ _ _ _ _ _ charge debit _ ih =>
      obtain ⟨rest, checked, length, need, cost⟩ := ih tail
      refine ⟨⟨charge,charge⟩ :: rest, .next (Nat.le_refl _) debit checked, ?_, ?_, ?_⟩
      · simp only [List.length_cons, length]
        omega
      · rw [required_exact, need, Nat.add_assoc]
      · change charge + charged rest = _
        omega
  obtain ⟨path, checked, length, need, cost⟩ := joined
  have result := checked.execute fuel gas
  rw [length] at result
  by_cases short : gas < price + required suffix <;>
    simpa [expected, need, cost, short, settle] using result

end SolarTV.EVM.State.BudgetedPath
