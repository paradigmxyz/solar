import SolarTV.MIR.Transforms.LowerReturndata
import SolarTV.EVMIR.Transforms.CallBudget
import SolarTV.MIR.Transforms.CallDebit

/-! Relate a priced final-bytecode CALL to the captured MIR cut. The incoming
gas is that of the internal helper. TransferFromFrame relates the shared
entry state to the message cut after the guard and encoder. The priced prefix establishes the budget
used on both MIR sides. Its earlier GAS observation may differ in value
from the bytecode GAS, but neither request can fall below the EIP-150 cap. -/

namespace SolarTV.MIR.FinalCallBudget
open EvmRules SolarTV.Execution MIR.State
open MIR.LowerReturndata
open EVM.CallBudget (Entry Trace referenceTrace beforeCall)

def withBudget (cfg : Config) (frame : Frame) (budget : Nat) : Config :=
  {cfg with messageGas := some (List.replicate (frame.messageGas.length+1) budget)}

private theorem budgetEntry (frame : Frame) (budget : Nat) :
    (List.replicate (frame.messageGas.length+1) budget)[frame.messageGas.length]? = some budget := by
  simp

private theorem cappedBudget (cfg : Config) (entry frame : Frame)
    (externalConfig : EVM.State.Config) (memory : List Byte) (message : Calls.Message)
    (opcode resolution budget : Nat) (a b : Word) (fork : cfg.fork = .cancun)
    (sameIndex : frame.messageGas.length = entry.messageGas.length)
    (ha : budget ≤ a.toNat) (hb : budget ≤ b.toNat) :
    callBudget (withBudget cfg entry budget) frame externalConfig memory message opcode resolution (some a) =
    callBudget (withBudget cfg entry budget) frame externalConfig memory message opcode resolution (some b) := by
  let price := fun count => EVM.State.callBoundaryCharge externalConfig frame.world message
    opcode resolution (memoryWords frame.memory) count
  have aCap : budget - price (memoryWords memory) ≤ a.toNat := Nat.le_trans (Nat.sub_le ..) ha
  have bCap : budget - price (memoryWords memory) ≤ b.toNat := Nat.le_trans (Nat.sub_le ..) hb
  have tangerine : cfg.fork.atLeast .tangerineWhistle = true := by
    simp [fork, Fork.atLeast, Fork.ordinal]
  dsimp only [price] at aCap bCap
  simp only [callBudget, withBudget, sameIndex, budgetEntry, required, Bind.bind, Except.bind,
    Option.map_some, Option.getD_some]
  split
  · rfl
  · rw [LowerAddressCall.forwardSample _ _ _ _ tangerine aCap,
      LowerAddressCall.forwardSample _ _ _ _ tangerine bCap]

private theorem cappedExternal (cfg : Config) (entry frame : Frame) (fuel : Nat)
    (target offset length a b : Word) (budget : Nat) (fork : cfg.fork = .cancun)
    (sameIndex : frame.messageGas.length = entry.messageGas.length)
    (ha : budget ≤ a.toNat) (hb : budget ≤ b.toNat)
    (aLimit : a.toNat ≤ 30000000) (bLimit : b.toNat ≤ 30000000) :
    external fuel (withBudget cfg entry budget) frame 0xf1 [target,0,offset,length,0,0] (some a) =
    external fuel (withBudget cfg entry budget) frame 0xf1 [target,0,offset,length,0,0] (some b) := by
  have newer : (withBudget cfg entry budget).fork.atLeast .amsterdam = false := by
    simp [withBudget, fork, Fork.atLeast, Fork.ordinal]
  cases cfgResult : (withBudget cfg entry budget).externalConfig with
  | none => simp [external, required, cfgResult, Bind.bind, Except.bind]
  | some ext =>
    simp [external, required, cfgResult, newer, Nat.not_lt.mpr aLimit, Nat.not_lt.mpr bLimit,
      Bind.bind, Except.bind, Pure.pure, Except.pure]
    have same (mem : List Byte) (message : Calls.Message) (resolution : Nat) :=
      cappedBudget cfg entry frame ext mem message 0xf1 resolution budget a b fork sameIndex ha hb
    simp only [same]

theorem changeRequest (cfg : Config) (frame : Frame) (fuel : Nat)
    (target object a b : Word) (budget : Nat) (fork : cfg.fork = .cancun)
    (ha : budget ≤ a.toNat) (hb : budget ≤ b.toNat)
    (aLimit : a.toNat ≤ 30000000) (bLimit : b.toNat ≤ 30000000) :
    invoke (withBudget cfg frame budget) fuel
      (LowerAddressCall.loweredMemoryObjectArgs (withBudget cfg frame budget) frame target a object)
      =
    invoke (withBudget cfg frame budget) fuel
      (LowerAddressCall.loweredMemoryObjectArgs (withBudget cfg frame budget) frame target b object)
      := by
  simp only [invoke, LowerAddressCall.loweredMemoryObjectArgs,
    memoryOperation, value, Bind.bind, Except.bind, Pure.pure, Except.pure]
  cases expand frame.memory object.toNat 32 with
  | error _ => rfl
  | ok mem =>
    dsimp only
    rw [cappedExternal cfg frame {frame with memory := mem} fuel target (object+32)
      (readWord mem object.toNat) a b budget fork rfl ha hb aLimit bLimit]

/-- Successful prefix execution determines the priced MIR input and both
request bounds. The early sample's exact elapsed cost is immaterial after
clipping. Its position, preceding CALL, is checked on the imported SSA. -/
theorem samples (trace : Trace) (shape : trace = referenceTrace 6)
    (entry : Entry) (heap : 128 ≤ entry.fmp)
    (sample available child retained words elapsed : Nat)
    (reaches : EVM.CallBudget.run trace entry = .call sample available child retained words)
    (earlier : elapsed ≤ beforeCall entry 6) :
    sample = entry.gas - beforeCall entry 6 ∧
      available = sample - 100 ∧ sample ≤ entry.gas - elapsed ∧ 100 ≤ sample ∧
      available - child = retained ∧ words = max entry.words ((entry.fmp+163)/32) := by
  rw [shape, EVM.CallBudget.exact entry heap 6] at reaches
  unfold EVM.CallBudget.reference at reaches
  by_cases guard : entry.gas < 20 + (if entry.warm then 100 else 2600)
  · simp [guard] at reaches
  by_cases noCode : (!entry.hasCode) = true
  · simp [guard, noCode] at reaches
  by_cases paid : entry.gas < beforeCall entry 6 + 100
  · simp [guard, noCode, paid] at reaches
  simp only [guard, noCode, paid, ite_false] at reaches
  cases reaches
  have divLe := Nat.div_le_self (entry.gas - beforeCall entry 6 - 100) 64
  exact ⟨rfl, rfl, by omega, by omega, by omega, rfl⟩


/-- At the corresponding prepared MIR frame the guard has warmed the token,
and the encoded object covers the CALL's whole input. There is no delegation
charge in Cancun, no output expansion, and no value stipend. -/
private theorem preparedPrice (trace : Trace) (shape : trace = referenceTrace 6)
    (entry : Entry) (heap : 128 ≤ entry.fmp) (limit : entry.gas ≤ 30000000)
    (sample available child retained words : Nat)
    (reaches : EVM.CallBudget.run trace entry = .call sample available child retained words)
    (cfg : Config) (fork : cfg.fork = .cancun) (frame : Frame)
    (ext : EVM.State.Config) (extFork : ext.fork = .cancun)
    (journal : Journal) (target : Address) (input : List Byte) (offset : Nat)
    (warm : frame.world = warmAddress journal target)
    (payload : expand frame.memory offset 100 = .ok frame.memory)
    (noOutput : expand frame.memory 0 0 = .ok frame.memory) :
    callBudget (withBudget cfg frame sample) frame ext frame.memory ⟨.call,target,0,input⟩
      0xf1 (EVM.State.executionCode cfg.fork frame.world target).2.1
      (some (BitVec.ofNat 256 sample)) = .ok (some (child,retained)) := by
  obtain ⟨sampled,availableEq,_,paid,net,_⟩ :=
    samples trace shape entry heap sample available child retained words 0 reaches (Nat.zero_le _)
  have sampleWord : (BitVec.ofNat 256 sample).toNat = sample := by
    simp [BitVec.toNat_ofNat, Nat.mod_eq_of_lt (by omega : sample < 2^256)]
  have resolution : (EVM.State.executionCode cfg.fork frame.world target).2.1 = 0 := by
    rw [fork]
    simp [EVM.State.executionCode, EVM.State.delegationTarget, Fork.atLeast, Fork.ordinal]
  have fee : EVM.State.callBoundaryCharge ext frame.world ⟨.call,target,0,input⟩
      0xf1 0 0 0 = 100 := by
    rw [warm]
    exact EVM.CallBudget.warmCall ext journal target input 0 extFork
  have cap := EVM.CallBudget.earlierRequest entry 6 sample available child retained words
    (beforeCall entry 6) ((EVM.CallBudget.exact entry heap 6).symm.trans (shape ▸ reaches))
    (Nat.le_refl _)
  rw [← sampled,availableEq] at cap
  rw [availableEq] at net
  rw [resolution, CallDebit.budget_eq (withBudget cfg frame sample) ext frame
    ⟨.call,target,0,input⟩ 0xf1 0 frame.memory frame.memory offset 100 0 0 _
    (BitVec.ofNat 256 sample) sample rfl (budgetEntry frame sample) (fork.trans extFork.symm)
    payload noOutput]
  simp only [fee, CallDebit.memoryCharge_zero _ _ _ payload,
    CallDebit.memoryCharge_zero _ _ _ noOutput, Nat.add_zero, EVM.CallBudget.charged,
    paid, ite_true, Nat.sub_zero, Option.bind_some, extFork, sampleWord]
  change Except.ok ((Calls.forwardGas .cancun (sample-100) sample false).map
    (fun b => (b.supplied,sample-100-b.charged))) = _
  simp [cap, net]


structure Ready (entry : Entry) (frame : Frame) (object target : Word) (words : Nat) where
  journal : Journal
  warm : frame.world = warmAddress journal (target.setWidth 160)
  pointer : object.toNat = entry.fmp
  extent : frame.memory.length = 32*words
  limit : frame.memory.length ≤ 65536
  lengthWord : readWord frame.memory object.toNat = 100

theorem expandCovered (memory : List Byte) (offset size : Nat)
    (aligned : memory.length % 32 = 0) (covered : offset+size ≤ memory.length)
    (bounded : memory.length ≤ 65536) :
    expand memory offset size = .ok memory := by
  have words : 32 * memoryWords memory = memory.length := by
    unfold memoryWords
    omega
  have extent : requiredWords memory offset size = memoryWords memory := by
    unfold requiredWords memoryWords
    by_cases zero : size = 0
    · simp [zero]
    · simp only [zero, ite_false, Nat.max_eq_left (by omega : (offset+size+31)/32 ≤ memory.length/32)]
  simp [expand,extent,words,Nat.not_lt.mpr bounded]

def priceAtCut (cfg : Config) (frame : Frame) (sample : Nat) (target object : Word) :
    Except String (Option (Nat × Nat)) := do
  let ext ← required (cfg.externalConfig) "missing MIR external execution configuration"
  let (opcode,args,prepared,gas) ←
    LowerAddressCall.loweredMemoryObjectArgs (withBudget cfg frame sample)
      frame target (BitVec.ofNat 256 sample) object
  let offset := (args[2]?.getD 0).toNat
  let length := (args[3]?.getD 0).toNat
  let memory ← expand prepared.memory offset length
  let memory ← expand memory (args[4]?.getD 0).toNat (args[5]?.getD 0).toNat
  let destination := (args[0]?.getD 0).setWidth 160
  let message : Calls.Message := ⟨.call,destination,args[1]?.getD 0,readBytes memory offset length⟩
  let (_,resolution,_) := EVM.State.executionCode cfg.fork prepared.world destination
  callBudget (withBudget cfg frame sample) prepared ext memory message opcode resolution gas

/-- The input header and the complete payload fit in the already active memory. -/
theorem callRanges (trace : Trace) (shape : trace = referenceTrace 6)
    (entry : Entry) (heap : 128 ≤ entry.fmp)
    (sample available child retained words : Nat)
    (reaches : EVM.CallBudget.run trace entry = .call sample available child retained words)
    (frame : Frame) (object target : Word) (ready : Ready entry frame object target words) :
    expand frame.memory object.toNat 32 = .ok frame.memory ∧
    expand frame.memory (object+32).toNat 100 = .ok frame.memory ∧
    expand frame.memory 0 0 = .ok frame.memory := by
  obtain ⟨_,_,_,_,_,highest⟩ :=
    samples trace shape entry heap sample available child retained words 0 reaches (Nat.zero_le _)
  have aligned : frame.memory.length % 32 = 0 := by rw [ready.extent]; simp
  have covered : entry.fmp+132 ≤ frame.memory.length := by
    have high := Nat.le_max_right entry.words ((entry.fmp+163)/32)
    rw [← highest] at high
    rw [ready.extent]
    omega
  have dataPointer : (object+32).toNat = entry.fmp+32 := by
    rw [BitVec.toNat_add, ready.pointer]
    change (entry.fmp+32)%2^256 = entry.fmp+32
    exact Nat.mod_eq_of_lt (by have := ready.limit; omega)
  have header := expandCovered frame.memory object.toNat 32 aligned
    (by rw [ready.pointer]; omega) ready.limit
  have payload := expandCovered frame.memory (object+32).toNat 100 aligned
    (by rw [dataPointer]; omega) ready.limit
  have noOutput := expandCovered frame.memory 0 0 aligned (by simp) ready.limit
  exact ⟨header,payload,noOutput⟩

/-- The concrete object footprint proves that neither the header load nor the
message's input/output accesses increase memory. The helper's EXTCODESIZE
has already warmed this same address. These are state relations at the cut,
so the only gas input is the proved budget at helper entry. -/
theorem readyPrice (trace : Trace) (shape : trace = referenceTrace 6)
    (entry : Entry) (heap : 128 ≤ entry.fmp) (limit : entry.gas ≤ 30000000)
    (sample available child retained words : Nat)
    (reaches : EVM.CallBudget.run trace entry = .call sample available child retained words)
    (cfg : Config) (fork : cfg.fork = .cancun) (frame : Frame)
    (ext : EVM.State.Config) (extFork : ext.fork = .cancun)
    (externalCfg : cfg.externalConfig = some ext)
    (object target : Word) (ready : Ready entry frame object target words) :
    priceAtCut cfg frame sample target object = .ok (some (child,retained)) := by
  obtain ⟨header,payload,noOutput⟩ :=
    callRanges trace shape entry heap sample available child retained words reaches frame object target ready
  simp only [priceAtCut, externalCfg, required, LowerAddressCall.loweredMemoryObjectArgs,
    memoryOperation, header, ready.lengthWord,
    value, Bind.bind, Except.bind, Pure.pure, Except.pure, List.getElem?_cons_succ,
    List.getElem?_cons_zero, Option.getD_some]
  have lengthNat : (100 : Word).toNat = 100 := rfl
  have zeroNat : (0 : Word).toNat = 0 := rfl
  simp only [lengthNat, zeroNat, payload, noOutput]
  exact preparedPrice trace shape entry heap limit sample available child retained words reaches
    cfg fork frame ext extFork ready.journal (target.setWidth 160)
    (readBytes frame.memory (object+32).toNat 100) (object+32).toNat
    ready.warm payload noOutput

end SolarTV.MIR.FinalCallBudget
