import SolarTV.EVMIR.State.GasTrace
import SolarTV.MIR.Transforms.CallDebit
import SolarTV.MIR.Transforms.TransferFromEntry

/-! Relate a staged zero-value CALL to the corresponding external instruction
in lowered MIR. The arguments and covered memory are supplied by the caller
certificate. Both interpreters execute the actual child using the common
journal, prepared calldata, and forwarded gas. -/

namespace SolarTV.MIR.CallHandoff
open EvmRules SolarTV.Execution
set_option linter.unusedSimpArgs false

structure Request where
  gas : Word
  destination : Word
  offset : Word
  length : Word

def Request.args (request : Request) : List Word :=
  [request.destination, 0, request.offset, request.length, 0, 0]

def Request.payload (request : Request) (memory : List Byte) : List Byte :=
  readBytes memory request.offset.toNat request.length.toNat

def Request.message (request : Request) (memory : List Byte) : Calls.Message :=
  ⟨.call, request.destination.setWidth 160, 0, request.payload memory⟩

def charge (cfg : EVM.State.Config) (request : Request) (world : Journal) : Nat :=
  EVM.State.callBoundaryCharge cfg world (request.message []) 241 0 0 0

def destination (request : Request) (world : Journal) : EVM.State.ExecutionCode :=
  ⟨request.destination.setWidth 160, (account world (request.destination.setWidth 160)).code, false⟩

def entered (request : Request) (world : Journal) : Journal :=
  warmAddress world (request.destination.setWidth 160)

theorem resolved (cfg : EVM.State.Config) (fork : cfg.fork = .cancun)
    (request : Request) (world : Journal) :
    EVM.State.executionCode cfg.fork world (request.destination.setWidth 160) =
      (destination request world,0,entered request world) := by
  simp [EVM.State.executionCode, EVM.State.delegationTarget, destination, entered,
    fork, Fork.atLeast, Fork.ordinal]

theorem charge_input (cfg : EVM.State.Config) (request : Request) (world : Journal)
    (memory : List Byte) (payload : List Byte) (words : Nat) :
    EVM.State.callBoundaryCharge cfg world
        {request.message memory with input := payload} 241 0 words words =
      charge cfg request world := by
  unfold charge
  unfold EVM.State.callBoundaryCharge Request.message
  dsimp only
  simp [Calls.hasValue, EVM.State.memoryCost, Nat.add_sub_cancel]

def forward (request : Request) (available : Nat) : Nat :=
  min request.gas.toNat (available - available/64)

theorem forward_eq (cfg : EVM.State.Config) (cancun : cfg.fork = .cancun)
    (request : Request) (memory : List Byte) (available : Nat) :
    Calls.forwardGas cfg.fork available request.gas.toNat (Calls.hasValue (request.message memory)) =
      some ⟨forward request available,forward request available⟩ := by
  simp [Calls.forwardGas, forward, cancun, Fork.atLeast, Fork.ordinal,
    Request.message, Calls.hasValue]

def mirResult (frame : MIR.State.Frame) (retained : Nat)
    (child : EVM.State.MessageOutcome) : Outcome (Option Word × MIR.State.Frame) × Nat :=
  (.returned (some child.value,{frame with
    world := child.world
    returnData := child.data
    messageGas := frame.messageGas ++ [retained+child.gas]}), child.work)

/-- The physical CALL budget can price the same MIR operation with an already
staged input. Unpaid access stops before any child runs. -/
theorem external (fuel : Nat) (cfg : MIR.State.Config) (ext : EVM.State.Config)
    (frame : MIR.State.Frame) (request : Request) (incoming : Nat) (budgets : List Nat)
    (fork : cfg.fork = .cancun) (externalFork : ext.fork = .cancun)
    (externalCfg : cfg.externalConfig = some ext)
    (ledger : cfg.messageGas = some budgets) (selected : budgets[frame.messageGas.length]? = some incoming)
    (valid : request.gas.toNat ≤ 30000000)
    (input : expand frame.memory request.offset.toNat request.length.toNat = .ok frame.memory)
    (output : expand frame.memory 0 0 = .ok frame.memory) :
    MIR.State.external fuel cfg frame 241 request.args (some request.gas) = (do
      let price := charge ext request frame.world
      if incoming < price then return none
      else
        let available := incoming-price
        let supplied := forward request available
        let child ← EVM.State.executeCall (EVM.State.run fuel ext) ext
          (entered request frame.world) cfg.environment (request.message frame.memory)
          (destination request frame.world) supplied
        return some (mirResult frame (available-supplied) child)) := by
  have resolution : EVM.State.executionCode cfg.fork frame.world (request.destination.setWidth 160) =
      (destination request frame.world,0,entered request frame.world) := by
    rw [fork, ← externalFork]
    exact resolved ext externalFork request frame.world
  have noWrite : writeBytes frame.memory 0 [] = frame.memory :=
    writeBytes_existing _ _ _ (readBytes_zero _ _)
  have forwards := forward_eq ext externalFork request frame.memory
  have signature : (⟨Calls.Kind.call,request.destination.setWidth 160,0,
        readBytes frame.memory request.offset.toNat request.length.toNat⟩ : Calls.Message) =
      request.message frame.memory := rfl
  have fee : EVM.State.callBoundaryCharge ext frame.world (request.message frame.memory) 241 0
      (memoryWords frame.memory) (memoryWords frame.memory) = charge ext request frame.world := by
    exact charge_input ext request frame.world frame.memory (request.payload frame.memory) _
  simp (config := { decide := true }) only [MIR.State.external, externalCfg, MIR.State.required, Bind.bind, Except.bind,
    show cfg.fork.atLeast .amsterdam = false by simp [fork, Fork.atLeast, Fork.ordinal],
    Bool.false_eq_true, ite_false, ite_true, decide_true, decide_false, Bool.true_or, Request.args, List.length_cons, List.length_nil,
    Nat.reduceAdd, List.getElem?_cons_succ, List.getElem?_cons_zero, Option.getD_some,
    Option.any_some, Option.isNone_some, Bool.false_and, Nat.not_lt_of_ge valid,
    show ((0 : Word) != 0) = false by decide, Bool.and_false,
    show (0 : Word).toNat = 0 by rfl, input, output, resolution, signature]
  simp only [MIR.State.callBudget, ledger, selected, MIR.State.required, fee, Bind.bind,
    Except.bind, Pure.pure, Except.pure, Option.map_some, Option.getD_some, fork, ← externalFork]
  by_cases short : incoming < charge ext request frame.world
  · simp [short]
  · simp only [short, ite_false, ite_true, forwards, Option.map_some, Option.isSome_some, List.take_zero,
      noWrite, mirResult, Bind.bind, Except.bind, Pure.pure, Except.pure]

/-- The physical instruction validates exactly the provided stack operands.
Both expansion debits are zero because the encoder already staged the input. -/
theorem prepare (cfg : EVM.State.Config) (frame : EVM.State.Frame) (journal : Journal)
    (request : Request) (rest : List Word) (size : Nat) (immediate : Nat → Word)
    (jump : Word → Bool)
    (fork : cfg.fork = .cancun) (known : cfg.unknownCode = none)
    (schema : EVM.State.available cfg 241 = ⟨true,2600,0,7,1⟩)
    (inImage : frame.pc < size) (shape : frame.stack = request.gas :: (request.args ++ rest))
    (height : rest.length ≤ 1017)
    (input : expand frame.memory request.offset.toNat request.length.toNat = .ok frame.memory)
    (output : expand frame.memory 0 0 = .ok frame.memory) :
    EVM.State.instructionWithImage cfg journal frame size 241 immediate jump =
      if frame.gas < charge cfg request journal then EVM.State.stepExhausted journal
      else .ok (.call (request.message frame.memory) 0 0 request.gas.toNat
        (destination request journal) (entered request journal)
        {frame with stack := rest, pc := frame.pc+1, gas := frame.gas-charge cfg request journal}) := by
  have covered : EVM.State.memoryCharge frame.memory request.offset.toNat request.length.toNat = 0 :=
    CallDebit.memoryCharge_zero frame.memory _ _ input
  have noOutput : EVM.State.memoryCharge frame.memory 0 0 = 0 :=
    CallDebit.memoryCharge_zero frame.memory _ _ output
  have resolution := resolved cfg fork request journal
  rw [fork] at resolution
  have bigEnough : ¬ rest.length + 1 > 1024 := by omega
  have enough : ¬ rest.length+1+1+1+1+1+1+1 < 7 := by omega
  have tariff : EVM.State.callBoundaryCharge cfg journal
      (⟨.call,request.destination.setWidth 160,0#256,[]⟩ : Calls.Message) 241 0 0 0 =
      charge cfg request journal :=
    charge_input cfg request journal frame.memory [] 0
  simp [EVM.State.instructionWithImage, EVM.State.pureOp, Request.args, shape, schema,
    EVM.State.withMemory, Nat.not_le_of_gt inImage, fork, Fork.atLeast, Fork.ordinal,
    known, resolution, tariff, covered, noOutput, input, output, bigEnough, enough,
    Bind.bind, Except.bind, Request.message, Request.payload]

/-- A certificate of the opcode byte binds the CALL preparation equation to
the unmodified emitted image. -/
theorem prepareAt (cfg : EVM.State.Config) (code : EVM.State.CodeChunks)
    (frame : EVM.State.Frame) (journal : Journal) (request : Request) (rest : List Word)
    (fork : cfg.fork = .cancun) (known : cfg.unknownCode = none)
    (schema : EVM.State.available cfg 241 = ⟨true,2600,0,7,1⟩)
    (inImage : frame.pc < code.size) (found : code.get? frame.pc = some 241)
    (image : frame.code = code.bytes) (shape : frame.stack = request.gas :: (request.args ++ rest))
    (height : rest.length ≤ 1017)
    (input : expand frame.memory request.offset.toNat request.length.toNat = .ok frame.memory)
    (output : expand frame.memory 0 0 = .ok frame.memory) :
    EVM.State.instruction cfg journal frame =
      if frame.gas < charge cfg request journal then EVM.State.stepExhausted journal
      else .ok (.call (request.message frame.memory) 0 0 request.gas.toNat
        (destination request journal) (entered request journal)
        {frame with stack := rest, pc := frame.pc+1, gas := frame.gas-charge cfg request journal}) := by
  have unchanged : {frame with code := code.bytes} = frame := by cases frame; simp_all
  rw [← unchanged, code.instruction_eq, unchanged, found, Option.getD_some]
  exact prepare cfg frame journal request rest code.size (code.immediate frame.pc) _
    fork known schema inImage shape height input output

/-- A paid CALL runs the same executable child on the EVM side.
The returned word, journal, returndata, and retained gas are all outputs
of that run. The MIR-side equation is `external`. -/
theorem resume (fuel : Nat) (cfg : EVM.State.Config) (parent : EVM.State.Frame)
    (journal : Journal) (request : Request)
    (fork : cfg.fork = .cancun) (depth : parent.environment.depth < 1024) :
    EVM.State.advance cfg (EVM.State.run fuel cfg)
      (.call (request.message parent.memory) 0 0 request.gas.toNat
        (destination request journal) (entered request journal) parent) = (do
      let supplied := forward request parent.gas
      let child ← EVM.State.executeCall (EVM.State.run fuel cfg) cfg (entered request journal)
        parent.environment (request.message parent.memory) (destination request journal) supplied
      return .inr (child.work,child.world,{parent with
        stack := child.value :: parent.stack
        returnData := child.data
        gas := parent.gas-supplied+child.gas})) := by
  have admitted : (decide (parent.environment.depth ≥ 1024) ||
      (((request.message parent.memory).kind == .call ||
        (request.message parent.memory).kind == .callcode) &&
        (account (entered request journal) parent.environment.address).balance.ult
          (request.message parent.memory).value)) = false := by
    simp [Request.message, BitVec.ult, show ¬ 1024 ≤ parent.environment.depth by omega]
  have forwarded := forward_eq cfg fork request parent.memory parent.gas
  have stable : warmAddress (entered request journal) (request.message parent.memory).destination =
      entered request journal :=
    CallDebit.warmTwice journal (request.destination.setWidth 160)
  rw [EVM.State.GasTrace.advance_noOutput _ _ _ _ _ _ _ _ _ admitted forwarded, stable]

abbrev Returned := Word × Journal × List Byte × Nat × Nat

def physical (fuel : Nat) (cfg : EVM.State.Config) (frame : EVM.State.Frame) (journal : Journal) :
    Except String (Option Returned) := do
  let step ← EVM.State.instruction cfg journal frame
  match ← EVM.State.advance cfg (EVM.State.run fuel cfg) step with
  | .inl result =>
    if result.stop = .outOfGas then return none else .error "unexpected physical CALL exit"
  | .inr (work,world,next) =>
    let value ← MIR.State.required next.stack.head? "missing physical CALL result"
    return some (value,world,next.returnData,next.gas,work)

def lowered (fuel : Nat) (cfg : MIR.State.Config) (frame : MIR.State.Frame)
    (op : MIR.State.MessageOp) (operands : List MIR.State.Operand) :
    Except String (Option Returned) := do
  let (opcode,args,frame,requested) ← MIR.State.messageArguments cfg frame op operands
  match ← MIR.State.external fuel cfg frame opcode args requested with
  | none => return none
  | some (.returned (some value,next),work) =>
    let gas ← MIR.State.required next.messageGas.getLast? "missing MIR CALL gas"
    return some (value,next.world,next.returnData,gas,work)
  | _ => .error "unexpected MIR CALL exit"

def loweredAt (fuel : Nat) (cfg : MIR.State.Config) (frame : MIR.State.Frame)
    (functions : MIR.State.Functions) (site : MIR.State.Cursor) :
    Except String (Option Returned) := do
  let inst ← MIR.State.required (MIR.TransferFromEntry.instAt functions site) "missing MIR CALL"
  if inst.opcode.any (fun opcode => !cfg.available.contains opcode) then
    .error "unavailable MIR CALL"
  else
    match inst.operation with
    | .external op => lowered fuel cfg frame op inst.operands
    | _ => .error "expected external MIR instruction"

/-- Compare the child result at a decoded CALL and the matching MIR
operands. Private memory can differ outside the input. -/
theorem agrees (fuel : Nat) (cfg : MIR.State.Config) (ext : EVM.State.Config)
    (code : EVM.State.CodeChunks) (frame : EVM.State.Frame) (mir : MIR.State.Frame)
    (request : Request) (rest : List Word) (budgets : List Nat)
    (op : MIR.State.MessageOp) (operands : List MIR.State.Operand)
    (fork : cfg.fork = .cancun) (externalFork : ext.fork = .cancun)
    (externalCfg : cfg.externalConfig = some ext) (environment : cfg.environment = frame.environment)
    (depth : frame.environment.depth < 1024) (known : ext.unknownCode = none)
    (schema : EVM.State.available ext 241 = ⟨true,2600,0,7,1⟩)
    (inImage : frame.pc < code.size) (found : code.get? frame.pc = some 241)
    (image : frame.code = code.bytes) (shape : frame.stack = request.gas :: (request.args ++ rest))
    (height : rest.length ≤ 1017) (valid : request.gas.toNat ≤ 30000000)
    (input : expand frame.memory request.offset.toNat request.length.toNat = .ok frame.memory)
    (mirInput : expand mir.memory request.offset.toNat request.length.toNat = .ok mir.memory)
    (payload : request.payload mir.memory = request.payload frame.memory)
    (ledger : cfg.messageGas = some budgets) (selected : budgets[mir.messageGas.length]? = some frame.gas)
    (args : MIR.State.messageArguments cfg mir op operands = .ok (241,request.args,mir,some request.gas)) :
    physical fuel ext frame mir.world = lowered fuel cfg mir op operands := by
  have output (memory : List Byte)
      (covered : expand memory request.offset.toNat request.length.toNat = .ok memory) :
      expand memory 0 0 = .ok memory := by
    have words : memoryWords memory ≤ requiredWords memory request.offset.toNat request.length.toNat := by
      unfold requiredWords
      split <;> omega
    unfold expand at covered ⊢
    dsimp only at covered ⊢
    split at covered
    · contradiction
    · simp only [requiredWords, ite_true]
      have small : ¬ 32 * memoryWords memory > 65536 := by omega
      have size : 32 * memoryWords memory ≤ memory.length := by unfold memoryWords; omega
      simp [small, Nat.sub_eq_zero_of_le size]
  have message : request.message mir.memory = request.message frame.memory := by
    unfold Request.message
    rw [payload]
  unfold physical lowered
  rw [prepareAt ext code frame mir.world request rest externalFork known schema inImage found image
    shape height input (output _ input), args]
  simp only [Bind.bind, Except.bind]
  rw [external fuel cfg ext mir request frame.gas budgets fork externalFork externalCfg ledger selected
    valid mirInput (output _ mirInput), environment, message]
  by_cases short : frame.gas < charge ext request mir.world
  · simp [short, EVM.State.stepExhausted, EVM.State.advance, Bind.bind, Except.bind,
      Pure.pure, Except.pure]
  · simp only [short, ite_false, ite_true]
    have resumed := resume fuel ext
      {frame with stack := rest, pc := frame.pc+1, gas := frame.gas-charge ext request mir.world}
      mir.world request externalFork depth
    dsimp only at resumed
    rw [resumed]
    generalize EVM.State.executeCall (EVM.State.run fuel ext) ext (entered request mir.world)
      frame.environment (request.message frame.memory) (destination request mir.world)
      (forward request (frame.gas-charge ext request mir.world)) = child
    cases child <;> simp [Bind.bind, Except.bind, Pure.pure, Except.pure, mirResult,
      MIR.State.required, List.getLast?_append]

end SolarTV.MIR.CallHandoff
