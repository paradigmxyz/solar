import SolarTV.MIR.Transforms.HeapCursor

/-! Noninterference of the private free-memory cursor word. Paired states
agree on the active extent and every other byte. Identical writes, including
writes that overlap the word, preserve the relation. Reads must prove that
their range avoids the word. The relation ends at a frame observation, or
continues through an internal return into a checked caller. -/

namespace SolarTV.MIR.PrivateCursor
open EvmRules SolarTV.Execution MIR.State

def Outside (offset size : Nat) : Prop := offset + size ≤ 64 ∨ 96 ≤ offset

def Agree (left right : List Byte) : Prop :=
  left.length = right.length ∧
    ∀ i, i < 64 ∨ 96 ≤ i → left[i]? = right[i]?

theorem self (bytes : List Byte) : Agree bytes bytes := ⟨rfl, fun _ _ => rfl⟩

theorem read (left right : List Byte) (offset size : Nat) (related : Agree left right)
    (separate : Outside offset size) :
    readBytes left offset size = readBytes right offset size := by
  unfold readBytes
  apply List.map_congr_left
  intro i mem
  have index := List.mem_range.mp mem
  have outside : offset + i < 64 ∨ 96 ≤ offset + i := by
    rcases separate with early | late
    · left; omega
    · right; omega
  rw [related.2 _ outside]

theorem write (left right bytes : List Byte) (offset : Nat) (related : Agree left right) :
    Agree (writeBytes left offset bytes) (writeBytes right offset bytes) := by
  refine ⟨by simpa using related.1, ?_⟩
  intro i separate
  by_cases active : i < left.length
  · have active' : i < right.length := related.1 ▸ active
    have eq := related.2 i separate
    rw [List.getElem?_eq_getElem active,List.getElem?_eq_getElem active'] at eq
    simp [writeBytes_eq_spec, writeBytesSpec, active, active',
      Option.some.inj eq]
  · have active' : ¬ i < right.length := related.1 ▸ active
    have l : (writeBytes left offset bytes)[i]? = none :=
      List.getElem?_eq_none (by simpa using Nat.le_of_not_gt active)
    have r : (writeBytes right offset bytes)[i]? = none :=
      List.getElem?_eq_none (by simpa using Nat.le_of_not_gt active')
    rw [l,r]

theorem publish (left right : List Byte) (word : Word) (related : Agree left right) :
    Agree (writeBytes left 64 (wordBytes word)) right := by
  refine ⟨by simpa using related.1, ?_⟩
  intro i outside
  have separate : ¬ (64 ≤ i ∧ i < 96) := by omega
  rw [← related.2 i outside]
  simp [writeBytes_eq_spec, writeBytesSpec, List.getElem?_mapIdx, separate]

theorem grow (left right : List Byte) (offset size : Nat) (related : Agree left right) :
    match expand left offset size, expand right offset size with
    | .ok a,.ok b => Agree a b
    | .error a,.error b => a = b
    | _,_ => False := by
  have same : requiredWords left offset size = requiredWords right offset size := by
    simp [requiredWords, memoryWords, related.1]
  simp only [expand, same, related.1]
  by_cases cap : 65536 < 32 * requiredWords right offset size
  · simp [cap]
  · simp only [ite_eq_right (fun h => False.elim (cap h))]
    refine ⟨by simp [related.1], ?_⟩
    intro i separate
    simp [List.getElem?_append, related.1, related.2 i separate]

def Frames (left right : Frame) : Prop :=
  {left with memory := right.memory} = right ∧ Agree left.memory right.memory

theorem replace_memory (left right : Frame) (a b : List Byte)
    (related : Frames left right) (mem : Agree a b) :
    Frames {left with memory := a} {right with memory := b} :=
  ⟨congrArg (fun f : Frame => {f with memory := b}) related.1,mem⟩

def Results (left right : Step) : Prop :=
  match left,right with
  | .ok (.returned (word,a)), .ok (.returned (other,b)) => word = other ∧ Frames a b
  | .ok (.reverted a), .ok (.reverted b) => a = b
  | .ok .invalid,.ok .invalid => True
  | .error a,.error b => a = b
  | _,_ => False

theorem frames_self (frame : Frame) : Frames frame frame := ⟨rfl, self frame.memory⟩

theorem skip_store (cfg : Config) (left right : Frame) (word : Word)
    (related : Frames left right) (active : 96 ≤ left.memory.length)
    (aligned : left.memory.length % 32 = 0) (bounded : left.memory.length ≤ 65536) :
    Results (memoryOperation cfg left .store [64,word]) (effect right) := by
  have expanded : expand left.memory (64 : Word).toNat (wordBytes word).length = .ok left.memory := by
    simpa using FreshHeap.active_slot _ bounded aligned active
  simp only [memoryOperation, expanded, Bind.bind, Except.bind, effect, Results]
  exact ⟨True.intro, related.1, publish _ _ _ related.2⟩

inductive Access where
  | load (offset : Word)
  | hash (offset size : Word)
  | store (offset word : Word)
  | zero (offset size : Word)

def Access.run (op : Access) (cfg : Config) (frame : Frame) : Step :=
  match op with
  | .load offset => memoryOperation cfg frame .load [offset]
  | .hash offset size => memoryOperation cfg frame .hash [offset,size]
  | .store offset word => memoryOperation cfg frame .store [offset,word]
  | .zero offset size => memoryOperation cfg frame .zero [offset,size]

def Access.safe : Access → Prop
  | .load offset => Outside offset.toNat 32
  | .hash offset size => Outside offset.toNat size.toNat
  | .store .. | .zero .. => True

private theorem expand_steps (left right : Frame) (offset size : Nat)
    (next : Frame → Step) (related : Frames left right)
    (continues : ∀ a b, Frames a b → Results (next a) (next b)) :
    Results ((expand left.memory offset size).bind fun memory => next {left with memory})
      ((expand right.memory offset size).bind fun memory => next {right with memory}) := by
  have grown := grow left.memory right.memory offset size related.2
  cases one : expand left.memory offset size <;> cases two : expand right.memory offset size <;>
    simp only [one,two,Except.bind] at grown ⊢
  all_goals try contradiction
  · exact grown
  · exact continues _ _ (replace_memory _ _ _ _ related grown)

theorem access (cfg : Config) (left right : Frame) (op : Access)
    (related : Frames left right) (safe : op.safe) :
    Results (op.run cfg left) (op.run cfg right) := by
  cases op <;>
    simp only [Access.run, Access.safe, memoryOperation, wordBytes_length, Bind.bind] at safe ⊢
  · rename_i offset
    apply expand_steps _ _ _ _ (fun a => value a (readWord a.memory offset.toNat)) related
    intro a b frames
    exact ⟨by unfold readWord; rw [read _ _ _ _ frames.2 safe],frames⟩
  · rename_i offset size
    apply expand_steps _ _ _ _
      (fun a => value a (Keccak.hash (readBytes a.memory offset.toNat size.toNat))) related
    intro a b frames
    exact ⟨by rw [read _ _ _ _ frames.2 safe],frames⟩
  · rename_i offset word
    apply expand_steps _ _ _ _
      (fun a => effect {a with memory := writeBytes a.memory offset.toNat (wordBytes word)}) related
    intro a b frames
    exact ⟨rfl,replace_memory _ _ _ _ frames (write _ _ _ _ frames.2)⟩
  · rename_i offset size
    apply expand_steps _ _ _ _ (fun a =>
      effect {a with memory := writeBytes a.memory offset.toNat (List.replicate size.toNat 0)}) related
    intro a b frames
    exact ⟨rfl,replace_memory _ _ _ _ frames (write _ _ _ _ frames.2)⟩

theorem storage_read (cfg : Config) (left right : Frame) (key : Word)
    (related : Frames left right) :
    Results (storageOperation cfg left false false [key])
      (storageOperation cfg right false false [key]) := by
  cases left
  cases right
  obtain ⟨eq,mem⟩ := related
  simp only [Frame.mk.injEq] at eq
  obtain ⟨rfl,_,rfl,rfl,rfl,rfl,rfl,rfl⟩ := eq
  exact ⟨rfl,rfl,mem⟩

def endBytes (offset size : Word) (reverted : Bool) (frame : Frame) :
    Except String Observation := do
  let memory ← expand frame.memory offset.toNat size.toNat
  let bytes := readBytes memory offset.toNat size.toNat
  return observe (finish (if reverted then .reverted bytes else .success bytes)
    {frame with memory})

theorem terminal (left right : Frame) (offset size : Word) (reverted : Bool)
    (related : Frames left right) (safe : Outside offset.toNat size.toNat) :
    endBytes offset size reverted left = endBytes offset size reverted right := by
  have grown := grow left.memory right.memory offset.toNat size.toNat related.2
  unfold endBytes
  cases a : expand left.memory offset.toNat size.toNat <;>
    cases b : expand right.memory offset.toNat size.toNat <;>
    simp only [a,b,Bind.bind,Except.bind] at grown ⊢
  all_goals try contradiction
  · rw [grown]
  · rw [read _ _ _ _ grown safe]
    have shared := congrArg Frame.world related.1
    have gas := congrArg Frame.messageGas related.1
    change left.world = right.world at shared
    change left.messageGas = right.messageGas at gas
    cases reverted <;> simp [observe, finish, shared, gas, Pure.pure, Except.pure]

/-- The ABI encoder places its leading word immediately before the object.
This bound applies even when the payload size is computed dynamically. -/
theorem floor_header (floor : Word) (size : Nat) (minimum : 128 ≤ floor.toNat) :
    Outside (floor + (0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0 : Word)).toNat size := by
  have sum : floor + (0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0 : Word) =
      floor - 32 := by
    rw [BitVec.sub_eq_add_neg]
    rfl
  have works : (32 : Word) ≤ floor := by
    change 32 ≤ floor.toNat
    omega
  rw [sum, BitVec.toNat_sub_of_le works]
  exact Or.inr (by change 96 ≤ floor.toNat - 32; omega)

theorem floor_read (floor : Word) (size : Nat) (minimum : 128 ≤ floor.toNat) :
    Outside floor.toNat size := Or.inr (by omega)

inductive Runs (step : Nat → Nat → Frame → Frame → Prop) : Frame → Frame → Prop
  | start (frame : Frame) : Runs step frame frame
  | next {pc target : Nat} {left right left' right' : Frame} :
      Runs step left right → step pc target left left' → step pc target right right' →
      Runs step left' right'
  | skipped {left right left' : Frame} (cfg : Config) (word : Word) :
      Runs step left right →
      96 ≤ left.memory.length → left.memory.length % 32 = 0 →
      left.memory.length ≤ 65536 →
      memoryOperation cfg left .store [64,word] = effect left' →
      Runs step left' right

/-- Preserve the frame relation across arbitrary repetitions of the paired
control steps, including interprocedural return continuations. -/
theorem runs (step : Nat → Nat → Frame → Frame → Prop)
    (shared : ∀ {pc target left right left' right'}, Frames left right →
      step pc target left left' → step pc target right right' → Frames left' right')
    {left right : Frame} (trace : Runs step left right) : Frames left right := by
  induction trace with
  | start => exact frames_self _
  | next _ a b ih => exact shared ih a b
  | @skipped left right next cfg word _ active aligned bounded wrote ih =>
    have h := skip_store cfg left right word ih active aligned bounded
    rw [wrote] at h
    exact h.2

end SolarTV.MIR.PrivateCursor
