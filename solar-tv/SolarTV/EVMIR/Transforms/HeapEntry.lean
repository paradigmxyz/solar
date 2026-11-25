import SolarTV.EVMIR.State.MeteredPrefix
import SolarTV.EVMIR.MemoryWrites
import SolarTV.MIR.Transforms.FreshHeap
import SolarTV.MIR.Transforms.HeapCursor

/-! A final bytecode entry with an explicit FMP producer. The actual PUSH
immediate, store address, continuation PC, and all three charges are
derived from the entry bytes. The arbitrary continuation is executed by
the ordinary bytecode interpreter. -/

namespace SolarTV.EVM.HeapEntry
open EvmRules SolarTV.Execution State
open MIR.ContinuedCounter (padTo padLength expandAligned maxAligned)

def image (floor : Byte) (suffix : List Byte) : List Byte :=
  [0x60,floor,0x60,0x40,0x52] ++ suffix

def initial (env : World.Environment) (floor : Byte) (suffix : List Byte) : Frame :=
  ⟨env,image floor suffix,0,[],[],[],0⟩

def ready (env : World.Environment) (floor : Byte) (suffix : List Byte) : Frame :=
  {initial env floor suffix with
    pc := 5
    memory := writeBytes (List.replicate 96 0) 64 (wordBytes (floor.setWidth 256))}

theorem metered (cfg : Config) (journal : Journal) (env : World.Environment)
    (floor : Byte) (suffix : List Byte)
    (push : available cfg 0x60 = ⟨true,3,0,0,1⟩)
    (store : available cfg 0x52 = ⟨true,3,0,2,0⟩) :
    MeteredPrefix cfg journal (initial env floor suffix) journal (ready env floor suffix)
      3 18 := by
  let first := {initial env floor suffix with pc := 2, stack := [floor.setWidth 256]}
  let second := {first with pc := 4, stack := (64 : Word) :: first.stack}
  have firstStep (gas : Nat) :
      instruction cfg journal {initial env floor suffix with gas} =
        if gas < 3 then stepExhausted journal else .ok (.next journal {first with gas := gas-3}) := by
    simp [instruction,instructionWithImage,initial,image,first,push,pureOp,pushValue,
      readBytes]
  have secondStep (gas : Nat) :
      instruction cfg journal {first with gas} =
        if gas < 3 then stepExhausted journal else .ok (.next journal {second with gas := gas-3}) := by
    simp [instruction,instructionWithImage,initial,image,first,second,push,pureOp,pushValue,
      readBytes]
  have thirdStep (gas : Nat) :
      instruction cfg journal {second with gas} =
        if gas < 12 then stepExhausted journal else
          .ok (.next journal {ready env floor suffix with gas := gas-12}) := by
    simp [instruction,instructionWithImage,initial,image,first,second,ready,store,pureOp,
      withMemory,memoryCharge,memoryCost,expand,requiredWords,memoryWords,next,
      Bind.bind,Except.bind]
  exact .step firstStep (.step secondStep (.step thirdStep (.done _ _)))

theorem floor_contract (frame : MIR.State.Frame) (env : World.Environment) (floor : Byte)
    (suffix : List Byte) (extent : Nat)
    (lower : 128 ≤ floor.toNat) (aligned : floor.toNat % 32 = 0)
    (range : floor.toNat + extent ≤ 65536) :
    MIR.FreshHeap.Contract {frame with memory := (ready env floor suffix).memory}
      (floor.setWidth 256) extent := by
  have wide : (floor.setWidth 256).toNat = floor.toNat := by
    rw [BitVec.toNat_setWidth, Nat.mod_eq_of_lt]
    have small := floor.isLt
    omega
  apply MIR.FreshHeap.initialized {frame with memory := List.replicate 96 0}
  · simp [MIR.Universal.MemoryValid]
  · simp
  · simpa only [wide] using lower
  · simpa only [wide] using aligned
  · simpa only [wide] using range

/-- A fetched FMP store establishes the MIR heap contract at any checked
bytecode location. The metered prefix provides the incoming stack separately. -/
theorem metered_store (cfg : Config) (journal : Journal) (frame : Frame)
    (caller : MIR.State.Frame) (floor : Word) (stack : EVM.Stack)
    (opcode : available cfg 0x52 = ⟨true,3,0,2,0⟩)
    (fetched : frame.code[frame.pc]? = some (0x52 : Byte))
    (words : frame.stack = 64 :: floor :: stack)
    (fits : (64 :: floor :: stack).length ≤ 1024)
    (aligned : frame.memory.length % 32 = 0) (bounded : frame.memory.length ≤ 65536)
    (lower : 128 ≤ floor.toNat) (upper : floor.toNat ≤ 2^64 - 1) :
    let grown := MIR.ContinuedCounter.padTo frame.memory 96
    let after := {frame with
      pc := frame.pc + 1
      stack := stack
      memory := writeBytes grown 64 (wordBytes floor)}
    let price := 3 + memoryCharge frame.memory 64 32
    MeteredPrefix cfg journal frame journal after 1 price ∧
      MIR.HeapCursor.Entry {caller with memory := after.memory} floor := by
  have expansion : expand frame.memory 64 32 = .ok (padTo frame.memory 96) :=
    expandAligned frame.memory 64 32 (by decide) aligned (by decide) bounded (by decide)
  refine ⟨.step (charge := 3 + memoryCharge frame.memory 64 32) (fun balance => ?_) (.done _ _), ?_⟩
  · have step := SolarTV.Memory.Projection.store_instruction cfg journal {frame with gas := balance}
      64 floor stack false 3 0 opcode fetched words fits
    simp [withMemory, expansion, next] at step
    exact step
  · have length : (padTo frame.memory 96).length = max frame.memory.length 96 := padLength _ _
    have covers : 64 + 32 ≤ (padTo frame.memory 96).length := by omega
    have value := readWord_writeWord (padTo frame.memory 96) 64 floor covers
    have aligned' := maxAligned frame.memory.length 96 aligned (by decide)
    have bounded' : max frame.memory.length 96 ≤ 65536 := by omega
    simp [MIR.HeapCursor.Entry, MIR.HeapCursor.At, MIR.Universal.MemoryValid, value,
      aligned', bounded', lower, upper]
    exact Nat.le_max_right _ _

end SolarTV.EVM.HeapEntry
