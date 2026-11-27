import SolarTV.MIR.Transforms.RoutedDispatch

/-! Library dispatch compares the runtime address with the deployed library's
immutable address. The binding models a runtime link, so the check reads no
constructor staging memory. Only non-view routes require a different address. -/

namespace SolarTV.MIR.LibraryDispatch
open EvmRules SolarTV.Execution State RoutedDispatch

def select (selectors : List (Word × Nat)) (guarded : List Nat)
    (deployment executing : Address) (input : List Byte) : Option Nat :=
  (RoutedDispatch.select ⟨selectors, none, none, false⟩ input 0).filter fun id =>
    !(deployment == executing && guarded.contains id)

theorem selector_target (selectors : List (Word × Nat)) (guarded : List Nat)
    (deployment executing : Address) (input : List Byte) (wrapper : Nat)
    (selected : select selectors guarded deployment executing input = some wrapper) :
    wrapper ∈ selectors.map Prod.snd := by
  unfold select at selected
  exact RoutedDispatch.selector_target _ _ _ _ _ (Option.filter_eq_some_iff.mp selected).1

def Binding (cfg : Config) (id : Nat) (deployment : Address) : Prop :=
  layoutBinding cfg.layout.immutableSlots id = none ∧
  layoutBinding cfg.layout.immutableValues id = some (deployment.setWidth 256)

theorem immutable (cfg : Config) (frame : Frame) (id : Nat) (deployment : Address)
    (bound : Binding cfg id deployment) :
    layoutOperation cfg frame (.immutableRead id 160 false) [] =
      value frame (deployment.setWidth 256) := by
  simp [layoutOperation, bound.1, bound.2, required, immutableWord, value,
    BitVec.setWidth_setWidth_of_le, Bind.bind, Except.bind]

theorem address (cfg : Config) (frame : Frame) :
    contextOperation cfg frame 0x30 [] = value frame (cfg.environment.address.setWidth 256) := rfl

end SolarTV.MIR.LibraryDispatch
