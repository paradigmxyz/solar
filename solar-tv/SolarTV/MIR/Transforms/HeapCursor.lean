import SolarTV.MIR.Transforms.StateReadReuse
import SolarTV.MIR.Transforms.FreshHeap

/-! Cursor facts at logical MIR function entry and their preservation along
the retained effects of the source CFG. Physical entry establishment is
separate from this conditional certificate. -/
namespace SolarTV.MIR.HeapCursor
open EvmRules SolarTV.Execution MIR.State

private theorem add_natural (base size : Word) (ordered : base.toNat ≤ (base + size).toNat) :
    (base + size).toNat = base.toNat + size.toNat := by
  have hb := base.isLt
  have hs := size.isLt
  rw [BitVec.toNat_add] at ordered ⊢
  by_cases small : base.toNat + size.toNat < 2^256
  · exact Nat.mod_eq_of_lt small
  · rw [Nat.mod_eq_sub_mod (by omega), Nat.mod_eq_of_lt (by omega)] at ordered
    omega

/-- On the continuing branch the actual heap operation publishes the end of its
reservation. Its result is the old cursor. No range separation is assumed. -/
theorem continuing_reservation (frame next : Frame) (pointer size : Word)
    (checked : Bool) (aligned : frame.memory.length % 32 = 0)
    (evaluation : allocate frame size false checked = value next pointer) :
    Universal.MemoryValid next ∧ 96 ≤ next.memory.length ∧
      readWord next.memory 64 = pointer + size ∧
      (pointer + size).toNat = pointer.toNat + size.toNat ∧
      (pointer + size).toNat ≤ 2^64 - 1 := by
  simp only [allocate, value, Bind.bind, Except.bind, Pure.pure, Except.pure] at evaluation
  split at evaluation
  · contradiction
  · rename_i grown expanded
    split at evaluation
    · split at evaluation <;> cases evaluation
    · rename_i admitted
      simp only [Bool.or_eq_true, decide_eq_true_eq, not_or] at admitted
      have ⟨a,b,c⟩ := StateReadReuse.expanded_extent _ _ 64 32 aligned (by decide) expanded
      cases evaluation
      simp only [Universal.MemoryValid, writeBytes_length]
      have written := readWord_writeWord grown 64 (readWord grown 64 + size) c
      exact ⟨⟨b,a⟩,c,written, add_natural _ _ (by omega),by omega⟩

/-- The published end stays observable after a store outside the reserved FMP
word, and a later FMP operation reads it without changing the frame. -/
theorem observe_after_store (frame : Frame) (cursor : Word) (destination : Nat)
    (payload : List Byte)
    (memory : Universal.MemoryValid frame) (active : 96 ≤ frame.memory.length)
    (published : readWord frame.memory 64 = cursor)
    (outside : destination + payload.length ≤ 64 ∨ 96 ≤ destination) :
    let after := {frame with memory := writeBytes frame.memory destination payload}
    freePointer after = value after cursor := by
  obtain ⟨bounded,aligned⟩ := memory
  have ⟨_,same⟩ := StateReadReuse.word_write_disjoint frame.memory payload destination 64 active outside
  have expands := MIR.FreshHeap.active_slot (writeBytes frame.memory destination payload)
    (by simpa using bounded) (by simpa using aligned) (by simpa using active)
  simp [freePointer, value, expands, same, published, Bind.bind, Except.bind, Pure.pure, Except.pure]

def At (frame : Frame) (floor : Word) (offset : Nat) : Prop :=
  Universal.MemoryValid frame ∧ 96 ≤ frame.memory.length ∧
    readWord frame.memory 64 = floor + BitVec.ofNat 256 offset ∧
    128 ≤ (floor + BitVec.ofNat 256 offset).toNat ∧
    (floor + BitVec.ofNat 256 offset).toNat ≤ 2^64 - 1

def Entry (frame : Frame) (floor : Word) : Prop := At frame floor 0

theorem entry_inhabited (frame : Frame) :
    Entry {frame with memory := writeBytes (List.replicate 96 0) 64 (wordBytes 128)} 128 := by
  simp [Entry, At, Universal.MemoryValid, ↓readWord_writeWord, BitVec.ofNat]

theorem offset_nat (floor : Word) (offset : Nat)
    (bounded : floor.toNat ≤ 2^64 - 1) (small : offset ≤ 65536) :
    (floor + BitVec.ofNat 256 offset).toNat = floor.toNat + offset := by
  have fits : offset < 2^256 := by omega
  have sum : floor.toNat + offset < 2^256 := by omega
  simp only [BitVec.toNat_add, BitVec.toNat_ofNat, Nat.mod_eq_of_lt fits, Nat.mod_eq_of_lt sum]

theorem floor_separates (floor : Word) (offset low width : Nat)
    (lower : 128 ≤ floor.toNat) (bounded : floor.toNat ≤ 2^64 - 1)
    (small : offset ≤ 65536) (fits : low + width ≤ 128 + offset) :
    low + width ≤ (floor + BitVec.ofNat 256 offset).toNat := by
  rw [offset_nat _ _ bounded small]
  omega

theorem offsets_separate (floor : Word) (left right width : Nat)
    (bounded : floor.toNat ≤ 2^64 - 1) (leftSmall : left ≤ 65536)
    (rightSmall : right ≤ 65536) (fits : left + width ≤ right) :
    (floor + BitVec.ofNat 256 left).toNat + width ≤
      (floor + BitVec.ofNat 256 right).toNat := by
  rw [offset_nat _ _ bounded leftSmall, offset_nat _ _ bounded rightSmall]
  omega

theorem reservation_at (before after : Frame) (floor pointer : Word) (old size : Nat)
    (checked : Bool) (valid : At before floor old)
    (evaluation : allocate before (BitVec.ofNat 256 size) false checked = value after pointer) :
    pointer = floor + BitVec.ofNat 256 old ∧ At after floor (old + size) := by
  obtain ⟨memory,active,cursor,minimum,_⟩ := valid
  have expanded := FreshHeap.active_slot _ memory.1 memory.2 active
  have origin : pointer = floor + BitVec.ofNat 256 old := by
    simp only [allocate, expanded, cursor, value, Bind.bind, Except.bind, Pure.pure, Except.pure] at evaluation
    split at evaluation
    · split at evaluation <;> cases evaluation
    · cases evaluation
      rfl
  have ⟨preserved,newActive,published,nonwrapping,bounded⟩ :=
    continuing_reservation _ _ _ _ checked memory.2 evaluation
  have endWord : pointer + BitVec.ofNat 256 size = floor + BitVec.ofNat 256 (old + size) := by
    rw [origin, BitVec.add_assoc, BitVec.ofNat_add]
  have lower : 128 ≤ (pointer + BitVec.ofNat 256 size).toNat := by
    rw [← origin] at minimum
    omega
  rw [endWord] at published lower bounded
  exact ⟨origin,preserved,newActive,published,lower,bounded⟩

theorem observe_at (frame : Frame) (floor : Word) (offset : Nat)
    (valid : At frame floor offset) :
    freePointer frame = value frame (floor + BitVec.ofNat 256 offset) := by
  obtain ⟨memory,active,cursor,_,_⟩ := valid
  simp [freePointer, value, FreshHeap.active_slot _ memory.1 memory.2 active, cursor,
    Bind.bind, Except.bind, Pure.pure, Except.pure]

theorem compose_offset (floor : Word) (first second : Nat) :
    floor + BitVec.ofNat 256 first + BitVec.ofNat 256 second =
      floor + BitVec.ofNat 256 (first + second) := by
  rw [BitVec.add_assoc, BitVec.ofNat_add]

theorem callee_entry (frame : Frame) (floor : Word) (offset : Nat)
    (known : At frame floor offset) :
    Entry frame (floor + BitVec.ofNat 256 offset) := by
  simpa [Entry, At] using known

theorem callee_return (frame : Frame) (floor : Word) (source bump : Nat)
    (returned : At frame (floor + BitVec.ofNat 256 source) bump) :
    At frame floor (source + bump) := by
  simpa only [At, compose_offset] using returned

theorem preserve_expansion (frame : Frame) (floor : Word) (cursor offset size : Nat)
    (grown : List Byte) (valid : At frame floor cursor)
    (expansion : expand frame.memory offset size = .ok grown) :
    At {frame with memory := grown} floor cursor := by
  obtain ⟨⟨_,aligned⟩,active,word,lower,upper⟩ := valid
  have ⟨length,bounded⟩ := DceEffects.expanded_size _ _ _ _ aligned expansion
  have ⟨active',same⟩ := StateReadReuse.expanded_read _ _ _ _ 64 active expansion
  have grownAligned : grown.length % 32 = 0 := by omega
  exact ⟨⟨bounded,grownAligned⟩,active',same.trans word,lower,upper⟩

theorem preserve_write (frame : Frame) (floor : Word) (cursor destination : Nat)
    (data : List Byte) (valid : At frame floor cursor)
    (disjoint : destination + data.length ≤ 64 ∨ 96 ≤ destination) :
    At {frame with memory := writeBytes frame.memory destination data} floor cursor := by
  obtain ⟨⟨bounded,aligned⟩,active,word,lower,upper⟩ := valid
  have ⟨active',same⟩ := StateReadReuse.word_write_disjoint _ _ _ 64 active disjoint
  exact ⟨⟨by simpa using bounded,by simpa using aligned⟩,active',same.trans word,lower,upper⟩

inductive RawMessage where
  | call | callcode | delegatecall | staticcall

def RawMessage.opcode : RawMessage → Nat
  | .call => 0xf1
  | .callcode => 0xf2
  | .delegatecall => 0xf4
  | .staticcall => 0xfa

def RawMessage.args (kind : RawMessage) (target amount inputOffset inputSize outOffset outSize : Word) :
    List Word :=
  match kind with
  | .call | .callcode => [target,amount,inputOffset,inputSize,outOffset,outSize]
  | .delegatecall | .staticcall => [target,inputOffset,inputSize,outOffset,outSize]

private theorem returned_output (fuel : Nat) (cfg : Config) (frame parent : Frame)
    (kind : RawMessage) (target amount inputOffset inputSize outputOffset outputSize : Word)
    (gas result : Option Word) (work : Nat)
    (returned : external fuel cfg frame kind.opcode
      (kind.args target amount inputOffset inputSize outputOffset outputSize) gas =
      .ok (some (.returned (result,parent),work))) :
    ∃ input output data, expand frame.memory inputOffset.toNat inputSize.toNat = .ok input ∧
      expand input outputOffset.toNat outputSize.toNat = .ok output ∧
      data.length ≤ outputSize.toNat ∧ parent.memory = writeBytes output outputOffset.toNat data := by
  cases kind <;>
    simp only [RawMessage.opcode, RawMessage.args, external, Bind.bind, Except.bind,
      Pure.pure, Except.pure] at returned
  all_goals
    split at returned <;> try contradiction
    split at returned <;> try contradiction
    simp at returned
  focus
    split at returned
    · cases returned
  all_goals
    split at returned <;> try contradiction
    split at returned <;> try contradiction
    split at returned <;> try contradiction
    rename_i input inMemory
    split at returned <;> try contradiction
    rename_i output outMemory
    split at returned <;> try contradiction
    split at returned
    · split at returned <;> try contradiction
      rename_i child childReturns
      injection returned with same
      simp only [Option.some.injEq, Prod.mk.injEq, Outcome.returned.injEq] at same
      rcases same with ⟨⟨_,rfl⟩,_⟩
      exact ⟨input,output,child.data.take outputSize.toNat,inMemory,outMemory,
        by simpa only [List.length_take] using Nat.min_le_left outputSize.toNat child.data.length,rfl⟩
    · cases returned

/-- A returning raw message expands memory and writes at most its own output
range. Child execution cannot modify the parent's other active words. -/
theorem raw_call_preserves_word (fuel : Nat) (cfg : Config) (frame parent : Frame)
    (kind : RawMessage) (target amount inputOffset inputSize outputOffset outputSize : Word)
    (gas result : Option Word) (work source : Nat)
    (valid : Universal.MemoryValid frame) (covered : source + 32 ≤ frame.memory.length)
    (disjoint : outputSize.toNat = 0 ∨ outputOffset.toNat + outputSize.toNat ≤ source ∨
      source + 32 ≤ outputOffset.toNat)
    (returned : external fuel cfg frame kind.opcode
      (kind.args target amount inputOffset inputSize outputOffset outputSize) gas =
      .ok (some (.returned (result,parent),work))) :
    Universal.MemoryValid parent ∧ source + 32 ≤ parent.memory.length ∧
      readWord parent.memory source = readWord frame.memory source := by
  obtain ⟨input,output,data,first,second,count,mem⟩ :=
    returned_output fuel cfg frame parent kind target amount inputOffset inputSize outputOffset
      outputSize gas result work returned
  have ⟨inputActive,inputRead⟩ := StateReadReuse.expanded_read _ _ _ _ _ covered first
  have ⟨outputActive,outputRead⟩ := StateReadReuse.expanded_read _ _ _ _ _ inputActive second
  have ⟨inputLength,_⟩ := DceEffects.expanded_size _ _ _ _ valid.2 first
  have inputAligned : input.length % 32 = 0 := by omega
  have ⟨outputLength,outputBound⟩ := DceEffects.expanded_size _ _ _ _ inputAligned second
  have validOutput : Universal.MemoryValid parent := by
    simp only [Universal.MemoryValid, mem, writeBytes_length]
    exact ⟨outputBound,by omega⟩
  refine ⟨validOutput,?_⟩
  rcases disjoint with empty | disjoint
  · have noWrite : data = [] := List.length_eq_zero_iff.mp (by omega)
    rw [mem, noWrite]
    simpa [writeBytes] using
      (show source + 32 ≤ output.length ∧ readWord output source = readWord frame.memory source
        from ⟨outputActive,outputRead.trans inputRead⟩)
  have outside : outputOffset.toNat + data.length ≤ source ∨ source + 32 ≤ outputOffset.toNat := by
    rcases disjoint with early | late
    · exact Or.inl (by omega)
    · exact Or.inr late
  rw [mem]
  have ⟨active,wordRead⟩ := StateReadReuse.word_write_disjoint _ _ _ _ outputActive outside
  exact ⟨active,wordRead.trans (outputRead.trans inputRead)⟩

structure Transfer where
  source : Nat
  into : Nat
  advance : Option Nat

structure Observation where
  point : Nat
  displacement : Nat
  offset : Nat

abbrev Offsets := Array (Option Nat)

def atPoint (offsets : Offsets) (point : Nat) : Option Nat :=
  offsets[point]?.join

def checkTransfer (offsets : Offsets) (edge : Transfer) : Bool :=
  match atPoint offsets edge.into with
  | none => true
  | some endOffset =>
    (do
      let start ← atPoint offsets edge.source
      let delta ← edge.advance
      pure (start + delta)) == some endOffset

def checkObservation (offsets : Offsets) (read : Observation) : Bool :=
  (atPoint offsets read.point |>.map (· + read.displacement)) == some read.offset

inductive Runs (edges : List Transfer) (step : Transfer → Frame → Frame → Prop) :
    Nat → Frame → Prop where
  | entry (frame : Frame) : Runs edges step 0 frame
  | next {edge before after} : Runs edges step edge.source before →
      edge ∈ edges → step edge before after → Runs edges step edge.into after

theorem invariant (offsets : Offsets) (edges : List Transfer)
    (initial : atPoint offsets 0 = some 0) (verified : edges.all (checkTransfer offsets) = true)
    (step : Transfer → Frame → Frame → Prop) (floor : Word)
    (start : ∀ frame, Runs edges step 0 frame → At frame floor 0)
    (keeps : ∀ edge before after head delta, edge ∈ edges →
      atPoint offsets edge.source = some head → edge.advance = some delta →
      step edge before after → At before floor head → At after floor (head + delta))
    {point frame} (run : Runs edges step point frame) {offset}
    (known : atPoint offsets point = some offset) :
    At frame floor offset := by
  induction run generalizing offset with
  | entry frame =>
    rw [initial] at known
    cases known
    exact start _ (.entry frame)
  | @next edge before after _ member action ih =>
    have transfer := List.all_eq_true.mp verified _ member
    simp only [checkTransfer, known] at transfer
    cases one : atPoint offsets edge.source with
    | none => simp [one, Bind.bind, Option.bind] at transfer
    | some first =>
      cases two : edge.advance with
      | none => simp [one, two, Bind.bind, Option.bind] at transfer
      | some delta =>
        have same : first + delta = offset := by
          simpa [one, two, Bind.bind, Option.bind] using transfer
        rw [← same]
        exact keeps edge before after first delta member one two action (ih one)

theorem observation (offsets : Offsets) (read : Observation)
    (verified : checkObservation offsets read = true) :
    ∃ origin, atPoint offsets read.point = some origin ∧ origin + read.displacement = read.offset := by
  cases found : atPoint offsets read.point with
  | none => simp [checkObservation, found] at verified
  | some origin => exact ⟨origin,rfl,by simpa [checkObservation, found] using verified⟩

end SolarTV.MIR.HeapCursor
