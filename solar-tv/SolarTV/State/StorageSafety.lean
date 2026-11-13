import SolarTV.State.Storage

/-! Storage boundary proofs independent of any importer projection. The examples
pin zero/no-op stores, cold/warm pricing, dirty-slot refunds, the stipend sentry
and the distinct transient-store rule. All proofs are checked in the kernel. -/

namespace SolarTV.Storage

example : writePrice .homestead false 0 0 0 = ⟨5000, 0⟩ := by decide
example : writePrice .constantinople false 0 0 0 = ⟨200, 0⟩ := by decide
example : writePrice .petersburg false 0 0 0 = ⟨5000, 0⟩ := by decide
example : writePrice .istanbul false 0 0 0 = ⟨800, 0⟩ := by decide
example : writePrice .berlin false 0 0 0 = ⟨2200, 0⟩ := by decide
example : writePrice .london false 7 7 0 = ⟨5000, 4800⟩ := by decide
example : writePrice .london true 7 0 7 = ⟨100, -2000⟩ := by decide
example : writePrice .london true 0 1 0 = ⟨100, 19900⟩ := by decide

/-- Equal storage contents do not suffice for gas equivalence: access warmth matters. -/
theorem cold_read_costs_more : readPrice .cancun false > readPrice .cancun true := by decide

/-- No-op stores still fail the gas sentry, even though their price is only 100. -/
theorem sentry_rejects (state : State) (a : Address) (k value : EvmRules.Word) :
    store .cancun state a k value 2300 false = .error .outOfGas := by
  simp [store, Fork.atLeast, Fork.ordinal]

/-- A transient store at the same budget succeeds and leaves persistent storage alone. -/
theorem transient_ignores_sentry (state : State) (a : Address) (k value : EvmRules.Word) :
    tstore .cancun state a k value 100 false =
      .ok ({state with transient := update state.transient a k value}, 0) := by
  simp [tstore, Fork.atLeast, Fork.ordinal]

end SolarTV.Storage
