import SolarTV.Machine

/-! A stage from two runs that end in the same result.

The procedure of [Tactic.lean](Tactic.lean) proves, for concrete programs, that the first ends
at fuel `n` on every call in scope and that both programs give the same result there, at fuels
`n` and `m`. Since a layer's runs are deterministic and only gain ends with fuel, that is a
stage of their fuel-free semantics, whatever fuel the first program's result was reached at. -/

namespace SolarTV.Milestone.W1

open Machine

/-- Every call: the w1 stages hold for any calldata, call value and storage. -/
def anyCall : Call → Prop := fun _ => True

theorem stage_of_runs {A B : Layer} {o : Oracle} {p : A.Prog} {q : B.Prog}
    {scope : Call → Prop} (n m : Nat)
    (ends : ∀ call, scope call → (A.run o p call n).isSome = true)
    (same : ∀ call, scope call → A.run o p call n = B.run o q call m) :
    Stage (A.sem o) (B.sem o) p q scope := by
  apply stage_of_certified
  intro call inScope r ran
  obtain ⟨r0, h0⟩ := Option.isSome_iff_exists.mp (ends call inScope)
  have e : r = r0 := A.ends_unique ran ⟨n, h0⟩
  subst e
  refine ⟨r, ⟨m, ?_⟩, Agree.refl r⟩
  rw [← same call inScope]
  exact h0

/-- The second program of such a pair ends where the first does. -/
theorem ends_of_runs {A B : Layer} {o : Oracle} {p : A.Prog} {q : B.Prog}
    {scope : Call → Prop} {n m : Nat}
    (ends : ∀ call, scope call → (A.run o p call n).isSome = true)
    (same : ∀ call, scope call → A.run o p call n = B.run o q call m) :
    ∀ call, scope call → (B.run o q call m).isSome = true := by
  intro call inScope
  rw [← same call inScope]
  exact ends call inScope

end SolarTV.Milestone.W1
