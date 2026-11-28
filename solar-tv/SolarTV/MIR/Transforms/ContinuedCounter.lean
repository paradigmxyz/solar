import SolarTV.MIR.State.StoreTrace

/-! Direct proof for the memory fill with connected induction variables. Both
imported bodies start at the real function entry with the same explicit heap
floor. The certificate checks termination, ordered writes, final byte memory
and ABI return data, with the actual interpreter's phi and guard semantics. -/

namespace SolarTV.MIR.ContinuedCounter
open EvmRules SolarTV.Execution MIR.State

def padTo (mem : List Byte) (size : Nat) : List Byte :=
  mem ++ List.replicate (size-mem.length) 0

@[simp] theorem padLength (mem : List Byte) (size : Nat) :
    (padTo mem size).length = max mem.length size := by
  simp [padTo]
  omega

theorem leMax (a b c : Nat) : a ≤ max b c ↔ a ≤ b ∨ a ≤ c := by omega

theorem maxAligned (a b : Nat) (h : a % 32 = 0) (g : b % 32 = 0) :
    max a b % 32 = 0 := by
  by_cases ab : a ≤ b
  · simpa [Nat.max_eq_right ab] using g
  · simpa [Nat.max_eq_left (by omega : b ≤ a)] using h

@[simp] theorem padCovered (mem : List Byte) (size : Nat) (covers : size ≤ mem.length) :
    padTo mem size = mem := by
  simp [padTo, Nat.sub_eq_zero_of_le covers]

@[simp↓] theorem readWord_padTo (mem : List Byte) (size offset : Nat)
    (covered : offset+32 ≤ mem.length) : readWord (padTo mem size) offset = readWord mem offset :=
  readWord_append_prefix _ _ _ covered

theorem expandAligned (mem : List Byte) (offset size : Nat) (nonempty : size > 0)
    (aligned : mem.length % 32 = 0) (ending : (offset+size)%32 = 0)
    (bounded : mem.length ≤ 65536) (fits : offset+size ≤ 65536) :
    expand mem offset size = .ok (padTo mem (offset+size)) := by
  unfold expand requiredWords memoryWords padTo
  rw [ite_eq_right (by omega : size ≠ 0)]
  by_cases active : offset+size ≤ mem.length
  · rw [Nat.max_eq_left (by omega : (offset+size+31)/32 ≤ mem.length/32)]
    simp [show 32*(mem.length/32) = mem.length by omega, Nat.not_lt.mpr bounded,
      Nat.sub_eq_zero_of_le active]
  · rw [Nat.max_eq_right (by omega : mem.length/32 ≤ (offset+size+31)/32)]
    simp [show 32*((offset+size+31)/32) = offset+size by omega, Nat.not_lt.mpr fits]

/-- Array header, six element writes, then the ABI offset word. -/
def expectedStores (base : Nat) : List Store :=
  let atOffset (offset : Nat) := BitVec.ofNat 256 (base+offset)
  [(atOffset 0,6),(atOffset 32,8),(atOffset 64,8),
   (atOffset 96,9),(atOffset 128,9),(atOffset 160,9),
   (atOffset 192,7),(BitVec.ofNat 256 (base-32),32)]

def expected (base : Nat) (frame : Frame) : List Store × Result :=
  let memory := (expectedStores base).foldl (fun memory (address,word) =>
    writeBytes (padTo memory (address.toNat+32)) address.toNat (wordBytes word)) frame.memory
  (expectedStores base,finish (.success (readBytes memory (base-32) 256)) {frame with memory})

def config (fork : Fork) (name : String) (env : World.Environment)
    (ctx : Execution.Block) (base : Nat) : Config :=
  ⟨fork,env,ctx,[1,3,16,17,23,27,28,81,82],true,128,none,[],
    {functions := [(name,{heapFloor := some (BitVec.ofNat 256 base)})]},[],false,none,false⟩

def initial (frame : Frame) : Machine :=
  ⟨{function := 0},[],{frame with arguments := [],locals := []}⟩

def Certificate (fork : Fork) (name : String) (body : Functions) : Prop :=
  ∀ (env : World.Environment) (ctx : Execution.Block) (f : Frame) (base : Nat),
    128 ≤ base → base+224 ≤ 65536 → base % 32 = 0 →
      f.memory.length % 32 = 0 → f.memory.length ≤ 65536 →
    runStores 128 (config fork name env ctx base) body (initial f) = .ok (expected base f)

def PairCertificate (fork : Fork) (name : String) (before after : Functions) : Prop :=
  ∀ (env : World.Environment) (ctx : Execution.Block) (f : Frame) (base : Nat),
    128 ≤ base → base+224 ≤ 65536 → base % 32 = 0 →
      f.memory.length % 32 = 0 → f.memory.length ≤ 65536 →
    let cfg := config fork name env ctx base
    runStores 128 cfg before (initial f) = runStores 128 cfg after (initial f) ∧
      run 128 cfg before (initial f) = run 128 cfg after (initial f)

theorem correspondence (fork : Fork) (name : String) (before after : Functions)
    (left : Certificate fork name before) (right : Certificate fork name after) :
    PairCertificate fork name before after := by
  intro env ctx f base minimum capacity baseAligned aligned bounded
  have same := (left env ctx f base minimum capacity baseAligned aligned bounded).trans
    (right env ctx f base minimum capacity baseAligned aligned bounded).symm
  refine ⟨same, ?_⟩
  simpa only [runStores_result] using congrArg (fun traced => traced.map Prod.snd) same

/-- Normalize the supplied typed graph with the kernel's direct MIR transition
rules. The goal carries the independent expected store sequence and result. -/
macro "prove_word_fill " body:ident : tactic => do
  let unfoldBody ← `(Lean.Parser.Tactic.simpLemma| $body:ident)
  `(tactic| (
  intro env ctx f base minimum capacity baseAligned aligned bounded
  have baseSmall : base < 2^256 := by omega
  have valueAt (n : Nat) (size : n ≤ 256) :
      (BitVec.ofNat 256 (base+n)).toNat = base+n := by
    rw [BitVec.toNat_ofNat, Nat.mod_eq_of_lt (by omega : base+n < 2^256)]
  have baseVal : (BitVec.ofNat 256 base).toNat = base := by
    rw [BitVec.toNat_ofNat, Nat.mod_eq_of_lt baseSmall]
  have orFits : base ||| 6 < 18446744073709551616 := by
    exact Nat.or_lt_two_pow (by omega : base < 2^64) (by decide : 6 < 2^64)
  have noWrap : ¬base+224 < base := by omega
  have noWide : ¬18446744073709551615 < base+224 := by omega
  have startOffset : (BitVec.ofNat 256 (base-32)).toNat = base-32 := by
    rw [BitVec.toNat_ofNat,Nat.mod_eq_of_lt (by omega : base-32<2^256)]
  have retOffset : BitVec.ofNat 256 base - 32 = BitVec.ofNat 256 (base-32) :=
    BitVec.ofNat_sub_ofNat_of_le (w := 256) base 32 (by decide) (by omega)
  have returnNat : (2^256-32+base) % 2^256 = base-32 := by omega
  have returnWord : BitVec.ofNat 256 (2^256-32+base) = BitVec.ofNat 256 (base-32) := by
    rw [← BitVec.toNat_inj, BitVec.toNat_ofNat, returnNat, startOffset]
  have alignedAt (n : Nat) (size : n%32 = 0) : (base+n)%32 = 0 := by
    rw [Nat.add_mod, baseAligned, size]
  have expandElement (mem : List Byte) (offset : Nat) (small : offset ≤ 192)
      (wordAligned : offset % 32 = 0) (memoryAligned : mem.length % 32 = 0)
      (memoryBound : mem.length ≤ 65536) :
      expand mem (base+offset) 32 = .ok (padTo mem (base+(offset+32))) := by
    rw [← Nat.add_assoc]
    apply expandAligned mem (base+offset) 32 (by decide) memoryAligned ?_ memoryBound (by omega)
    rw [Nat.add_assoc, alignedAt _ (by omega)]
  have retEnd : base-32+256 = base+224 := by omega
  have retSize : base-32+32 = base := by omega
  have within32 : base+32 ≤ 65536 := by omega
  have within64 : base+64 ≤ 65536 := by omega
  have within96 : base+96 ≤ 65536 := by omega
  have within128 : base+128 ≤ 65536 := by omega
  have within160 : base+160 ≤ 65536 := by omega
  have within192 : base+192 ≤ 65536 := by omega
  have within224 : base+224 ≤ 65536 := by omega
  have sameAddr (left right : Nat) (hl : left ≤ 256) (hr : right ≤ 256) :
      BitVec.ofNat 256 (base+left) = BitVec.ofNat 256 (base+right) ↔ left=right := by
    rw [← BitVec.toNat_inj, valueAt left hl, valueAt right hr]
    omega
  have recovery : BitVec.ofNat 256 (base+96) - BitVec.ofNat 256 (base+32) = 64 := by
    rw [BitVec.ofNat_sub_ofNat_of_le _ _ (by omega) (by omega)]
    have diff : base+96-(base+32) = 64 := by omega
    rw [diff]
    rfl
  have baseWithin : base ≤ 65536 := by omega
  simp (config := {maxDischargeDepth := 32}) [config, ↓reduceFunction, ↓reduceStoreRun,
    $unfoldBody, finish, transition, next, stop, enter, MIR.State.branch, advance, instruction, assign,
    assignValue,
    MIR.State.read, readBits, readValue, Value.word, ↓reduceFits, ValueType.width, ValueType.same, put,
    operation, layoutOperation, placementBinding, required, memoryOperation, effect, value,
    Scalar.ofWord, MIR.compare, MIR.eval, MIR.shift, Word.shift,
    Bind.bind, Except.bind, Pure.pure, Except.pure, Except.map, ↓reduceStoreAt, initial, expected, expectedStores, List.foldl,
    expandElement, expandAligned, maxAligned, aligned, bounded, Nat.max_le, leMax,
    ← BitVec.ofNat_add, Nat.add_assoc, BitVec.ult_iff_toNat_lt, valueAt, baseVal, orFits, noWrap, noWide,
    startOffset, retOffset, retEnd, retSize, returnNat, returnWord, Nat.mod_eq_of_lt baseSmall, alignedAt, baseAligned, baseWithin,
    within32, within64, within96, within128, within160, within192, within224, sameAddr, recovery]
    <;> exact retOffset
))

end SolarTV.MIR.ContinuedCounter
