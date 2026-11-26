import Verify.Spec.Contract

/-!
Specs of `tests/snapshots/lean_export_enum.sol`, exported by solar-verify 0.3.0 with `--emit-lean-spec`.
Generated: do not edit.
-/

namespace SolidCore.Spec.Export.Enum

open SolidCore.Spec

/-- What the specs were exported from and are about. -/
def manifest : Manifest :=
  { source := "tests/snapshots/lean_export_enum.sol",
    sourceSha256 := "06e61b1a66b9d0f0136cc47393d7b29b42a549ad73785136bcba1f85c79f875a",
    compiler := "",
    options := [],
    bytecodeSha256 := "" }

/-- The specs of `E`. -/
def E : Bundle where
  fuel := 0
  spec :=
  { name := "E",
    fns := [
    ],
    invariants := [
    ],
    ghosts := [],
    specs := [
    { name := "E.setBlue",
      selector := 0x75056e81,
      params := [],
      rets := [],
      payable := false,
      requires := [],
      ensures := [
        (.bin .eq (.load (.slot 1) (.lit 0) 1 (.enum 3)) (.lit 2))],
      succeedsIf := [] },
    { name := "E.set",
      selector := 0x24b8ba5f,
      params := [(.enum 3)],
      rets := [],
      payable := false,
      requires := [
        (.bin .ne (.arg 0) (.lit 0))],
      ensures := [
        (.bin .and (.bin .eq (.load (.slot 1) (.lit 0) 1 (.enum 3)) (.arg 0)) (.bin .eq (.load (.slot 0) (.lit 0) 32 (.uint 256)) (.old (.load (.slot 0) (.lit 0) 32 (.uint 256)))))],
      succeedsIf := [] }] }

/-- Every spec function call resolves and nests within the fuel. -/
theorem E_wf : E.WF = true := by decide +kernel

/-- Trusted-base items these specs' proofs rely on beyond the verdict
(`SolidCore.Spec.Assumptions`). -/
def trusted : List String := []

end SolidCore.Spec.Export.Enum
