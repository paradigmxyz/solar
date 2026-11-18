-- Light check of the shape-open proof term's elaboration pattern, outside solidity-lean: the same
-- binder shape as Shape.published (∃ c, A c ∧ Certified …) and Certified.functional's signature.
structure Certified (p : Nat) (G : Nat → Nat) : Prop where
  inhabited : True
  resources : G p = G p
structure Functional (p : Nat) : Prop where
  inhabited : True
theorem Certified.functional {p : Nat} {G : Nat → Nat} (h : Certified p G) : Functional p := ⟨h.inhabited⟩
def contract? : Option Nat := some 1
theorem published : ∃ c, contract? = some c ∧ Certified c (fun _ => 7) := ⟨1, rfl, ⟨trivial, rfl⟩⟩
-- The failing form (the runner's first version):
example : True := by
  first
  | obtain ⟨c, hc, h⟩ := ((published).imp fun _ h => ⟨h.1, Certified.functional h.2⟩); exact trivial
  | exact trivial
-- The fixed form:
example : ∃ c, contract? = some c ∧ Functional c := by
  obtain ⟨c, hc, h⟩ := ((published).imp fun _ => And.imp_right Certified.functional)
  exact ⟨c, hc, h⟩
