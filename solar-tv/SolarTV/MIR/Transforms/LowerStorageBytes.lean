import SolarTV.MIR.Transforms.LowerStorageHeader
import SolarTV.MIR.Transforms.StorageWordLoop

/-! Lowering the object form of a storage byte-string write. The live header
and length are read before any scratch writes. The clear helper runs only on
shrink, and the data words are loaded from live memory after that helper.
Complete words and a possible masked tail are written in source order. -/

namespace SolarTV.MIR.LowerStorageBytes
open EvmRules SolarTV.Execution MIR.State

def load (frame : Frame) (address : Word) : Except String (Word × Frame) := do
  let memory ← expand frame.memory address.toNat 32
  return (readWord memory address.toNat,{frame with memory})

def mask (keep : Nat) : Word := ~~~(((1 : Word) <<< (8*(32-keep))) - 1)

private theorem mask_eq (keep : Nat) :
    mask keep = ~~~(BitVec.ofNat 256 (2^(8*(32-keep))-1)) := by
  have power : 1 ≤ 2^(8*(32-keep)) := Nat.one_le_two_pow
  have shifted : (1 : Word) <<< (8*(32-keep)) = BitVec.ofNat 256 (2^(8*(32-keep))) := by
    apply BitVec.eq_of_toNat_eq
    simp [Nat.shiftLeft_eq]
  have h := BitVec.ofNat_add (n := 256) (2^(8*(32-keep))-1) 1
  rw [Nat.sub_add_cancel power] at h
  unfold mask
  rw [shifted, h]
  congr 1
  exact BitVec.add_sub_cancel _ _

private theorem short_mask (length : Nat) :
    mask length = (if length = 0 then 0 else ~~~(BitVec.ofNat 256 (2^(8*(32-length))-1))) := by
  split
  · rename_i zero
    subst length
    decide
  · exact mask_eq _

def checkedExtent (length : Word) : Bool := (length+31).ult length

theorem extent (length : Word) :
    checkedExtent length = decide (2^256 ≤ length.toNat+31) := by
  have h := length.isLt
  rw [checkedExtent, BitVec.ult, BitVec.toNat_add]
  change decide ((length.toNat + 31) % 2^256 < length.toNat) = _
  by_cases fits : length.toNat+31 < 2^256
  · rw [Nat.mod_eq_of_lt fits]
    simp [Nat.not_le_of_gt fits]
  · rw [Nat.mod_eq_sub_mod (by omega : 2^256 ≤ length.toNat+31),
      Nat.mod_eq_of_lt (by omega : length.toNat+31-2^256 < 2^256)]
    have overflow : 2^256 ≤ length.toNat+31 := by omega
    have wrapped : length.toNat+31-2^256 < length.toNat := by omega
    rw [decide_eq_true_eq.mpr overflow, decide_eq_true_eq.mpr wrapped]

def ceilWords (length : Word) : Nat := ((length+31) / 32).toNat

theorem ceilWords_eq (length : Word) (fits : length.toNat+31 < 2^256) :
    ceilWords length = (length.toNat+31)/32 := by
  rw [ceilWords, BitVec.toNat_udiv, BitVec.toNat_add]
  change ((length.toNat+31) % 2^256) / 32 = _
  rw [Nat.mod_eq_of_lt fits]

def tailBytes (length : Word) : Nat := (length &&& 31).toNat

theorem tailBytes_eq (length : Word) : tailBytes length = length.toNat%32 := by
  rw [tailBytes, BitVec.toNat_and]
  change length.toNat &&& (2^5-1) = length.toNat % 2^5
  exact Nat.and_two_pow_sub_one_eq_mod _ _

def writeAt (cfg : Config) (data base : Word) (frame : Frame)
    (index : Word) (keep : Nat) : Except String Frame := do
  let (word,frame) ← load frame (data + 32*index)
  storageWrite cfg frame (base + index) (word &&& mask keep)

def writeWord (cfg : Config) (data base : Word) (frame : Frame)
    (part : Nat × Nat) : Except String Frame :=
  writeAt cfg data base frame (BitVec.ofNat 256 part.1) part.2

def writePlan (length : Nat) : List (Nat × Nat) :=
  (List.range (length/32)).map (fun i => (i,32)) ++
    (if length % 32 = 0 then [] else [(length/32,length % 32)])

def writeLoop (cfg : Config) (frame : Frame) (data base : Word) (length : Nat) :
    Except String Frame := do
  let full := length/32
  let tailCount := tailBytes (BitVec.ofNat 256 length)
  let step := fun i state => writeAt cfg data base state i 32
  let frame ← match StorageWordLoop.lowered (BitVec.ofNat 256 full) step (2^256) 0 0 frame with
    | .ok frame => pure frame
    | .error (some reason) => throw reason
    | .error none => throw "MIR storage loop fuel exhausted"
  if tailCount = 0 then pure frame else writeWord cfg data base frame (full,tailCount)

private theorem indexedFold (f : Frame → Nat → Except String Frame) (indices : List Nat)
    (frame : Frame) :
    indices.foldlM (fun frame i => (f frame i).mapError some) frame =
      (indices.foldlM f frame).mapError some := by
  induction indices generalizing frame with
  | nil => rfl
  | cons i rest ih =>
    simp only [List.foldlM_cons, Bind.bind, Except.bind]
    cases f frame i with
    | error _ => rfl
    | ok next => exact ih next

private theorem writeLoop_eq (cfg : Config) (frame : Frame) (data base : Word)
    (length : Nat) (bounded : length < 2^256) :
    writeLoop cfg frame data base length =
      (writePlan length).foldlM (writeWord cfg data base) frame := by
  have fullBound : length/32 < 2^256 := by omega
  have fuel : (BitVec.ofNat 256 (length/32)).toNat - (0 : Word).toNat < 2^256 := by
    simpa [BitVec.toNat_ofNat, Nat.mod_eq_of_lt fullBound] using fullBound
  have zeroNat : (0 : Word).toNat = 0 := rfl
  have zeroAdd (x : Word) : (0 : Word) + x = x := BitVec.zero_add x
  simp only [writeLoop, StorageWordLoop.corresponds _ _ _ _ _ _ fuel,
    StorageWordLoop.reference, tailBytes_eq, BitVec.toNat_ofNat,
    Nat.mod_eq_of_lt bounded, Nat.mod_eq_of_lt fullBound, zeroNat,
    Nat.sub_zero, zeroAdd, writePlan, List.foldlM_append, List.foldlM_map]
  change (match (List.range (length/32)).foldlM
      (fun state i => (writeWord cfg data base state (i,32)).mapError some) frame with
    | .ok next => _
    | .error (some reason) => _
    | .error none => _) = _
  rw [indexedFold]
  cases (List.range (length/32)).foldlM
      (fun frame i => writeWord cfg data base frame (i,32)) frame with
  | error _ => rfl
  | ok next =>
    by_cases aligned : length % 32 = 0
    · simp [aligned, Except.mapError, Except.bind, Bind.bind, Pure.pure, Except.pure]
    · simp [aligned, Except.mapError, Except.bind, Bind.bind, Pure.pure, Except.pure]
      cases writeWord cfg data base next (length/32,length%32) <;> rfl

private theorem storeWords (cfg : Config) (frame : Frame) (object base : Word) (length : Nat) :
    (do
      let frame ← (writePlan length).foldlM (writeWord cfg (object+32) base) frame
      pure (Outcome.returned (none,frame))) =
    (do
      let frame ← (List.range ((length+31)/32)).foldlM (fun frame i => do
        let address := object + BitVec.ofNat 256 (32+32*i)
        let memory ← expand frame.memory address.toNat 32
        storageWrite cfg {frame with memory} (base + BitVec.ofNat 256 i)
          (readWord memory address.toNat &&&
            ~~~(BitVec.ofNat 256 (2^(8*(32-min 32 (length-32*i)))-1)))) frame
      pure (.returned (none,frame)) : Step) := by
  simp only [List.foldlM_map, writePlan,
    ← StorageWordLoop.fullWordsThenPartial, writeWord, writeAt, load, mask_eq,
    BitVec.ofNat_add, BitVec.add_assoc, Except.bind, Bind.bind, Pure.pure, Except.pure]
  congr 2
  funext frame i
  have offset : (32 : Word) * BitVec.ofNat 256 i = BitVec.ofNat 256 (32*i) :=
    BitVec.ofNat_mul_ofNat 32 i
  rw [offset]
  cases expand frame.memory (object + (32 + BitVec.ofNat 256 (32*i))).toNat 32 <;> rfl

def loweredPayload (cfg : Config) (frame : Frame) (slot object : Word) (length : Nat) : Step := do
  if length < 32 then
    let (word,next) ← load frame (object+32)
    if cfg.environment.isStatic then return .invalid else
    let next ← storageWrite cfg next slot ((word &&& mask length) ||| BitVec.ofNat 256 (2*length))
    return .returned (none,next)
  else
    if cfg.environment.isStatic then return .invalid else
    let frame ← storageWrite cfg frame slot (BitVec.ofNat 256 (2*length+1))
    let (base,next) ← fixedSlot frame [slot]
    if (length+31)/32 > 2048 then .error "MIR storage work limit of 2048 words" else
    let next ← writeLoop cfg next (object+32) base length
    return .returned (none,next)

private theorem payload (cfg : Config) (frame : Frame) (slot object : Word) (length : Nat) :
    loweredPayload cfg frame slot object length =
      storeStoragePayload cfg frame slot object none length := by
  simp only [loweredPayload, storeStoragePayload]
  split
  · simp [load, short_mask]
  · by_cases static : cfg.environment.isStatic = true
    · simp [static]
    · simp only [static, Bool.false_eq_true, ite_false, Except.bind, Bind.bind, Pure.pure, Except.pure]
      generalize storageWrite cfg frame slot (BitVec.ofNat 256 (2*length+1)) = written
      cases written with
      | error _ => rfl
      | ok next =>
        dsimp only
        generalize fixedSlot next [slot] = hashed
        cases hashed with
        | error _ => rfl
        | ok hashed =>
          obtain ⟨base,hashed⟩ := hashed
          by_cases tooLarge : (length+31)/32 > 2048
          · simp [tooLarge]
          · have bounded : length < 2^256 := by omega
            simpa only [tooLarge, ite_false, writeLoop_eq _ _ _ _ _ bounded,
              Except.bind, Bind.bind, Pure.pure, Except.pure]
              using storeWords cfg hashed object base length

/-- State at the joined exit of the lowered CFG. Words in the plan denote the
index loop followed by its conditional partial-word block. The clear loop has
a separate fuel bound. Both bounds concern the validator, not EVM gas. -/
def lowered (cfg : Config) (frame : Frame) (slot object : Word) : Step := do
  let (header,frame) := storageRead cfg frame slot
  match LowerStorageHeader.lowered header with
  | .error data => return .reverted data
  | .ok (oldLong,oldLength) =>
    let (lengthWord,frame) ← load frame object
    if checkedExtent lengthWord then return .reverted (.panic 0x41) else
    let length := lengthWord.toNat
    let count := if length < 32 then 0 else ceilWords lengthWord
    let mut frame := frame
    if oldLong && oldLength > length then
      match ← StorageWordLoop.clear cfg frame slot (BitVec.ofNat 256 count)
          (BitVec.ofNat 256 ((oldLength+31)/32)) (2^256) with
      | .returned (_,next) => frame := next
      | .invalid => return .invalid
      | .reverted data => return .reverted data
    loweredPayload cfg frame slot object length

theorem store (cfg : Config) (frame : Frame) (slot object : Word) :
    lowered cfg frame slot object = storeStorageBytes cfg frame slot object none := by
  simp only [lowered, storeStorageBytes, LowerStorageHeader.decode]
  cases storageRead cfg frame slot with
  | mk header readFrame =>
    cases bytesHeader header with
    | error _ => rfl
    | ok decoded =>
      obtain ⟨oldLong,oldLength⟩ := decoded
      simp only [load, Except.bind, Bind.bind, Pure.pure, Except.pure]
      cases expand readFrame.memory object.toNat 32 with
      | error _ => rfl
      | ok memory =>
        simp only [extent, decide_eq_true_eq, payload]
        have clear_eq (next : Frame) (first last : Word) :=
          StorageWordLoop.clear_corresponds cfg next slot first last (2^256)
            (Nat.lt_of_le_of_lt (Nat.sub_le _ _) last.isLt)
        simp only [clear_eq]
        by_cases overflow : 2^256 ≤ (readWord memory object.toNat).toNat+31
        · simp [overflow]
        · have fits := Nat.lt_of_not_ge overflow
          simp only [overflow, ite_false, ceilWords_eq _ fits]
          rfl

end SolarTV.MIR.LowerStorageBytes
