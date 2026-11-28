import Verify.Spec.Contract

/-!
Specs of `tests/snapshots/lean_export_same_name.sol`, exported by solar-verify 0.3.0 with `--emit-lean-spec`.
Generated: do not edit.
-/

namespace SolidCore.Spec.Export.SameName

open SolidCore.Spec

/-- What the specs were exported from and are about. -/
def manifest : Manifest :=
  { source := "tests/snapshots/lean_export_same_name.sol",
    sourceSha256 := "e9b6a91b95342bbedca4184f3e761622243faf89ea50c4e9f8a06bd7863763e4",
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
    (.bin .le ((.load (.offset (.slot 1) (.lit 0)) (.lit 0) 16 (.uint 128))) ((.load (.offset (.slot 1) (.lit 0)) (.lit 16) 16 (.uint 128))))],
    ghosts := [],
    specs := [
    { name := "Base.setY",
      selector := 0x68d466b8,
      params := [(.uint 256)],
      rets := [],
      payable := false,
      requires := [],
      ensures := [
        (.bin .eq (.load (.slot 2) (.lit 0) 32 (.uint 256)) (.arg 0))],
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
    (.bin .le ((.load (.offset (.slot 1) (.lit 0)) (.lit 0) 16 (.uint 128))) ((.load (.offset (.slot 1) (.lit 0)) (.lit 16) 16 (.uint 128)))),
    (.bin .le ((.load (.offset (.slot 4) (.lit 0)) (.lit 0) 16 (.uint 128))) ((.load (.offset (.slot 4) (.lit 0)) (.lit 16) 16 (.uint 128))))],
    ghosts := [],
    specs := [
    { name := "Derived.setY",
      selector := 0x68d466b8,
      params := [(.uint 256)],
      rets := [],
      payable := false,
      requires := [],
      ensures := [
        (.bin .eq (.load (.slot 2) (.lit 0) 32 (.uint 256)) (.arg 0))],
      succeedsIf := [] },
    { name := "Derived.setZ",
      selector := 0x3f2bff51,
      params := [(.uint 256)],
      rets := [],
      payable := false,
      requires := [],
      ensures := [
        (.bin .eq (.load (.slot 5) (.lit 0) 32 (.uint 256)) (.arg 0))],
      succeedsIf := [] }] }

/-- Every spec function call resolves and nests within the fuel. -/
theorem Derived_wf : Derived.WF = true := by decide +kernel

/-- Trusted-base items these specs' proofs rely on beyond the verdict
(`SolidCore.Spec.Assumptions`). -/
def trusted : List String := []

end SolidCore.Spec.Export.SameName
