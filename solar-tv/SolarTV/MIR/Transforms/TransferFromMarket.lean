import SolarTV.MIR.Transforms.TransferFromABI

/-! Morpho's first Market storage access. Four callers hash the decoded
tuple and use it to address the packed word that contains lastUpdate.
The MIR operations and emitted stack code use the same hash inputs,
address-qualified storage read, and scratch words. -/

namespace SolarTV.MIR.TransferFromMarket
open EvmRules SolarTV.Execution MIR.TransferFromABI

def marketHash (market : List Word) : Word :=
  Keccak.hash (wordArena market)

def mappingHash (market : List Word) : Word :=
  Keccak.hash (wordArena [marketHash market,3])

def marketKey (market : List Word) : Word :=
  mappingHash market + 2

/-- The high half of the market word carries a different field. -/
abbrev marketMissing (loaded : Word) : Prop :=
  loaded.setWidth 128 = 0

def marketGuardFlag (loaded : Word) : Word :=
  if marketMissing loaded then 1 else 0

theorem marketMask (loaded : Word) :
    Evm.and (BitVec.ofNat 256 (2^128-1)) loaded = (loaded.setWidth 128).setWidth 256 := by
  apply BitVec.eq_of_toNat_eq
  rw [Evm.and, BitVec.toNat_and, Nat.and_comm, show
    (BitVec.ofNat 256 (2^128-1)).toNat = 2^128-1 by decide, Nat.and_two_pow_sub_one_eq_mod,
    BitVec.toNat_setWidth, BitVec.toNat_setWidth]
  exact (Nat.mod_eq_of_lt
    (Nat.lt_trans (Nat.mod_lt _ (by decide : 0 < 2^128)) (by decide : 2^128 < 2^256))).symm

theorem marketWidenedZero (loaded : Word) :
    (loaded.setWidth 128).setWidth 256 = 0 ↔ marketMissing loaded := by
  unfold marketMissing
  rw [← BitVec.toNat_inj, ← BitVec.toNat_inj (x := loaded.setWidth 128)]
  rw [BitVec.toNat_setWidth,
    Nat.mod_eq_of_lt (Nat.lt_trans (loaded.setWidth 128).isLt (by decide : 2^128 < 2^256))]
  rfl

theorem maskGuard (loaded : Word) :
    Evm.iszero (Evm.and (BitVec.ofNat 256 (2^128-1)) loaded) =
      marketGuardFlag loaded := by
  rw [marketMask]
  simp only [Evm.iszero, beq_iff_eq, marketWidenedZero, marketGuardFlag, Bool.cond_eq_ite]

theorem marketScalar (loaded : Word) :
    (MIR.State.Scalar.ofWord 128 ((loaded.setWidth 128).setWidth 256)).word =
      (loaded.setWidth 128).setWidth 256 := by
  change MIR.State.widenBits (MIR.State.wordBits 128
    (MIR.State.widenBits (loaded.setWidth 128))) = _
  rw [MIR.State.wordBits_widenBits _ _ (by decide)]
  rfl

theorem arenaCharge (words : List Word) (index count : Nat)
    (within : index + count ≤ words.length) :
    EVM.State.memoryCharge (wordArena words) (32*index) (32*count) = 0 := by
  simp [EVM.State.memoryCharge,arenaRange words index count within,
    memoryWords,Nat.mul_comm words.length]

theorem hashMarket (cfg : MIR.State.Config) (frame : MIR.State.Frame)
    (market : List Word) (size : market.length = 5)
    (memory : frame.memory = wordArena ([0,0,288,0] ++ market)) :
    MIR.State.operation cfg frame (.memory .hash) [128,160] =
      MIR.State.value frame (marketHash market) := by
  have within : 4+5 ≤ ([0,0,288,0] ++ market).length := by simp [size]
  have active : expand (wordArena ([0,0,288,0] ++ market)) 128 160 =
      .ok (wordArena ([0,0,288,0] ++ market)) :=
    expandArenaRange ([0,0,288,0] ++ market) 4 5 within (by simp [size])
  have bytes : readBytes (wordArena ([0,0,288,0] ++ market)) 128 160 = wordArena market := by
    change readBytes (wordArena ([0,0,288,0] ++ market)) (32*4) (32*5) = _
    rw [readArena _ _ _ within]
    change wordArena (market.take 5) = _
    rw [List.take_of_length_le (Nat.le_of_eq size)]
  change (expand frame.memory 128 160 >>= fun arena =>
    MIR.State.value {frame with memory := arena} (Keccak.hash (readBytes arena 128 160))) = _
  rw [memory,active]
  change MIR.State.value {frame with memory := wordArena ([0,0,288,0] ++ market)}
    (Keccak.hash (readBytes (wordArena ([0,0,288,0] ++ market)) 128 160)) = _
  rw [bytes, ← memory]
  cases frame
  rfl

theorem readMarketStorage (cfg : MIR.State.Config) (frame : MIR.State.Frame) (key : Word) :
    MIR.State.operation cfg frame (.storage false false) [key] =
      MIR.State.value {frame with world := warmSlot frame.world cfg.environment.address key}
        (slotsRead (account frame.world cfg.environment.address).storage key) := rfl

def marketMemory (market : List Word) : List Byte :=
  wordArena ([marketHash market,3,288,0] ++ market)

theorem mapMarket (cfg : MIR.State.Config) (frame : MIR.State.Frame)
    (market : List Word) (size : market.length = 5)
    (memory : frame.memory = wordArena ([0,0,288,0] ++ market)) :
    MIR.State.operation cfg frame (.slot .mapping) [marketHash market,3] =
      MIR.State.value {frame with memory := marketMemory market} (mappingHash market) := by
  let words : List Word := [0,0,288,0] ++ market
  have within : 0 + 2 ≤ words.length := by simp [words]
  have active : expand (wordArena words) 0 64 = .ok (wordArena words) :=
    expandArenaRange words 0 2 within (by simp [words,size])
  have length : (wordArena words).length = 288 := by simp [words,size]
  have bytes : (wordBytes (marketHash market) ++ wordBytes 3).length ≤ (wordArena words).length := by
    simp [length]
  have written : writeBytes (wordArena words) 0 (wordBytes (marketHash market) ++ wordBytes 3) =
      marketMemory market := by
    unfold writeBytes
    simp only [Nat.sub_zero]
    rw [List.take_of_length_le bytes]
    simp only [List.take_zero,List.nil_append,
      List.length_append,wordBytes_length,Nat.reduceAdd,Nat.zero_add]
    rw [show (64 : Nat) = 32*2 by rfl, dropArena]
    unfold marketMemory
    rfl
  have mapped : MIR.State.fixedSlot frame [marketHash market,3] =
      .ok (mappingHash market,{frame with memory := marketMemory market}) := by
    simp only [MIR.State.fixedSlot,List.flatMap_cons,List.flatMap_nil,List.append_nil,
      List.length_append,wordBytes_length]
    change (expand frame.memory 0 64 >>= fun arena => Except.ok
      (mappingHash market,{frame with memory := writeBytes arena 0 (wordBytes (marketHash market) ++ wordBytes 3)})) = _
    rw [memory,active]
    exact congrArg (fun arena => Except.ok (mappingHash market,{frame with memory := arena})) written
  simp only [MIR.State.operation,mapped,Bind.bind,Except.bind]

def mirStart (entry : Nat) (journal : Journal) (market : List Word)
    (rest : List MIR.State.Value) : MIR.State.Machine :=
  ⟨{function := entry},[],⟨journal,wordArena ([0,0,288,0] ++ market),
    MIR.TransferFromEntry.scalar 256 128 :: rest,[],[],0,0,[]⟩⟩

/-- Count reached MIR transitions without assigning physical gas to SSA instructions. -/
def mirStepsFrom (cfg : MIR.State.Config) (functions : MIR.State.Functions)
    (machine : MIR.State.Machine) (steps : Nat := 10) (fuel : Nat := 1024) :
    Except String (Unit ⊕ MIR.State.Machine) :=
  Trace.segment (S := MIR.State.Machine) (R := Unit) (fun fuel machine => do
    match ← MIR.State.transition fuel cfg functions machine with
    | .next _ after => return .inr after
    | .finished _ => .error "MIR stopped before the market checkpoint") steps fuel machine

theorem mirStepsContinue {cfg : MIR.State.Config} {functions : MIR.State.Functions}
    {initial reached : MIR.State.Machine} (first rest fuel : Nat)
    (checked : mirStepsFrom cfg functions initial first fuel = .ok (.inr reached)) :
    mirStepsFrom cfg functions initial (first + rest) fuel =
      mirStepsFrom cfg functions reached rest (fuel - first) := by
  unfold mirStepsFrom at checked ⊢
  exact Trace.segment_continue first rest fuel checked

def mirSteps (cfg : MIR.State.Config) (functions : MIR.State.Functions) (entry : Nat)
    (journal : Journal) (market : List Word) (rest : List MIR.State.Value) (steps : Nat := 10) :=
  mirStepsFrom cfg functions (mirStart entry journal market rest) steps

def mirObservation (result : Nat) (outcome : Unit ⊕ MIR.State.Machine) :
    Except String (Word × List Byte × Journal) := do
  let .inr next := outcome | .error "MIR stopped before market read"
  let value ← MIR.State.read next.frame ⟨.local result,.word 256⟩
  return (value,next.frame.memory,next.frame.world)

def mirGuardObservation (outcome : Unit ⊕ MIR.State.Machine) :
    Except String (Nat × List Byte × Journal) := do
  let .inr next := outcome | .error "MIR stopped before the market branch"
  return (next.cursor.block,next.frame.memory,next.frame.world)

end SolarTV.MIR.TransferFromMarket
