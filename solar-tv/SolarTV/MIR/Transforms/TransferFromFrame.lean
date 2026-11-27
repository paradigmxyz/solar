import SolarTV.MIR.Transforms.FinalCallBudget

/-! Relate the helper's typed ABI encoder to the memory effects imported from
final bytecode. Both sides start with arbitrary shared byte memory, including
dirty words and already active memory beyond the object. -/

namespace SolarTV.MIR.TransferFromFrame
open EvmRules SolarTV.Execution MIR.State
open EVM.CallBudget (Entry)

def selector : Word := BitVec.ofNat 256 (0x23b872dd * 2^224)

inductive Stored where
  | literal (value : Nat)
  | heap (offset : Nat)
  | sender | receiver | amount
  deriving DecidableEq

inductive Access where
  | loadFmp
  | store (address : EVM.CallBudget.Address) (value : Stored)
  deriving DecidableEq

def stores : List Access :=
  [.loadFmp, .store (.fixed 64) (.heap 160), .store (.heap 0) (.literal 100),
   .store (.heap 32) (.literal (0x23b872dd * 2^224)), .store (.heap 36) .sender,
   .store (.heap 68) .receiver, .store (.heap 100) .amount]

def storeWord (memory : List Byte) (atByte : Nat) (word : Word) :
    Except String (List Byte) := do
  let memory ← expand memory atByte 32
  return writeBytes memory atByte (wordBytes word)

def runMemory (sender receiver amount : Word) : List Access → List Byte → Word →
    Except String (List Byte)
  | [], memory, _ => .ok memory
  | .loadFmp :: rest, memory, _ => do
    let memory ← expand memory 64 32
    runMemory sender receiver amount rest memory (readWord memory 64)
  | .store address value :: rest, memory, pointer => do
    let word := match value with
      | .literal n => BitVec.ofNat 256 n
      | .heap delta => pointer + BitVec.ofNat 256 delta
      | .sender => sender
      | .receiver => receiver
      | .amount => amount
    let memory ← storeWord memory (address.at pointer.toNat) word
    runMemory sender receiver amount rest memory pointer

private theorem noWrap (pointer delta : Word) (bound : pointer.toNat + delta.toNat < 2^256) :
    (pointer + delta).toNat = pointer.toNat + delta.toNat := by
  rw [BitVec.toNat_add, Nat.mod_eq_of_lt bound]

private theorem writeFits (memory : List Byte) (offset : Nat) (word : Word)
    (aligned : memory.length % 32 = 0) (bounded : memory.length ≤ 65536)
    (range : offset + 32 ≤ 65536) :
    ∃ next, storeWord memory offset word = .ok next ∧
      next.length = max memory.length (32 * ((offset + 63) / 32)) ∧
      offset + 32 ≤ next.length ∧
      readWord next offset = word ∧
      ∀ other, (other + 32 ≤ offset ∨ offset + 32 ≤ other) →
        other + 32 ≤ memory.length → readWord next other = readWord memory other := by
  let size := 32 * max (memory.length / 32) ((offset + 63) / 32)
  have maximum : size = max memory.length (32 * ((offset + 63) / 32)) := by
    dsimp only [size]
    omega
  have larger : memory.length ≤ size := by omega
  have covered : offset + 32 ≤ size := by omega
  have limit : size ≤ 65536 := by omega
  let ext := memory ++ List.replicate (size - memory.length) (0 : Byte)
  have length : ext.length = size := by simp [ext]; omega
  have expanded : expand memory offset 32 = .ok ext := by
    simpa [expand, requiredWords, memoryWords, ← show offset + 32 + 31 = offset + 63 by omega,
      ext, size] using Nat.not_lt.mpr limit
  let next := writeBytes ext offset (wordBytes word)
  have nextLength : next.length = size := by simpa [next] using length
  refine ⟨next, by unfold storeWord; rw [expanded]; rfl, nextLength.trans maximum,
    by omega, readWord_writeWord _ _ _ (length ▸ covered), ?_⟩
  intro other disjoint inside
  dsimp only [next]
  rw [readWord_writeBytes_disjoint _ _ _ _ (by
    simpa [wordBytes] using disjoint.symm)]
  exact readWord_append_prefix _ _ _ inside

private theorem written (memory : List Byte) (p : Word) (sender receiver amount : Word)
    (aligned : memory.length % 32 = 0) (bounded : memory.length ≤ 65536)
    (live : 96 ≤ memory.length) (valid : p.toNat + 160 ≤ 65536)
    (heap : 128 ≤ p.toNat) :
    ∃ m0 m1 m2 m3 m4 m5,
      storeWord memory 64 (p+160) = .ok m0 ∧
      storeWord m0 p.toNat 100 = .ok m1 ∧
      storeWord m1 (p.toNat+32) selector = .ok m2 ∧
      storeWord m2 (p.toNat+36) sender = .ok m3 ∧
      storeWord m3 (p.toNat+68) receiver = .ok m4 ∧
      storeWord m4 (p.toNat+100) amount = .ok m5 ∧
      m5.length = 32 * max (memoryWords memory) ((p.toNat+163)/32) ∧
      m5.length ≤ 65536 ∧ readWord m5 p.toNat = 100 ∧ readWord m5 64 = p+160 := by
  obtain ⟨m0,h0,l0,_,r0,_⟩ := writeFits memory 64 (p+160) aligned bounded (by decide)
  obtain ⟨m1,h1,l1,in1,r1,keep1⟩ := writeFits m0 p.toNat 100
    (by omega) (by omega) (by omega)
  obtain ⟨m2,h2,l2,_,_,r2⟩ := writeFits m1 (p.toNat+32) selector
    (by omega) (by omega) (by omega)
  obtain ⟨m3,h3,l3,_,_,r3⟩ := writeFits m2 (p.toNat+36) sender
    (by omega) (by omega) (by omega)
  obtain ⟨m4,h4,l4,_,_,r4⟩ := writeFits m3 (p.toNat+68) receiver
    (by omega) (by omega) (by omega)
  obtain ⟨m5,h5,l5,_,_,r5⟩ := writeFits m4 (p.toNat+100) amount
    (by omega) (by omega) (by omega)
  refine ⟨m0,m1,m2,m3,m4,m5,h0,h1,h2,h3,h4,h5,?_,by omega,?_,?_⟩
  · unfold memoryWords
    omega
  · rw [r5 _ (by omega) (by omega), r4 _ (by omega) (by omega),
      r3 _ (by omega) (by omega), r2 _ (by omega) (by omega), r1]
  · rw [r5 _ (by omega) (by omega), r4 _ (by omega) (by omega),
      r3 _ (by omega) (by omega), r2 _ (by omega) (by omega),
      keep1 _ (by omega) (by omega), r0]

private theorem writeAbiWord (memory : List Byte) (start value : Word)
    (rest : List (Word × Bool)) :
    abiWriteWords memory start ((value,false)::rest) =
      (storeWord memory start.toNat value).bind (fun next => abiWriteWords next (start+32) rest) := by
  rw [abiWriteWords]
  unfold storeWord
  cases expand memory start.toNat 32 <;> simp only [Bool.false_eq_true,ite_false,
    Bind.bind,Except.bind,Pure.pure,Except.pure]

set_option maxRecDepth 4096 in
/-- A field-for-field match with the scalar ABI operation in the captured MIR.
The exact write sequence retains all untouched bytes, including padding. -/
theorem encode (frame : Frame) (sender receiver amount : Word)
    (aligned : frame.memory.length % 32 = 0) (bounded : frame.memory.length ≤ 65536)
    (live : 96 ≤ frame.memory.length) (heap : 128 ≤ (readWord frame.memory 64).toNat)
    (valid : (readWord frame.memory 64).toNat + 160 ≤ 65536) :
    let p := readWord frame.memory 64
    ∃ memory,
      abiEncode frame 1 false true [false,false,false] [selector,sender,receiver,amount] =
        .ok (.scalar (Scalar.ofWord 256 p),{frame with memory}) ∧
      runMemory sender receiver amount stores frame.memory 0 = .ok memory ∧
      memory.length = 32 * max (memoryWords frame.memory) ((p.toNat+163)/32) ∧
      memory.length ≤ 65536 ∧ readWord memory p.toNat = 100 ∧ readWord memory 64 = p+160 := by
  dsimp only
  let p := readWord frame.memory 64
  change 128 ≤ p.toNat at heap
  change p.toNat + 160 ≤ 65536 at valid
  have hdr := FinalCallBudget.expandCovered frame.memory 64 32 aligned live bounded
  obtain ⟨m0,m1,m2,m3,m4,m5,h0,h1,h2,h3,h4,h5,length,limit,word,cursor⟩ :=
    written frame.memory p sender receiver amount aligned bounded live valid heap
  refine ⟨m5,?_,?_,length,limit,word,cursor⟩
  · have bump := noWrap p 160 (by change p.toNat+160 < 2^256; omega)
    have addr32 := noWrap p 32 (by change p.toNat+32 < 2^256; omega)
    have addr36 := noWrap (p+32) 4 (by rw [addr32]; change p.toNat+32+4 < 2^256; omega)
    have addr68 := noWrap (p+32+4) 32 (by rw [addr36,addr32]; change p.toNat+32+4+32 < 2^256; omega)
    have addr100 := noWrap (p+32+4+32) 32 (by rw [addr68,addr36,addr32]; change p.toNat+32+4+32+32 < 2^256; omega)
    have past : ¬ (p+160).toNat < p.toNat := by rw [bump]; change ¬ p.toNat+160 < p.toNat; omega
    have within : ¬ (p+160).toNat > 2^64-1 := by rw [bump]; change ¬ p.toNat+160 > 2^64-1; omega
    have alloc : allocate frame 160 false false =
        .ok (.returned (some p,{frame with memory := m0})) := by
      simp only [storeWord,hdr,Bind.bind,Except.bind,Pure.pure,Except.pure] at h0
      cases h0
      have a : ¬ (readWord frame.memory 64 + 160).toNat < (readWord frame.memory 64).toNat := past
      have b : ¬ (readWord frame.memory 64 + 160).toNat > 2^64-1 := within
      simp only [allocate,hdr,Bind.bind,Except.bind,a,b,decide_false,
        Bool.or_self,Bool.false_eq_true,ite_false,Pure.pure,Except.pure]
      rfl
    have payload : abiWriteWords m2 (p+32+4) [(sender,false),(receiver,false),(amount,false)] =
        .ok m5 := by
      have first : (p+32+4).toNat = p.toNat+36 := by
        rw [addr36,addr32]; rfl
      have next : (p+32+4+32).toNat = p.toNat+68 := by
        rw [addr68,first]; rfl
      have last : (p+32+4+32+32).toNat = p.toNat+100 := by
        rw [addr100,next]; rfl
      rw [writeAbiWord,first,h3]
      simp only [Except.bind]
      rw [writeAbiWord,next,h4]
      simp only [Except.bind]
      rw [writeAbiWord,last,h5]
      rfl
    have selectorOffset : (p+32).toNat = p.toNat+32 := addr32
    unfold abiEncode
    simp only [required,List.getElem?_cons_zero,List.drop_succ_cons,List.drop_zero,
      List.length_cons,List.length_nil,Bind.bind,Except.bind,Pure.pure,Except.pure]
    simp (config := { decide := true }) only [List.zip_cons_cons, ite_false, ite_true,
      Nat.reduceAdd, Nat.reduceMul, Nat.reduceDiv]
    erw [alloc]
    simp only [selectorOffset]
    have expanded {memory next : List Byte} {offset : Nat} {w : Word}
        (h : storeWord memory offset w = .ok next) :
        ∃ m, expand memory offset 32 = .ok m ∧ writeBytes m offset (wordBytes w) = next := by
      unfold storeWord at h
      cases eq : expand memory offset 32 with
      | error err => simp [eq,Bind.bind,Except.bind] at h
      | ok m =>
        refine ⟨m,rfl,?_⟩
        rw [eq] at h
        exact Except.ok.inj h
    obtain ⟨a,ea,wa⟩ := expanded h1
    obtain ⟨b,eb,wb⟩ := expanded h2
    rw [ea]
    dsimp only
    erw [wa,eb]
    dsimp only
    erw [wb,payload]
  · simp only [stores,runMemory,hdr,Bind.bind,Except.bind,EVM.CallBudget.Address.at,Nat.add_zero]
    erw [h0]
    dsimp only
    erw [h1]
    dsimp only
    erw [h2]
    dsimp only
    erw [h3]
    dsimp only
    erw [h4]
    dsimp only
    erw [h5]


private theorem readStore (before after : List Byte) (offset : Nat) (word : Word)
    (written : storeWord before offset word = .ok after)
    (within : offset + 32 ≤ after.length) :
    readBytes after offset 32 = wordBytes word := by
  unfold storeWord at written
  cases ex : expand before offset 32 with
  | error _ => simp [ex,Bind.bind,Except.bind] at written
  | ok active =>
    simp only [ex,Bind.bind,Except.bind,Pure.pure,Except.pure,Except.ok.injEq] at written
    subst after
    rw [writeBytes_length] at within
    simpa using readBytes_writeBytes_same active (wordBytes word) offset (by simpa using within)

private theorem readStoreDisjoint (before after : List Byte) (offset source count : Nat)
    (word : Word) (written : storeWord before offset word = .ok after)
    (within : source + count ≤ before.length)
    (disjoint : offset + 32 ≤ source ∨ source + count ≤ offset) :
    readBytes after source count = readBytes before source count := by
  unfold storeWord at written
  cases ex : expand before offset 32 with
  | error _ => simp [ex,Bind.bind,Except.bind] at written
  | ok active =>
    simp only [ex,Bind.bind,Except.bind,Pure.pure,Except.pure,Except.ok.injEq] at written
    subst after
    rw [readBytes_writeBytes_disjoint _ _ _ _ _ (by simpa using disjoint)]
    unfold expand at ex
    dsimp only at ex
    split at ex
    · contradiction
    · cases ex
      exact readBytes_append_prefix _ _ _ _ within

def canonicalInput (sender receiver amount : Word) : List Byte :=
  (wordBytes selector).take 4 ++ wordBytes sender ++ wordBytes receiver ++ wordBytes amount

private theorem payloadAfterStores (p : Nat) (sender receiver amount : Word)
    (m1 m2 m3 m4 m5 : List Byte)
    (h2 : storeWord m1 (p+32) selector = .ok m2)
    (h3 : storeWord m2 (p+36) sender = .ok m3)
    (h4 : storeWord m3 (p+68) receiver = .ok m4)
    (h5 : storeWord m4 (p+100) amount = .ok m5)
    (l2 : p+64 ≤ m2.length) (l3 : p+68 ≤ m3.length)
    (l4 : p+100 ≤ m4.length) (l5 : p+132 ≤ m5.length) :
    readBytes m5 (p+32) 100 = canonicalInput sender receiver amount := by
  have first : readBytes m2 (p+32) 4 = (wordBytes selector).take 4 := by
    rw [←readBytes_take m2 (p+32) 32 4 (by decide),readStore _ _ _ _ h2 (by omega)]
  rw [show 100 = 4 + (32 + (32+32)) by decide,readBytes_split,readBytes_split m5 (p+32+4),
    readBytes_split m5 (p+32+4+32)]
  have offsets : p+32+4 = p+36 ∧ p+32+4+32 = p+68 ∧ p+32+4+32+32 = p+100 := by omega
  rw [offsets.2.2,offsets.2.1,offsets.1]
  rw [readStoreDisjoint _ _ _ (p+32) 4 _ h5 (by omega) (Or.inr (by omega)),
    readStoreDisjoint _ _ _ (p+32) 4 _ h4 (by omega) (Or.inr (by omega)),
    readStoreDisjoint _ _ _ (p+32) 4 _ h3 (by omega) (Or.inr (by omega)),first]
  rw [readStoreDisjoint _ _ _ (p+36) 32 _ h5 (by omega) (Or.inr (by omega)),
    readStoreDisjoint _ _ _ (p+36) 32 _ h4 (by omega) (Or.inr (by omega)),
    readStore _ _ _ _ h3 (by omega)]
  rw [readStoreDisjoint _ _ _ (p+68) 32 _ h5 (by omega) (Or.inr (by omega)),
    readStore _ _ _ _ h4 (by omega),readStore _ _ _ _ h5 (by omega)]
  simp only [canonicalInput,List.append_assoc]

private theorem storeExtent (memory after : List Byte) (offset : Nat) (word : Word)
    (written : storeWord memory offset word = .ok after) : offset + 32 ≤ after.length := by
  unfold storeWord at written
  cases ex : expand memory offset 32 with
  | error _ => simp [ex,Bind.bind,Except.bind] at written
  | ok active =>
    simp only [ex,Bind.bind,Except.bind,Pure.pure,Except.pure,Except.ok.injEq] at written
    subst after
    rw [writeBytes_length]
    unfold expand at ex
    dsimp only at ex
    split at ex
    · contradiction
    · cases ex
      simp only [List.length_append,List.length_replicate,requiredWords,
        show ¬ (32 : Nat) = 0 by decide,ite_false]
      have lower := Nat.le_max_right (memoryWords memory) ((offset+32+31)/32)
      omega

/-- The physical stores initialize every byte sent to the child. The payload
depends on the three ABI words and is independent of the incoming arena. -/
theorem projectedInput (memory : List Byte) (sender receiver amount : Word)
    (aligned : memory.length % 32 = 0) (bounded : memory.length ≤ 65536)
    (live : 96 ≤ memory.length) (heap : 128 ≤ (readWord memory 64).toNat)
    (valid : (readWord memory 64).toNat + 160 ≤ 65536) :
    ∃ after, runMemory sender receiver amount stores memory 0 = .ok after ∧
      readBytes after ((readWord memory 64).toNat+32) 100 =
        canonicalInput sender receiver amount := by
  let p := (readWord memory 64).toNat
  obtain ⟨m0,m1,m2,m3,m4,m5,h0,h1,h2,h3,h4,h5,_,_,_,_⟩ :=
    written memory (readWord memory 64) sender receiver amount aligned bounded live valid heap
  refine ⟨m5,?_,payloadAfterStores p sender receiver amount m1 m2 m3 m4 m5 h2 h3 h4 h5
    (by have := storeExtent _ _ _ _ h2; omega) (storeExtent _ _ _ _ h3)
    (storeExtent _ _ _ _ h4) (storeExtent _ _ _ _ h5)⟩
  have hdr := FinalCallBudget.expandCovered memory 64 32 aligned live bounded
  simp only [stores,runMemory,hdr,Bind.bind,Except.bind,EVM.CallBudget.Address.at,Nat.add_zero]
  erw [h0]
  dsimp only
  erw [h1]
  dsimp only
  erw [h2]
  dsimp only
  erw [h3]
  dsimp only
  erw [h4]
  dsimp only
  erw [h5]

/-- Relate the child input from distinct MIR and machine arenas. Each arena
keeps its own placement and unrelated bytes. The caller separately relates
its journal and any memory it reads after the helper. -/
theorem independentMemoryInput (frame : Frame) (physical : List Byte)
    (sender receiver amount : Word)
    (mirAligned : frame.memory.length % 32 = 0) (mirBound : frame.memory.length ≤ 65536)
    (mirLive : 96 ≤ frame.memory.length) (mirHeap : 128 ≤ (readWord frame.memory 64).toNat)
    (mirFits : (readWord frame.memory 64).toNat + 160 ≤ 65536)
    (evmAligned : physical.length % 32 = 0) (evmBound : physical.length ≤ 65536)
    (evmLive : 96 ≤ physical.length) (evmHeap : 128 ≤ (readWord physical 64).toNat)
    (evmFits : (readWord physical 64).toNat + 160 ≤ 65536) :
    ∃ mirAfter evmAfter,
      abiEncode frame 1 false true [false,false,false] [selector,sender,receiver,amount] =
        .ok (.scalar (Scalar.ofWord 256 (readWord frame.memory 64)),{frame with memory := mirAfter}) ∧
      runMemory sender receiver amount stores physical 0 = .ok evmAfter ∧
      readBytes mirAfter ((readWord frame.memory 64).toNat+32) 100 =
        readBytes evmAfter ((readWord physical 64).toNat+32) 100 := by
  obtain ⟨left,encoded,storesLeft,_,_,_,_⟩ :=
    encode frame sender receiver amount mirAligned mirBound mirLive mirHeap mirFits
  obtain ⟨same,leftWrites,leftInput⟩ :=
    projectedInput frame.memory sender receiver amount mirAligned mirBound mirLive mirHeap mirFits
  obtain ⟨right,rightWrites,rightInput⟩ :=
    projectedInput physical sender receiver amount evmAligned evmBound evmLive evmHeap evmFits
  have equality := Except.ok.inj (storesLeft.symm.trans leftWrites)
  exact ⟨left,right,encoded,rightWrites,(equality ▸ leftInput).trans rightInput.symm⟩

structure Inputs where
  token : Address
  sender : Address
  receiver : Address
  amount : Word

def argument (i width : Nat) : Operand := ⟨.argument i,.word width⟩

def localWord (i width : Nat) : Operand := ⟨.local i,.word width⟩

def targetOperand : Operand := argument 0 160
def objectOperand : Operand := ⟨.local 6,.pointer 0⟩
def gasOperand : Operand := localWord 3 256

def guard : List Instruction :=
  [⟨some 0,.word 256,none,.word,[targetOperand]⟩,
   ⟨some 1,.word 256,some 0x3b,.context (.environment 0x3b),[localWord 0 256]⟩,
   ⟨some 2,.word 1,none,.notEqual,[localWord 1 256,⟨.constant ⟨256,0⟩,.word 256⟩]⟩]

def setup : List Instruction :=
  [⟨some 3,.word 256,some 0x5a,.context .gasSample,[]⟩,
   ⟨some 4,.word 256,none,.word,[argument 1 160]⟩,
   ⟨some 5,.word 256,none,.word,[argument 2 160]⟩,
   ⟨some 6,.pointer 0,none,.abi (.encode 1 false true [false,false,false]),
     [⟨.constant ⟨256,selector⟩,.word 256⟩,localWord 4 256,localWord 5 256,argument 3 256]⟩]

def runSteps (cfg : Config) : List Instruction → Frame → Except String Frame
  | [], frame => .ok frame
  | inst :: rest, frame => do
    let .returned (none,next) ← instruction cfg frame inst | .error "unexpected prefix outcome"
    runSteps cfg rest next

private theorem runStep (cfg : Config) (inst : Instruction) (rest : List Instruction)
    (frame next : Frame) (step : instruction cfg frame inst = effect next) :
    runSteps cfg (inst :: rest) frame = runSteps cfg rest next := by
  rw [runSteps,step]
  rfl

def mirEntry (evm : EVM.State.Frame) (world : Journal) (args : Inputs) : Frame :=
  { world, memory := evm.memory, returnData := evm.returnData,
    arguments := [.scalar ⟨160,args.token⟩,.scalar ⟨160,args.sender⟩,
      .scalar ⟨160,args.receiver⟩,.scalar ⟨256,args.amount⟩] }

/-- GAS has an explicit input in the MIR semantics. The earlier sample uses a
ledger position no later than the physical GAS, as in the call-budget proof. -/
def runMirPrefix (guardOps setupOps : List Instruction) (cfg : Config)
    (evm : EVM.State.Frame) (world : Journal) (args : Inputs)
    (elapsed : Nat) : Except String (Option Frame) := do
  let mid ← runSteps cfg guardOps (mirEntry evm world args)
  let condition ← read mid (localWord 2 1)
  if condition = 0 then return none
  let timed := {cfg with layout.gasBefore := some (evm.gas-elapsed+2)}
  return some (← runSteps timed setupOps mid)

abbrev runMir := runMirPrefix guard setup

structure Cut where
  memory : List Byte
  world : Journal
  returnData : List Byte
  token : Address
  input : List Byte

def runBytes (tape : List Access) (evm : EVM.State.Frame) (world : Journal) :
    Except String (Option Cut) := do
  let _ :: token :: sender :: receiver :: amount :: _ := evm.stack
    | .error "transferFrom entry stack lacks arguments"
  let target := token.setWidth 160
  if (account world target).code.isEmpty then return none
  let memory ← runMemory sender receiver amount tape evm.memory 0
  return some ⟨memory,warmAddress world target,evm.returnData,target,
    readBytes memory ((readWord evm.memory 64).toNat+32) 100⟩

private def codeSize (world : Journal) (args : Inputs) : Word :=
  BitVec.ofNat 256 (account world args.token).code.length

private def probed (frame : Frame) (args : Inputs) : Frame :=
  { frame with
    world := warmAddress frame.world args.token
    locals := put frame.locals 1 (.scalar ⟨256,codeSize frame.world args⟩) }

private def widened (frame : Frame) (id : Nat) (word : Word) : Frame :=
  {frame with locals := put frame.locals id (.scalar ⟨256,word⟩)}

private def guardFrame (evm : EVM.State.Frame) (world : Journal) (args : Inputs) : Frame :=
  let mid := probed (widened (mirEntry evm world args) 0 (widenBits args.token)) args
  {mid with locals := put mid.locals 2 (.scalar ⟨1,if codeSize world args = 0 then 0 else 1⟩)}

set_option maxRecDepth 8192 in
set_option maxHeartbeats 600000 in
private theorem guardResult (cfg : Config) (evm : EVM.State.Frame) (world : Journal)
    (args : Inputs) (probe : cfg.available.contains 0x3b = true)
    (known : args.token ≠ cfg.environment.address) :
    runSteps cfg guard (mirEntry evm world args) = .ok (guardFrame evm world args) ∧
    MIR.State.read (guardFrame evm world args) (localWord 2 1) =
      .ok (if codeSize world args = 0 then 0 else 1) := by
  let initial := mirEntry evm world args
  let mid := widened initial 0 (widenBits args.token)
  let ready := probed mid args
  have first : instruction cfg initial guard[0] = effect mid := rfl
  have target : (widenBits args.token).setWidth 160 = args.token :=
    wordBits_widenBits 160 _ (by decide)
  have other : (args.token == cfg.environment.address) = false := beq_eq_false_iff_ne.mpr known
  have second : instruction cfg mid guard[1] = effect ready := by
    rw [show guard[1] =
      (⟨some 1,.word 256,some 0x3b,.context (.environment 0x3b),[localWord 0 256]⟩ : Instruction) by rfl]
    simp only [instruction,Option.any_some,probe,Bool.not_true,Bool.false_eq_true,ite_false]
    have readMid : ([localWord 0 256].mapM (MIR.State.read mid)) = .ok [widenBits args.token] := rfl
    rw [readMid]
    simp only [Bind.bind,Except.bind]
    rw [operation]
    have observed : contextOperation cfg mid 59 [widenBits args.token] =
        value {mid with world := warmAddress mid.world args.token} (codeSize mid.world args) := by
      generalize widenBits args.token = token at target ⊢
      change (if (token.setWidth 160 == cfg.environment.address) = true then
        .error "MIR root runtime code is unknown"
        else value {mid with world := warmAddress mid.world (token.setWidth 160)}
          (BitVec.ofNat 256 (account mid.world (token.setWidth 160)).code.length)) = _
      rw [target,other]
      rfl
    rw [observed]
    rfl
  have last : instruction cfg ready guard[2] = effect (guardFrame evm world args) := rfl
  constructor
  · change (runSteps cfg (guard[0] :: guard[1] :: guard[2] :: []) initial) = _
    rw [runStep _ _ _ _ _ first,runStep _ _ _ _ _ second,runStep _ _ _ _ _ last]
    rfl
  · by_cases h : codeSize world args = 0
    · change (Except.ok (widenBits (if codeSize world args = 0 then 0 else 1 : BitVec 1))) = _
      simp [h,widenBits]
    · change (Except.ok (widenBits (if codeSize world args = 0 then 0 else 1 : BitVec 1))) = _
      simp [widenBits]

private def staged (evm : EVM.State.Frame) (world : Journal) (args : Inputs) (elapsed : Nat) : Frame :=
  let mid := guardFrame evm world args
  let mid := widened mid 3 (BitVec.ofNat 256 (evm.gas-elapsed))
  let mid := widened mid 4 (widenBits args.sender)
  widened mid 5 (widenBits args.receiver)

private def finished (mid : Frame) (object : Scalar) (memory : List Byte) : Frame :=
  {mid with memory := memory, locals := put mid.locals 6 (.scalar object)}

set_option maxRecDepth 8192 in
set_option maxHeartbeats 600000 in
private theorem setupResult (cfg : Config) (evm : EVM.State.Frame) (world : Journal)
    (args : Inputs) (elapsed : Nat) (gasOp : cfg.available.contains 0x5a = true)
    (memory : List Byte) (object : Word)
    (encode : abiEncode (staged evm world args elapsed) 1 false true [false,false,false]
      [selector,widenBits args.sender,widenBits args.receiver,args.amount] =
      .ok (.scalar (Scalar.ofWord 256 object),{staged evm world args elapsed with memory})) :
    runSteps {cfg with layout.gasBefore := some (evm.gas-elapsed+2)} setup (guardFrame evm world args) =
      .ok (finished (staged evm world args elapsed) (Scalar.ofWord 256 object) memory) := by
  let timed := {cfg with layout.gasBefore := some (evm.gas-elapsed+2)}
  let start := guardFrame evm world args
  let sampled := widened start 3 (BitVec.ofNat 256 (evm.gas-elapsed))
  let sender := widened sampled 4 (widenBits args.sender)
  let mid := staged evm world args elapsed
  have first : instruction timed start setup[0] = effect sampled := by
    rw [show setup[0] =
      (⟨some 3,.word 256,some 0x5a,.context .gasSample,[]⟩ : Instruction) by rfl]
    have available : timed.available.contains 0x5a = true := gasOp
    simp only [instruction,Option.any_some,available,Bool.not_true,Bool.false_eq_true,ite_false,
      List.mapM_nil,Bind.bind,Except.bind,Pure.pure,Except.pure]
    simp only [operation,layoutOperation,timed,required,Bind.bind,Except.bind,
      show ¬ evm.gas-elapsed+2 < 2 by omega,ite_false,
      show evm.gas-elapsed+2-2 = evm.gas-elapsed by omega]
    rfl
  have second : instruction timed sampled setup[1] = effect sender := rfl
  have third : instruction timed sender setup[2] = effect mid := rfl
  have last : instruction timed mid setup[3] =
      effect (finished mid (Scalar.ofWord 256 object) memory) := by
    rw [show setup[3] =
      (⟨some 6,.pointer 0,none,.abi (.encode 1 false true [false,false,false]),
        [⟨.constant ⟨256,selector⟩,.word 256⟩,localWord 4 256,localWord 5 256,argument 3 256]⟩ : Instruction) by rfl]
    simp only [instruction,Option.any_none,Bool.false_eq_true,ite_false]
    have reads : ([⟨.constant ⟨256,selector⟩,.word 256⟩,localWord 4 256,localWord 5 256,
        argument 3 256].mapM (MIR.State.read mid)) =
        .ok [selector,widenBits args.sender,widenBits args.receiver,args.amount] := rfl
    rw [reads]
    simp only [Bind.bind,Except.bind]
    rw [show abiEncode mid 1 false true [false,false,false]
      [selector,widenBits args.sender,widenBits args.receiver,args.amount] =
        .ok (.scalar (Scalar.ofWord 256 object),{mid with memory}) from encode]
    rfl
  change runSteps timed (setup[0]::setup[1]::setup[2]::setup[3]::[]) start = _
  rw [runStep _ _ _ _ _ first,runStep _ _ _ _ _ second,runStep _ _ _ _ _ third,
    runStep _ _ _ _ _ last]
  rfl

private theorem codeSize_zero (world : Journal) (args : Inputs)
    (bound : (account world args.token).code.length < 2^256) :
    codeSize world args = 0 ↔ (account world args.token).code.isEmpty = true := by
  rw [← BitVec.toNat_inj]
  simp [codeSize,Nat.mod_eq_of_lt bound]

/-- Successful bytecode prefix execution establishes the pre-call MIR frame.
Memory and the journal are related from the shared entry, including bytes
outside the object and returndata left by earlier calls. -/
theorem fromEntry (cfg : Config) (evm : EVM.State.Frame) (world : Journal) (args : Inputs)
    (trace : EVM.CallBudget.Trace) (shape : trace = EVM.CallBudget.referenceTrace 6)
    (tape : List Access) (writes : tape = stores)
    (sample available child retained words elapsed : Nat)
    (entryStack : ∃ ret rest, evm.stack = ret :: widenBits args.token :: widenBits args.sender ::
      widenBits args.receiver :: args.amount :: rest)
    (probe : cfg.available.contains 0x3b = true) (gasOp : cfg.available.contains 0x5a = true)
    (known : args.token ≠ cfg.environment.address)
    (codeBound : (account world args.token).code.length < 2^256)
    (aligned : evm.memory.length % 32 = 0) (bounded : evm.memory.length ≤ 65536)
    (live : 96 ≤ evm.memory.length) (heap : 128 ≤ (Entry.ofFrame world evm args.token).fmp)
    (valid : (Entry.ofFrame world evm args.token).fmp + 160 ≤ 65536)
    (reaches : EVM.CallBudget.run trace (Entry.ofFrame world evm args.token) =
      .call sample available child retained words) :
    let p := readWord evm.memory 64
    let object := Scalar.ofWord 256 p
    ∃ frame,
      runMir cfg evm world args elapsed = .ok (some frame) ∧
      runBytes tape evm world = .ok (some ⟨frame.memory,frame.world,frame.returnData,args.token,
        readBytes frame.memory (p+32).toNat 100⟩) ∧
      Nonempty (FinalCallBudget.Ready (Entry.ofFrame world evm args.token) frame
        object.word (widenBits args.token) words) ∧
      MIR.State.read frame targetOperand = .ok (widenBits args.token) ∧
      readValue frame objectOperand = .ok (.scalar object) ∧
      MIR.State.read frame gasOperand = .ok (BitVec.ofNat 256 (evm.gas-elapsed)) ∧
      readWord frame.memory 64 = p+160 := by
  let entry := Entry.ofFrame world evm args.token
  have priced := (EVM.CallBudget.exact entry heap 6).symm.trans (shape ▸ reaches)
  have nonempty : (account world args.token).code.isEmpty = false := by
    cases empty : (account world args.token).code.isEmpty
    · rfl
    · have absent : entry.hasCode = false := by simp [entry,Entry.ofFrame,empty]
      simp [EVM.CallBudget.reference,absent] at priced
      split at priced <;> split at priced <;> contradiction
  have code : codeSize world args ≠ 0 := by
    intro h
    have eq := (codeSize_zero world args codeBound).mp h
    simp [nonempty] at eq
  obtain ⟨_,_,_,_,_,highest⟩ := FinalCallBudget.samples trace shape entry heap
    sample available child retained words 0 reaches (Nat.zero_le _)
  let mid := staged evm world args elapsed
  obtain ⟨memory,encoding,byteMemory,extent,limit,lengthWord,cursor⟩ :=
    encode mid (widenBits args.sender) (widenBits args.receiver) args.amount aligned bounded live heap valid
  dsimp only
  let p := readWord evm.memory 64
  let object := Scalar.ofWord 256 p
  let frame := finished mid object memory
  have target : (widenBits args.token).setWidth 160 = args.token :=
    wordBits_widenBits 160 _ (by decide)
  obtain ⟨guarded,condition⟩ := guardResult cfg evm world args probe known
  obtain ⟨ret,rest,stack⟩ := entryStack
  refine ⟨frame,?_,?_,?_,rfl,rfl,rfl,cursor⟩
  · simp only [runMir,runMirPrefix,guarded,condition,Bind.bind,Except.bind,code,ite_false,
      show (1 : Word) ≠ 0 by decide,setupResult cfg evm world args elapsed gasOp memory p encoding,
      Pure.pure,Except.pure]
    rfl
  · have offset : (p+32).toNat = p.toNat+32 := noWrap _ _ (by
      change p.toNat + 32 < 2^256
      change p.toNat + 160 ≤ 65536 at valid
      omega)
    simp only [runBytes,stack,target,nonempty,Bool.false_eq_true,ite_false,
      writes,Bind.bind,Except.bind,Pure.pure,Except.pure]
    have bytesEq : runMemory (widenBits args.sender) (widenBits args.receiver) args.amount
        stores evm.memory 0 = .ok memory := byteMemory
    rw [bytesEq,offset]
    rfl
  · refine ⟨⟨world,?_,rfl,?_,limit,lengthWord⟩⟩
    · change warmAddress world args.token = warmAddress world ((widenBits args.token).setWidth 160)
      rw [target]
    · rw [highest]
      exact extent

end SolarTV.MIR.TransferFromFrame
