import SolarTV.MIR.State.Interpreter

/-! A checked prefix of direct MIR execution. The prefix fixes each successor
and its work charge, while the ordinary interpreter runs the arbitrary suffix. -/

namespace SolarTV.MIR.State
open EvmRules

inductive Prefix (cfg : Config) (functions : Functions) : Machine → Machine → Nat → Prop where
  | done (machine : Machine) : Prefix cfg functions machine machine 0
  | step {before after reached extra rest} :
      (∀ budget, extra ≤ budget → transition budget cfg functions before = .ok (.next extra after)) →
      Prefix cfg functions after reached rest →
      Prefix cfg functions before reached (rest + extra + 1)

theorem Prefix.resume {cfg : Config} {functions : Functions} {before reached : Machine}
    {spent : Nat} (trace : Prefix cfg functions before reached spent) (work : Nat) :
    run (work + spent) cfg functions before = run work cfg functions reached := by
  induction trace with
  | done => simp
  | @step before after reached extra rest checked _ ih =>
    have enough : extra ≤ work + rest + extra := by omega
    rw [show work + (rest + extra + 1) = (work + rest + extra) + 1 by omega,
      run_next _ (checked _ enough), ite_eq_right (by omega),
      show work + rest + extra - extra = work + rest by omega]
    exact ih

/-- Resume ordinary execution with the original entry frame as its rollback checkpoint. -/
theorem Prefix.resumeExecute {cfg : Config} {functions : Functions} {entry spent : Nat}
    {frame initial : Frame} {reached : Machine}
    (trace : Prefix cfg functions ⟨{function := entry}, [], initial⟩ reached spent)
    (depth : cfg.callDepth ≠ 0)
    (ready : start functions entry frame.arguments frame = .ok initial) (work : Nat) :
    execute (work + spent) cfg functions entry frame = conclude cfg frame (run work cfg functions reached) := by
  simp only [execute, depth, ite_false, ready, Bind.bind, Except.bind]
  rw [trace.resume work]

end SolarTV.MIR.State
