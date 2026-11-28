import Verify.Spec.Contract

/-!
Specs of `tests/snapshots/lean_export_enum_scope.sol`, exported by solar-verify 0.3.0 with `--emit-lean-spec`.
Generated: do not edit.
-/

namespace SolidCore.Spec.Export.EnumScope

open SolidCore.Spec

/-- What the specs were exported from and are about. -/
def manifest : Manifest :=
  { source := "tests/snapshots/lean_export_enum_scope.sol",
    sourceSha256 := "27554aa88ade081cbafdb6ab7c51813a8cb2fb0510a714de55f89939918e904c",
    compiler := "",
    options := [],
    bytecodeSha256 := "" }

/-- The specs of `Base`. -/
def Base : Bundle where
  fuel := 0
  spec :=
  { name := "Base",
    fns := [
    ],
    invariants := [
    ],
    ghosts := [],
    specs := [
    ] }

/-- Every spec function call resolves and nests within the fuel. -/
theorem Base_wf : Base.WF = true := by decide +kernel

/-- The specs of `Derived`. -/
def Derived : Bundle where
  fuel := 0
  spec :=
  { name := "Derived",
    fns := [
    ],
    invariants := [
    ],
    ghosts := [],
    specs := [
    { name := "Derived.set",
      selector := 0xb8e010de,
      params := [],
      rets := [],
      payable := false,
      requires := [],
      ensures := [
        (.bin .and (.bin .and (.bin .eq (.load (.slot 0) (.lit 0) 1 (.enum 3)) (.lit 2)) (.bin .eq (.load (.slot 0) (.lit 1) 1 (.enum 4)) (.lit 3))) (.bin .eq (.load (.slot 0) (.lit 2) 1 (.enum 3)) (.lit 2)))],
      succeedsIf := [] },
    { name := "Derived.red",
      selector := 0x2930cf24,
      params := [],
      rets := [],
      payable := false,
      requires := [],
      ensures := [
        (.bin .eq (.load (.slot 0) (.lit 3) 1 (.uint 8)) (.lit 1))],
      succeedsIf := [] }] }

/-- Every spec function call resolves and nests within the fuel. -/
theorem Derived_wf : Derived.WF = true := by decide +kernel

/-- Trusted-base items these specs' proofs rely on beyond the verdict
(`SolidCore.Spec.Assumptions`). -/
def trusted : List String := []

end SolidCore.Spec.Export.EnumScope
