import SolarTV.MIR.State.Ops

/-! A typed slice holds two immutable logical words. A projected field
uses the same carrier supplied to the construction. Neither operation
requires a memory access or observes the neighboring field. -/
namespace SolarTV.MIR.LowerSlices
open EvmRules MIR.State

def sliceValue (pointer length : Word) : Value :=
  .aggregate [.scalar (Scalar.ofWord 256 pointer),.scalar (Scalar.ofWord 256 length)]

theorem fields (location : Nat) (pointer length : Word) :
    extractField (.slice location) (sliceValue pointer length) 0 =
      .ok (.scalar (Scalar.ofWord 256 pointer)) ∧
    extractField (.slice location) (sliceValue pointer length) 1 =
      .ok (.scalar (Scalar.ofWord 256 length)) :=
  ⟨rfl,rfl⟩

theorem defined (location : Nat) (pointer length : Word) :
    (sliceValue pointer length).fits 32 (.slice location) = true :=
  rfl

end SolarTV.MIR.LowerSlices
