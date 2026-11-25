import SolarTV.MIR.Transforms.PhaseShaping
import SolarTV.State.MemoryLaws

/-! A fixed-width storage key hashes one or two words in reserved scratch.
Compare the semantic key operation with its ordinary memory instructions at
the boundary after the hash, including the scratch bytes and active extent. -/
namespace SolarTV.MIR.LowerMappingSlots
open EvmRules SolarTV.Execution MIR.State MIR.ContinuedCounter

def storeThen (cfg : Config) (frame : Frame) (offset word : Word)
    (after : Frame → Step) : Step := do
  match ← memoryOperation cfg frame .store [offset, word] with
  | .returned (none, next) => after next
  | result => pure result

def arrayCode (cfg : Config) (frame : Frame) (slot : Word) (hash : List Byte → Word) : Step :=
  storeThen cfg frame 0 slot fun next =>
    memoryOperation cfg next .hash [0,32] hash

def mappingCode (cfg : Config) (frame : Frame) (key slot : Word) (hash : List Byte → Word) : Step :=
  storeThen cfg frame 0 key fun next =>
    storeThen cfg next 32 slot fun next =>
      memoryOperation cfg next .hash [0,64] hash

private theorem writeAppend (base suffix payload : List Byte) (offset : Nat)
    (fits : offset + payload.length ≤ base.length) :
    writeBytes (base ++ suffix) offset payload = writeBytes base offset payload ++ suffix := by
  unfold writeBytes
  rw [List.take_append_of_le_length (by omega), List.drop_append_of_le_length fits]
  simp only [List.length_append, List.append_assoc]
  rw [List.take_of_length_le (by omega : payload.length ≤ base.length + suffix.length - offset),
    List.take_of_length_le (by omega : payload.length ≤ base.length - offset)]

private theorem padMore (memory : List Byte) :
    padTo (padTo memory 32) 64 = padTo memory 64 := by
  unfold padTo
  simp only [List.length_append, List.length_replicate, List.append_assoc, List.replicate_append_replicate]
  congr 2
  omega

private theorem padStored (memory : List Byte) (key : Word) :
    padTo (writeBytes (padTo memory 32) 0 (wordBytes key)) 64 =
      writeBytes (padTo memory 64) 0 (wordBytes key) := by
  let base := padTo memory 32
  have fits : 0 + (wordBytes key).length ≤ base.length := by simp [base]; omega
  calc
    padTo (writeBytes base 0 (wordBytes key)) 64 =
        writeBytes (padTo base 64) 0 (wordBytes key) := by
      change writeBytes base 0 (wordBytes key) ++
        List.replicate (64 - (writeBytes base 0 (wordBytes key)).length) 0 =
          writeBytes (base ++ List.replicate (64 - base.length) 0) 0 (wordBytes key)
      rw [writeBytes_length, writeAppend _ _ _ _ fits]
    _ = _ := by rw [show padTo base 64 = padTo memory 64 from padMore memory]

private theorem writeTwo (memory : List Byte) (key slot : Word) (fits : 64 ≤ memory.length) :
    writeBytes (writeBytes memory 0 (wordBytes key)) 32 (wordBytes slot) =
      writeBytes memory 0 (wordBytes key ++ wordBytes slot) := by
  have keyFits : (wordBytes key).length ≤ memory.length := by simp; omega
  have slotFits : (wordBytes slot).length ≤ memory.length - 32 := by simp; omega
  have bothFit : (wordBytes key ++ wordBytes slot).length ≤ memory.length := by simp; omega
  have first : writeBytes memory 0 (wordBytes key) = wordBytes key ++ memory.drop 32 := by
    simp [writeBytes, List.take_of_length_le keyFits]
  have take : (wordBytes key ++ memory.drop 32).take 32 = wordBytes key := by
    simpa only [wordBytes_length] using
      (List.take_append_length (l₁ := wordBytes key) (l₂ := memory.drop 32))
  have drop : (wordBytes key ++ memory.drop 32).drop 64 = memory.drop 64 := by
    simp [List.drop_append, List.drop_drop, show (64 - 32 : Nat) = 32 by decide]
  unfold writeBytes
  simp only [List.take_zero, List.nil_append, Nat.sub_zero, Nat.zero_add]
  change (writeBytes memory 0 (wordBytes key)).take 32 ++
    (wordBytes slot).take ((writeBytes memory 0 (wordBytes key)).length - 32) ++
    (writeBytes memory 0 (wordBytes key)).drop (32 + (wordBytes slot).length) = _
  rw [writeBytes_length, List.take_of_length_le slotFits, List.take_of_length_le bothFit, first]
  simp only [wordBytes_length, List.length_append]
  rw [take, drop]

private theorem expandWritten (memory payload : List Byte) (upto : Nat)
    (nonzero : 0 < upto) (width : upto % 32 = 0)
    (aligned : memory.length % 32 = 0) (bounded : memory.length ≤ 65536) (fits : upto ≤ 65536) :
    expand (writeBytes (padTo memory upto) 0 payload) 0 upto =
      .ok (writeBytes (padTo memory upto) 0 payload) := by
  simpa only [Nat.zero_add] using
    MIR.PhaseShaping.expand_after_write memory payload upto 0 0 upto nonzero aligned
      width (by simpa using width) bounded fits (by omega)

theorem array (cfg : Config) (frame : Frame) (slot : Word) (hash : List Byte → Word)
    (aligned : frame.memory.length % 32 = 0) (bounded : frame.memory.length ≤ 65536) :
    arrayCode cfg frame slot hash = operation cfg frame (.slot .arrayData) [slot] 0 hash := by
  have padding := expandAligned frame.memory 0 32 (by decide) aligned (by decide) bounded (by decide)
  have written := expandWritten frame.memory (wordBytes slot) 32 (by decide) (by decide) aligned bounded (by decide)
  have reading := readBytes_writeBytes_same (padTo frame.memory 32) (wordBytes slot) 0 (by simp; omega)
  simp only [arrayCode, storeThen, memoryOperation, operation, fixedSlot, value, effect,
    List.flatMap_cons, List.flatMap_nil, List.append_nil, wordBytes_length,
    show (0 : Word).toNat = 0 by decide, show (32 : Word).toNat = 32 by decide,
    padding, Nat.zero_add, written, Except.bind, Bind.bind, Except.pure, Pure.pure]
  rw [wordBytes_length] at reading
  rw [reading]

theorem mapping (cfg : Config) (frame : Frame) (key slot : Word) (hash : List Byte → Word)
    (aligned : frame.memory.length % 32 = 0) (bounded : frame.memory.length ≤ 65536) :
    mappingCode cfg frame key slot hash = operation cfg frame (.slot .mapping) [key,slot] 0 hash := by
  let first := writeBytes (padTo frame.memory 32) 0 (wordBytes key)
  have firstAligned : first.length % 32 = 0 := by
    simp only [first, writeBytes_length, padLength]
    exact maxAligned _ _ aligned (by decide)
  have firstBound : first.length ≤ 65536 := by
    simp only [first, writeBytes_length, padLength]
    omega
  have grow := expandAligned first 32 32 (by decide) firstAligned (by decide) firstBound (by decide)
  have padding32 := expandAligned frame.memory 0 32 (by decide) aligned (by decide) bounded (by decide)
  have padding64 := expandAligned frame.memory 0 64 (by decide) aligned (by decide) bounded (by decide)
  have staged : writeBytes (padTo first 64) 32 (wordBytes slot) =
      writeBytes (padTo frame.memory 64) 0 (wordBytes key ++ wordBytes slot) := by
    rw [show padTo first 64 = _ from padStored frame.memory key]
    exact writeTwo _ _ _ (by simp; omega)
  have written := expandWritten frame.memory (wordBytes key ++ wordBytes slot) 64
    (by decide) (by decide) aligned bounded (by decide)
  have reading := readBytes_writeBytes_same (padTo frame.memory 64) (wordBytes key ++ wordBytes slot) 0
    (by simp; omega)
  simp only [List.length_append, wordBytes_length] at reading
  simp only [mappingCode, storeThen, memoryOperation, operation, fixedSlot, value, effect,
    List.flatMap_cons, List.flatMap_nil, List.append_nil, List.length_append, wordBytes_length,
    show (0 : Word).toNat = 0 by decide, show (32 : Word).toNat = 32 by decide,
    show (64 : Word).toNat = 64 by decide, padding32, Nat.zero_add, Except.bind, Bind.bind,
    Except.pure, Pure.pure]
  rw [show expand (writeBytes (padTo frame.memory 32) 0 (wordBytes key)) 32 32 = _ from grow]
  dsimp only
  rw [staged, written, padding64]
  dsimp only
  rw [reading]

end SolarTV.MIR.LowerMappingSlots
