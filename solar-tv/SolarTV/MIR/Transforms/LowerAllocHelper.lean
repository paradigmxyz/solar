import SolarTV.MIR.Transforms.LowerAllocStage
import SolarTV.MIR.State.Normalize
import SolarTV.MIR.Transforms.LowerUnsignedArithmetic

/-! The canonical checked helper consumes one i256 size and returns one
memory pointer. Its direct interpreter observation includes the returned
pointer and shared frame on success, and the exact panic bytes on failure. -/

namespace SolarTV.MIR.LowerAlloc.Stage
open EvmRules SolarTV.Execution MIR.State MIR.ContinuedCounter
set_option maxRecDepth 16384
set_option maxHeartbeats 2000000

private def literal (value : Word) : Operand :=
  ⟨.constant ⟨256,value⟩,.word 256⟩

private def localWord (index : Nat) (width := 256) : Operand :=
  ⟨.local index,.word width⟩

private def store (address : Word) (value : Operand) : Instruction :=
  ⟨none,.word 0,some 82,.memory .store,[literal address,value]⟩

def checkedBody : Function :=
  let cursor := localWord 2
  let pointer : Operand := ⟨.local 3,.pointer 0⟩
  let bumped := localWord 4
  let flag := localWord 7 1
  ⟨[.word 256],[.pointer 0],[
    ⟨[],[
      ⟨some 2,.word 256,some 81,.memory .load,[literal 64]⟩,
      ⟨some 3,.pointer 0,none,.word,[cursor]⟩,
      ⟨some 4,.word 256,some 1,.binary .add 256,[cursor,⟨.argument 0,.word 256⟩]⟩,
      ⟨some 5,.word 1,some 16,.comparison .lt 256,[bumped,cursor]⟩,
      ⟨some 6,.word 1,some 17,.comparison .gt 256,
        [bumped,literal 18446744073709551615#256]⟩,
      ⟨some 7,.word 1,some 23,.binary .or 1,[localWord 5 1,localWord 6 1]⟩,
      ⟨some 8,.word 256,none,.word,[flag]⟩
    ],.branch flag 2 1⟩,
    ⟨[],[store 64 bumped],.returned [pointer]⟩,
    ⟨[],[store 32 (literal 65), store 0 (literal 0x4e487b71)],
      .reverted (literal 28) (literal 36)⟩
  ],some "alloc"⟩

def checkedHelper : Functions := [(0,checkedBody)]

theorem helper_start (frame : Frame) (size : Word) :
    start checkedHelper 0 [abiScalar size] frame =
      .ok {frame with arguments := [abiScalar size], locals := []} := rfl

theorem helper_return (cfg : Config) (frame : Frame) (cursor : Word) :
    publishReturns cfg checkedBody [abiScalar cursor] frame = .ok frame := by
  unfold publishReturns
  generalize (checkedBody.returnBuffer.bind (fun name =>
    (cfg.layout.functions.find? (·.1 == name)).bind (·.2.returnBuffer))) = placement
  cases placement <;> rfl

private def observedAllocation (frame : Frame) (size : Word) : Run := do
  match ← allocate frame size false true with
  | .returned (some cursor,next) => return finish (.returned [cursor]) next
  | .returned (none,_) => .error "allocation has no pointer"
  | .reverted data => return finish (.reverted (failureData data)) frame
  | .invalid => return finish .invalid frame

private def checkedObserved (cfg : Config) (frame : Frame) (size : Word) : Run :=
  run 14 cfg checkedHelper ⟨{function := 0},[],{frame with arguments := [abiScalar size],locals := []}⟩

attribute [local irreducible] Execution.readWord Execution.readBytes Execution.wordBytes
  Execution.writeBytes Execution.expand

theorem checked_helper (cfg : Config) (frame : Frame) (size : Word)
    (aligned : frame.memory.length % 32 = 0) (bounded : frame.memory.length ≤ 65536)
    (hasAdd : 1 ∈ cfg.available) (hasLt : 16 ∈ cfg.available)
    (hasGt : 17 ∈ cfg.available) (hasOr : 23 ∈ cfg.available)
    (hasLoad : 81 ∈ cfg.available) (hasStore : 82 ∈ cfg.available) :
    equivalent (observedAllocation frame size) (checkedObserved cfg frame size) := by
  let memory := padTo frame.memory 96
  let cursor := readWord memory 64
  have openSlot : expand frame.memory 64 32 = .ok memory :=
    expandAligned _ 64 32 (by decide) aligned (by decide) bounded (by decide)
  have memAtLeast : 96 ≤ memory.length := by simp only [memory,padLength]; omega
  have memAligned : memory.length % 32 = 0 := by
    simp only [memory,padLength]
    exact maxAligned _ _ aligned (by decide)
  have memBound : memory.length ≤ 65536 := by simp only [memory,padLength]; omega
  have activeSlot := MIR.FreshHeap.active_slot memory memBound memAligned memAtLeast
  have arithmetic (w : Word) : invalidBump cursor w =
      (decide ((cursor + w).toNat < cursor.toNat) ||
        decide ((cursor + w).toNat > 2^64 - 1)) := by
    simp [invalidBump, BitVec.ult, BitVec.toNat_ofNat]
  have sumEq : BitVec.ofNat 256 ((cursor.toNat + size.toNat) % 2^256) = cursor + size := by
    apply BitVec.eq_of_toNat_eq
    simp only [BitVec.toNat_ofNat, Nat.mod_mod, BitVec.toNat_add]
  have testEq :
      (MIR.compare .lt (cursor + size) cursor |||
        MIR.compare .gt (cursor + size) 18446744073709551615#256) =
        (if invalidBump cursor size then 1 else 0) := bump_guard cursor size
  let codeMemory := writeBytes memory 32 (wordBytes 65#256)
  let panicMemory := writeBytes codeMemory 0 (wordBytes 1313373041#256)
  have covered (bytes : List Byte) (length : bytes.length = memory.length)
      (offset count : Nat) (positive : 0 < count) (ending : (offset + count) % 32 = 0)
      (fits : offset + count ≤ 96) : expand bytes offset count = .ok bytes := by
    rw [expandAligned _ _ _ positive (by rwa [length]) ending (by rwa [length]) (by omega),
      padCovered _ _ (by omega)]
  have writeCode := covered memory rfl 32 32 (by decide) (by decide) (by decide)
  have writeSelector := covered codeMemory (by simp [codeMemory]) 0 32 (by decide) (by decide) (by decide)
  have readPanic := covered panicMemory (by simp [panicMemory, codeMemory]) 28 36 (by decide) (by decide) (by decide)
  have rawPanic : readBytes panicMemory 28 36 = failureData (.panic 65#256) := by
    have h := MIR.LowerUnsignedArithmetic.panic_payload memory 65 memAligned memBound
    have stage : MIR.LowerUnsignedArithmetic.panicPayload memory 65 = .ok (readBytes panicMemory 28 36) := by
      unfold MIR.LowerUnsignedArithmetic.panicPayload
      rw [writeCode]
      dsimp only [Bind.bind, Except.bind]
      rw [show expand (writeBytes memory 32 (wordBytes 65)) 0 32 = .ok codeMemory from writeSelector]
      dsimp only [Bind.bind, Except.bind]
      rw [show expand (writeBytes codeMemory 0 (wordBytes 1313373041)) 28 36 = .ok panicMemory from readPanic]
      rfl
    rw [h] at stage
    exact (Except.ok.inj stage).symm.trans (by unfold readBytes failureData wordBytes; rfl)
  by_cases bad : invalidBump cursor size = true
  all_goals have known := arithmetic size
  · have sourceBad := known.symm.trans bad
    simp only [observedAllocation, allocate, openSlot, Bind.bind, Except.bind, sourceBad,
      cursor, ite_true, Pure.pure, Except.pure]
    simp [checkedObserved, checkedHelper, checkedBody, literal, localWord, store, abiScalar, openSlot, hasAdd, hasLt, hasGt, hasOr,
      hasLoad, hasStore, wordBytes_length, sumEq, testEq, cursor, bad, writeCode, writeSelector,
      readPanic, rawPanic, codeMemory, panicMemory, ↓MIR.State.reduceRun, MIR.State.transition,
      MIR.State.next, MIR.State.stop, MIR.State.enter, MIR.State.branch, MIR.State.advance,
      MIR.State.instruction, MIR.State.operation, MIR.State.assign, MIR.State.read,
      MIR.State.readBits, MIR.State.readValue, MIR.State.Value.word, ↓MIR.State.reduceFits,
      MIR.State.ValueType.width, MIR.State.assignValue, MIR.State.Scalar.word,
      MIR.State.Scalar.ofWord, MIR.State.wordBits, MIR.State.widenBits, MIR.State.required,
      MIR.State.put, MIR.State.value, MIR.State.effect, BitVec.cast, MIR.State.finish,
      MIR.State.memoryOperation, Except.bind, Except.pure, Bind.bind, Pure.pure, List.mapM.loop,
      List.mapM, List.foldl, ↓MIR.State.reduceFunction, BitVec.ofNatLT_eq_ofNat]

    rfl
  · have good := Bool.eq_false_iff.mpr bad
    have sourceGood := known.symm.trans good
    simp only [observedAllocation, allocate, openSlot, Bind.bind, Except.bind, sourceGood,
      cursor, Bool.false_eq_true, ite_false, Pure.pure, Except.pure]
    simp [checkedObserved, checkedHelper, checkedBody, literal, localWord, store, abiScalar, openSlot, activeSlot, hasAdd, hasLt, hasGt,
      hasOr, hasLoad, hasStore, wordBytes_length, sumEq, testEq, cursor, good, Bool.false_eq_true,
      ↓MIR.State.reduceRun, MIR.State.transition, MIR.State.next, MIR.State.stop, MIR.State.enter,
      MIR.State.typedWords, MIR.State.branch, MIR.State.advance, MIR.State.instruction,
      MIR.State.operation, MIR.State.assign, MIR.State.read, MIR.State.readBits,
      MIR.State.readValue, MIR.State.Value.word, ↓MIR.State.reduceFits, MIR.State.Value.flatten,
      MIR.State.ValueType.width, MIR.State.ValueType.fields, MIR.State.assignValue,
      MIR.State.Scalar.word, MIR.State.Scalar.ofWord, MIR.State.wordBits, MIR.State.widenBits,
      MIR.State.required, MIR.State.put, MIR.State.value, MIR.State.effect, BitVec.cast,
      MIR.State.finish, MIR.State.memoryOperation, Except.bind, Except.pure, Bind.bind, Pure.pure,
      List.mapM.loop, List.mapM, List.foldl, ↓MIR.State.reduceFunction, BitVec.ofNatLT_eq_ofNat]

    rfl
end SolarTV.MIR.LowerAlloc.Stage
