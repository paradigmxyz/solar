import SolidCore.Spec.Contract

/-!
Specs of `tests/snapshots/lean_export.sol`, exported by solar-verify 0.3.0 with `--emit-lean-spec`.
Generated: do not edit.
-/

namespace SolidCore.Spec.Export.Test

open SolidCore.Spec

/-- What the specs were exported from and are about. -/
def manifest : Manifest :=
  { source := "tests/snapshots/lean_export.sol",
    sourceSha256 := "2a96b8a65f9b1b3fb66b0ded9d712639f11d1922cef275332a8a20ecd33ee073",
    compiler := "",
    options := [],
    bytecodeSha256 := "" }

/-- The specs of `Export`. -/
def Export : Bundle where
  fuel := 1
  spec :=
  { name := "Export",
    fns := [
    { name := "sq",
      params := [("a", (.uint 256))],
      ret := (.uint 256),
      body := .bin .mul (.var "a") (.var "a"),
      isOpaque := false },
    { name := "half",
      params := [("a", (.uint 256))],
      ret := (.uint 256),
      body := .bin .div (.var "a") (.lit 2),
      isOpaque := true }],
    invariants := [
    (.bin .le (.load (.slot 1) (.lit 0) 32 (.uint 256)) (.lit 1000)),
    (.bin .eq (.ghost "total" []) (.load (.slot 1) (.lit 0) 32 (.uint 256))),
    (.bin .le ((.load (.offset (.slot 6) (.lit 0)) (.lit 0) 8 (.uint 64))) ((.load (.offset (.slot 6) (.lit 0)) (.lit 8) 8 (.uint 64)))),
    (.all "__key" (.uint 256) (.bin .le ((.load (.offset (.entry (.slot 7) (.var "__key")) (.lit 0)) (.lit 0) 8 (.uint 64))) ((.load (.offset (.entry (.slot 7) (.var "__key")) (.lit 0)) (.lit 8) 8 (.uint 64)))))],
    ghosts := ["total"],
    specs := [
    { name := "Export.deposit",
      selector := 0xb6b55f25,
      params := [(.uint 256)],
      rets := [(.uint 256)],
      payable := false,
      requires := [
        (.bin .lt (.load (.slot 1) (.lit 0) 32 (.uint 256)) (.lit 1000))],
      ensures := [
        (.bin .eq (.load (.slot 1) (.lit 0) 32 (.uint 256)) (.bin .add (.old (.load (.slot 1) (.lit 0) 32 (.uint 256))) (.lit 1))),
        (.bin .eq (.load (.entry (.slot 2) (.env .sender)) (.lit 0) 32 (.uint 256)) (.bin .add (.old (.load (.entry (.slot 2) (.env .sender)) (.lit 0) 32 (.uint 256))) (.arg 0))),
        (.bin .eq (.ghost "total" []) (.bin .add (.old (.ghost "total" [])) (.lit 1))),
        (.bin .eq (.ret 0) (.load (.slot 1) (.lit 0) 32 (.uint 256)))],
      succeedsIf := [] },
    { name := "Export.probe",
      selector := 0xe357a605,
      params := [.address, (.uint 256)],
      rets := [.bool],
      payable := false,
      requires := [],
      ensures := [
        (.all "i" (.uint 256) (.bin .implies (.bin .lt (.var "i") (.len (.slot 4))) (.bin .eq (.load (.elem (.slot 4) (.var "i") 1) (.lit 0) 32 (.uint 256)) (.old (.load (.elem (.slot 4) (.var "i") 1) (.lit 0) 32 (.uint 256)))))),
        (.ex "a" .address (.bin .eq (.bin .gt (.load (.entry (.slot 2) (.var "a")) (.lit 0) 32 (.uint 256)) (.lit 0)) (.ret 0))),
        (.bin .implies (.load (.entry (.entry (.slot 3) (.arg 0)) (.arg 1)) (.lit 0) 1 .bool) (.bin .ge (.call "sq" [(.arg 1)]) (.arg 1))),
        (.bin .and (.bin .and (.bin .le (.call "half" [(.arg 1)]) (.arg 1)) (.bin .eq (.shr (.arg 1) 1) (.call "half" [(.arg 1)]))) (.bin .ge (.shl (.arg 1) 2) (.lit 0))),
        (.bin .eq (.len (.slot 5)) (.old (.len (.slot 5)))),
        (.bin .le (.ite (.bin .gt (.len (.slot 5)) (.lit 0)) (.load (.packedElem (.slot 5) (.lit 0) 32) (.bin .mul (.bin .mod (.lit 0) (.lit 32)) (.lit 1)) 1 (.uint 8)) (.lit 0)) (.lit 255)),
        (.bin .and (.bin .le (.load (.offset (.slot 6) (.lit 0)) (.lit 0) 8 (.uint 64)) (.load (.offset (.slot 6) (.lit 0)) (.lit 8) 8 (.uint 64))) (.bin .eq (.load (.offset (.entry (.slot 7) (.arg 1)) (.lit 0)) (.lit 16) 1 .bool) (.old (.load (.offset (.entry (.slot 7) (.arg 1)) (.lit 0)) (.lit 16) 1 .bool)))),
        (.bin .and (.bin .and (.bin .lt (.load (.slot 0) (.lit 0) 1 (.uint 8)) (.lit 256)) (.bin .ge (.load (.slot 0) (.lit 1) 2 (.int 16)) (.un .neg (.lit 32768)))) (.bin .eq (.load (.slot 0) (.lit 3) 20 .address) (.load (.slot 0) (.lit 3) 20 .address))),
        (.bin .and (.bin .le (.env .time) (.env .timestamp)) (.bin .eq (.env .self) (.env .self)))],
      succeedsIf := [
        (.bin .and (.bin .and (.un .not (.bin .gt (.load (.slot 0) (.lit 0) 1 (.uint 8)) (.lit 3))) (.bin .le (.un .neg (.load (.slot 0) (.lit 1) 2 (.int 16))) (.lit 32768))) (.bin .ge (.bin .bor (.bin .band (.load (.slot 1) (.lit 0) 32 (.uint 256)) (.lit 1)) (.lit 2)) (.bin .sub (.bin .mod (.bin .bxor (.load (.slot 1) (.lit 0) 32 (.uint 256)) (.lit 3)) (.lit 4)) (.lit 4))))] }] }

/-- Every spec function call resolves and nests within the fuel. -/
theorem Export_wf : Export.WF = true := by decide +kernel

/-- Trusted-base items these specs' proofs rely on beyond the verdict
(`SolidCore.Spec.Assumptions`). -/
def trusted : List String := []

end SolidCore.Spec.Export.Test
