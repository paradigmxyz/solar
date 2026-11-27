import Verify.Spec.Contract

/-!
Specs of `Counter.sol`, exported by solar-verify 0.3.0 with `--emit-lean-spec`.
Generated: do not edit.
-/

namespace Verify.Phase2.CounterCaller.Spec

open SolidCore.Spec

/-- What the specs were exported from and are about. -/
def manifest : Manifest :=
  { source := "Counter.sol",
    sourceSha256 := "f1c87a334b8db31fc8f80541febeaf8617318d20b685634f44e26c74146ad326",
    compiler := "0.8.38+commit.unknown.solar.0.3.0",
    options := [],
    bytecodeSha256 := "509845d3b18cf635ec7ee9b1a9d225cfeec995099a9f5e0d55f27ae6559f411d" }

/-- The specs of `Counter`. -/
def Counter : Bundle where
  fuel := 0
  spec :=
  { name := "Counter",
    fns := [
    ],
    invariants := [
    (.bin .le (.load (.slot 0) (.lit 0) 32 (.uint 256)) (.load (.slot 1) (.lit 0) 32 (.uint 256)))],
    ghosts := [],
    specs := [
    { name := "Counter.increment",
      selector := 0xd09de08a,
      params := [],
      rets := [],
      payable := false,
      requires := [
        (.bin .lt (.load (.slot 0) (.lit 0) 32 (.uint 256)) (.load (.slot 1) (.lit 0) 32 (.uint 256)))],
      ensures := [
        (.bin .eq (.load (.slot 0) (.lit 0) 32 (.uint 256)) (.bin .add (.old (.load (.slot 0) (.lit 0) 32 (.uint 256))) (.lit 1)))],
      succeedsIf := [] },
    { name := "Counter.bump",
      selector := 0xb20eb4c4,
      params := [(.uint 256)],
      rets := [],
      payable := false,
      requires := [],
      ensures := [
        (.bin .eq (.load (.slot 0) (.lit 0) 32 (.uint 256)) (.bin .add (.old (.load (.slot 0) (.lit 0) 32 (.uint 256))) (.arg 0)))],
      succeedsIf := [] },
    { name := "Counter.max",
      selector := 0x6d5433e6,
      params := [(.uint 256), (.uint 256)],
      rets := [(.uint 256)],
      payable := false,
      requires := [],
      ensures := [
        (.bin .eq (.ret 0) (.ite (.bin .ge (.arg 0) (.arg 1)) (.arg 0) (.arg 1))),
        (.bin .and (.bin .ge (.ret 0) (.arg 0)) (.bin .ge (.ret 0) (.arg 1)))],
      succeedsIf := [] }] }

/-- Every spec function call resolves and nests within the fuel. -/
theorem Counter_wf : Counter.WF = true := by decide +kernel

/-- Trusted-base items these specs' proofs rely on beyond the verdict
(`SolidCore.Spec.Assumptions`). -/
def trusted : List String := []

end Verify.Phase2.CounterCaller.Spec
