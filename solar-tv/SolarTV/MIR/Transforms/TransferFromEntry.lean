import SolarTV.MIR.State.Interpreter
import SolarTV.MIR.Transforms.TransferFromCallers
import SolarTV.State.Trace
import SolarTV.State.CheckedHashes
import SolarTV.EVMIR.State.CodeChunks
import SolarTV.EVMIR.State.Observe
import Lean.Data.Json

/-! Execute both representations to the helper entry. The bytecode starts at PC 0;
MIR starts with typed arguments decoded from the same ABI input. The cut is
before the first helper instruction, so changes inside the helper cannot affect
the entry certificate. Child messages along the ABI path execute normally. -/

namespace SolarTV.MIR.TransferFromEntry
open EvmRules SolarTV.Execution
open MIR.State MIR.TransferFromCallers
open MIR.TransferFromFrame (Inputs)

structure ByteCut where
  frame : EVM.State.Frame
  world : Journal
  samples : List Nat
  budgets : List Nat
  pcs : List Nat

structure ByteExit where
  result : EVM.State.Result
  samples : List Nat
  budgets : List Nat
  unpaidCall : Option (Nat × Nat)

/-- Reaching the cut consumes no instruction. A continuing transition spends
one unit of root fuel and evaluates any child with the remaining fuel. -/
private def byteTransition (fuel : Nat) (cfg : EVM.State.Config)
    (instruction : Journal → EVM.State.Frame → EVM.State.Action) (readByte : Nat → Option Byte)
    (state : ByteCut) : Except String (ByteExit ⊕ ByteCut) := do
  let ⟨frame,world,samples,budgets,pcs⟩ := state
  let signal ← instruction world frame
  let budgets := match signal with
    | .call .. => frame.gas :: budgets
    | _ => budgets
  let samples := match signal with
    | .next .. => if readByte frame.pc == some 0x5a then frame.gas :: samples else samples
    | _ => samples
  let unpaidCall := match signal with
    | .done result =>
      if result.stop == .outOfGas && readByte frame.pc == some 0xf1 then
        some (frame.pc,frame.gas) else none
    | _ => none
  match ← EVM.State.advance cfg (EVM.State.run fuel cfg) signal with
  | .inl result => return .inl ⟨result,samples.reverse,budgets.reverse,unpaidCall⟩
  | .inr (_,nextWorld,nextFrame) =>
    return .inr ⟨nextFrame,nextWorld,samples,budgets,frame.pc :: pcs⟩

private def byteStepUsing (pc fuel : Nat) (cfg : EVM.State.Config) (code : List Byte)
    (instruction : Journal → EVM.State.Frame → EVM.State.Action) (readByte : Nat → Option Byte)
    (state : ByteCut) : Except String (ByteCut ⊕ ByteCut) := do
  let frame := {state.frame with code}
  if frame.pc = pc then
    return .inl ⟨frame,state.world,state.samples.reverse,state.budgets.reverse,state.pcs.reverse⟩
  match ← byteTransition fuel cfg instruction readByte {state with frame} with
  | .inl _ => .error "bytecode terminated before the helper"
  | .inr next => return .inr next

def byteStep (pc : Nat) (cfg : EVM.State.Config) (code : List Byte) :
    Trace.Step ByteCut ByteCut := fun fuel =>
  byteStepUsing pc fuel cfg code (EVM.State.instruction cfg) (fun i => code[i]?)

def chunkedByteStep (pc : Nat) (cfg : EVM.State.Config) (code : EVM.State.CodeChunks)
    (hash : List Byte → Word := Keccak.hash) :
    Trace.Step ByteCut ByteCut := fun fuel =>
  byteStepUsing pc fuel cfg code.bytes
    (fun world frame =>
      EVM.State.instructionWithImage cfg world frame code.size ((code.get? frame.pc).getD 0)
        (code.immediate frame.pc) (fun target => (code.jumps 0).contains target.toNat) hash)
    code.get?

theorem chunkedByteStep_eq (pc : Nat) (cfg : EVM.State.Config) (code : EVM.State.CodeChunks) :
    chunkedByteStep pc cfg code = byteStep pc cfg code.bytes := by
  funext fuel state
  cases state
  unfold chunkedByteStep byteStep byteStepUsing byteTransition EVM.State.instruction
  have immediate (i : Nat) : code.immediate i = EVM.State.pushValue code.bytes i :=
    funext (code.immediate_eq i)
  simp only [EVM.State.CodeChunks.get_eq, immediate,
    EVM.State.CodeChunks.jump_eq, ← EVM.State.CodeChunks.size_eq]

/-- The dispatcher uses only ordinary EVM steps. Each symbolic edge has to
establish the actual instruction action before extending the trace. -/
theorem ordinaryNext (helper fuel : Nat) (cfg : EVM.State.Config) (code : EVM.State.CodeChunks)
    (frame next : EVM.State.Frame) (journal nextWorld : Journal) (pcs : List Nat) (opcode : Byte)
    (outside : frame.pc ≠ helper) (get : code.get? frame.pc = some opcode) (notGas : opcode ≠ 0x5a)
    (executes : EVM.State.instructionWithImage cfg journal {frame with code := code.bytes}
      code.size opcode (code.immediate frame.pc) (fun target => (code.jumps 0).contains target.toNat) =
        .ok (.next nextWorld next)) :
    chunkedByteStep helper cfg code Keccak.hash fuel ⟨frame,journal,[],[],pcs⟩ =
      .ok (.inr ⟨next,nextWorld,[],[],frame.pc :: pcs⟩) := by
  simp only [chunkedByteStep, byteStepUsing, byteTransition, outside, get, Option.getD_some,
    executes, notGas, EVM.State.advance, Bind.bind, Except.bind, Pure.pure, Except.pure,
    beq_iff_eq, Option.some.injEq, ite_false]

theorem dispatchNext (helper fuel : Nat) (cfg : EVM.State.Config) (code : EVM.State.CodeChunks)
    (frame next : EVM.State.Frame) (journal : Journal) (pcs : List Nat) (opcode : Byte)
    (outside : frame.pc ≠ helper) (get : code.get? frame.pc = some opcode) (notGas : opcode ≠ 0x5a)
    (executes : EVM.State.instructionWithImage cfg journal {frame with code := code.bytes}
      code.size opcode (code.immediate frame.pc) (fun target => (code.jumps 0).contains target.toNat) =
        .ok (.next journal next)) :
    chunkedByteStep helper cfg code Keccak.hash fuel ⟨frame,journal,[],[],pcs⟩ =
      .ok (.inr ⟨next,journal,[],[],frame.pc :: pcs⟩) :=
  ordinaryNext helper fuel cfg code frame next journal journal pcs opcode outside get notGas executes

def runBytes (pc : Nat) (fuel : Nat) (cfg : EVM.State.Config) (state : ByteCut) :
    Except String ByteCut :=
  Trace.run (byteStep pc cfg state.frame.code) fuel state

def byteExitStart (cut : ByteCut) : ByteCut := {cut with samples := [], budgets := [], pcs := []}

def byteExitStep (cfg : EVM.State.Config) (code : List Byte) : Trace.Step ByteCut ByteExit :=
  fun fuel state => byteTransition fuel cfg (EVM.State.instruction cfg) (fun i => code[i]?)
    {state with frame := {state.frame with code}}

def chunkedByteExitStep (cfg : EVM.State.Config) (code : EVM.State.CodeChunks)
    (hash : List Byte → Word := Keccak.hash) : Trace.Step ByteCut ByteExit := fun fuel state =>
  byteTransition fuel cfg (fun world frame =>
    EVM.State.instructionWithImage cfg world frame code.size ((code.get? frame.pc).getD 0)
      (code.immediate frame.pc) (fun target => (code.jumps 0).contains target.toNat) hash)
    code.get? {state with frame := {state.frame with code := code.bytes}}

theorem storageExhaustion (fuel : Nat) (cfg : EVM.State.Config) (code : EVM.State.CodeChunks)
    (frame : EVM.State.Frame) (journal : Journal) (pcs : List Nat)
    (get : code.get? frame.pc = some 0x54)
    (executes : EVM.State.instructionWithImage cfg journal {frame with code := code.bytes}
      code.size 0x54 (code.immediate frame.pc) (fun target => (code.jumps 0).contains target.toNat) =
        EVM.State.stepExhausted journal) :
    chunkedByteExitStep cfg code Keccak.hash fuel ⟨frame,journal,[],[],pcs⟩ =
      .ok (.inl ⟨⟨.outOfGas,[],journal,0,0⟩,[],[],none⟩) := by
  simp only [chunkedByteExitStep, byteTransition, get, Option.getD_some,
    executes, EVM.State.stepExhausted, EVM.State.advance, Bind.bind, Except.bind,
    Pure.pure, Except.pure]
  rfl

theorem chunkedByteExitStep_eq (cfg : EVM.State.Config) (code : EVM.State.CodeChunks) :
    chunkedByteExitStep cfg code = byteExitStep cfg code.bytes := by
  funext fuel state
  unfold chunkedByteExitStep byteExitStep byteTransition EVM.State.instruction
  have immediate (i : Nat) : code.immediate i = EVM.State.pushValue code.bytes i :=
    funext (code.immediate_eq i)
  simp only [EVM.State.CodeChunks.get_eq, immediate,
    EVM.State.CodeChunks.jump_eq, ← EVM.State.CodeChunks.size_eq]

structure MirCut where
  args : List Word
  returns : List Word
  frame : MIR.State.Frame
  cursor : Cursor
  samples : List Nat

def instAt (functions : Functions) (cursor : Cursor) : Option Instruction := do
  let body ← (functions.find? (·.1 == cursor.function)).map Prod.snd
  let block ← body.blocks[cursor.block]?
  if cursor.entered then block.instructions[cursor.next]? else none

structure MirCursor where
  machine : Machine
  samples : List Nat
  preSampleExhausted : Bool
  exitGas : Option Nat

private def mirTransition (fuel : Nat) (cfg : MIR.State.Config) (functions : Functions)
    (state : MirCursor) (site : Option Instruction) (hash : List Byte → Word) :
    Except String (Transition × List Nat) := do
  let (stepCfg,samples) ← match site with
    | some ⟨_,_,_,.context .gasSample,_⟩ => do
      let sample ← required state.samples.head? "missing physical GAS sample"
      pure ({cfg with layout := {cfg.layout with gasBefore := some sample}},state.samples.drop 1)
    | _ => pure (cfg,state.samples)
  return (← transition fuel stepCfg functions state.machine hash,samples)

/-- Gas samples and per-message pricing come from the physical prefix. MIR
prices child messages, and its intervening SSA instructions remain unpriced. -/
def mirStep (helper : Nat) (returned : List Operand) (fuel : Nat) (cfg : MIR.State.Config)
    (functions : Functions) (state : MirCursor) (hash : List Byte → Word := Keccak.hash) :
    Except String (MirCut ⊕ MirCursor) := do
  let ⟨machine,samples,_,_⟩ := state
  let site := instAt functions machine.cursor
  if let some ⟨_,_,_,.call callee,callOperands⟩ := site then
    if callee = helper then
      if !machine.stack.isEmpty then .error "unexpected nested caller at helper cut" else
      return .inl ⟨← callOperands.mapM (read machine.frame), ← returned.mapM (read machine.frame),
        machine.frame,machine.cursor,samples⟩
  let (result,samples) ← mirTransition fuel cfg functions state site hash
  match result with
  | .finished _ => .error "MIR terminated before the helper"
  | .next _ next => return .inr ⟨next,samples,false,none⟩

/-- Prompts for a digest certificate. The step still computes any hash absent
from this list, including hashes in nested messages. -/
def byteHashInput (code : EVM.State.CodeChunks) (state : ByteCut) : Option (List Byte) := do
  let frame := state.frame
  if code.get? frame.pc != some 0x20 then none else
  let offset ← frame.stack[0]?
  let count ← frame.stack[1]?
  some (readBytes frame.memory offset.toNat count.toNat)

def mirHashInput (functions : Functions) (state : Except String MirCursor) : Option (List Byte) := do
  let machine := (← state.toOption).machine
  let inst ← instAt functions machine.cursor
  let frame := machine.frame
  let args ← (inst.operands.mapM (read frame)).toOption
  match inst.operation,args with
  | .memory .hash,[offset,count] => some (readBytes frame.memory offset.toNat count.toNat)
  | .slot .mapping,[key,slot] => some (wordBytes key ++ wordBytes slot)
  | .slot .arrayData,[slot] | .slot (.arrayElement _),slot :: _ => some (wordBytes slot)
  | _,_ => none

def scalar (width : Nat) (word : Word) : Value := .scalar (Scalar.ofWord width word)

def hasMarket : Caller → Bool
  | .flashLoan => false
  | _ => true

def parameterWidths : Caller → List Nat
  | .supply | .repay => [256,256,160]
  | .supplyCollateral => [256,160]
  | .liquidate => [160,256,256]
  | .flashLoan => [160,256]

def canonicalAddresses (market : List Word) (parameters : List (Nat × Word)) : Bool :=
  (market.take 4 ++ (parameters.filterMap fun (width,value) =>
    if width == 160 then some value else none)).all (·.toNat < 2^160)

def entryFrame (caller : Caller) (journal : Journal) (market : List Word)
    (parameters : List (Nat × Word)) (start length : Nat) : Except String MIR.State.Frame := do
  let hasMarket := hasMarket caller
  let memory ← expand [] 0 (if hasMarket then 288 else 96)
  let frameMemory := writeBytes memory 64 (wordBytes (if hasMarket then 288 else 128))
  let frameMemory := market.zipIdx.foldl
    (fun memory (word,index) => writeBytes memory (128+32*index) (wordBytes word)) frameMemory
  let arguments := (if hasMarket then [scalar 256 128] else []) ++
    parameters.map (fun (width,word) => scalar width word) ++
    [.aggregate [scalar 256 (BitVec.ofNat 256 start),scalar 256 (BitVec.ofNat 256 length)]]
  return ⟨journal,frameMemory,arguments,[],[],0,0,[]⟩

/-- Morpho's caller signatures have a static MarketParams tuple (except
flashLoan), scalar head values, and a final calldata bytes parameter. -/
def decode (selector : Word) (caller : Caller) (cfg : MIR.State.Config)
    (journal : Journal) : Except String MIR.State.Frame := do
  let input := cfg.environment.input
  let hasMarket := hasMarket caller
  let widths := parameterWidths caller
  let scalarBase := if hasMarket then 164 else 4
  let offsetAt := scalarBase + 32 * widths.length
  if input.length < offsetAt + 32 || (readWord input 0 >>> 224) != selector then
    .error "bad Morpho selector or ABI head" else
  let tuple := if hasMarket then (List.range 5).map (fun i => readWord input (4+32*i)) else []
  let values := widths.zipIdx.map fun (width,i) => (width,readWord input (scalarBase+32*i))
  if !canonicalAddresses tuple values then .error "noncanonical Morpho address" else
  let offset := (readWord input offsetAt).toNat
  let start := 4 + offset + 32
  let length := (readWord input (4+offset)).toNat
  if start > input.length || length > input.length-start then .error "bad Morpho calldata slice" else
  entryFrame caller journal tuple values start length

private def hexDigit (digit : Char) : Nat :=
  if digit.toNat < 97 then digit.toNat - 48 else digit.toNat - 87

def hexBytes : List Char → List Byte
  | a :: b :: rest => BitVec.ofNat 8 (16 * hexDigit a + hexDigit b) :: hexBytes rest
  | _ => []

def imageBytes (image : String) : List Byte := hexBytes image.toList

private def liveAccounts (world : Journal) : List Account :=
  ordered (fun a b => a.address.ule b.address) <| world.accounts.map fun a =>
    {a with storage := normalizedSlots a.storage, transient := normalizedSlots a.transient}

def argumentWords (args : Inputs) : List Word :=
  [widenBits args.token,widenBits args.sender,widenBits args.receiver,args.amount]

/-- The transfer arguments and saved ABI words are a sufficient relation on
the watched parts of the incoming stack. The proof is valid for any snapshots
that satisfy it, independently of how those snapshots were obtained. -/
def related (caller : Caller) (suffix : Suffix) (returnPC : Word) (args : Inputs) (other : Word)
    (stack arguments returns : List Word) : Prop :=
  arguments = argumentWords args ∧ returns = liveOperands caller args.amount other ∧
  stack.take 5 = returnPC :: argumentWords args ∧
  (suffix.publication.words (stack.drop 5)).toOption = some returns

instance (caller : Caller) (suffix : Suffix) (returnPC : Word) (args : Inputs) (other : Word)
    (stack arguments returns : List Word) :
    Decidable (related caller suffix returnPC args other stack arguments returns) := by
  unfold related
  infer_instance

def mirStart (cfg : MIR.State.Config) (functions : Functions) (entry : Nat)
    (selector : Word) (caller : Caller) (journal : Journal) (priced : ByteCut) :
    Except String MirCursor := do
  let frame ← decode selector caller cfg journal
  let initial ← start functions entry frame.arguments frame
  return ⟨⟨{function := entry},[],initial⟩,priced.samples,false,none⟩

def mirTrace (cfg : MIR.State.Config) (functions : Functions) (helper : Nat)
    (returned : List Operand) (priced : ByteCut) (hash : List Byte → Word := Keccak.hash) :
    Trace.Step (Except String MirCursor) MirCut :=
  Trace.checkedStart fun fuel =>
    fun cursor => mirStep helper returned fuel {cfg with messageGas := some priced.budgets} functions cursor hash

def mirPrefix (cfg : MIR.State.Config) (functions : Functions) (entry helper : Nat)
    (returned : List Operand) (selector : Word) (caller : Caller) (journal : Journal)
    (priced : ByteCut) (fuel : Nat) : Except String MirCut :=
  Trace.run (mirTrace cfg functions helper returned priced) fuel
    (mirStart cfg functions entry selector caller journal priced)

def finishByte (checkpoint : Journal) (result : EVM.State.Result) : EVM.State.Result :=
  {result with world := if result.stop = .success then EVM.State.finalize result.world else checkpoint}

def runByteExit (cfg : EVM.State.Config) (fuel : Nat) (cut : ByteCut) : Except String ByteExit :=
  Trace.run (byteExitStep cfg cut.frame.code) fuel (byteExitStart cut)

/-- Price the checked transferFrom prefix at the helper boundary. Its physical
schedule stages the input before GAS, while the typed SSA samples gas before
encoding. The charge uses the incoming gas and active memory from the physical
frame and the helper's own MIR arguments, journal and free memory pointer. -/
private def helperPricing (cfg : MIR.State.Config) (functions : Functions)
    (physical : ByteCut) (cut : MirCut) : Option (Nat × EVM.CallBudget.Entry) := do
  let ⟨_,_,_,.call callee,_⟩ ← instAt functions cut.cursor | none
  let (_,helper) ← functions.find? (·.1 == callee)
  let entry ← helper.blocks[0]?
  let [⟨some a,.word 256,none,.word,[⟨.argument 0,.word 160⟩]⟩,
       ⟨some b,.word 256,some 0x3b,.context (.environment 0x3b),[⟨.local x,.word 256⟩]⟩,
       ⟨some c,.word 1,none,.notEqual,[⟨.local y,.word 256⟩,⟨.constant zero,.word 256⟩]⟩] :=
    entry.instructions | none
  let .branch ⟨.local condition,.word 1⟩ yes _ := entry.terminator | none
  let ⟨_,_,some 0x5a,.context .gasSample,[]⟩ ←
    helper.blocks[yes]?.bind (·.instructions.head?) | none
  let token ← cut.args.head?
  let address := token.setWidth 160
  let fmp := (readWord cut.frame.memory 64).toNat
  if cfg.fork != .cancun || address == cfg.environment.address ||
      a != x || b != y || c != condition || zero.word != 0 ||
      fmp < 128 || fmp + 160 > 65536 then none else
  let world := cut.frame.world
  let priced : EVM.CallBudget.Entry := ⟨physical.frame.gas,fmp,memoryWords physical.frame.memory,
    world.warmAddresses.contains address, !(account world address).code.isEmpty⟩
  return (callee,priced)

/-- The no-code branch builds Error("no code") in bytes 0 through 127.
Its fixed cost is checked against the emitted branch and shared revert body. -/
def noCodeCharge (entry : EVM.CallBudget.Entry) : Nat :=
  60 + EVM.State.memoryCost (max entry.words 4) - EVM.State.memoryCost entry.words

def preSampleExhausted (cfg : MIR.State.Config) (functions : Functions)
    (physical : ByteCut) (cut : MirCut) : Bool :=
  (helperPricing cfg functions physical cut).any fun (_,entry) =>
    let need := EVM.CallBudget.sampleCharge entry 6
    entry.gas < need + (if entry.hasCode then 0 else noCodeCharge entry)

structure HelperCallPrice where
  callee : Nat
  gas : Nat
  paid : Bool

/-- Derive the parent budget from the checked schedule, independently of
the physical tail's recorded termination and message ledger. -/
def helperCallPrice (cfg : MIR.State.Config) (functions : Functions)
    (physical : ByteCut) (cut : MirCut) : Option HelperCallPrice := do
  let (callee,entry) ← helperPricing cfg functions physical cut
  match EVM.CallBudget.reference entry 6 with
  | .call sample .. => return ⟨callee,sample,true⟩
  | .outOfGas => return ⟨callee, ← EVM.CallBudget.unpaidCallBudget entry 6,false⟩
  | .noCode _ => none

def unpaidHelperBudget (price : Option HelperCallPrice) : Option Nat :=
  (price.filter (!·.paid)).map (·.gas)

def paidHelperBudget (price : Option HelperCallPrice) : Option Nat :=
  (price.filter (·.paid)).map (·.gas)

/-- The MIR GAS operation consumes the balance before paying two gas. -/
def helperSamples (cfg : MIR.State.Config) (functions : Functions)
    (physical : ByteCut) (cut : MirCut) : List Nat :=
  (helperCallPrice cfg functions physical cut).toList.map (·.gas + 2)

def mirExitBudgets (cfg : MIR.State.Config) (functions : Functions) (physical : ByteCut)
    (cut : MirCut) : List Nat :=
  physical.budgets ++ (paidHelperBudget (helperCallPrice cfg functions physical cut)).toList

def mirExitStart (cfg : MIR.State.Config) (functions : Functions) (physical : ByteCut)
    (cut : MirCut) : Except String MirCursor := do
  let some (_,entry) := helperPricing cfg functions physical cut
    | .error "cannot price the MIR helper at this entry"
  let exitGas := match EVM.CallBudget.reference entry 6 with
    | .noCode gas => EVM.CallBudget.charged gas (noCodeCharge entry)
    | _ => none
  .ok ⟨⟨cut.cursor,[],cut.frame⟩,cut.samples ++ helperSamples cfg functions physical cut,
    preSampleExhausted cfg functions physical cut,exitGas⟩

private def priceContinuation (caller : Caller) (inst : Instruction) (parent : Frame) :
    Except String (Option Nat) := do
  let id ← required inst.result "missing helper CALL result"
  let flag ← read parent ⟨.local id,inst.ty⟩
  let gas ← required parent.messageGas.getLast? "missing returned gas for helper CALL"
  let cursor := readWord parent.memory 64
  let child : EVM.State.MessageOutcome := ⟨flag,parent.returnData,parent.world,gas,0⟩
  let exit ← TransferFromReturn.executeTail TransferFromReturn.referencePrices parent.memory
    cursor 0 child
  gasAfter caller exit

/-- The checked helper CALL uses its derived budget. After a paid call the
physical continuation is priced from the actual MIR child result and memory.
An affordable suffix still executes the caller and helper SSA. -/
def mirExitStep (cfg : MIR.State.Config) (functions : Functions) (caller : Caller)
    (budgets : List Nat) (price : Option HelperCallPrice)
    (hash : List Byte → Word := Keccak.hash) : Trace.Step (Except String MirCursor) EVM.State.Result :=
  Trace.checkedStart fun fuel state => do
    if state.preSampleExhausted then
      return .inl ⟨.outOfGas,[],state.machine.frame.world,0,0⟩
    let site := instAt functions state.machine.cursor
    let activePrice := match site.map (·.operation), price with
      | some (.external (.raw 0xf1)), some price
      | some (.external (.address 0xf1 ..)), some price =>
        if state.machine.cursor.function == price.callee then some price else none
      | _,_ => none
    let ledger := match activePrice with
      | none => budgets
      | some price => budgets.take state.machine.frame.messageGas.length ++ [price.gas]
    let (result,samples) ← mirTransition fuel {cfg with messageGas := some ledger} functions state site hash
    match result with
    | .next _ next =>
      let exitGas ← match activePrice with
        | none => pure state.exitGas
        | some price =>
          if !price.paid then .error "MIR helper paid the underfunded CALL" else do
          if !samples.isEmpty || next.frame.messageGas.length != budgets.length then
            .error "emitted call ledger does not match MIR execution" else
          priceContinuation caller (← required site "missing helper CALL") next.frame
      if activePrice.isSome && exitGas.isNone then
        return .inl ⟨.outOfGas,[],next.frame.world,0,0⟩
      return .inr ⟨next,samples,false,exitGas⟩
    | .finished result =>
      let (stop,data) := match result.completion with
        | .returned words => (EVM.State.Stop.success,words.flatMap wordBytes)
        | .success data => (.success,data)
        | .reverted data => (.reverted,data)
        | .invalid => (.exception,[])
        | .outOfGas => (.outOfGas,[])
      if !samples.isEmpty || result.messageGas.length != budgets.length ||
          (price.any (fun p => !p.paid) && !(activePrice.isSome && stop == .outOfGas)) then
        .error "emitted call ledger does not match MIR execution" else
      return .inl ⟨stop,data,result.world,state.exitGas.getD 0,0⟩

def runMirExit (cfg : MIR.State.Config) (functions : Functions) (caller : Caller) (fuel : Nat)
    (physical : ByteCut) (cut : MirCut) : EVM.State.Run :=
  Trace.run (mirExitStep cfg functions caller (mirExitBudgets cfg functions physical cut)
    (helperCallPrice cfg functions physical cut)) fuel
    (mirExitStart cfg functions physical cut)

structure CallerBody where
  functions : Functions
  entry : Nat
  helper : Nat
  returned : List Operand

/-- Full ABI effects and exact remaining gas. Transaction refunds on these
paths still come from the physical storage trace. -/
def abiObservation (cfg : EVM.State.Config) (result : EVM.State.Result) : EVM.State.Observation :=
  {EVM.State.observe cfg .effects result with gas := result.gas}

/-- One observation for a complete ABI invocation through the certified helper
cut. MIR uses the physical prefix gas and prices the helper and its return
from the checked schedule. Private memory is local to each execution and is discarded on exit. -/
def fromAbi (cfg : EVM.State.Config) (mirCfg : MIR.State.Config) (initial : ByteCut)
    (pc : Nat) (selector : Word) (caller : Caller) (body : CallerBody)
    (byteFuel mirFuel exitFuel : Nat) (physical : Bool) : Except String EVM.State.Observation := do
  let cut ← runBytes pc byteFuel cfg initial
  let result ← if physical then (·.result) <$> runByteExit cfg exitFuel cut else do
    let mirCut ← mirPrefix mirCfg body.functions body.entry body.helper body.returned
      selector caller initial.world cut mirFuel
    runMirExit mirCfg body.functions caller exitFuel cut mirCut
  return abiObservation cfg (finishByte initial.world result)

theorem fromAbi_byte (cfg : EVM.State.Config) (mirCfg : MIR.State.Config) (initial : ByteCut)
    (pc : Nat) (selector : Word) (caller : Caller) (body : CallerBody)
    (byteFuel mirFuel exitFuel : Nat) (cut : ByteCut) (terminal : ByteExit)
    (enters : runBytes pc byteFuel cfg initial = .ok cut)
    (resume : runByteExit cfg exitFuel cut = .ok terminal) :
    fromAbi cfg mirCfg initial pc selector caller body byteFuel mirFuel exitFuel true =
      .ok (abiObservation cfg (finishByte initial.world terminal.result)) := by
  simp [fromAbi, enters, resume, Bind.bind, Except.bind, Pure.pure, Except.pure,
    Functor.map, Except.map]

theorem fromAbi_mir (cfg : EVM.State.Config) (mirCfg : MIR.State.Config) (initial : ByteCut)
    (pc : Nat) (selector : Word) (caller : Caller) (body : CallerBody)
    (byteFuel mirFuel exitFuel : Nat) (cut : ByteCut) (mirCut : MirCut)
    (terminal : EVM.State.Result)
    (enters : runBytes pc byteFuel cfg initial = .ok cut)
    (mirPrefix : TransferFromEntry.mirPrefix mirCfg body.functions body.entry body.helper body.returned
      selector caller initial.world cut mirFuel = .ok mirCut)
    (mirResume : runMirExit mirCfg body.functions caller exitFuel cut mirCut = .ok terminal) :
    fromAbi cfg mirCfg initial pc selector caller body byteFuel mirFuel exitFuel false =
      .ok (abiObservation cfg (finishByte initial.world terminal)) := by
  simp [fromAbi, enters, mirPrefix, mirResume, Bind.bind, Except.bind, Pure.pure, Except.pure]

structure Invocation where
  cfg : EVM.State.Config
  mirCfg : MIR.State.Config
  initial : ByteCut
  pc : Nat
  selector : Word
  caller : Caller
  before : CallerBody
  after : CallerBody
  byteFuel : Nat
  mirFuel : Nat
  exitFuel : Nat

def Invocation.Verified (entry : Invocation) : Prop :=
  let execute := fromAbi entry.cfg entry.mirCfg entry.initial entry.pc entry.selector entry.caller
  ∃ observation,
    execute entry.before entry.byteFuel entry.mirFuel entry.exitFuel true = .ok observation ∧
    execute entry.before entry.byteFuel entry.mirFuel entry.exitFuel false = .ok observation ∧
    execute entry.after entry.byteFuel entry.mirFuel entry.exitFuel false = .ok observation

theorem liveIns (caller : Caller) (suffix : Suffix) (returnPC : Word) (args : Inputs) (other : Word)
    (stack arguments returns : List Word)
    (checked : related caller suffix returnPC args other stack arguments returns) :
    ∃ rest, stack = returnPC :: (argumentWords args ++ rest) ∧
      suffix.publication.words rest = .ok (liveOperands caller args.amount other) := by
  obtain ⟨_,returned,head,publish⟩ := checked
  refine ⟨stack.drop 5,?_,?_⟩
  · have all := List.take_append_drop 5 stack
    rw [head] at all
    exact all.symm
  · rw [← returned]
    cases w : suffix.publication.words (stack.drop 5) <;> simp_all [Except.toOption]

def snapshot (cut : ByteCut) : Lean.Json :=
  Lean.Json.mkObj [
      ("pc",Lean.toJson cut.frame.pc),
      ("pcs",Lean.toJson cut.pcs),
      ("samples",Lean.toJson cut.samples),
      ("budgets",Lean.toJson cut.budgets),
      ("gas",Lean.toJson cut.frame.gas),
      ("refund",Lean.toJson cut.world.refund),
      ("stack",Lean.toJson (cut.frame.stack.map (fun word => toString word.toNat))),
      ("memory",Lean.toJson (cut.frame.memory.map BitVec.toNat)),
      ("returndata",Lean.toJson (cut.frame.returnData.map BitVec.toNat))]

private def words (values : List Word) : Lean.Json :=
  Lean.toJson (values.map fun word => toString word.toNat)

def described (bytes : Except String ByteCut) (before after : ByteCut → Except String MirCut) :
    Except String Lean.Json := do
  let cut ← bytes
  let a ← before cut
  let b ← after cut
  let matchesEffects (mir : MirCut) :=
    decide (liveAccounts cut.world = liveAccounts mir.frame.world ∧
      cut.world.logs = mir.frame.world.logs ∧ cut.frame.returnData = mir.frame.returnData)
  if !(matchesEffects a && matchesEffects b) then
    .error "MIR and bytecode entry effects differ" else
  return Lean.Json.mkObj [
    ("bytecode",snapshot cut),
    ("before",Lean.toJson [words a.args,words a.returns]),
    ("after",Lean.toJson [words b.args,words b.returns])]

def describe (bytes : Except String ByteCut) (before after : ByteCut → Except String MirCut) : String :=
  match described bytes before after with
  | .ok value => value.compress
  | .error reason => "unsupported: " ++ reason

end SolarTV.MIR.TransferFromEntry
