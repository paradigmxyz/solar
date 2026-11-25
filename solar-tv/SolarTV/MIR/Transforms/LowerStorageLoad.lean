import SolarTV.MIR.Transforms.LowerChecks
import SolarTV.MIR.Transforms.LowerStorageHeader
import SolarTV.MIR.Transforms.LowerStorageBytes
import SolarTV.MIR.Transforms.RoundedExtent

/-! The emitted storage-byte loader preserves the unpriced MIR frame and
returned pointer. The index proof tracks the unsigned loop guard, storage
warming, memory stores and exact back edge. The same memory and storage-work
bounds apply to the semantic operation and its expansion. -/

set_option autoImplicit false
namespace SolarTV.MIR.LowerStorageLoad
open EvmRules SolarTV.Execution MIR.State

private theorem decoded_length_bound (header : Word) (long : Bool) (length : Nat)
    (decoded : bytesHeader header = .ok (long,length)) : length < 2^255 := by
  rw [← MIR.LowerStorageHeader.decode] at decoded
  unfold MIR.LowerStorageHeader.lowered at decoded
  dsimp only at decoded
  split at decoded
  · contradiction
  · have lengthEq := congrArg Prod.snd (Except.ok.inj decoded)
    dsimp only at lengthEq
    rw [← lengthEq]
    apply MIR.RoundedExtent.masked_length
    rw [BitVec.toNat_ushiftRight, Nat.shiftRight_eq_div_pow]
    have max := header.isLt
    omega

private theorem allocation_extent (header : Word) (long : Bool) (length : Nat)
    (decoded : bytesHeader header = .ok (long,length)) :
    let word := BitVec.ofNat 256 length
    MIR.LowerStorageBytes.ceilWords word = (length + 31) / 32 ∧
    MIR.LowerReturndata.size word = BitVec.ofNat 256 ((length + 31) / 32 * 32 + 32) ∧
    MIR.compare .lt (MIR.LowerReturndata.size word) word = 0 := by
  have small := decoded_length_bound header long length decoded
  have wordLength : (BitVec.ofNat 256 length).toNat = length := by
    rw [BitVec.toNat_ofNat, Nat.mod_eq_of_lt (by omega)]
  refine ⟨?_, ?_, ?_⟩
  · rw [MIR.LowerStorageBytes.ceilWords_eq _ (by rw [wordLength]; omega), wordLength]
  · rw [MIR.LowerReturndata.roundedSize length (by omega)]
    congr 1
    omega
  · apply MIR.RoundedExtent.overflow_clear
    rwa [wordLength]

private theorem point (pointer : Word) (index : Nat) :
    pointer + MIR.eval .mul (BitVec.ofNat 256 index) 32 + 32 =
      pointer + BitVec.ofNat 256 (32 + 32 * index) := by
  change pointer + BitVec.ofNat 256 index * BitVec.ofNat 256 32 + BitVec.ofNat 256 32 = _
  rw [← BitVec.ofNat_mul, BitVec.add_assoc, ← BitVec.ofNat_add]
  congr 2
  omega

private def storeFrame (step : Step) : Except String Frame := do
  match ← step with
  | .returned (none,next) => pure next
  | _ => .error "MIR storage loader did not return a memory frame"

private theorem store_frame (cfg : Config) (frame : Frame) (address word : Word) :
    storeFrame (memoryOperation cfg frame .store [address,word]) = (do
      let memory ← expand frame.memory address.toNat 32
      pure {frame with memory := writeBytes memory address.toNat (wordBytes word)}) := by
  simp only [storeFrame, memoryOperation, wordBytes_length]
  cases expand frame.memory address.toNat 32 <;> rfl

/-- Execute the index phi, the unsigned comparison, and the back edge.
Fuel counts loop tests, including the final unsuccessful test. -/
private def indexLoop (cfg : Config) (base pointer count : Word) :
    Nat → Word → Frame → Except String Frame
  | 0, _, _ => .error "MIR storage loader exhausted its loop fuel"
  | fuel+1, index, frame =>
    if MIR.compare .lt index count = 1 then do
      let (word,next) := storageRead cfg frame (base + index)
      let next ← storeFrame
        (memoryOperation cfg next .store [pointer + MIR.eval .mul index 32 + 32,word])
      indexLoop cfg base pointer count fuel (index + 1) next
    else .ok frame

private theorem word_lt (a b : Nat) (ha : a < 2^256) (hb : b < 2^256) :
    (MIR.compare .lt (BitVec.ofNat 256 a) (BitVec.ofNat 256 b) = 1) ↔ a < b := by
  simp [MIR.compare, BitVec.ult, BitVec.toNat_ofNat, Nat.mod_eq_of_lt ha, Nat.mod_eq_of_lt hb]

private theorem loop_range (cfg : Config) (base pointer : Word) (n index : Nat)
    (frame : Frame) (fits : index + n < 2^256) :
    indexLoop cfg base pointer (BitVec.ofNat 256 (index+n)) (n+1) (BitVec.ofNat 256 index) frame =
      forIn (List.range' index n) frame (fun i state => do
        let (word,next) := storageRead cfg state (base + BitVec.ofNat 256 i)
        let destination := pointer + BitVec.ofNat 256 (32 + 32*i)
        let memory ← expand next.memory destination.toNat 32
        pure (.yield {next with memory := writeBytes memory destination.toNat (wordBytes word)})) := by
  induction n generalizing index frame with
  | zero =>
    have guard : ¬ (MIR.compare .lt (BitVec.ofNat 256 index) (BitVec.ofNat 256 (index+0)) = 1) :=
      fun h => by have := (word_lt index (index+0) (by omega) fits).mp h; omega
    rw [indexLoop, ite_eq_right guard]
    rfl
  | succ n ih =>
    have guard := (word_lt index (index+(n+1)) (by omega) fits).mpr (by omega)
    rw [indexLoop, ite_eq_left guard]
    simp only [List.range'_succ, List.forIn_cons, store_frame]
    rw [point pointer index]
    have nextIndex : BitVec.ofNat 256 index + (1 : Word) = BitVec.ofNat 256 (index+1) :=
      (BitVec.ofNat_add index 1).symm
    have countEq : index+(n+1) = index+1+n := by omega
    rw [nextIndex, countEq]
    generalize expand (storageRead cfg frame (base + BitVec.ofNat 256 index)).2.memory
      (pointer + BitVec.ofNat 256 (32 + 32*index)).toNat 32 = expanded
    cases expanded with
    | error message => rfl
    | ok memory =>
      simp only [Bind.bind, Except.bind, Pure.pure, Except.pure]
      apply ih
      omega

private def headerPanic (frame : Frame) (code : Word) : Step := do
  let bytes ← MIR.LowerChecks.scratchPayload frame.memory 0x4e487b71 [code]
  return .reverted (RevertData.fromBytes bytes)

private theorem bad_header (header : Word) (data : RevertData)
    (bad : bytesHeader header = .error data) : data = .panic 0x22 := by
  unfold bytesHeader at bad
  dsimp only at bad
  split at bad <;> split at bad <;> first | exact (Except.error.inj bad).symm | contradiction

/-- Operational interpretation of the typed loader shape. Gas on individual
MIR instructions is outside this functional frame model. -/
def lowered (cfg : Config) (frame : Frame) (slot : Word) : Step := do
  let (header,frame) := storageRead cfg frame slot
  match MIR.LowerStorageHeader.lowered header with
  | .error _ => headerPanic frame 34
  | .ok (long,length) =>
    let lengthWord := BitVec.ofNat 256 length
    let count := MIR.LowerStorageBytes.ceilWords lengthWord
    let size := MIR.LowerReturndata.size lengthWord
    if MIR.compare .lt size lengthWord = 1 then headerPanic frame 65 else
    match ← allocate frame size false true with
    | .reverted data => return .reverted data
    | .invalid => return .invalid
    | .returned (some pointer,frame) =>
      let frame ← storeFrame (memoryOperation cfg frame .store [pointer,lengthWord])
      if long then
        let (base,frame) ← fixedSlot frame [slot]
        if count > 2048 then .error "MIR storage work limit of 2048 words" else
        let frame ← indexLoop cfg base pointer (BitVec.ofNat 256 count) (count+1) 0 frame
        return .returned (some pointer,frame)
      else
        let frame ← storeFrame
          (memoryOperation cfg frame .store [pointer + 32, header &&& (~~~(255 : Word))])
        return .returned (some pointer,frame)
    | _ => .error "MIR allocation result missing"

theorem load (cfg : Config) (frame : Frame) (slot : Word)
    (bounded : frame.memory.length ≤ 65536) :
    lowered cfg frame slot = loadStorageBytes cfg frame slot := by
  simp only [lowered, loadStorageBytes, MIR.LowerStorageHeader.decode]
  cases read : storageRead cfg frame slot with
  | mk header readFrame =>
    cases decoded : bytesHeader header with
    | error data =>
      have sameMemory : readFrame.memory = frame.memory := by
        simp only [storageRead] at read
        rw [← (Prod.mk.inj read).2]
      rw [bad_header header data decoded]
      simp only [headerPanic, sameMemory,
        MIR.LowerChecks.scratch_payload _ 0x4e487b71 [34] bounded (by decide),
        Bind.bind, Except.bind, Pure.pure, Except.pure]
      rfl
    | ok pair =>
      obtain ⟨long,length⟩ := pair
      have bound := decoded_length_bound header long length decoded
      have ⟨countEq, sizeEq, noOverflow⟩ := allocation_extent header long length decoded
      dsimp only at countEq sizeEq noOverflow ⊢
      rw [noOverflow, sizeEq, countEq]
      have sourceGuard : ¬ 2^256 ≤ length + 63 := by omega
      simp only [show ¬ (0 : BitVec 1) = 1 by decide, ite_false, sourceGuard, store_frame]
      cases allocation : allocate readFrame
          (BitVec.ofNat 256 ((length + 31) / 32 * 32 + 32)) false true with
      | error _ => rfl
      | ok result =>
        cases result with
        | invalid => rfl
        | reverted _ => rfl
        | returned value =>
          obtain ⟨result,next⟩ := value
          cases result with
          | none => rfl
          | some pointer =>
            simp only [Bind.bind, Except.bind, Pure.pure, Except.pure]
            cases expand next.memory pointer.toNat 32 with
            | error _ => rfl
            | ok memory =>
              cases long
              · simp only [Bool.false_eq_true, ite_false]
                cases expand (writeBytes memory pointer.toNat (wordBytes (BitVec.ofNat 256 length)))
                  (pointer+32).toNat 32 <;> rfl
              · simp only [ite_true]
                generalize fixedSlot {next with
                  memory := writeBytes memory pointer.toNat (wordBytes (BitVec.ofNat 256 length))
                } [slot] = hashed
                cases hashed with
                | error _ => rfl
                | ok pair =>
                  obtain ⟨base, next⟩ := pair
                  by_cases tooLarge : (length+31)/32 > 2048
                  · simp [tooLarge]
                  · simp only [tooLarge, ite_false]
                    have loop := loop_range cfg base pointer ((length+31)/32) 0 next (by omega)
                    rw [Nat.zero_add, ← List.range_eq_range'] at loop
                    rw [show (0 : Word) = BitVec.ofNat 256 0 from rfl, loop]
                    simp only [Bind.bind, Except.bind, Pure.pure, Except.pure]

end SolarTV.MIR.LowerStorageLoad
