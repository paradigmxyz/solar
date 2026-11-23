import SolarTV.EVMIR.State.MeteredPrefix

/-! Exact EVM gas accounting up to an optional instruction boundary. A CALL
runs its child before resuming the caller, so the prefix includes unused
child gas, returndata, reentrant state changes and failures. The work counter
is evaluator fuel. It is separate from the charged EVM gas. -/

namespace SolarTV.EVM.State.GasTrace
open EvmRules SolarTV.Execution

structure MessageDebit where
  prepared : Nat
  requested : Nat
  charged : Nat
  supplied : Nat
  returned : Nat

structure Debit where
  pc : Nat
  opcode : Byte
  before : Nat
  after : Nat
  message : Option MessageDebit

def Debit.net (row : Debit) : Int := (row.before : Int) - row.after

inductive Exit where
  | reached (remainingWork : Nat) (world : Journal) (frame : Frame) (work : Nat)
  | halted (result : Result)

def Exit.gas : Exit → Nat
  | .reached _ _ frame _ => frame.gas
  | .halted result => result.gas

def Exit.at (target : Nat) : Exit → Prop
  | .reached _ _ frame _ => frame.pc = target
  | .halted _ => True

def Exit.addWork (work : Nat) : Exit → Exit
  | .reached remaining world frame spent => .reached remaining world frame (spent+work)
  | .halted result => .halted {result with work := result.work+work}

def Exit.resume (cfg : Config) : Exit → Run
  | .reached remaining world frame spent => do
    let result ← run remaining cfg world frame
    return {result with work := result.work+spent}
  | .halted result => .ok result

theorem Exit.resume_addWork (cfg : Config) (work : Nat) (exit : Exit) :
    (exit.addWork work).resume cfg = (exit.resume cfg).map (fun result =>
      {result with work := result.work+work}) := by
  cases exit with
  | halted result => rfl
  | reached remaining world frame spent =>
    cases result : run remaining cfg world frame <;> simp [Exit.addWork,Exit.resume,result,Except.map,
      Bind.bind,Except.bind,Pure.pure,Except.pure, Nat.add_assoc]

structure Report where
  exit : Exit
  debits : List Debit

/-- Recover the gas returned by a child from the priced parent frame and
the actual continuation. A depth or funds rejection has no forwarded budget. -/
def callDebit (cfg : Config) (signal : Step) (after : Nat) : Option MessageDebit := do
  let .call message _ _ requested _ world parent := signal | none
  if parent.environment.depth ≥ 1024 ||
      ((message.kind == .call || message.kind == .callcode) &&
        (account world parent.environment.address).balance.ult
          message.value) then none else
  let budget ← Calls.forwardGas cfg.fork parent.gas requested (Calls.hasValue message)
  return ⟨parent.gas,requested,budget.charged,budget.supplied,after-(parent.gas-budget.charged)⟩

/-- For a forwarded message, the reported unused gas is the actual amount
returned by the child, including when the child reverts or exhausts its gas. -/
theorem returnedGas (cfg : Config) (execute : ChildExecutor)
    (message : Calls.Message) (outOffset outSize requested : Nat) (code : ExecutionCode)
    (world : Journal) (parent : Frame) (child : MessageOutcome) (budget : Calls.Budget)
    (admitted : (decide (parent.environment.depth ≥ 1024) ||
      ((message.kind == .call || message.kind == .callcode) &&
       (account world parent.environment.address).balance.ult
        message.value)) = false)
    (forwarded : Calls.forwardGas cfg.fork parent.gas requested (Calls.hasValue message) = some budget)
    (executed : executeCall execute cfg (warmAddress world message.destination)
      parent.environment message code budget.supplied = .ok child) :
    let signal := Step.call message outOffset outSize requested code world parent
    (advance cfg execute signal).map (fun
      | .inl _ => none
      | .inr (_,_,next) => callDebit cfg signal next.gas) =
      .ok (some ⟨parent.gas,requested,budget.charged,budget.supplied,child.gas⟩) := by
  simp only [advance,callDebit,admitted,Bool.false_eq_true,ite_false,forwarded,executed,
    Bind.bind,Except.bind,Pure.pure,Except.pure,Except.map,Nat.add_sub_cancel_left,Option.bind_some]

/-- The cut itself executes no instruction. Child evaluation uses the same
executor and work accounting as the full bytecode interpreter. -/
def toBoundary (cfg : Config) (target : Nat) (fuel : Nat) (world : Journal) (frame : Frame) :
    Except String Report := do
  if frame.pc = target then
    return ⟨.reached fuel world frame 0,[]⟩
  match fuel with
  | 0 => .error "world instruction/depth evaluation limit"
  | work+1 =>
    let signal ← instruction cfg world frame
    let op := (frame.code[frame.pc]?).getD 0
    match ← advance cfg (run work cfg) signal with
    | .inl result =>
      return ⟨.halted {result with work := result.work+1},
        [⟨frame.pc,op,frame.gas,result.gas,callDebit cfg signal result.gas⟩]⟩
    | .inr (spent,world,next) =>
      let rest ← toBoundary cfg target (work-spent) world next
      return ⟨rest.exit.addWork (spent+1),
        ⟨frame.pc,op,frame.gas,next.gas,callDebit cfg signal next.gas⟩ :: rest.debits⟩
termination_by fuel
decreasing_by omega

/-- Resuming a reached cut, or consuming a terminal result, is equivalent
to uninterrupted execution, even when arbitrary children call back. -/
theorem resume_until (cfg : Config) (target fuel : Nat) (world : Journal) (frame : Frame) :
    ((toBoundary cfg target fuel world frame) >>= fun report => report.exit.resume cfg) =
      run fuel cfg world frame := by
  induction fuel using Nat.strongRecOn generalizing world frame with
  | ind fuel ih =>
    rw [toBoundary.eq_def]
    by_cases cut : frame.pc = target
    · simp only [cut,ite_true,Bind.bind,Except.bind,Exit.resume]
      cases result : run fuel cfg world frame <;>
        simp [result,Pure.pure,Except.pure]
    · simp only [cut,ite_false]
      cases fuel with
      | zero => simp [run,Bind.bind,Except.bind]
      | succ work =>
        rw [run]
        cases op : instruction cfg world frame with
        | error _ => rfl
        | ok signal =>
          simp only [Bind.bind,Except.bind]
          cases act : advance cfg (run work cfg) signal with
          | error _ => rfl
          | ok effect =>
            cases effect with
            | inl terminal => simp [Exit.resume,Pure.pure,Except.pure]
            | inr pending =>
              obtain ⟨spent,nextWorld,next⟩ := pending
              have step := ih (work-spent) (by omega) nextWorld next
              cases found : toBoundary cfg target (work-spent) nextWorld next with
              | error _ =>
                simp only [found,Bind.bind,Except.bind] at step ⊢
                simp [← step]
              | ok rest =>
                simp only [found,Bind.bind,Except.bind] at step ⊢
                simp only [Pure.pure,Except.pure,Exit.resume_addWork,← step]
                cases rest.exit.resume cfg <;> simp [Except.map,Nat.add_assoc]

/-- Sum of exact instruction debits plus the available gas equals the ABI
frame's starting gas. Signed debits preserve credits such as value stipends. -/
theorem until_accounted (cfg : Config) (target fuel : Nat) (world : Journal) (frame : Frame)
    (report : Report) (checked : toBoundary cfg target fuel world frame = .ok report) :
    report.exit.at target ∧
      (report.debits.map Debit.net).sum + (report.exit.gas : Int) = (frame.gas : Int) := by
  induction fuel using Nat.strongRecOn generalizing world frame report with
  | ind fuel ih =>
    rw [toBoundary.eq_def] at checked
    by_cases cut : frame.pc = target
    · simp only [cut,ite_true,Pure.pure,Except.pure,Except.ok.injEq] at checked
      rw [← checked]
      simp [Exit.gas,Exit.at,cut]
    · simp only [cut,ite_false] at checked
      cases fuel with
      | zero => contradiction
      | succ work =>
        cases op : instruction cfg world frame with
        | error _ => simp [op,Bind.bind,Except.bind] at checked
        | ok signal =>
          simp only [op,Bind.bind,Except.bind] at checked
          cases act : advance cfg (run work cfg) signal with
          | error _ => simp [act] at checked
          | ok effect =>
            simp only [act] at checked
            cases effect with
            | inl terminal =>
              simp only [Pure.pure,Except.pure,Except.ok.injEq] at checked
              rw [← checked]
              simp [Debit.net,Exit.gas,Exit.at]
            | inr pending =>
              obtain ⟨spent,nextWorld,next⟩ := pending
              cases found : toBoundary cfg target (work-spent) nextWorld next with
              | error _ => simp [found] at checked
              | ok rest =>
                have tail := ih (work-spent) (by omega) nextWorld next rest found
                simp only [found,Pure.pure,Except.pure,Except.ok.injEq] at checked
                rw [← checked]
                obtain ⟨atCut,balanced⟩ := tail
                constructor
                · cases h : rest.exit <;>
                    simp [Exit.at,Exit.addWork,h] at atCut ⊢ <;> exact atCut
                · cases h : rest.exit <;>
                    simp [Debit.net,Exit.gas,Exit.addWork,h] at balanced ⊢ <;> omega

end SolarTV.EVM.State.GasTrace
