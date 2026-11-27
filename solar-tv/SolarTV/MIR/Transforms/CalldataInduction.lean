import SolarTV.MIR.State.Interpreter

/-! The index-to-pointer rewrite for a stride of one calldata word.
Both loops test before reading, return on a matching word, and leave the
result for an identical scratch-memory publication. The ABI guard bounds the
length to fewer than 2^251 words. The base address may wrap. -/

namespace SolarTV.MIR.CalldataInduction
open EvmRules SolarTV.Execution

def offset (index : Word) : Word := index <<< (5 : Nat)

def address (base index : Word) : Word := base + offset index

def recover (base cursor : Word) : Word := (cursor - base) >>> (5 : Nat)

def indexed (read : Word → Word) (base limit needle : Word) : Nat → Word → Option Word
  | 0, _ => none
  | fuel+1, index =>
    if index.toNat < limit.toNat then
      if read (address base index) = needle then some index
      else indexed read base limit needle fuel (index+1)
    else some index

def walked (read : Word → Word) (base finish needle : Word) : Nat → Word → Option Word
  | 0, _ => none
  | fuel+1, cursor =>
    if cursor = finish then some (recover base cursor)
    else if read cursor = needle then some (recover base cursor)
    else walked read base finish needle fuel (cursor+32)

theorem guardBounds (remaining length : Word)
    (admitted : ¬ (remaining >>> (5 : Nat)).toNat < length.toNat) :
    length.toNat < 2^251 := by
  rw [BitVec.toNat_ushiftRight] at admitted
  have bound := remaining.isLt
  omega

theorem offsetFits (index : Word) (fits : index.toNat < 2^251) :
    (offset index).toNat = index.toNat * 32 := by
  rw [offset, BitVec.toNat_shiftLeft, Nat.shiftLeft_eq]
  exact Nat.mod_eq_of_lt (by omega)

theorem addressStep (base index : Word) :
    address base (index+1) = address base index + 32 := by
  unfold address offset
  rw [BitVec.shiftLeft_eq_mul_twoPow, BitVec.shiftLeft_eq_mul_twoPow, BitVec.add_mul,
    ← BitVec.add_assoc]
  rfl

theorem recoverIndex (base index : Word) (fits : index.toNat < 2^251) :
    recover base (address base index) = index := by
  have cancel : address base index - base = offset index := by
    rw [address, BitVec.add_comm, BitVec.add_sub_cancel]
  apply BitVec.eq_of_toNat_eq
  rw [recover, cancel, BitVec.toNat_ushiftRight, offsetFits index fits]
  omega

theorem sameIndex (base index limit : Word)
    (within : index.toNat ≤ limit.toNat) (small : limit.toNat < 2^251) :
    address base index = address base limit ↔ index = limit := by
  constructor
  · intro same
    have bound : index.toNat < 2^251 := by omega
    rw [← recoverIndex base index bound, ← recoverIndex base limit small, same]
  · intro same
    rw [same]

theorem simulation (read : Word → Word) (base limit needle : Word)
    (small : limit.toNat < 2^251) (fuel : Nat) (index : Word)
    (within : index.toNat ≤ limit.toNat) :
    indexed read base limit needle fuel index =
      walked read base (address base limit) needle fuel (address base index) := by
  induction fuel generalizing index with
  | zero => rfl
  | succ fuel ih =>
    have fits : index.toNat < 2^251 := by omega
    have guard := sameIndex base index limit within small
    by_cases active : index.toNat < limit.toNat
    · have nextNat : (index+1).toNat = index.toNat+1 := by
        rw [BitVec.toNat_add]
        change (index.toNat+1) % 2^256 = index.toNat+1
        exact Nat.mod_eq_of_lt (by omega)
      have different : index ≠ limit := by
        intro equal
        simp [equal] at active
      simp only [indexed, walked, guard, different, active, ite_false, ite_true,
        recoverIndex base index fits]
      rw [← addressStep]
      rw [ih (index+1) (by omega)]
    · have equal : index = limit := BitVec.eq_of_toNat_eq (by omega)
      simp [indexed, walked, equal, recoverIndex base limit small]

theorem indexedTerminates (read : Word → Word) (base limit needle : Word)
    (fuel : Nat) (index : Word) (enough : limit.toNat - index.toNat < fuel) :
    (indexed read base limit needle fuel index).isSome = true := by
  induction fuel generalizing index with
  | zero => omega
  | succ fuel ih =>
    by_cases active : index.toNat < limit.toNat
    · have large := limit.isLt
      have nextNat : (index+1).toNat = index.toNat+1 := by
        rw [BitVec.toNat_add]
        change (index.toNat+1) % 2^256 = index.toNat+1
        exact Nat.mod_eq_of_lt (by omega)
      simp only [indexed, active, ite_true]
      split
      · rfl
      · exact ih (index+1) (by omega)
    · simp [indexed, active]

/-- ABI rejects both paths together, or starts the two scans at the same item.
The proof compares all budgets, with termination at the input's own bound. -/
theorem admittedScan (input : List Byte) (base remaining length needle : Word)
    (admitted : ¬ (remaining >>> (5 : Nat)).toNat < length.toNat) :
    let read := fun (ptr : Word) => readWord input ptr.toNat
    (∀ fuel, indexed read base length needle fuel 0 =
      walked read base (address base length) needle fuel base) ∧
    (indexed read base length needle (length.toNat+1) 0).isSome = true := by
  constructor
  · intro fuel
    have initial : address base 0 = base := by simp [address, offset]
    simpa only [initial] using
      simulation _ base length needle (guardBounds remaining length admitted) fuel 0 (by simp)
  · exact indexedTerminates _ base length needle _ 0 (by simp)

/-! Exact typed templates for the captured loop. The importer renumbers SSA
definitions and block labels, then the kernel checks equality with each entire
template. A changed operation, branch, phi or ABI word fails that check. -/

open State in
section

private def localWord (id : Nat) (width : Nat := 256) : Operand :=
  ⟨.local id,.word width⟩

private def literal (word : Word) : Operand := ⟨.constant ⟨256,word⟩,.word 256⟩

private def wordOp (id code : Nat) (op : Operation) (args : List Operand)
    (width : Nat := 256) : Instruction := ⟨some id,.word width,some code,op,args⟩

private def load (id : Nat) (at_ : Operand) : Instruction :=
  wordOp id 0x35 (.memory .calldataLoad) [at_]

private def add (id : Nat) (a b : Operand) : Instruction :=
  wordOp id 0x01 (.binary .add 256) [a,b]

private def sub (id : Nat) (a b : Operand) : Instruction :=
  wordOp id 0x03 (.binary .sub 256) [a,b]

private def lt (id : Nat) (a b : Operand) : Instruction :=
  wordOp id 0x10 (.comparison .lt 256) [a,b] 1

private def shl (id : Nat) (word : Operand) : Instruction :=
  wordOp id 0x1b (.shift .shl 256) [literal 5,word]

private def shr (id : Nat) (word : Operand) : Instruction :=
  wordOp id 0x1c (.shift .shr 256) [literal 5,word]

private def br (cond yes no : Nat) : Terminator := .branch (localWord cond 1) yes no

private def abi : List State.Block := [
  ⟨[],[wordOp 0 0x36 (.memory .calldataSize) [],lt 1 (localWord 0) (literal 68)],br 1 1 2⟩,
  ⟨[],[],.reverted (literal 0) (literal 0)⟩,
  ⟨[],[load 2 (literal 36),load 3 (literal 4),add 4 (localWord 3) (literal 4),
    sub 5 (localWord 0) (literal 36),lt 6 (localWord 5) (localWord 3)],br 6 1 3⟩,
  ⟨[],[load 7 (localWord 4),add 8 (localWord 4) (literal 32),sub 9 (localWord 0) (localWord 8),
    shr 10 (localWord 9),lt 11 (localWord 10) (localWord 7)],br 11 1 4⟩]

private def publishBlock (setup : List Instruction) (index : Operand) : State.Block :=
  ⟨[],setup ++ [⟨none,.word 0,some 0x52,.memory .store,[literal 0,index]⟩],
    .returndata (literal 0) (literal 32)⟩

def indexBody (name : String) : Function := ⟨[],[],abi ++ [
  ⟨[],[],.jump 5⟩,
  ⟨[⟨12,.word 256,[(4,literal 0),(8,localWord 18)]⟩],[lt 13 (localWord 12) (localWord 7)],br 13 6 7⟩,
  ⟨[],[shl 14 (localWord 12),add 15 (localWord 8) (localWord 14),load 16 (localWord 15),
    wordOp 17 0x14 (.comparison .eq 256) [localWord 16,localWord 2] 1],br 17 7 8⟩,
  publishBlock [] (localWord 12),
  ⟨[],[add 18 (localWord 12) (literal 1)],.jump 5⟩],some name⟩

def pointerBody (name : String) (stride : Word := 32) : Function := ⟨[],[],abi ++ [
  ⟨[],[shl 12 (localWord 7),add 13 (localWord 8) (localWord 12)],.jump 5⟩,
  ⟨[⟨14,.word 256,[(4,localWord 8),(8,localWord 20)]⟩],
    [⟨some 15,.word 1,none,.notEqual,[localWord 13,localWord 14]⟩],br 15 6 7⟩,
  ⟨[],[load 16 (localWord 14),wordOp 17 0x14 (.comparison .eq 256) [localWord 16,localWord 2] 1],br 17 7 8⟩,
  publishBlock [sub 18 (localWord 14) (localWord 8),shr 19 (localWord 18)] (localWord 19),
  ⟨[],[add 20 (localWord 14) (literal stride)],.jump 5⟩],some name⟩

end

def scanArguments (input : List Byte) : Option (Word × Word × Word) := do
  let size : Word := BitVec.ofNat 256 input.length
  if size.toNat < 68 then none else
  let offset := readWord input 4
  if (size-36).toNat < offset.toNat then none else
  let header := offset+4
  let length := readWord input header.toNat
  let base := header+32
  if ((size-base) >>> (5 : Nat)).toNat < length.toNat then none else
  some (base,length,readWord input 36)

theorem scanArgumentsBound (input : List Byte) (base length needle : Word)
    (selected : scanArguments input = some (base,length,needle)) :
    length.toNat < 2^251 := by
  simp only [scanArguments] at selected
  split at selected
  · contradiction
  split at selected
  · contradiction
  split at selected
  · contradiction
  next bounded =>
    have lengthEq := congrArg (fun triple : Word × Word × Word => triple.2.1)
      (Option.some.inj selected)
    change readWord input (readWord input 4 + 4).toNat = length at lengthEq
    rw [← lengthEq]
    exact guardBounds _ _ bounded

def publish (frame : State.Frame) (result : Option Word) : State.Run := do
  let some index := result | .error "calldata scan work limit"
  let memory ← expand frame.memory 0 32
  let written := writeBytes memory 0 (wordBytes index)
  let memory ← expand written 0 32
  return State.finish (.success (readBytes memory 0 32)) {frame with memory}

/-- Logical scan fuel counts visits to the loop header, including its exit.
Both projections retain the emitted ABI guards and scratch publication. -/
def executeScan (pointer : Bool) (input : List Byte) (frame : State.Frame) (fuel : Nat) :
    State.Run := do
  let some (base,length,needle) := scanArguments input
    | do
      let memory ← expand frame.memory 0 0
      return State.finish (.reverted (readBytes memory 0 0)) {frame with memory}
  let read := fun (cursor : Word) => readWord input cursor.toNat
  publish frame (if pointer then walked read base (address base length) needle fuel base
    else indexed read base length needle fuel 0)

theorem scanEquivalence (input : List Byte) (frame : State.Frame) (fuel : Nat) :
    executeScan false input frame fuel = executeScan true input frame fuel := by
  unfold executeScan
  cases selected : scanArguments input with
  | none => rfl
  | some args =>
    obtain ⟨base,length,needle⟩ := args
    have small := scanArgumentsBound input base length needle selected
    have initial : address base 0 = base := by simp [address, offset]
    simp only [Bool.false_eq_true, ite_false, ite_true]
    simpa only [initial] using congrArg (publish frame)
      (simulation _ base length needle small fuel 0 (by simp))

end SolarTV.MIR.CalldataInduction
