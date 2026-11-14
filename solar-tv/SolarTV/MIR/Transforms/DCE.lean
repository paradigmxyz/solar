import SolarTV.MIR.Ops

/-! MIR dead-code elimination: removing an unused pure SSA definition. -/

namespace SolarTV.MIR

abbrev Env := Nat → EvmRules.Word

/-- Removing a pure SSA definition preserves any continuation whose live-ins
exclude the defined ID. Both operands already have defined word values. -/
theorem erase_unused (op : Op) (dst lhs rhs : Nat) (env : Env) (k : Env → α)
    (unused : ∀ x y : Env, (∀ i, i ≠ dst → x i = y i) → k x = k y) :
    k (fun i => if i = dst then eval op (env lhs) (env rhs) else env i) = k env := by
  apply unused
  intro i h
  simp [h]

end SolarTV.MIR
