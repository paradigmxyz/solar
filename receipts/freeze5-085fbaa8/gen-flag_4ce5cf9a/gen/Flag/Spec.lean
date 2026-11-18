import Verify.Spec.Contract

/-!
Specs of `Certified/flag_4ce5cf9a/flag_4ce5cf9a.sol`, exported by solar-verify 0.3.0 with `--emit-lean-spec`.
Generated: do not edit.
-/

namespace Certified.flag_4ce5cf9a.gen.Flag.Spec

open SolidCore.Spec

/-- What the specs were exported from and are about. -/
def manifest : Manifest :=
  { source := "Certified/flag_4ce5cf9a/flag_4ce5cf9a.sol",
    sourceSha256 := "4ce5cf9a87520dc956a1e7366d5f646a07739832e44a3d8dec2eb656deb6388d",
    compiler := "",
    options := [],
    bytecodeSha256 := "" }

/-- The specs of `Flag`. -/
def Flag : Bundle where
  fuel := 0
  spec :=
  { name := "Flag",
    fns := [
    ],
    invariants := [
    ],
    ghosts := [],
    specs := [
    { name := "Flag.raise",
      selector := 0x004f4803,
      params := [(.uint 256)],
      rets := [],
      payable := false,
      requires := [],
      ensures := [
        (.bin .and (.bin .eq (.load (.slot 0) (.lit 0) 32 (.uint 256)) (.arg 0)) (.bin .eq (.load (.slot 1) (.lit 0) 32 (.uint 256)) (.lit 1)))],
      succeedsIf := [
        (.bin .eq (.env .value) (.lit 0))] },
    { name := "Flag.lower",
      selector := 0xb951486d,
      params := [],
      rets := [],
      payable := false,
      requires := [],
      ensures := [
        (.bin .and (.bin .eq (.load (.slot 1) (.lit 0) 32 (.uint 256)) (.lit 0)) (.bin .eq (.load (.slot 0) (.lit 0) 32 (.uint 256)) (.old (.load (.slot 0) (.lit 0) 32 (.uint 256)))))],
      succeedsIf := [
        (.bin .eq (.env .value) (.lit 0))] }] }

/-- Every spec function call resolves and nests within the fuel. -/
theorem Flag_wf : Flag.WF = true := by decide +kernel

/-- Trusted-base items these specs' proofs rely on beyond the verdict
(`SolidCore.Spec.Assumptions`). -/
def trusted : List String := []

end Certified.flag_4ce5cf9a.gen.Flag.Spec
