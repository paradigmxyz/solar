import SolarTV.MIR.Transforms.TransferFromFrame

/-! Validation at the emitted transferFrom CALL and its continuation. The
child executes once in the shared EVM interpreter. The imported continuation
consumes its success word and volatile buffer, materializes the object, then
returns to the internal caller or reverts with the selected bytes. -/

namespace SolarTV.MIR.TransferFromReturn
open EvmRules SolarTV.Execution MIR.State
open MIR.FinalCallBudget
open MIR.TransferFromFrame (Cut)
open MIR.LowerReturndata

def byteCall (cfg : EVM.State.Config) (fuel : Nat) (environment : World.Environment)
    (cut : Cut) (supplied : Nat) : Except String EVM.State.MessageOutcome :=
  let (code,_,world) := EVM.State.executionCode cfg.fork cut.world cut.token
  EVM.State.executeCall (EVM.State.run fuel cfg) cfg world environment
    ⟨.call,cut.token,0,cut.input⟩ code supplied

def postCall (frame : Frame) (retained : Nat) (child : EVM.State.MessageOutcome) : Frame :=
  { frame with
    world := child.world
    returnData := child.data
    messageGas := frame.messageGas ++ [retained+child.gas]}

private theorem noWrite (memory : List Byte) : writeBytes memory 0 [] = memory := by
  simp [writeBytes]

set_option maxRecDepth 4096 in
/-- Both sides run the same child bytecode with the proved forwarded gas.
executeCall commits only a successful child and retains revert data with
the checkpoint journal. Unspent child gas returns to the retained budget. -/
theorem childMatches (trace : EVM.CallBudget.Trace) (shape : trace = EVM.CallBudget.referenceTrace 6)
    (entry : EVM.CallBudget.Entry) (heap : 128 ≤ entry.fmp) (limit : entry.gas ≤ 30000000)
    (sample available supplied retained words : Nat)
    (reaches : EVM.CallBudget.run trace entry = .call sample available supplied retained words)
    (cfg : Config) (fork : cfg.fork = .cancun) (frame : Frame)
    (ext : EVM.State.Config) (extFork : ext.fork = .cancun)
    (externalCfg : cfg.externalConfig = some ext) (fuel : Nat)
    (object target : Word) (ready : Ready entry frame object target words) :
    invoke (withBudget cfg frame sample) fuel
      (MIR.LowerAddressCall.loweredMemoryObjectArgs
        (withBudget cfg frame sample) frame target (BitVec.ofNat 256 sample) object) =
    (do
      let child ← byteCall ext fuel cfg.environment
        ⟨frame.memory,frame.world,frame.returnData,target.setWidth 160,
          readBytes frame.memory (object+32).toNat 100⟩ supplied
      return some (.returned (some child.value,postCall frame retained child),child.work)) := by
  obtain ⟨header,payload,noOutput⟩ :=
    callRanges trace shape entry heap sample available supplied retained words reaches frame object target ready
  have priced := readyPrice trace shape entry heap limit sample available supplied retained words
    reaches cfg fork frame ext extFork externalCfg object target ready
  simp only [priceAtCut, externalCfg, required, MIR.LowerAddressCall.loweredMemoryObjectArgs,
    memoryOperation, header, ready.lengthWord, value, Bind.bind, Except.bind, Pure.pure, Except.pure,
    List.getElem?_cons_succ, List.getElem?_cons_zero, Option.getD_some,
    show (100 : Word).toNat = 100 by rfl, show (0 : Word).toNat = 0 by rfl,
    payload,noOutput] at priced
  have newer : (withBudget cfg frame sample).fork.atLeast .amsterdam = false := by
    simp [withBudget, fork, Fork.atLeast, Fork.ordinal]
  have ⟨eq,_,_,_,_,_⟩ :=
    samples trace shape entry heap sample available supplied retained words 0 reaches (Nat.zero_le _)
  have cap : ¬ (BitVec.ofNat 256 sample).toNat > 30000000 := by
    rw [BitVec.toNat_ofNat, Nat.mod_eq_of_lt (by omega : sample < 2^256)]
    omega
  simp (config := { decide := true }) only [
    invoke,MIR.LowerAddressCall.loweredMemoryObjectArgs,memoryOperation,header,value,ready.lengthWord,
    external,show (withBudget cfg frame sample).externalConfig = some ext from externalCfg,
    required,newer,Bool.false_eq_true,ite_false,List.length_cons,List.length_nil,Nat.reduceAdd,
    List.getElem?_cons_succ,List.getElem?_cons_zero,Option.getD_some,Option.any_some,
    Option.isNone_some,Bool.false_and,Bool.and_false,decide_false,ite_true,
    show ((0 : Word) != 0) = false by decide,
    show (100 : Word).toNat = 100 by rfl,show (0 : Word).toNat = 0 by rfl,payload,noOutput,
    Bind.bind,Except.bind,Pure.pure,Except.pure,cap]
  rw [show (withBudget cfg frame sample).fork = cfg.fork by rfl, priced]
  simp only [byteCall,extFork,fork]
  simp only [postCall,noWrite,withBudget, List.take_zero, Option.isSome_some, ite_true]

/-- The physical collector uses the cursor carried across the CALL on the stack. -/
def collectMemory (memory buffer : List Byte) (cursor : Word) : Except String (List Byte) := do
  let length := BitVec.ofNat 256 buffer.length
  let bumped := cursor + ((length+63) &&& ~~~(31 : Word))
  let memory ← MIR.TransferFromFrame.storeWord memory 64 bumped
  let memory ← MIR.TransferFromFrame.storeWord memory cursor.toNat length
  let start := cursor+32
  let memory ← expand memory start.toNat buffer.length
  return writeBytes memory start.toNat (readBytes buffer 0 buffer.length)

def collectBytes (frame : Frame) (cursor : Word) : Step := do
  let memory ← collectMemory frame.memory frame.returnData cursor
  value {frame with memory} cursor

theorem collect_eq (frame : Frame) (cursor : Word)
    (aligned : frame.memory.length % 32 = 0) (boundedMemory : frame.memory.length ≤ 65536)
    (live : 96 ≤ frame.memory.length) (fmp : readWord frame.memory 64 = cursor)
    (bound : cursor.toNat + frame.returnData.length + 63 ≤ 2^64-1) :
    collectBytes frame cursor = returnDataBytes frame := by
  have header := expandCovered frame.memory 64 32 aligned live boundedMemory
  have nBound : frame.returnData.length + 63 < 2^256 := by omega
  let size := (frame.returnData.length+63)/32*32
  have sBound : size < 2^256 := by dsimp [size]; omega
  have bump : (cursor + BitVec.ofNat 256 size).toNat = cursor.toNat + size := by
    rw [BitVec.toNat_add,BitVec.toNat_ofNat,Nat.mod_eq_of_lt sBound]
    apply Nat.mod_eq_of_lt
    dsimp [size]
    omega
  have noUnder : ¬ (cursor + BitVec.ofNat 256 size).toNat < cursor.toNat := by rw [bump]; omega
  have noOver : ¬ (cursor + BitVec.ofNat 256 size).toNat > 2^64-1 := by rw [bump]; dsimp [size]; omega
  have nOverflow : ¬ frame.returnData.length + 63 ≥ 2^256 := by omega
  dsimp only [size] at noUnder noOver
  simp only [collectBytes,collectMemory,MIR.LowerReturndata.roundedMask _ nBound,returnDataBytes,
    MIR.TransferFromFrame.storeWord,allocate,header,fmp,nOverflow,noUnder,noOver,decide_false,
    Bool.false_eq_true,Bool.or_self,ite_false,MIR.LowerReturndata.readWhole,
    Bind.bind,Except.bind,Pure.pure,Except.pure]
  cases expand (writeBytes frame.memory 64
    (wordBytes (cursor + BitVec.ofNat 256 ((frame.returnData.length+63)/32*32)))) cursor.toNat 32 with
  | error _ => rfl
  | ok mem =>
    dsimp only
    cases expand (writeBytes mem cursor.toNat (wordBytes (BitVec.ofNat 256 frame.returnData.length)))
      (cursor+32).toNat frame.returnData.length <;> rfl

private theorem expandInfo (memory : List Byte) (offset count : Nat) (next : List Byte)
    (aligned : memory.length % 32 = 0)
    (h : expand memory offset count = .ok next) :
    (∃ extra, next = memory ++ extra) ∧
      memory.length ≤ next.length ∧ next.length % 32 = 0 ∧ next.length ≤ 65536 ∧
      (count = 0 ∨ offset+count ≤ next.length) := by
  let size := 32 * requiredWords memory offset count
  have larger : memory.length ≤ size := by
    unfold size requiredWords memoryWords
    split <;> omega
  unfold expand at h
  change (if size > 65536 then Except.error "world memory limit of 64 KiB"
    else Except.ok (memory ++ List.replicate (size-memory.length) 0)) = Except.ok next at h
  by_cases limit : size > 65536
  · simp [limit] at h
  · simp only [limit,ite_false,Except.ok.injEq] at h
    have length : next.length = size := by rw [← h]; simp; omega
    refine ⟨⟨_,h.symm⟩,by omega,?_,by omega,?_⟩
    · rw [length]; simp [size]
    · rw [length]
      unfold size requiredWords memoryWords
      split <;> omega

/-- The physical copy preserves its length word and leaves both the header and
the first ABI word covered whenever the corresponding read is reached. -/
theorem collectedRanges (frame result : Frame) (cursor pointer : Word)
    (aligned : frame.memory.length % 32 = 0) (limit : frame.memory.length ≤ 65536)
    (live : 96 ≤ frame.memory.length)
    (bound : cursor.toNat + frame.returnData.length + 63 ≤ 2^64-1)
    (collected : collectBytes frame cursor = .ok (.returned (some pointer,result))) :
    readWord result.memory pointer.toNat = BitVec.ofNat 256 frame.returnData.length ∧
    result.returnData = frame.returnData ∧
    expand result.memory pointer.toNat 32 = .ok result.memory ∧
    (32 ≤ frame.returnData.length →
      expand result.memory (pointer+32).toNat 32 = .ok result.memory) := by
  let len := frame.returnData.length
  let first := writeBytes frame.memory 64 (wordBytes (cursor +
    ((BitVec.ofNat 256 len + 63) &&& ~~~(31 : Word))))
  let start := (cursor+32).toNat
  have firstAligned : first.length % 32 = 0 := by simpa [first] using aligned
  have startEq : start = cursor.toNat+32 := by
    change (cursor.toNat+32)%2^256 = _
    apply Nat.mod_eq_of_lt
    omega
  have fmp := expandCovered frame.memory 64 32 aligned live limit
  simp only [collectBytes, collectMemory, MIR.TransferFromFrame.storeWord, fmp,
    Bind.bind,Except.bind,Pure.pure,Except.pure] at collected
  cases h : expand first cursor.toNat 32 with
  | error err =>
    dsimp only [first,len] at h
    erw [h] at collected
    contradiction
  | ok header =>
    obtain ⟨_,_,alignHeader,_,inHeader⟩ := expandInfo _ _ _ _ firstAligned h
    have coversHeader : cursor.toNat+32 ≤ header.length := by omega
    let stored := writeBytes header cursor.toNat (wordBytes (BitVec.ofNat 256 len))
    have storedAligned : stored.length % 32 = 0 := by simpa [stored] using alignHeader
    have within : cursor.toNat+32 ≤ stored.length := by simpa [stored] using coversHeader
    dsimp only [first,len] at h
    erw [h] at collected
    dsimp only at collected
    cases payload : expand stored start len with
    | error err =>
      dsimp only [stored,start,len] at payload
      erw [payload] at collected
      contradiction
    | ok next =>
      obtain ⟨⟨extra,appended⟩,grew,alignNext,limitNext,covered⟩ :=
        expandInfo _ _ _ _ storedAligned payload
      dsimp only [stored,start,len] at payload
      erw [payload] at collected
      dsimp only at collected
      simp only [value,Except.ok.injEq,Outcome.returned.injEq,Prod.mk.injEq,Option.some.injEq] at collected
      obtain ⟨rfl,rfl⟩ := collected
      let memory := writeBytes next start (readBytes frame.returnData 0 len)
      have total : memory.length = next.length := by simp [memory]
      refine ⟨?_,rfl,?_,?_⟩
      · change readWord memory cursor.toNat = _
        dsimp only [memory]
        rw [readWord_writeBytes_disjoint _ _ _ _ (Or.inr (by omega)),appended,
          readWord_append_prefix _ _ _ within, readWord_writeWord _ _ _ coversHeader]
      · exact expandCovered memory cursor.toNat 32 (by omega) (by omega) (by omega)
      · intro large
        exact expandCovered memory start 32 (by omega) (by omega) (by omega)

inductive Route where
  | childFailed | empty | short | malformed | returnedFalse | returnedTrue
  deriving DecidableEq

structure TailPrices where
  childFailed : Nat
  empty : Nat
  short : Nat
  malformed : Nat
  returnedFalse : Nat
  returnedTrue : Nat
  copy : Nat
  deriving DecidableEq

def referencePrices : TailPrices := ⟨137,109,145,173,261,218,3⟩

def Route.fixed (route : Route) (prices : TailPrices) : Nat :=
  match route with
  | .childFailed => prices.childFailed
  | .empty => prices.empty
  | .short => prices.short
  | .malformed => prices.malformed
  | .returnedFalse => prices.returnedFalse
  | .returnedTrue => prices.returnedTrue

def Route.accepts (route : Route) : Bool :=
  route == .empty || route == .returnedTrue

def reasonWord (reason : String) : Word :=
  readWord (reason.toUTF8.data.toList.map (fun byte => BitVec.ofNat 8 byte.toNat)) 0

def revertError (memory : List Byte) (reason : String) : List Byte :=
  let memory := writeBytes memory 32 (wordBytes 32)
  let memory := writeBytes memory 64 (wordBytes (BitVec.ofNat 256 reason.utf8ByteSize))
  let memory := writeBytes memory 96 (wordBytes (reasonWord reason))
  let memory := writeBytes memory 0 (wordBytes 0x08c379a0)
  readBytes memory 28 100

def Route.data (route : Route) (memory : List Byte) : List Byte :=
  match route with
  | .childFailed => revertError memory "transferFrom reverted"
  | .returnedFalse => revertError memory "transferFrom returned false"
  | _ => []

def byteRoute (flag length word : Word) : Route :=
  if flag = 0 then .childFailed else
  if length = 0 then .empty else
  if Evm.sgt 32 length ≠ 0 then .short else
  if Evm.lt 1 word ≠ 0 then .malformed else
  if Evm.lt 0 word = 0 then .returnedFalse else .returnedTrue

def mirRoute (flag length word : Word) : Route :=
  if flag = 0 then .childFailed else
  if length = 0 then .empty else
  if length.toNat ≥ 2^255 || length.toNat < 32 then .short else
  if !abiCanonical .boolean word then .malformed else
  if word = 0 then .returnedFalse else .returnedTrue

/-- The dispatched decoder uses a signed comparison for its length check.
On both sides a nonempty buffer must contain a canonical ABI bool. -/
theorem route_eq (flag length word : Word) : byteRoute flag length word = mirRoute flag length word := by
  have lengthCheck : (Evm.sgt 32 length ≠ 0) ↔ length.toNat ≥ 2^255 ∨ length.toNat < 32 := by
    simp only [Evm.sgt,BitVec.slt_eq_decide,cond_eq_ite,decide_eq_true_eq,BitVec.toInt]
    split <;> simp_all <;> omega
  have zero : word.toNat = 0 ↔ word = 0 := by rw [← BitVec.toNat_inj]; rfl
  have compareOne : (Evm.lt 1 word ≠ 0) ↔ 1 < word.toNat := by
    simp [Evm.lt,BitVec.ult]
  have compareZero : (Evm.lt 0 word = 0) ↔ word = 0 := by
    simpa only [zero] using (show (Evm.lt 0 word = 0) ↔ word.toNat = 0 from by
      simp [Evm.lt,BitVec.ult])
  have invalid : (!abiCanonical .boolean word) = true ↔ 1 < word.toNat := by
    simp only [abiCanonical,Bool.not_eq_true',decide_eq_false_iff_not]
    omega
  simp only [byteRoute,mirRoute,lengthCheck,compareOne,compareZero,invalid,Bool.or_eq_true,decide_eq_true_eq]

inductive Exit where
  | returned (memory : List Byte) (world : Journal) (returnData : List Byte) (gas : Nat)
  | reverted (data : List Byte) (gas : Nat)
  | outOfGas

/-- Fused charges are nonnegative. All memory touched after the copy is either
scratch within the already active prefix or the copied header or first word. -/
def finish (prices : TailPrices) (start memory : List Byte) (world : Journal) (buffer : List Byte)
    (route : Route) (gas : Nat) : Exit :=
  let charge := route.fixed prices + prices.copy * ((buffer.length+31)/32) +
    EVM.State.memoryCost (memoryWords memory) - EVM.State.memoryCost (memoryWords start)
  if gas < charge then .outOfGas
  else if route.accepts then .returned memory world buffer (gas-charge)
  else .reverted (route.data memory) (gas-charge)

def executeTail (prices : TailPrices) (start : List Byte) (cursor : Word) (retained : Nat)
    (child : EVM.State.MessageOutcome) : Except String Exit := do
  let memory ← collectMemory start child.data cursor
  let route := byteRoute child.value (BitVec.ofNat 256 child.data.length)
    (readWord memory (cursor+32).toNat)
  return finish prices start memory child.world child.data route (retained+child.gas)

/-- Interpret the typed continuation: skip decoding for an empty success,
otherwise run the direct MIR boolean decoder at the materialized pointer. -/
def routeFromMir (cfg : Config) (frame : Frame) (flag pointer : Word) : Except String Route := do
  if flag = 0 then return .childFailed
  let .returned (some length,next) ← memoryOperation cfg frame .load [pointer]
    | .error "unexpected MIR returndata header"
  if length = 0 then return .empty
  match ← abiDecode cfg next (some .boolean) false false (.scalar (Scalar.ofWord 256 pointer)) with
  | .reverted .empty =>
    return if length.toNat ≥ 2^255 || length.toNat < 32 then .short else .malformed
  | .returned (some word,_) => return if word = 0 then .returnedFalse else .returnedTrue
  | _ => .error "unexpected MIR returndata decode"

private theorem routeFromMir_eq (cfg : Config) (frame : Frame) (flag pointer length : Word)
    (header : expand frame.memory pointer.toNat 32 = .ok frame.memory)
    (size : readWord frame.memory pointer.toNat = length)
    (data : 32 ≤ length.toNat → expand frame.memory (pointer+32).toNat 32 = .ok frame.memory) :
    routeFromMir cfg frame flag pointer = .ok (mirRoute flag length (readWord frame.memory (pointer+32).toNat)) := by
  by_cases flagZero : flag = 0
  · simp [routeFromMir,mirRoute,flagZero,Pure.pure,Except.pure]
  simp only [routeFromMir,mirRoute,flagZero,ite_false,memoryOperation,header,value,size,
    Bind.bind,Except.bind,Pure.pure,Except.pure]
  by_cases zero : length = 0
  · simp [zero]
  simp only [zero,ite_false]
  have object : (Scalar.ofWord 256 pointer).word = pointer := rfl
  simp only [abiDecode, object, Bool.false_eq_true,ite_false,header,size,
    Option.isSome_some,ite_true,Bind.bind,Except.bind,Pure.pure,Except.pure]
  by_cases short : (decide (length.toNat ≥ 2^255) || decide (length.toNat < 32)) = true
  · simp only [short,ite_true]
  · have long : 32 ≤ length.toNat := by
      simp only [Bool.or_eq_true,decide_eq_true_eq] at short
      omega
    simp only [short,data long]
    by_cases valid : (!abiCanonical .boolean (readWord frame.memory (pointer+32).toNat)) = true <;>
      simp only [valid,Bool.false_eq_true,ite_true,ite_false]

/-- Interpret a completed MIR CALL and its materialized buffer. Price the
machine continuation on both sides, since MIR has no physical gas per step. -/
def resumeMir (cfg : Config) (prices : TailPrices) (start : List Byte)
    (result : Except String (Option (Outcome (Option Word × Option Word × Frame) × Nat))) :
    Except String Exit := do
  let some (.returned (some flag,some pointer,frame),_) ← result
    | .error "unexpected MIR call continuation"
  let gas ← required frame.messageGas.getLast? "missing returned CALL gas"
  let route ← routeFromMir cfg frame flag pointer
  return finish prices start frame.memory frame.world frame.returnData route gas

theorem tailMatches (cfg : Config) (prices : TailPrices) (frame : Frame) (cursor : Word)
    (retained : Nat) (child : EVM.State.MessageOutcome)
    (aligned : frame.memory.length % 32 = 0) (limit : frame.memory.length ≤ 65536)
    (live : 96 ≤ frame.memory.length) (fmp : readWord frame.memory 64 = cursor)
    (bound : cursor.toNat + child.data.length + 63 ≤ 2^64-1) :
    resumeMir cfg prices frame.memory
      (collectMessage returnDataBytes (.ok (some (.returned
        (some child.value,postCall frame retained child),child.work)))) =
    executeTail prices frame.memory cursor retained child := by
  let called := postCall frame retained child
  have collect := collect_eq called cursor aligned limit live fmp bound
  unfold resumeMir executeTail collectMessage
  simp only [Bind.bind,Except.bind,Pure.pure,Except.pure]
  rw [← collect]
  unfold collectBytes
  have memory_ : called.memory = frame.memory := rfl
  have data_ : called.returnData = child.data := rfl
  simp only [memory_,data_,Bind.bind,Except.bind]
  cases copied : collectMemory frame.memory child.data cursor with
  | error reason => rfl
  | ok memory =>
    let next := {called with memory}
    have result : collectBytes called cursor = .ok (.returned (some cursor,next)) := by
      simp only [collectBytes,memory_,data_,copied,Bind.bind,Except.bind,value]
      rfl
    obtain ⟨size,_,header,coverage⟩ :=
      collectedRanges called next cursor cursor aligned limit live bound result
    have length : (BitVec.ofNat 256 child.data.length).toNat = child.data.length := by
      rw [BitVec.toNat_ofNat,Nat.mod_eq_of_lt]
      omega
    have first : 32 ≤ (BitVec.ofNat 256 child.data.length).toNat →
        expand next.memory (cursor+32).toNat 32 = .ok next.memory := by
      rw [length]
      exact coverage
    have remaining : next.messageGas.getLast? = some (retained+child.gas) := by
      simp [next,called,postCall]
    change (do
      let gas ← required next.messageGas.getLast? "missing returned CALL gas"
      let route ← routeFromMir cfg next child.value cursor
      return finish prices frame.memory memory child.world child.data route gas) = _
    simp only [remaining,required,Bind.bind,Except.bind,Pure.pure,Except.pure]
    rw [routeFromMir_eq cfg next child.value cursor (BitVec.ofNat 256 child.data.length) header size first,
      route_eq]

def executeCut (ext : EVM.State.Config) (fuel : Nat) (environment : World.Environment)
    (cut : Except String (Option Cut)) (prices : TailPrices) (cursor : Word)
    (supplied retained : Nat) : Except String Exit := do
  let some cut ← cut | .error "CALL prefix did not reach the message"
  let child ← byteCall ext fuel environment cut supplied
  executeTail prices cut.memory cursor retained child

theorem resumeMatched (cfg : Config) (ext : EVM.State.Config) (fuel : Nat)
    (prices : TailPrices) (frame : Frame) (cursor : Word)
    (cut : Cut) (supplied retained : Nat) (message : MessageStep)
    (memory : cut.memory = frame.memory)
    (matched : message = (do
      let child ← byteCall ext fuel cfg.environment cut supplied
      return some (.returned (some child.value,postCall frame retained child),child.work)))
    (aligned : frame.memory.length % 32 = 0) (limit : frame.memory.length ≤ 65536)
    (live : 96 ≤ frame.memory.length) (fmp : readWord frame.memory 64 = cursor)
    (bound : ∀ child, byteCall ext fuel cfg.environment cut supplied = .ok child →
      cursor.toNat + child.data.length + 63 ≤ 2^64-1) :
    resumeMir cfg prices frame.memory (collectMessage returnDataBytes message) =
    executeCut ext fuel cfg.environment (.ok (some cut)) prices cursor supplied retained := by
  rw [matched]
  unfold executeCut
  simp only [Bind.bind,Except.bind,Pure.pure,Except.pure,memory]
  cases childEq : byteCall ext fuel cfg.environment cut supplied with
  | error reason => rfl
  | ok child =>
    exact tailMatches cfg prices frame cursor retained child aligned limit live fmp (bound child childEq)

end SolarTV.MIR.TransferFromReturn
