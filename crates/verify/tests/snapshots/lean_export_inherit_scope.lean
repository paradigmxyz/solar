import Verify.Spec.Contract

/-!
Specs of `tests/snapshots/lean_export_inherit_scope.sol`, exported by solar-verify 0.3.0 with `--emit-lean-spec`.
Generated: do not edit.
-/

namespace SolidCore.Spec.Export.InheritScope

open SolidCore.Spec

/-- What the specs were exported from and are about. -/
def manifest : Manifest :=
  { source := "tests/snapshots/lean_export_inherit_scope.sol",
    sourceSha256 := "9c4739d43d3599c23e9324ad7a3dbd18f8e79e48ed5b0b0e3a76d8e10341721a",
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
    (.bin .le (.load (.slot 1) (.lit 0) 32 (.uint 256)) (.lit 7))],
    ghosts := [],
    specs := [
    { name := "Base.f",
      selector := 0x26121ff0,
      params := [],
      rets := [],
      payable := false,
      requires := [],
      ensures := [
        (.bin .and (.bin .eq (.load (.slot 0) (.lit 0) 1 (.uint 8)) (.lit 1)) (.bin .eq (.load (.slot 0) (.lit 1) 20 .address) (.env .sender)))],
      succeedsIf := [] },
    { name := "Base.k",
      selector := 0xb4f40c61,
      params := [],
      rets := [(.uint 256)],
      payable := false,
      requires := [],
      ensures := [
        (.bin .eq (.ret 0) (.lit 7))],
      succeedsIf := [] }] }

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
    (.bin .le (.load (.slot 1) (.lit 0) 32 (.uint 256)) (.lit 7))],
    ghosts := [],
    specs := [
    { name := "Derived.f",
      selector := 0x26121ff0,
      params := [],
      rets := [],
      payable := false,
      requires := [],
      ensures := [
        (.bin .and (.bin .eq (.load (.slot 0) (.lit 0) 1 (.uint 8)) (.lit 1)) (.bin .eq (.load (.slot 0) (.lit 1) 20 .address) (.env .sender)))],
      succeedsIf := [] },
    { name := "Derived.k",
      selector := 0xb4f40c61,
      params := [],
      rets := [(.uint 256)],
      payable := false,
      requires := [],
      ensures := [
        (.bin .eq (.ret 0) (.lit 7))],
      succeedsIf := [] }] }

/-- Every spec function call resolves and nests within the fuel. -/
theorem Derived_wf : Derived.WF = true := by decide +kernel

/-- Trusted-base items these specs' proofs rely on beyond the verdict
(`SolidCore.Spec.Assumptions`). -/
def trusted : List String := []

end SolidCore.Spec.Export.InheritScope
