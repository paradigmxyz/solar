import SolarTV.EVMIR.Transforms.MessageBoundary
import SolarTV.MIR.Transforms.FinalCallBudget
import SolarTV.MIR.Transforms.LowerStaticWord

/-! At the prepared recovery message, the emitted frame supplies its own
remaining gas. Both models pay for the same two memory extents and execute
the same child at the EIP-150 allowance. Work and returned gas are live. -/

namespace SolarTV.MIR.StaticCallCut
open EvmRules SolarTV.Execution MIR.State
open MIR.FinalCallBudget (withBudget)
open EVM.CallBudget (charged charged_add)

def project (frame : Frame)
    (step : Except String (EVM.State.Result ⊕ (Nat × Journal × EVM.State.Frame))) : MessageStep := do
  match ← step with
  | .inl result =>
    if result.stop = .outOfGas then return none
    else .error "unexpected terminal message action"
  | .inr (work,world,next) =>
    let value ← required next.stack.head? "missing message result"
    return some (.returned (some value,{frame with
      world, memory := next.memory, returnData := next.returnData,
      messageGas := frame.messageGas ++ [next.gas]}),work)

theorem prepared (cfg : Config) (ext : EVM.State.Config) (fuel : Nat)
    (mir : Frame) (physical : EVM.State.Frame) (rest : List Word)
    (target offset length : Word) (input active : List Byte)
    (fork : cfg.fork = .cancun) (extFork : ext.fork = .cancun)
    (bound : cfg.externalConfig = some ext)
    (environment : physical.environment = cfg.environment)
    (memory : physical.memory = mir.memory)
    (known : ext.unknownCode ≠ some (target.setWidth 160))
    (limit : physical.gas ≤ 30000000)
    (inputOk : expand mir.memory offset.toNat length.toNat = .ok input)
    (outputOk : expand input 0 32 = .ok active) :
    project mir (do
      let action ← EVM.MessageBoundary.staticAction ext mir.world physical physical.gas
        target offset length 0 32 rest
      EVM.State.advance ext (EVM.State.run fuel ext) action) =
    external fuel (withBudget cfg mir physical.gas) mir 0xfa [target,offset,length,0,32]
      (some (BitVec.ofNat 256 physical.gas)) := by
  let address := target.setWidth 160
  let code : EVM.State.ExecutionCode := ⟨address,(account mir.world address).code,false⟩
  let message : Calls.Message := ⟨.staticcall,address,0,readBytes active offset.toNat length.toNat⟩
  have addressEq : target.setWidth 160 = address := rfl
  have codeAddress : code.address = address := rfl
  have knownAddress : ext.unknownCode ≠ some address := known
  have codeEq : EVM.State.executionCode .cancun mir.world address =
      (code,0,warmAddress mir.world address) := by
    simp [code, EVM.State.executionCode, EVM.State.delegationTarget, Fork.atLeast, Fork.ordinal]
  have available : (BitVec.ofNat 256 physical.gas).toNat = physical.gas := by
    simp [BitVec.toNat_ofNat, Nat.mod_eq_of_lt (by omega : physical.gas < 2^256)]
  have selected : (List.replicate (mir.messageGas.length+1) physical.gas)[mir.messageGas.length]? =
      some physical.gas := by simp
  have current : (withBudget cfg mir physical.gas).fork = .cancun := fork
  have configured : (withBudget cfg mir physical.gas).externalConfig = some ext := bound
  have old : Fork.cancun.atLeast .amsterdam = false := by decide
  have inputPhysical : expand physical.memory offset.toNat length.toNat = .ok input :=
    memory ▸ inputOk
  simp only [EVM.MessageBoundary.staticAction, extFork, addressEq, codeEq, codeAddress,
    beq_iff_eq, knownAddress, ite_false, show (0 : Word).toNat = 0 from rfl,
    show (32 : Word).toNat = 32 from rfl]
  rw [CallDebit.prepare_eq ext mir.world physical ⟨.staticcall,address,0,[]⟩ 0xfa 0 input active
    offset.toNat length.toNat 0 32 inputPhysical outputOk]
  simp [external, configured, current, old, required, available, Nat.not_lt.mpr limit,
    inputOk, outputOk, codeEq, addressEq,
    Bind.bind, Except.bind, Pure.pure, Except.pure]
  -- Pricing depends on the destination, value and memory, independently of
  -- whether the message's bytes have been read yet.
  let price := EVM.State.callBoundaryCharge ext mir.world message 0xfa 0
    (memoryWords mir.memory) (memoryWords active)
  have pricing :
      EVM.State.callBoundaryCharge ext mir.world ⟨.staticcall,address,0#256,[]⟩ 0xfa 0
        (memoryWords physical.memory) (memoryWords active) = price := by
    rw [memory]
    rfl
  have messageEq :
      (⟨.staticcall,address,0#256,readBytes active offset.toNat length.toNat⟩ : Calls.Message) =
        message := rfl
  rw [pricing]
  rw [messageEq]
  simp only [callBudget, withBudget, selected, required, Bind.bind, Except.bind,
    Option.map_some, Option.getD_some, fork, charged, available]
  have priceEq : EVM.State.callBoundaryCharge ext mir.world message 0xfa 0
      (memoryWords mir.memory) (memoryWords active) = price := rfl
  simp only [priceEq]
  by_cases paid : price ≤ physical.gas
  · simp only [paid, ite_true, Nat.not_lt.mpr paid, ite_false]
    let child := (physical.gas-price)-(physical.gas-price)/64
    have child_le : child ≤ physical.gas-price := Nat.sub_le ..
    have cap : min physical.gas child = child := Nat.min_eq_right (by omega)
    have forward : Calls.forwardGas .cancun (physical.gas-price) physical.gas false =
        some ⟨child,child⟩ := by
      simp [Calls.forwardGas, Fork.atLeast, Fork.ordinal]
      exact cap
    have noValue : Calls.hasValue message = false := rfl
    have kind : message.kind = .staticcall := rfl
    have destination : message.destination = address := rfl
    have returnedGas : physical.gas-price-child+child = physical.gas-price := by omega
    by_cases depth : 1024 ≤ cfg.environment.depth
    · simp [project, EVM.State.advance, forward, CallDebit.warmTwice, environment, kind, noValue,
        destination, EVM.State.executeCall, codeAddress, knownAddress, depth, returnedGas, required,
        Bind.bind, Except.bind, Pure.pure, Except.pure, writeBytes]
    · simp [project, EVM.State.advance, forward, CallDebit.warmTwice, environment, kind, noValue,
        extFork, destination, depth, required, Bind.bind, Except.bind, Pure.pure, Except.pure]
      cases EVM.State.executeCall (EVM.State.run fuel ext) ext (warmAddress mir.world address)
        cfg.environment message code child <;> rfl
  · simp [paid, Nat.lt_of_not_ge paid, project, EVM.State.stepExhausted, EVM.State.advance,
      Bind.bind, Except.bind, Pure.pure, Except.pure]

def priced (cfg : Config) (mir : Frame) (physical : EVM.State.Frame) : Config :=
  withBudget {cfg with layout := {cfg.layout with gasBefore := some physical.gas}}
    mir (physical.gas-2)

/-- Match the semantic request after the actual GAS step has supplied the
budget. No caller-supplied price or sample participates in this equality. -/
theorem recovery (cfg : Config) (ext : EVM.State.Config) (fuel : Nat)
    (mir : Frame) (physical : EVM.State.Frame) (rest : List Word)
    (address inputArg : Operand) (offset length : Scalar) (scratch input active : List Byte)
    (readAddress : read mir address = .ok 1)
    (readSlice : readValue mir inputArg = .ok (.aggregate [.scalar offset,.scalar length]))
    (available : cfg.available.contains 0xfa = true)
    (fork : cfg.fork = .cancun) (extFork : ext.fork = .cancun)
    (bound : cfg.externalConfig = some ext)
    (environment : physical.environment = cfg.environment)
    (known : ext.unknownCode ≠ some 1)
    (paid : 2 ≤ physical.gas) (limit : physical.gas-2 ≤ 30000000)
    (cleared : expand mir.memory 0 32 = .ok scratch)
    (memory : physical.memory = writeBytes scratch 0 (wordBytes 0))
    (inputOk : expand physical.memory offset.word.toNat length.word.toNat = .ok input)
    (outputOk : expand input 0 32 = .ok active) :
    let call := EVM.MessageBoundary.afterGas physical
    let before := {mir with memory := physical.memory}
    project before (do
      let action ← EVM.MessageBoundary.staticAction ext mir.world call call.gas
        1 offset.word length.word 0 32 rest
      EVM.State.advance ext (EVM.State.run fuel ext) action) =
    LowerReturndata.invoke (priced cfg mir physical) fuel
      (messageArguments (priced cfg mir physical) mir (.address 0xfa false false true true true)
        [address,inputArg]) := by
  let budgets := List.replicate (mir.messageGas.length+1) (physical.gas-2)
  have selected : budgets[mir.messageGas.length]? = some (physical.gas-2) := by simp [budgets]
  have old : (priced cfg mir physical).fork.atLeast .amsterdam = false := by
    simp [priced, withBudget, fork, Fork.atLeast, Fork.ordinal]
  have eip150 : (priced cfg mir physical).fork.atLeast .tangerineWhistle = true := by
    simp [priced, withBudget, fork, Fork.atLeast, Fork.ordinal]
  dsimp only
  rw [LowerStaticWord.staticWord (priced cfg mir physical) fuel mir address inputArg 1 offset length readAddress readSlice
    available old eip150 physical.gas (physical.gas-2) budgets rfl paid (Nat.le_refl _) limit
    rfl selected]
  simp [LowerStaticWord.lowered, memoryOperation, layoutOperation, priced, withBudget,
    cleared, Nat.not_lt.mpr paid, required, effect, Bind.bind, Except.bind]
  have memory' : physical.memory = writeBytes scratch 0 (wordBytes 0#256) := memory
  rw [← memory']
  exact prepared {cfg with layout := {cfg.layout with gasBefore := some physical.gas}}
    ext fuel {mir with memory := physical.memory} (EVM.MessageBoundary.afterGas physical)
    rest 1 offset.word length.word input active fork extFork bound environment rfl known limit
    inputOk outputOk

/-- Reassemble the live physical continuation from the typed result and the
untouched stack. A failed caller charge stops the frame before any child. -/
def resumeWith (fuel : Nat)
    (proceed : (remaining : Nat) → remaining ≤ fuel → EVM.State.ChildExecutor) (frame : EVM.State.Frame)
    (world : Journal) (rest : List Word) (message : MessageStep) : EVM.State.Run := do
  match ← message with
  | none => return ⟨.outOfGas,[],world,0,2⟩
  | some (.returned (some flag,mir),work) =>
    let gas ← required mir.messageGas.getLast? "missing recovery continuation gas"
    let result ← proceed (fuel-work) (Nat.sub_le ..) mir.world
      {frame with
        pc := frame.pc+2, stack := flag :: rest
        memory := mir.memory, returnData := mir.returnData, gas := gas}
    return {result with work := result.work + work + 2}
  | _ => .error "unexpected typed recovery result"

def resume (cfg : EVM.State.Config) (fuel : Nat) (frame : EVM.State.Frame)
    (world : Journal) (rest : List Word) (message : MessageStep) : EVM.State.Run :=
  resumeWith fuel (fun remaining _ => EVM.State.run remaining cfg) frame world rest message

theorem resume_eq (cfg : EVM.State.Config) (fuel : Nat) (frame : EVM.State.Frame)
    (mir : Frame) (rest : List Word) (target offset length : Word)
    (fork : cfg.fork = .cancun)
    (sample : EVM.State.instruction cfg mir.world frame =
      .ok (.next mir.world (EVM.MessageBoundary.afterGas frame)))
    (action : EVM.State.instruction cfg mir.world (EVM.MessageBoundary.afterGas frame) =
      EVM.MessageBoundary.staticAction cfg mir.world (EVM.MessageBoundary.afterGas frame)
        (frame.gas-2) target offset length 0 32 rest) :
    resume cfg fuel frame mir.world rest (project mir (do
      let signal ← EVM.State.instruction cfg mir.world (EVM.MessageBoundary.afterGas frame)
      EVM.State.advance cfg (EVM.State.run fuel cfg) signal)) =
    EVM.State.run (fuel+2) cfg mir.world frame := by
  rw [EVM.State.run_next (fuel+1) sample]
  rw [EVM.State.run]
  simp only [action]
  let atGas := EVM.MessageBoundary.afterGas frame
  let dest := target.setWidth 160
  let execution := EVM.State.executionCode cfg.fork mir.world dest
  let code := execution.1
  let resolution := execution.2.1
  let entered := execution.2.2
  let charge := EVM.State.callBoundaryCharge cfg mir.world
    ⟨.staticcall,dest,0,[]⟩ 0xfa resolution 0 0
  let first := charge + EVM.State.memoryCharge frame.memory offset.toNat length.toNat
  have callShape : EVM.MessageBoundary.staticAction cfg mir.world (EVM.MessageBoundary.afterGas frame) (frame.gas-2)
      target offset length 0 32 rest =
      (if cfg.unknownCode == some code.address then .error "MIR root runtime code is unknown"
      else if atGas.gas < first then EVM.State.stepExhausted mir.world else do
        let input ← expand frame.memory offset.toNat length.toNat
        let second := EVM.State.memoryCharge input 0 32
        if atGas.gas-first < second then EVM.State.stepExhausted mir.world else do
          let active ← expand input 0 32
          return .call ⟨.staticcall,dest,0,readBytes active offset.toNat length.toNat⟩
            0 32 (frame.gas-2) code entered
            {frame with pc := frame.pc+2, stack := rest, memory := active,
                        gas := atGas.gas-first-second}) := by
    simp [EVM.MessageBoundary.staticAction, EVM.State.withMemory, EVM.MessageBoundary.afterGas,
      atGas, code, resolution, entered, execution, dest, first, charge, Nat.add_assoc,
      Bind.bind, Except.bind, Pure.pure, Except.pure]
  change resume cfg fuel frame mir.world rest (project mir (do
      let signal ← EVM.MessageBoundary.staticAction cfg mir.world (EVM.MessageBoundary.afterGas frame) (frame.gas-2)
        target offset length 0 32 rest
      EVM.State.advance cfg (EVM.State.run fuel cfg) signal)) = _
  simp only [callShape]
  simp only [Bind.bind, Except.bind, Pure.pure, Except.pure]
  by_cases known : (cfg.unknownCode == some code.address) = true
  · simp [known, resume, resumeWith, project, Bind.bind, Except.bind]
  simp only [known]
  by_cases paid : atGas.gas < first
  · simp [paid, resume, resumeWith, project, EVM.State.stepExhausted, EVM.State.advance, Bind.bind, Except.bind, Pure.pure, Except.pure]
  simp only [paid, ite_false]
  cases expand frame.memory offset.toNat length.toNat with
  | error err => simp [resume, resumeWith, project, Bind.bind, Except.bind]
  | ok input =>
    by_cases outputPaid : atGas.gas-first < EVM.State.memoryCharge input 0 32
    · simp [outputPaid, resume, resumeWith, project, EVM.State.stepExhausted, EVM.State.advance, Bind.bind, Except.bind, Pure.pure, Except.pure]
    simp only [outputPaid, ite_false]
    cases expand input 0 32 with
    | error err => simp [resume, resumeWith, project, Bind.bind, Except.bind]
    | ok active =>
      let message : Calls.Message := ⟨.staticcall,dest,0,readBytes active offset.toNat length.toNat⟩
      let gas := atGas.gas-first-EVM.State.memoryCharge input 0 32
      let requested := frame.gas-2
      let supplied := min requested (gas-gas/64)
      have cap : Calls.forwardGas cfg.fork gas requested false = some ⟨supplied,supplied⟩ := by
        simp [fork, Calls.forwardGas, Fork.atLeast, Fork.ordinal, supplied]
      have noValue : Calls.hasValue message = false := rfl
      by_cases depth : 1024 ≤ frame.environment.depth
      · simp [EVM.State.advance, depth, Calls.hasValue, resume, resumeWith, project, required,
          Bind.bind, Except.bind, Pure.pure, Except.pure]
        cases EVM.State.run fuel cfg (warmAddress entered dest) _ <;> rfl
      · have cap' : Calls.forwardGas cfg.fork (atGas.gas-first-EVM.State.memoryCharge input 0 32)
            (frame.gas-2) false = some ⟨supplied,supplied⟩ := cap
        simp only [EVM.State.advance, depth, decide_false, Bool.false_eq_true, ite_false,
          show (Calls.Kind.staticcall == .call) = false from rfl,
          show (Calls.Kind.staticcall == .callcode) = false from rfl,
          Bool.or_false, Bool.false_and, show Calls.hasValue
            ⟨.staticcall,dest,0,readBytes active offset.toNat length.toNat⟩ = false from noValue,
          cap', Bind.bind, Except.bind, Pure.pure, Except.pure]
        cases EVM.State.executeCall (EVM.State.run fuel cfg) cfg (warmAddress entered dest)
          frame.environment message code supplied <;>
          simp [resume, resumeWith, project, required, Bind.bind, Except.bind, Pure.pure, Except.pure]
        split <;> simp_all

end SolarTV.MIR.StaticCallCut
