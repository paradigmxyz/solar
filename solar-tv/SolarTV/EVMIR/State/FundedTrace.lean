import SolarTV.EVMIR.State.CallRefinement
import SolarTV.EVMIR.State.GasTrace
import SolarTV.EVMIR.State.Observe
import SolarTV.EVMIR.Transforms.MessageBoundary
import SolarTV.MIR.Transforms.CallHandoff

/-! Compose gas-debited paths with admitted calls. A prepared call may need
more available gas than its net debit because the caller retains an EIP-150
reserve. The minimum budget and the net debit are tracked separately. -/

namespace SolarTV.EVM.State
open EvmRules SolarTV.Execution

structure FundedTrace (cfg : Config) (firstWorld : Journal) (first : Frame)
    (lastWorld : Journal) (last : Frame) (work minimum debit : Nat) : Prop where
  affordable : debit ≤ minimum
  run : ∀ fuel gas, minimum ≤ gas → gas < 2^256 →
    State.run (fuel+work) cfg firstWorld {first with gas} =
      (State.run fuel cfg lastWorld {last with gas := gas-debit}).map
        (fun outcome => {outcome with work := outcome.work+work})

namespace FundedTrace

theorem metered {cfg firstWorld first lastWorld last work debit}
    (trace : MeteredPrefix cfg firstWorld first lastWorld last work debit) :
    FundedTrace cfg firstWorld first lastWorld last work debit debit := by
  refine ⟨Nat.le_refl _, ?_⟩
  intro fuel gas paid _
  rw [trace.paid fuel gas paid]
  cases State.run fuel cfg lastWorld {last with gas := gas-debit} <;> rfl

theorem append {cfg firstWorld first middleWorld middle lastWorld last
      left right minimum nextMinimum debit nextDebit}
    (head : FundedTrace cfg firstWorld first middleWorld middle left minimum debit)
    (tail : FundedTrace cfg middleWorld middle lastWorld last right nextMinimum nextDebit) :
    FundedTrace cfg firstWorld first lastWorld last (left+right)
      (max minimum (debit+nextMinimum)) (debit+nextDebit) := by
  have enough := tail.affordable
  refine ⟨by omega, ?_⟩
  intro fuel gas paid word
  have leftPaid : minimum ≤ gas := by omega
  have rightPaid : nextMinimum ≤ gas-debit := by omega
  rw [show fuel+(left+right) = fuel+right+left by omega, head.run _ _ leftPaid word,
    tail.run _ _ rightPaid (by omega), Nat.sub_sub]
  cases State.run fuel cfg lastWorld {last with gas := gas-(debit+nextDebit)} <;>
    simp [Except.map, Nat.add_comm, Nat.add_left_comm]

/-- An issued CALL carries its actual argument and gas operand through the
ordinary interpreter. A paid child may consume work as well as gas. -/
theorem message {cfg firstWorld first lastWorld last childWork minimum debit}
    (affordable : debit ≤ minimum)
    (signal : Nat → Step)
    (issued : ∀ gas, minimum ≤ gas →
      instruction cfg firstWorld {first with gas} = .ok (signal gas))
    (resumes : ∀ fuel gas, minimum ≤ gas →
      advance cfg (State.run (fuel+childWork) cfg) (signal gas) =
        .ok (.inr (childWork,lastWorld,{last with gas := gas-debit}))) :
    FundedTrace cfg firstWorld first lastWorld last (childWork+1) minimum debit := by
  refine ⟨affordable, ?_⟩
  intro fuel gas paid _
  rw [← Nat.add_assoc, GasTrace.run_signal (fuel+childWork) (issued gas paid), resumes fuel gas paid]
  simp only [Bind.bind, Except.bind, Nat.add_sub_cancel]
  cases State.run fuel cfg lastWorld {last with gas := gas-debit} <;>
    simp [Except.map, Pure.pure, Except.pure, Nat.add_assoc]

/-- A stable halt keeps the stop, data and journal fixed and publishes the
remaining balance. -/
theorem exit {cfg firstWorld first lastWorld last work minimum debit}
    (trace : FundedTrace cfg firstWorld first lastWorld last work minimum debit)
    (halt : Result)
    (stopped : ∀ gas, instruction cfg lastWorld {last with gas} = .ok (.done {halt with gas}))
    (fuel gas : Nat) (paid : minimum ≤ gas) (word : gas < 2^256) :
    State.run (fuel+1+work) cfg firstWorld {first with gas} =
      .ok {halt with gas := gas-debit, work := halt.work+1+work} := by
  rw [trace.run _ _ paid word, run_done fuel (stopped _)]
  rfl

end FundedTrace

/-- Compare paid paths from their entries. Layout, work and original gas
operands may differ. The final journals are compared after ordinary frame
settlement and root-image erasure by `observe`. -/
theorem funded_refines {cfg₁ cfg₂ w₁ w₂ f₁ f₂ j₁ j₂ z₁ z₂ work₁ work₂
      min₁ min₂ debit₁ debit₂}
    (old : FundedTrace cfg₁ w₁ f₁ j₁ z₁ work₁ min₁ debit₁)
    (new : FundedTrace cfg₂ w₂ f₂ j₂ z₂ work₂ min₂ debit₂)
    (r₁ r₂ : Result)
    (halt₁ : ∀ gas, instruction cfg₁ j₁ {z₁ with gas} = .ok (.done {r₁ with gas}))
    (halt₂ : ∀ gas, instruction cfg₂ j₂ {z₂ with gas} = .ok (.done {r₂ with gas}))
    (same : observe cfg₁ .effects {r₁ with
        world := if r₁.stop = .success then finalize r₁.world else w₁} =
      observe cfg₂ .effects {r₂ with
        world := if r₂.stop = .success then finalize r₂.world else w₂})
    (refund : (if r₁.stop = .success then finalize r₁.world else w₁).refund =
      (if r₂.stop = .success then finalize r₂.world else w₂).refund)
    (cheaper : debit₂ ≤ debit₁) (earlier : min₂ ≤ min₁)
    (fuel₁ fuel₂ gas : Nat) (paid : min₁ ≤ gas) (word : gas < 2^256) :
    equivalent cfg₁ cfg₂ .refine
      (execute (fuel₁+1+work₁) cfg₁ w₁ {f₁ with gas})
      (execute (fuel₂+1+work₂) cfg₂ w₂ {f₂ with gas}) := by
  have paid₂ : min₂ ≤ gas := Nat.le_trans earlier paid
  have stopEq := congrArg Observation.stop same
  change r₁.stop = r₂.stop at stopEq
  by_cases success : r₁.stop = .success
  · have success₂ := stopEq ▸ success
    simp only [success, success₂, ite_true] at same refund
    simp only [execute, old.exit r₁ halt₁ fuel₁ gas paid word, new.exit r₂ halt₂ fuel₂ gas paid₂ word,
      Bind.bind, Except.bind, Pure.pure, Except.pure, success, success₂, ite_true,
      equivalent]
    exact Or.inr ⟨same,refund,by omega⟩
  · have success₂ : r₂.stop ≠ .success := stopEq ▸ success
    simp only [success, success₂, ite_false] at same refund
    simp only [execute, old.exit r₁ halt₁ fuel₁ gas paid word, new.exit r₂ halt₂ fuel₂ gas paid₂ word,
      Bind.bind, Except.bind, Pure.pure, Except.pure, success, success₂, ite_false,
      equivalent]
    exact Or.inr ⟨same,refund,by rw [← stopEq]; split <;> omega⟩

/-- Compose a real GAS instruction with a paid message. The sampled word is
passed to the call constructor. Extra child budget must satisfy the admitted
child policy through the ordinary `advance` transition. -/
theorem FundedTrace.sampledMessage {cfg firstWorld first lastWorld last childWork minimum debit}
    (affordable : debit ≤ minimum) (samplePaid : 2 ≤ minimum)
    (schema : available cfg 0x5a = ⟨true,2,0,0,1⟩)
    (opcode : first.code[first.pc]? = some 0x5a) (space : first.stack.length < 1024)
    (signal : Nat → Step)
    (issued : ∀ gas, minimum ≤ gas → gas < 2^256 →
      instruction cfg firstWorld (EVM.MessageBoundary.afterGas {first with gas}) = .ok (signal gas))
    (resumes : ∀ fuel gas, minimum ≤ gas → gas < 2^256 →
      advance cfg (State.run (fuel+childWork) cfg) (signal gas) =
        .ok (.inr (childWork,lastWorld,{last with gas := gas-debit}))) :
    FundedTrace cfg firstWorld first lastWorld last (childWork+2) minimum debit := by
  refine ⟨affordable, ?_⟩
  intro fuel gas paid word
  have sampled := EVM.MessageBoundary.sample cfg firstWorld {first with gas} opcode schema space
  simp only [show ¬ gas < 2 by omega, ite_false] at sampled
  rw [show fuel+(childWork+2) = (fuel+childWork+1)+1 by omega,
    run_next (fuel+childWork+1) sampled, GasTrace.run_signal (fuel+childWork) (issued gas paid word),
    resumes fuel gas paid word]
  simp only [Bind.bind, Except.bind, Nat.add_sub_cancel]
  cases State.run fuel cfg lastWorld {last with gas := gas-debit} <;>
    simp [Except.map, Pure.pure, Except.pure, Nat.add_assoc, Nat.add_comm, Nat.add_left_comm]

/-- The GAS/STATICCALL boundary used by both emitted permit images. The
arguments below the sample can be arbitrary words whose two memory ranges
are already active. The prepared budget's EIP-150 cap pays recovery at 3047. -/
theorem funded_recovery (cfg : Config) (world : Journal) (frame : Frame)
    (inputStart inputSize outStart outSize : Word) (tail : Stack)
    (fork : cfg.fork = .cancun) (known : cfg.unknownCode = none)
    (gasSchema : available cfg 0x5a = ⟨true,2,0,0,1⟩)
    (callSchema : available cfg 0xfa = ⟨true,2600,0,6,1⟩)
    (gasOp : frame.code[frame.pc]? = some 0x5a)
    (callOp : frame.code[frame.pc+1]? = some 0xfa)
    (shape : frame.stack = 1 :: inputStart :: inputSize :: outStart :: outSize :: tail)
    (room : tail.length + 5 < 1024) (depth : frame.environment.depth < 1024)
    (empty : (account world 1).code = [])
    (input : expand frame.memory inputStart.toNat inputSize.toNat = .ok frame.memory)
    (output : expand frame.memory outStart.toNat outSize.toNat = .ok frame.memory) :
    let payload := readBytes frame.memory inputStart.toNat inputSize.toNat
    let fee := callBoundaryCharge cfg world ⟨.staticcall,1,0,[]⟩ 0xfa 0 0 0
    let data := Precompiles.Signatures.recover payload
    let next := {frame with
      pc := frame.pc+2
      stack := 1 :: tail
      returnData := data
      memory := writeBytes frame.memory outStart.toNat (data.take outSize.toNat)}
    FundedTrace cfg world frame (warmAddress world 1) next 2 (2+fee+3047) (2+fee+3000) := by
  dsimp only
  let payload := readBytes frame.memory inputStart.toNat inputSize.toNat
  let fee := callBoundaryCharge cfg world ⟨.staticcall,1,0,[]⟩ 0xfa 0 0 0
  let prepared (gas : Nat) := {frame with pc := frame.pc+2, stack := tail, gas := gas-2-fee}
  let signal (gas : Nat) := Step.call ⟨.staticcall,1,0,payload⟩ outStart.toNat outSize.toNat (gas-2)
    ⟨1,[],false⟩ (warmAddress world 1) (prepared gas)
  have fits : frame.stack.length < 1024 := by simpa [shape, Nat.add_assoc] using room
  apply FundedTrace.sampledMessage (childWork := 0) (by omega) (by omega)
    gasSchema gasOp fits signal
  · intro gas paid word
    obtain ⟨within,_⟩ := List.getElem?_eq_some_iff.mp callOp
    have request : (BitVec.ofNat 256 (gas-2)).toNat = gas-2 := by
      simp [Nat.mod_eq_of_lt (by omega : gas-2 < 2^256)]
    have affordable : ¬ gas-2 < fee := by omega
    have paidAccess : ¬ gas - 2 <
        callBoundaryCharge cfg world ⟨.staticcall,1#160,0#256,[]⟩ 250 0 0 0 := affordable
    have bare : (account world 1#160).code = [] := empty
    have arguments : ¬ tail.length+1+1+1+1+1+1 < 6 := by omega
    have inCharge := MIR.CallDebit.memoryCharge_zero _ _ _ input
    have outCharge := MIR.CallDebit.memoryCharge_zero _ _ _ output
    simp [instruction, instructionWithImage, callOp, callSchema, Nat.not_le_of_gt within,
      EVM.MessageBoundary.afterGas, shape, fork, Fork.atLeast, Fork.ordinal, pureOp,
      show ¬ 1024 < tail.length+1 by omega, executionCode, delegationTarget, known,
      withMemory, input, output, inCharge, outCharge, paidAccess, arguments,
      signal, payload, prepared, request, Bind.bind, Except.bind, bare, fee, Nat.add_assoc]
  · intro fuel gas paid _
    have admitted : 3000 ≤ min (gas-2) ((prepared gas).gas-(prepared gas).gas/64) := by
      dsimp only [prepared]
      omega
    have ready := CallRefinement.advance_recovery cfg (State.run fuel cfg) (warmAddress world 1)
      (prepared gas) payload outStart.toNat outSize.toNat (gas-2) fork known depth admitted
    dsimp only at ready
    have amount : gas-2-fee-3000 = gas-(2+fee+3000) := by omega
    simp only [signal, prepared, Nat.add_zero, MIR.CallDebit.warmTwice] at ready ⊢
    rw [amount] at ready
    exact ready

open MIR.CallHandoff (Request)

/-- A sampled zero-value CALL with staged input and empty output. The caller
supplies the matched child policy and an admission threshold for its debit. -/
theorem funded_zero_call (cfg : Config) (world : Journal) (frame : Frame)
    (request : Request) (tail : Stack) (reply : MessageOutcome) (childMinimum childDebit : Nat)
    (fork : cfg.fork = .cancun) (known : cfg.unknownCode = none)
    (gasSchema : available cfg 0x5a = ⟨true,2,0,0,1⟩)
    (callSchema : available cfg 0xf1 = ⟨true,2600,0,7,1⟩)
    (gasOp : frame.code[frame.pc]? = some 0x5a)
    (callOp : frame.code[frame.pc+1]? = some 0xf1)
    (shape : frame.stack = request.args ++ tail)
    (room : tail.length+6 < 1024) (depth : frame.environment.depth < 1024)
    (input : expand frame.memory request.offset.toNat request.length.toNat = .ok frame.memory)
    (output : expand frame.memory 0 0 = .ok frame.memory)
    (capacity : childDebit ≤ childMinimum)
    (cap : ∀ gas, childMinimum ≤ gas → childDebit ≤ gas-gas/64)
    (stable : ∀ fuel gas, childDebit ≤ gas →
      executeCall (State.run (fuel+reply.work) cfg) cfg (MIR.CallHandoff.entered request world)
        frame.environment (request.message frame.memory) (MIR.CallHandoff.destination request world) gas =
        .ok {reply with gas := gas-childDebit}) :
    let fee := MIR.CallHandoff.charge cfg request world
    let next := {frame with
      pc := frame.pc+2
      stack := reply.value :: tail
      returnData := reply.data}
    FundedTrace cfg world frame reply.world next (reply.work+2)
      (2+fee+childMinimum) (2+fee+childDebit) := by
  dsimp only
  let fee := MIR.CallHandoff.charge cfg request world
  let sampled (gas : Nat) : Request := {request with gas := BitVec.ofNat 256 (gas-2)}
  let prepared (gas : Nat) := {frame with pc := frame.pc+2, stack := tail, gas := gas-2-fee}
  let signal (gas : Nat) := Step.call (request.message frame.memory) 0 0 (gas-2)
    (MIR.CallHandoff.destination request world) (MIR.CallHandoff.entered request world) (prepared gas)
  have fits : frame.stack.length < 1024 := by
    simp [shape, Request.args]
    omega
  have tariff (gas : Nat) : MIR.CallHandoff.charge cfg (sampled gas) world = fee := rfl
  have equivalentWord (gas : Nat) (word : gas < 2^256) : (sampled gas).gas.toNat = gas-2 := by
    simp [sampled, Nat.mod_eq_of_lt (by omega : gas-2 < 2^256)]
  apply FundedTrace.sampledMessage (childWork := reply.work) (by omega) (by omega)
    gasSchema gasOp fits signal
  · intro gas paid word
    obtain ⟨within,_⟩ := List.getElem?_eq_some_iff.mp callOp
    let caller := EVM.MessageBoundary.afterGas {frame with gas}
    have operands : caller.stack = (sampled gas).gas :: ((sampled gas).args ++ tail) := by
      simp [caller, EVM.MessageBoundary.afterGas, sampled, Request.args, shape]
    have stored := MIR.CallHandoff.prepare cfg caller world (sampled gas) tail frame.code.length
      (pushValue caller.code caller.pc) (validJump caller.code) fork known callSchema within operands
      (by omega) input output
    have actual : instruction cfg world caller = instructionWithImage cfg world caller frame.code.length
        241 (pushValue caller.code caller.pc) (validJump caller.code) := by
      change instructionWithImage cfg world caller frame.code.length
        (frame.code[frame.pc+1]?.getD 0) (pushValue caller.code caller.pc) (validJump caller.code) = _
      rw [callOp]
      rfl
    change instruction cfg world caller = _
    rw [actual, stored, tariff, equivalentWord gas word]
    have paidFee : ¬ (gas-2) < fee := by omega
    simp [caller, EVM.MessageBoundary.afterGas, paidFee, signal, prepared, sampled,
      Request.message, Request.payload, MIR.CallHandoff.entered, MIR.CallHandoff.destination, Nat.add_assoc]
  · intro fuel gas paid word
    have admitted := cap (gas-2-fee) (by omega)
    have requestPaid : childDebit ≤ min (gas-2) ((gas-2-fee)-(gas-2-fee)/64) := by omega
    have amount : gas-2-fee-childDebit = gas-(2+fee+childDebit) := by omega
    have notRejected : (decide ((prepared gas).environment.depth ≥ 1024) ||
        (((request.message frame.memory).kind == .call || (request.message frame.memory).kind == .callcode) &&
          (account (MIR.CallHandoff.entered request world) (prepared gas).environment.address).balance.ult
            (request.message frame.memory).value)) = false := by
      simp [prepared, Request.message, Nat.not_le_of_gt depth, BitVec.ult]
    have response := CallRefinement.stable_zero_value_call cfg (State.run (fuel+reply.work) cfg)
      (request.message frame.memory) 0 0 (gas-2) (MIR.CallHandoff.destination request world)
      (MIR.CallHandoff.entered request world) (prepared gas) reply childDebit fork rfl notRejected requestPaid
      (by
        intro supplied enough
        simpa [prepared, Request.message, MIR.CallHandoff.entered, MIR.CallDebit.warmTwice] using stable fuel supplied enough)
    have noWrite : writeBytes frame.memory 0 [] = frame.memory := writeBytes_existing _ _ _ (readBytes_zero _ _)
    simp only [signal, prepared, List.take_zero, noWrite, amount] at response ⊢
    exact response

/-- The return-true token admitted by the FlashLoan fixture. Its exact
bytecode and the non-precompile check discharge the general child policy. -/
theorem funded_return_true (cfg : Config) (world : Journal) (frame : Frame)
    (request : Request) (tail : Stack)
    (fork : cfg.fork = .cancun) (known : cfg.unknownCode = none)
    (gasSchema : available cfg 0x5a = ⟨true,2,0,0,1⟩)
    (callSchema : available cfg 0xf1 = ⟨true,2600,0,7,1⟩)
    (push : available cfg 0x60 = ⟨true,3,0,0,1⟩)
    (zero : available cfg 0x5f = ⟨true,2,0,0,1⟩)
    (store : available cfg 0x52 = ⟨true,3,0,2,0⟩)
    (returnOp : available cfg 0xf3 = ⟨true,0,0,2,0⟩)
    (gasOp : frame.code[frame.pc]? = some 0x5a)
    (callOp : frame.code[frame.pc+1]? = some 0xf1)
    (shape : frame.stack = request.args ++ tail)
    (room : tail.length+6 < 1024) (depth : frame.environment.depth < 1024)
    (input : expand frame.memory request.offset.toNat request.length.toNat = .ok frame.memory)
    (output : expand frame.memory 0 0 = .ok frame.memory)
    (code : (MIR.CallHandoff.destination request world).bytes = [0x60,1,0x5f,0x52,0x60,32,0x5f,0xf3])
    (token : precompile cfg.fork (request.destination.setWidth 160) = false) :
    let fee := MIR.CallHandoff.charge cfg request world
    let next := {frame with pc := frame.pc+2, stack := 1 :: tail, returnData := wordBytes 1}
    FundedTrace cfg world frame (MIR.CallHandoff.entered request world) next 8 (2+fee+16) (2+fee+16) := by
  let entered := MIR.CallHandoff.entered request world
  let reply : MessageOutcome := ⟨1,wordBytes 1,entered,0,6⟩
  apply funded_zero_call cfg world frame request tail reply 16 16 fork known gasSchema callSchema
    gasOp callOp shape room depth input output (by omega) (by omega)
  intro fuel gas paid
  let env := {(Calls.childEnvironment frame.environment (request.message frame.memory)) with
    codeAddress := (MIR.CallHandoff.destination request world).address}
  have child : run (fuel+reply.work) cfg (MIR.CallHandoff.entered request world)
      ⟨env,(account world (request.destination.setWidth 160)).code,0,[],[],[],gas⟩ =
      .ok ⟨.success,wordBytes 1,entered,gas-16,6⟩ := by
    change (account world (request.destination.setWidth 160)).code = _ at code
    rw [code]
    exact CallRefinement.returnTrueRun cfg entered env fuel gas push zero store returnOp paid
  unfold executeCall
  simp [known, Request.message, BitVec.ult, Nat.not_le_of_gt depth, SolarTV.Execution.transfer,
    MIR.CallHandoff.destination, token, Bind.bind, Except.bind, Pure.pure, Except.pure]
  simp only [env, Request.message, MIR.CallHandoff.destination] at child
  erw [child]
  rfl

end SolarTV.EVM.State
