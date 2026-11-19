import SolarTV.MIR.State.Interpreter
import SolarTV.MIR.Transforms.TransferFromCallers
import SolarTV.State.Trace
import SolarTV.State.CheckedHashes
import SolarTV.EVMIR.State.CodeChunks
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

/-- Reaching the cut consumes no instruction. A continuing transition spends
one unit of root fuel and evaluates any child with the remaining fuel. -/
private def byteStepUsing (pc fuel : Nat) (cfg : EVM.State.Config) (code : List Byte)
    (instruction : Journal → EVM.State.Frame → EVM.State.Action) (readByte : Nat → Option Byte)
    (state : ByteCut) : Except String (ByteCut ⊕ ByteCut) := do
  let ⟨frame,world,samples,budgets,pcs⟩ := state
  let frame := {frame with code}
  if frame.pc = pc then return .inl ⟨frame,world,samples.reverse,budgets.reverse,pcs.reverse⟩
  let signal ← instruction world frame
  let budgets := match signal with
    | .call .. => frame.gas :: budgets
    | _ => budgets
  let samples := if readByte frame.pc == some 0x5a then frame.gas :: samples else samples
  match ← EVM.State.advance cfg (EVM.State.run fuel cfg) signal with
  | .inl _ => .error "bytecode terminated before the helper"
  | .inr (_,nextWorld,nextFrame) =>
    return .inr ⟨nextFrame,nextWorld,samples,budgets,frame.pc :: pcs⟩

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
  unfold chunkedByteStep byteStep byteStepUsing EVM.State.instruction
  have immediate (i : Nat) : code.immediate i = EVM.State.pushValue code.bytes i :=
    funext (code.immediate_eq i)
  simp only [EVM.State.CodeChunks.get_eq, immediate,
    EVM.State.CodeChunks.jump_eq, ← EVM.State.CodeChunks.size_eq]

def runBytes (pc : Nat) (fuel : Nat) (cfg : EVM.State.Config) (state : ByteCut) :
    Except String ByteCut :=
  Trace.run (byteStep pc cfg state.frame.code) fuel state

structure MirCut where
  args : List Word
  returns : List Word
  frame : MIR.State.Frame

def instAt (functions : Functions) (cursor : Cursor) : Option Instruction := do
  let body ← (functions.find? (·.1 == cursor.function)).map Prod.snd
  let block ← body.blocks[cursor.block]?
  if cursor.entered then block.instructions[cursor.next]? else none

structure MirCursor where
  machine : Machine
  samples : List Nat

/-- Gas samples and per-message pricing come from the physical prefix. MIR
prices child messages, and its intervening SSA instructions remain unpriced. -/
def mirStep (helper : Nat) (returned : List Operand) (fuel : Nat) (cfg : MIR.State.Config)
    (functions : Functions) (state : MirCursor) (hash : List Byte → Word := Keccak.hash) :
    Except String (MirCut ⊕ MirCursor) := do
  let ⟨machine,samples⟩ := state
  let site := instAt functions machine.cursor
  if let some ⟨_,_,_,.call callee,callOperands⟩ := site then
    if callee = helper then
      if !machine.stack.isEmpty then .error "unexpected nested caller at helper cut" else
      return .inl ⟨← callOperands.mapM (read machine.frame), ← returned.mapM (read machine.frame), machine.frame⟩
  let (stepCfg,samples) ← match site with
    | some ⟨_,_,_,.context .gasSample,_⟩ => do
      let sample ← required samples.head? "missing physical GAS sample"
      pure ({cfg with layout := {cfg.layout with gasBefore := some sample}},samples.drop 1)
    | _ => pure (cfg,samples)
  match ← transition fuel stepCfg functions machine hash with
  | .finished _ => .error "MIR terminated before the helper"
  | .next _ next => return .inr ⟨next,samples⟩

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

/-- Morpho's caller signatures have a static MarketParams tuple (except
flashLoan), scalar head values, and a final calldata bytes parameter. -/
def decode (selector : Word) (caller : Caller) (cfg : MIR.State.Config)
    (journal : Journal) : Except String MIR.State.Frame := do
  let input := cfg.environment.input
  let hasMarket := match caller with | .flashLoan => false | _ => true
  let widths : List Nat := match caller with
    | .supply | .repay => [256,256,160]
    | .supplyCollateral => [256,160]
    | .liquidate => [160,256,256]
    | .flashLoan => [160,256]
  let scalarBase := if hasMarket then 164 else 4
  let offsetAt := scalarBase + 32 * widths.length
  if input.length < offsetAt + 32 || (readWord input 0 >>> 224) != selector then
    .error "bad Morpho selector or ABI head" else
  let tuple := if hasMarket then (List.range 5).map (fun i => readWord input (4+32*i)) else []
  let values := widths.zipIdx.map fun (width,i) => (width,readWord input (scalarBase+32*i))
  if !(tuple.take 4 ++ (values.filterMap fun (width,value) => if width == 160 then some value else none)).all
      (·.toNat < 2^160) then .error "noncanonical Morpho address" else
  let offset := (readWord input offsetAt).toNat
  let start := 4 + offset + 32
  let length := (readWord input (4+offset)).toNat
  if start > input.length || length > input.length-start then .error "bad Morpho calldata slice" else
  let memory ← expand [] 0 (if hasMarket then 288 else 96)
  let frameMemory := writeBytes memory 64 (wordBytes (if hasMarket then 288 else 128))
  let frameMemory := tuple.zipIdx.foldl
    (fun memory (word,index) => writeBytes memory (128+32*index) (wordBytes word)) frameMemory
  let arguments := (if hasMarket then [scalar 256 128] else []) ++
    values.map (fun (width,word) => scalar width word) ++
    [.aggregate [scalar 256 (BitVec.ofNat 256 start),scalar 256 (BitVec.ofNat 256 length)]]
  return ⟨journal,frameMemory,arguments,[],[],0,0,[]⟩

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
  return ⟨⟨{function := entry},[],initial⟩,priced.samples⟩

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
