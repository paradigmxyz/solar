/-
Lemmas the discharge tactic (`SolarVerify.Discharge`) builds proofs from. Nothing an obligation
means depends on this file.
-/
namespace SolarVerify.Discharge

/-! Rewrites the tactic uses; `omega` reads disjunctions but not implications. -/

theorem imp_iff_not_or (a b : Prop) : (a → b) ↔ ¬a ∨ b :=
  ⟨fun h => (Classical.em a).elim (fun ha => .inr (h ha)) .inl,
   fun h ha => h.elim (fun hn => absurd ha hn) id⟩

theorem ite_pos {α : Type} {c : Prop} {_ : Decidable c} (h : c) (a b : α) : ite c a b = a := by
  simp [h]

theorem ite_neg {α : Type} {c : Prop} {_ : Decidable c} (h : ¬c) (a b : α) : ite c a b = b := by
  simp [h]

/-! Congruence, as disjunctions `omega` can case on (Ackermann's reduction). -/

theorem congr1 {β : Type} (f : Int → β) (a b : Int) : ¬a = b ∨ f a = f b :=
  (Classical.em (a = b)).elim (fun h => .inr (h ▸ rfl)) .inl

theorem congr2 {β : Type} (f : Int → Int → β) (a b c d : Int) :
    ¬a = c ∨ ¬b = d ∨ f a b = f c d :=
  (Classical.em (a = c)).elim
    (fun h => (Classical.em (b = d)).elim (fun h' => .inr (.inr (h ▸ h' ▸ rfl))) (fun h' => .inr (.inl h')))
    .inl

theorem congr3 {β : Type} (f : Int → Int → Int → β) (a b c d e g : Int) :
    ¬a = d ∨ ¬b = e ∨ ¬c = g ∨ f a b c = f d e g :=
  (Classical.em (a = d)).elim
    (fun h => (Classical.em (b = e)).elim
      (fun h' => (Classical.em (c = g)).elim (fun h'' => .inr (.inr (.inr (h ▸ h' ▸ h'' ▸ rfl))))
        (fun h'' => .inr (.inr (.inl h''))))
      (fun h' => .inr (.inl h')))
    .inl

theorem true_eq_iff (b : Bool) : (true = b) ↔ (b = true) := ⟨Eq.symm, Eq.symm⟩

theorem false_eq_iff (b : Bool) : (false = b) ↔ (b = false) := ⟨Eq.symm, Eq.symm⟩

/-- `decide` at any instance, such as the classical one quantifiers evaluate with. -/
theorem decide_any_eq_true {p : Prop} {_ : Decidable p} : (decide p = true) = p := by simp

/-- The classical `if` a quantifier evaluates with, as a function of its condition alone, so
`simp` can rewrite the condition without a `Decidable` instance for the new one. -/
noncomputable def qite {α : Type} (p : Prop) (a b : α) : α := @ite α p (Classical.propDecidable p) a b

/-- The classical `decide` a quantifier evaluates with. -/
noncomputable def qdec (p : Prop) : Bool := @decide p (Classical.propDecidable p)

theorem qite_true {α : Type} (a b : α) : qite True a b = a := by simp [qite]
theorem qite_false {α : Type} (a b : α) : qite False a b = b := by simp [qite]
theorem qdec_eq_true (p : Prop) : (qdec p = true) = p := by simp [qdec]

/-- Every `if` as a `qite`, whose condition `simp` rewrites like any other proposition. -/
theorem ite_eq_qite {α : Type} {c : Prop} {_ : Decidable c} (a b : α) : ite c a b = qite c a b := by
  unfold qite; split <;> simp_all

theorem qite_pos {α : Type} {c : Prop} (h : c) (a b : α) : qite c a b = a := by simp [qite, h]

theorem qite_neg {α : Type} {c : Prop} (h : ¬c) (a b : α) : qite c a b = b := by simp [qite, h]

end SolarVerify.Discharge
