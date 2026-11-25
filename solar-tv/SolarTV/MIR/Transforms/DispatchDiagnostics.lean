import SolarTV.MIR.Transforms.RoutedDispatch

/-! Observable debug reverts at runtime dispatch. The diagnostic is chosen
from the source routes and execution context. The reference writes the
canonical Error(string) ABI header and the two padded string words. -/

namespace SolarTV.MIR.DispatchDiagnostics
open EvmRules SolarTV.Execution State RoutedDispatch CallContext

inductive Failure where
  | missingFallback | unknownSelector | valueRejected | directLibrary

def Failure.message : Failure → String
  | .missingFallback => "Contract does not have fallback nor receive functions"
  | .unknownSelector => "Unknown signature and no fallback defined"
  | .valueRejected => "Ether sent to non-payable function"
  | .directLibrary => "Non-view function of library called without DELEGATECALL"

def Failure.size : Failure → Nat
  | .missingFallback => 53
  | .unknownSelector => 41
  | .valueRejected => 34
  | .directLibrary => 56

def select (routes : Routes) (input : List Byte) (value : Word)
    (libraryDenied : Nat → Bool) : Failure ⊕ Nat :=
  if routes.rejectValue && value != 0 then .inl .valueRejected
  else if input.isEmpty && routes.receive.isSome then
    match routes.receive with
    | some id => .inr id
    | none => .inl .missingFallback
  else
    let matched := if input.length < 4 then none else lookup (TransferFromDispatch.selector input) routes.selectors
    match matched with
    | some id => if libraryDenied id then .inl .directLibrary else .inr id
    | none =>
      match routes.fallback with
      | some (id,payable) => if payable || value == 0 then .inr id else .inl .valueRejected
      | none => .inl (if routes.receive.isSome then .unknownSelector else .missingFallback)

def grown (memory : List Byte) (offset count : Nat) : List Byte :=
  memory ++ List.replicate (32 * requiredWords memory offset count - memory.length) 0

theorem grown_bounded (memory : List Byte) (offset count : Nat) :
    (grown memory offset count).length ≤ 65536 ↔
      memory.length ≤ 65536 ∧ (count = 0 ∨ offset + count ≤ 65536) := by
  simp only [grown, List.length_append, List.length_replicate, requiredWords, memoryWords]
  split <;> omega

theorem expand_small (memory : List Byte) (offset count : Nat)
    (bounded : memory.length ≤ 65536) (range : offset + count ≤ 65536) :
    expand memory offset count = .ok (grown memory offset count) := by
  have limit := (grown_bounded memory offset count).mpr ⟨bounded, Or.inr range⟩
  simp only [grown, List.length_append, List.length_replicate] at limit
  unfold expand
  simp [grown, show ¬65536 < 32 * requiredWords memory offset count by omega]

def writeWord (memory : List Byte) (offset : Nat) (word : Word) : List Byte :=
  writeBytes (grown memory offset 32) offset (wordBytes word)

def encodeMemory (memory : List Byte) (length first second : Word) : List Byte :=
  let memory := writeWord memory 32 32
  let memory := writeWord memory 64 length
  let memory := writeWord memory 96 first
  let memory := writeWord memory 128 second
  grown (writeWord memory 0 0x08c379a0) 28 132

def messageBytes (reason : Failure) : List Byte :=
  reason.message.toUTF8.toList.map (fun b => BitVec.ofNat 8 b.toNat)

def expected (frame : Frame) (reason : Failure) : Result :=
  let message := messageBytes reason
  let memory := encodeMemory frame.memory (BitVec.ofNat 256 reason.size)
    (readWord message 0) (readWord message 32)
  finish (.reverted (readBytes memory 28 132)) {frame with memory}

private theorem grown_length (memory : List Byte) (offset count : Nat) :
    (grown memory offset count).length =
      max memory.length (if count = 0 then 0 else 32 * ((offset + count + 31) / 32)) := by
  simp only [grown, List.length_append, List.length_replicate, requiredWords, memoryWords]
  split <;> omega

private theorem written_length (memory : List Byte) (offset : Nat) (word : Word) :
    (writeWord memory offset word).length = max memory.length (32 * ((offset + 63) / 32)) := by
  simp [writeWord, grown_length, Nat.add_assoc]

private theorem read_grown (memory : List Byte) (offset count source size : Nat)
    (within : source + size ≤ memory.length) :
    readBytes (grown memory offset count) source size = readBytes memory source size := by
  unfold readBytes
  apply List.map_congr_left
  intro i hi
  have bound : source + i < memory.length := by
    have := List.mem_range.mp hi
    omega
  simp [grown, List.getElem?_append_left bound]

private theorem read_written (memory : List Byte) (offset : Nat) (word : Word) :
    readBytes (writeWord memory offset word) offset 32 = wordBytes word := by
  have within : offset + (wordBytes word).length ≤ (grown memory offset 32).length := by
    simp [grown_length]
    omega
  simpa [writeWord] using readBytes_writeBytes_same (grown memory offset 32) (wordBytes word) offset within

private theorem read_other (memory : List Byte) (offset source size : Nat) (word : Word)
    (disjoint : offset + 32 ≤ source ∨ source + size ≤ offset)
    (within : source + size ≤ memory.length) :
    readBytes (writeWord memory offset word) source size = readBytes memory source size := by
  unfold writeWord
  rw [readBytes_writeBytes_disjoint _ _ _ _ _ (by simpa using disjoint)]
  exact read_grown _ _ _ _ _ within

private theorem read_split (memory : List Byte) (offset count rest : Nat) :
    readBytes memory offset (count + rest) =
      readBytes memory offset count ++ readBytes memory (offset + count) rest := by
  simp [readBytes, List.range_add, List.map_map, Nat.add_assoc]

private theorem read_header (memory : List Byte) :
    readBytes (writeWord memory 0 0x08c379a0) 28 4 =
      (wordBytes 0x08c379a0).drop 28 := by
  have whole := read_written memory 0 0x08c379a0
  change readBytes _ 0 (28+4) = _ at whole
  rw [read_split] at whole
  have drop := congrArg (fun bytes : List Byte => bytes.drop 28) whole
  have length : (readBytes (writeWord memory 0 0x08c379a0) 0 28).length = 28 := by
    simp [readBytes]
  have slice (a b : List Byte) (h : a.length = 28) : (a ++ b).drop 28 = b := by
    rw [← h, List.drop_left]
  rw [slice _ _ length] at drop
  exact drop

private theorem read_encoded (memory : List Byte) (length first second : Word) :
    readBytes (encodeMemory memory length first second) 28 132 =
      (wordBytes 0x08c379a0).drop 28 ++ wordBytes 32 ++ wordBytes length ++
        wordBytes first ++ wordBytes second := by
  have split_words (mem : List Byte) :
      readBytes mem 28 132 = readBytes mem 28 4 ++ readBytes mem 32 32 ++ readBytes mem 64 32 ++
        readBytes mem 96 32 ++ readBytes mem 128 32 := by
    rw [show readBytes mem 28 132 = readBytes mem 28 (4+128) from rfl, read_split]
    rw [show readBytes mem 32 128 = readBytes mem 32 (32+96) from rfl, read_split]
    rw [show readBytes mem 64 96 = readBytes mem 64 (32+64) from rfl, read_split]
    rw [show readBytes mem 96 64 = readBytes mem 96 (32+32) from rfl, read_split]
    simp [List.append_assoc]
  let a := writeWord memory 32 32
  let b := writeWord a 64 length
  let c := writeWord b 96 first
  let d := writeWord c 128 second
  let e := writeWord d 0 0x08c379a0
  have aSize : 64 ≤ a.length := by simp [a, written_length] <;> omega
  have bSize : 96 ≤ b.length := by simp [b, written_length] <;> omega
  have cSize : 128 ≤ c.length := by simp [c, written_length] <;> omega
  have dSize : 160 ≤ d.length := by simp [d, written_length] <;> omega
  have eSize : 160 ≤ e.length := by simp [e, written_length] <;> omega
  change readBytes (grown e 28 132) 28 132 = _
  rw [read_grown e 28 132 28 132 eSize, split_words]
  have read0 : readBytes e 28 4 = (wordBytes 0x08c379a0).drop 28 := read_header d
  have read32 : readBytes e 32 32 = wordBytes 32 := by
    rw [read_other d 0 32 32 _ (by decide) (by omega),
      read_other c 128 32 32 _ (by decide) (by omega),
      read_other b 96 32 32 _ (by decide) (by omega),
      read_other a 64 32 32 _ (by decide) aSize]
    exact read_written memory 32 _
  have read64 : readBytes e 64 32 = wordBytes length := by
    rw [read_other d 0 64 32 _ (by decide) (by omega),
      read_other c 128 64 32 _ (by decide) (by omega),
      read_other b 96 64 32 _ (by decide) bSize]
    exact read_written a 64 _
  have read96 : readBytes e 96 32 = wordBytes first := by
    rw [read_other d 0 96 32 _ (by decide) (by omega),
      read_other c 128 96 32 _ (by decide) cSize]
    exact read_written b 96 _
  have read128 : readBytes e 128 32 = wordBytes second := by
    rw [read_other d 0 128 32 _ (by decide) dSize]
    exact read_written c 128 _
  rw [read0, read32, read64, read96, read128]

/-- The decoded result of the reference writes is the usual ABI payload for
every incoming memory image, including partially initialized scratch space. -/
theorem expected_payload (frame : Frame) (reason : Failure) :
    (expected frame reason).completion = .reverted (requireLiteral (messageBytes reason)) := by
  unfold expected
  simp only [finish]
  rw [read_encoded]
  apply congrArg Completion.reverted
  cases reason <;> decide +kernel

def Agrees (entry : Nat) (frame : Frame) (decision : Failure ⊕ Nat)
    (actual : Except String Cut) : Prop :=
  match decision with
  | .inr id => RoutedDispatch.Agrees entry frame (some id) actual
  | .inl reason => actual = .ok (.inl (expected frame reason))

end SolarTV.MIR.DispatchDiagnostics
