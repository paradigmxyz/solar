import SolarTV.MIR.Transforms.StaticCallCut

/-! Match zero-value CALL at a reached physical frame to the typed message.
The requested gas can be an earlier GAS sample while the incoming budget is
the balance at CALL. Both models pay for the same input and output ranges. -/

namespace SolarTV.MIR.PreHelperCall
open EvmRules SolarTV.Execution MIR.State
open MIR.FinalCallBudget (withBudget)
open MIR.StaticCallCut (project)
open EVM.CallBudget (charged)

def zeroCall (cfg : EVM.State.Config) (journal : Journal) (physical : EVM.State.Frame)
    (request target input size out outSize : Word) (rest : List Word) : EVM.State.Action :=
  let destination := target.setWidth 160
  let (code,resolution,entered) := EVM.State.executionCode cfg.fork journal destination
  if cfg.unknownCode == some code.address then .error "MIR root runtime code is unknown" else
  EVM.State.withMemory journal physical
    (EVM.State.callBoundaryCharge cfg journal ⟨.call,destination,0,[]⟩ 0xf1 resolution 0 0)
    input.toNat size.toNat fun physical =>
  EVM.State.withMemory journal physical 0 out.toNat outSize.toNat fun physical =>
    .ok (.call ⟨.call,destination,0,readBytes physical.memory input.toNat size.toNat⟩
      out.toNat outSize.toNat request.toNat code entered
      {physical with stack := rest, pc := physical.pc+1})

theorem decodeCall (cfg : EVM.State.Config) (journal : Journal) (physical : EVM.State.Frame)
    (base : Nat) (request target input size out outSize : Word) (rest : List Word)
    (stack : physical.stack = request :: target :: 0 :: input :: size :: out :: outSize :: rest)
    (opcode : physical.code[physical.pc]? = some 0xf1)
    (schema : EVM.State.available cfg 0xf1 = ⟨true,base,0,7,1⟩)
    (fork : cfg.fork.atLeast .amsterdam = false)
    (space : rest.length + 7 ≤ 1024) :
    EVM.State.instruction cfg journal physical =
      zeroCall cfg journal physical request target input size out outSize rest := by
  obtain ⟨within,_⟩ := List.getElem?_eq_some_iff.mp opcode
  simp [EVM.State.instruction, EVM.State.instructionWithImage, opcode, schema,
    Nat.not_le_of_gt within, stack, fork, EVM.State.pureOp, zeroCall,
    show ¬ rest.length+1+1+1+1+1+1+1 < 7 by omega,
    show ¬ 1024 < rest.length+1 by omega]

theorem preparedCall (cfg : Config) (ext : EVM.State.Config) (work : Nat)
    (mir : Frame) (physical : EVM.State.Frame) (rest : List Word)
    (request target offset length out outSize : Word) (input active : List Byte)
    (fork : cfg.fork = .cancun) (extFork : ext.fork = .cancun)
    (bound : cfg.externalConfig = some ext)
    (environment : physical.environment = cfg.environment)
    (memory : physical.memory = mir.memory)
    (known : ext.unknownCode ≠ some (target.setWidth 160))
    (limit : request.toNat ≤ 30000000)
    (inputOk : expand mir.memory offset.toNat length.toNat = .ok input)
    (outputOk : expand input out.toNat outSize.toNat = .ok active) :
    project mir (do
      let action ← zeroCall ext mir.world physical request target offset length out outSize rest
      EVM.State.advance ext (EVM.State.run work ext) action) =
    external work (withBudget cfg mir physical.gas) mir 0xf1 [target,0,offset,length,out,outSize]
      (some request) := by
  let address := target.setWidth 160
  let code : EVM.State.ExecutionCode := ⟨address,(account mir.world address).code,false⟩
  let message : Calls.Message := ⟨.call,address,0,readBytes active offset.toNat length.toNat⟩
  have addressEq : target.setWidth 160 = address := rfl
  have codeAddress : code.address = address := rfl
  have knownAddress : ext.unknownCode ≠ some address := known
  have codeEq : EVM.State.executionCode .cancun mir.world address =
      (code,0,warmAddress mir.world address) := by
    simp [code, EVM.State.executionCode, EVM.State.delegationTarget, Fork.atLeast, Fork.ordinal]
  have selected : (List.replicate (mir.messageGas.length+1) physical.gas)[mir.messageGas.length]? =
      some physical.gas := by simp
  have current : (withBudget cfg mir physical.gas).fork = .cancun := fork
  have configured : (withBudget cfg mir physical.gas).externalConfig = some ext := bound
  have old : Fork.cancun.atLeast .amsterdam = false := by decide
  have inputPhysical : expand physical.memory offset.toNat length.toNat = .ok input := memory ▸ inputOk
  simp only [zeroCall, extFork, addressEq, codeEq, codeAddress,
    beq_iff_eq, knownAddress, ite_false]
  rw [CallDebit.prepare_eq ext mir.world physical ⟨.call,address,0,[]⟩ 0xf1 0 input active
    offset.toNat length.toNat out.toNat outSize.toNat inputPhysical outputOk]
  simp [external, configured, current, old, required, Nat.not_lt.mpr limit,
    inputOk, outputOk, codeEq, addressEq, Bind.bind, Except.bind, Pure.pure, Except.pure]
  let price := EVM.State.callBoundaryCharge ext mir.world message 0xf1 0
    (memoryWords mir.memory) (memoryWords active)
  have pricing : EVM.State.callBoundaryCharge ext mir.world ⟨.call,address,0#256,[]⟩ 0xf1 0
      (memoryWords physical.memory) (memoryWords active) = price := by
    rw [memory]
    rfl
  have messageEq : (⟨.call,address,0#256,readBytes active offset.toNat length.toNat⟩ : Calls.Message) =
      message := rfl
  rw [pricing,messageEq]
  simp only [callBudget, withBudget, selected, required, Bind.bind, Except.bind,
    Option.map_some, Option.getD_some, fork, charged]
  have priceEq : EVM.State.callBoundaryCharge ext mir.world message 0xf1 0
      (memoryWords mir.memory) (memoryWords active) = price := rfl
  simp only [priceEq]
  by_cases paid : price ≤ physical.gas
  · simp only [paid, ite_true, Nat.not_lt.mpr paid, ite_false]
    let balance := physical.gas-price
    let sent := min request.toNat (balance-balance/64)
    have le : sent ≤ balance := Nat.le_trans (Nat.min_le_right ..) (Nat.sub_le ..)
    have forward : Calls.forwardGas .cancun (physical.gas-price) request.toNat false =
        some ⟨sent,sent⟩ := by
      change Calls.forwardGas .cancun balance request.toNat false = _
      simp [sent, Calls.forwardGas, Fork.atLeast, Fork.ordinal]
    have noValue : Calls.hasValue message = false := rfl
    have kind : message.kind = .call := rfl
    have destination : message.destination = address := rfl
    have amount : message.value = 0 := rfl
    have returnedGas : physical.gas-price-sent+sent = physical.gas-price := by
      change balance - sent + sent = balance
      omega
    by_cases depth : 1024 ≤ cfg.environment.depth
    · simp [project, EVM.State.advance, forward, CallDebit.warmTwice, environment, kind, noValue,
        destination, amount, BitVec.ult_eq_decide, EVM.State.executeCall, codeAddress, knownAddress, depth,
        returnedGas, required, Bind.bind, Except.bind, Pure.pure, Except.pure, writeBytes]
    · simp [project, EVM.State.advance, forward, CallDebit.warmTwice, environment, kind, noValue,
        extFork, destination, amount, BitVec.ult_eq_decide, depth, required, Bind.bind, Except.bind,
        Pure.pure, Except.pure]
      cases EVM.State.executeCall (EVM.State.run work ext) ext (warmAddress mir.world address)
        cfg.environment message code sent <;> rfl
  · simp [paid, Nat.lt_of_not_ge paid, project, EVM.State.stepExhausted, EVM.State.advance,
      Bind.bind, Except.bind, Pure.pure, Except.pure]

end SolarTV.MIR.PreHelperCall
