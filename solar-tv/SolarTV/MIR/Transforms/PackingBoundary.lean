import SolarTV.MIR.Transforms.LowerPacked
import SolarTV.MIR.Transforms.FinalCallBudget

/-! Reconstruct the five stores of the signing envelope from the typed MIR
result. The checked emitted path has already made the entire object active
when it hashed the preceding ABI record. -/

namespace SolarTV.MIR.PackingBoundary
open EvmRules SolarTV.Execution MIR.State MIR.LowerPacked

def address (memory : List Byte) : Word := readWord memory 64

structure Active (memory : List Byte) : Prop where
  aligned : memory.length % 32 = 0
  limit : memory.length ≤ 65536
  valid : 128 ≤ (address memory).toNat
  extent : (address memory).toNat + 128 ≤ memory.length

theorem addAddress (memory : List Byte) (ready : Active memory) (n : Nat) (small : n ≤ 128) :
    (address memory + BitVec.ofNat 256 n).toNat = (address memory).toNat + n := by
  have size := ready.extent
  have limit : memory.length ≤ 65536 := ready.limit
  have offset : n < 2^256 := by omega
  have sum : (address memory).toNat + n < 2^256 := by omega
  simp [BitVec.toNat_add, Nat.mod_eq_of_lt offset, Nat.mod_eq_of_lt sum]

def bump (memory : List Byte) : List Byte :=
  writeBytes memory 64 (wordBytes (address memory+128))

def header (memory : List Byte) : List Byte :=
  writeBytes (bump memory) (address memory).toNat (wordBytes 66)

def marker (memory : List Byte) : List Byte :=
  writeBytes (header memory) ((address memory).toNat+32) (wordBytes signingPrefix)

def withDomain (memory : List Byte) (domain : Word) : List Byte :=
  writeBytes (marker memory) ((address memory).toNat+34) (wordBytes domain)

def withDigest (memory : List Byte) (domain digest : Word) : List Byte :=
  writeBytes (withDomain memory domain) ((address memory).toNat+66) (wordBytes digest)

@[simp] theorem bumpSize (memory : List Byte) : (bump memory).length = memory.length := by simp [bump]
@[simp] theorem headerSize (memory : List Byte) : (header memory).length = memory.length := by simp [header]
@[simp] theorem markerSize (memory : List Byte) : (marker memory).length = memory.length := by simp [marker]
@[simp] theorem domainSize (memory : List Byte) (domain : Word) :
    (withDomain memory domain).length = memory.length := by simp [withDomain]

theorem memoryWrite (original before : List Byte) (ready : Active original)
    (size : before.length = original.length) (offset : Nat) (inside : offset+32 ≤ original.length) :
    expand before offset 32 = .ok before ∧ EVM.State.memoryCharge before offset 32 = 0 := by
  have expansion := FinalCallBudget.expandCovered before offset 32
    (size ▸ ready.aligned) (size ▸ inside) (size ▸ ready.limit)
  exact ⟨expansion, CallDebit.memoryCharge_zero before offset 32 expansion⟩

theorem expanded (frame : Frame) (ready : Active frame.memory) (domain digest : Word) :
    loweredSigningEnvelope frame domain digest =
      .ok (.returned (some (address frame.memory), {frame with memory := withDigest frame.memory domain digest})) := by
  let m := frame.memory
  have bounds : (address m).toNat + 128 ≤ m.length := ready.extent
  have limit : m.length ≤ 65536 := ready.limit
  have valid : 128 ≤ (address m).toNat := ready.valid
  have charged := memoryWrite m m ready rfl 64 (by omega)
  have wroteHeader := memoryWrite m (bump m) ready (by simp) (address m).toNat (by omega)
  have wroteMarker := memoryWrite m (header m) ready (by simp) ((address m).toNat+32) (by omega)
  have wroteDomain := memoryWrite m (marker m) ready (by simp) ((address m).toNat+34) (by omega)
  have wroteDigest := memoryWrite m (withDomain m domain) ready (by simp) ((address m).toNat+66) (by omega)
  have add32 : (address m+32).toNat = (address m).toNat+32 := addAddress m ready 32 (by decide)
  have add34 : (address m+34).toNat = (address m).toNat+34 := addAddress m ready 34 (by decide)
  have add66 : (address m+66).toNat = (address m).toNat+66 := addAddress m ready 66 (by decide)
  have add128 : (address m+128).toNat = (address m).toNat+128 := addAddress m ready 128 (by decide)
  have allocation : allocate frame 128 false false =
      .ok (.returned (some (address m), {frame with memory := bump m})) := by
    unfold allocate
    change (do
      let memory ← expand m 64 32
      let pointer := address memory
      let next := pointer + 128
      if next.toNat < pointer.toNat || next.toNat > 2^64 - 1 then
        .error "infallible allocation validity precondition"
      else .ok (Outcome.returned (some pointer,{frame with memory := writeBytes memory 64 (wordBytes next)}))) = _
    have fits : (address m).toNat+128 ≤ 2^64-1 := by omega
    have nowrap : ¬ (address m).toNat+128 < (address m).toNat := by omega
    simp only [charged.1,Bind.bind,Except.bind,add128,nowrap,Nat.not_lt.mpr fits]
    rfl
  simp only [loweredSigningEnvelope, allocation, Bind.bind, Except.bind, store, add32, add34, add66,
    wroteHeader.1, Pure.pure, Except.pure]
  simp only [header, marker, withDomain, withDigest] at wroteMarker wroteDomain wroteDigest ⊢
  rw [wroteMarker.1]
  simp only []
  rw [wroteDomain.1]
  simp only []
  rw [wroteDigest.1]

end SolarTV.MIR.PackingBoundary
