import SolarTV.EVMIR.State.MeteredPrefix
import SolarTV.State.MemoryLaws

/-! Gas refinement at a paid, zero-value message boundary. The policy
supplies a common child outcome with a fixed actual debit once the child
has enough gas. The request and forwarded gas may differ between runs. -/

namespace SolarTV.EVM.State.CallRefinement
open EvmRules SolarTV.Execution

private theorem ecrecover_paid (input : List Byte) (gas : Nat) (paid : 3000 ≤ gas) :
    Precompiles.execute .cancun 1 input gas =
      .ok (.success, Precompiles.Signatures.recover input, gas-3000) := by
  simp [Precompiles.execute, Precompiles.compute, Precompiles.price,
    precompile, Fork.atLeast, Fork.ordinal, Nat.not_lt_of_ge paid,
    Bind.bind, Except.bind, Pure.pure, Except.pure]

/-- When the actual child returns the supplied gas minus a stable debit,
EIP-150 only partitions the available balance. The net parent debit equals
the child debit, with no requirement that the two gas operands agree. -/
theorem stable_zero_value_call (cfg : Config) (execute : ChildExecutor)
    (message : Calls.Message) (outOffset outSize requested : Nat) (code : ExecutionCode)
    (world : Journal) (parent : Frame) (reply : MessageOutcome) (debit : Nat)
    (fork : cfg.fork = .cancun) (noValue : Calls.hasValue message = false)
    (admitted : (decide (parent.environment.depth ≥ 1024) ||
      ((message.kind == .call || message.kind == .callcode) &&
       (account world parent.environment.address).balance.ult message.value)) = false)
    (paid : debit ≤ min requested (parent.gas-parent.gas/64))
    (stable : ∀ gas, debit ≤ gas → executeCall execute cfg (warmAddress world message.destination)
      parent.environment message code gas = .ok {reply with gas := gas-debit}) :
    advance cfg execute (.call message outOffset outSize requested code world parent) =
      .ok (.inr (reply.work,reply.world,{parent with
        stack := reply.value :: parent.stack
        memory := writeBytes parent.memory outOffset (reply.data.take outSize)
        returnData := reply.data
        gas := parent.gas-debit})) := by
  let forwarded := min requested (parent.gas-parent.gas/64)
  have splitGas : Calls.forwardGas cfg.fork parent.gas requested (Calls.hasValue message) =
      some ⟨forwarded,forwarded⟩ := by
    simp [Calls.forwardGas, fork, Fork.atLeast, Fork.ordinal, noValue, forwarded]
  have debitEq : parent.gas-forwarded+(forwarded-debit) = parent.gas-debit := by
    have cap : forwarded ≤ parent.gas := Nat.le_trans (Nat.min_le_right ..) (Nat.sub_le ..)
    omega
  simp [advance, admitted, splitGas, stable forwarded paid, debitEq,
    Bind.bind, Except.bind, Pure.pure, Except.pure]

/-- The static recovery call has a stable child result at every paid
forwarding budget, including gas operands and caps that differ. -/
theorem advance_recovery (cfg : Config) (execute : ChildExecutor) (world : Journal)
    (parent : Frame) (input : List Byte) (offset size requested : Nat)
    (fork : cfg.fork = .cancun) (known : cfg.unknownCode = none)
    (depth : parent.environment.depth < 1024)
    (paid : 3000 ≤ min requested (parent.gas-parent.gas/64)) :
    let data := Precompiles.Signatures.recover input
    advance cfg execute (.call ⟨.staticcall,1,0,input⟩ offset size requested
      ⟨1,[],false⟩ world parent) =
      .ok (.inr (0,warmAddress world 1,{parent with
        stack := 1 :: parent.stack
        memory := writeBytes parent.memory offset (data.take size)
        returnData := data
        gas := parent.gas-3000})) := by
  let message : Calls.Message := ⟨.staticcall,1,0,input⟩
  let reply : MessageOutcome := ⟨1,Precompiles.Signatures.recover input,warmAddress world 1,0,0⟩
  have admitted : (decide (parent.environment.depth ≥ 1024) ||
      ((message.kind == .call || message.kind == .callcode) &&
       (account world parent.environment.address).balance.ult message.value)) = false := by
    simp [message, Nat.not_le_of_gt depth]
  refine stable_zero_value_call cfg execute message offset size requested ⟨1,[],false⟩
    world parent reply 3000 fork (by rfl) admitted paid ?_
  intro gas enough
  simp [executeCall, fork, known, message, reply, Nat.not_le_of_gt depth,
    precompile, Fork.atLeast, Fork.ordinal, Bind.bind, Except.bind, Pure.pure, Except.pure]
  rw [show Precompiles.execute .cancun (1#160) input gas =
    .ok (.success, Precompiles.Signatures.recover input, gas-3000) from ecrecover_paid input gas enough]
  rfl

/-- Code of the return-true token in the explicit FlashLoan policy. -/
private def returnTrueCode : List Byte := [0x60,1,0x5f,0x52,0x60,32,0x5f,0xf3]

theorem returnTrueRun (cfg : Config) (world : Journal) (env : World.Environment)
    (fuel gas : Nat)
    (push : available cfg 0x60 = ⟨true,3,0,0,1⟩)
    (zero : available cfg 0x5f = ⟨true,2,0,0,1⟩)
    (store : available cfg 0x52 = ⟨true,3,0,2,0⟩)
    (returnOp : available cfg 0xf3 = ⟨true,0,0,2,0⟩)
    (paid : 16 ≤ gas) :
    run (fuel+6) cfg world ⟨env,returnTrueCode,0,[],[],[],gas⟩ =
      .ok ⟨.success,wordBytes 1,world,gas-16,6⟩ := by
  let start : Frame := ⟨env,returnTrueCode,0,[],[],[],0⟩
  let first := {start with pc := 2, stack := [1]}
  let second := {first with pc := 3, stack := [0,1]}
  let stored := {second with pc := 4, stack := [], memory := wordBytes 1}
  let sized := {stored with pc := 6, stack := [32]}
  let ready := {sized with pc := 7, stack := [0,32]}
  have one (n : Nat) : instruction cfg world {start with gas := n} =
      if n < 3 then stepExhausted world else .ok (.next world {first with gas := n-3}) := by
    simp [instruction,instructionWithImage,start,first,returnTrueCode,push,pureOp,pushValue,readBytes]
  have two (n : Nat) : instruction cfg world {first with gas := n} =
      if n < 2 then stepExhausted world else .ok (.next world {second with gas := n-2}) := by
    simp [instruction,instructionWithImage,start,first,second,returnTrueCode,zero,pureOp,next]
  have written : writeBytes (List.replicate 32 0) 0 (wordBytes 1) = wordBytes 1 := by decide +kernel
  have three (n : Nat) : instruction cfg world {second with gas := n} =
      if n < 6 then stepExhausted world else .ok (.next world {stored with gas := n-6}) := by
    simp [instruction,instructionWithImage,start,first,second,stored,returnTrueCode,store,pureOp,next,
      withMemory,memoryCharge,memoryCost,expand,requiredWords,memoryWords,
      Bind.bind,Except.bind]
    split
    · rfl
    · exact congrArg (fun memory => Except.ok (Step.next world {stored with memory, gas := n-6})) written
  have four (n : Nat) : instruction cfg world {stored with gas := n} =
      if n < 3 then stepExhausted world else .ok (.next world {sized with gas := n-3}) := by
    simp [instruction,instructionWithImage,start,first,second,stored,sized,returnTrueCode,push,pureOp,pushValue,readBytes]
  have five (n : Nat) : instruction cfg world {sized with gas := n} =
      if n < 2 then stepExhausted world else .ok (.next world {ready with gas := n-2}) := by
    simp [instruction,instructionWithImage,start,first,second,stored,sized,ready,returnTrueCode,zero,pureOp,next]
  have output : readBytes (wordBytes 1) 0 32 = wordBytes 1 := by decide +kernel
  have last (n : Nat) : instruction cfg world {ready with gas := n} =
      .ok (.done ⟨.success,wordBytes 1,world,n,0⟩) := by
    simp [instruction,instructionWithImage,start,first,second,stored,sized,ready,returnTrueCode,returnOp,
      pureOp,withMemory,memoryCharge,memoryCost,expand,requiredWords,memoryWords,
      Bind.bind,Except.bind]
    exact output
  have metered : MeteredPrefix cfg world start world ready 5 16 :=
    .step one (.step two (.step three (.step four (.step five (.done _ _)))))
  exact metered.paid_halt (fun n => ⟨.success,wordBytes 1,world,n,0⟩) last fuel gas paid

/-- The admitted borrower stub can return all of its budget. -/
theorem stopRun (cfg : Config) (world : Journal) (env : World.Environment)
    (fuel gas : Nat) (stop : available cfg 0 = ⟨true,0,0,0,0⟩) :
    run (fuel+1) cfg world ⟨env,[0],0,[],[],[],gas⟩ =
      .ok ⟨.success,[],world,gas,1⟩ := by
  have halt : instruction cfg world ⟨env,[0],0,[],[],[],gas⟩ =
      .ok (.done ⟨.success,[],world,gas,0⟩) := by
    simp [instruction,instructionWithImage,stop,pureOp]
  exact run_done fuel halt

end SolarTV.EVM.State.CallRefinement
