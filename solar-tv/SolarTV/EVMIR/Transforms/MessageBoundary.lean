import SolarTV.EVMIR.State.Interpreter

/-! Exact metering at an emitted GAS/STATICCALL boundary. A physical stack
already contains the call arguments below GAS's result. The bytecode
interpreter prices each memory access before producing a child request. -/

namespace SolarTV.EVM.MessageBoundary
open EvmRules SolarTV.Execution
open State

def afterGas (frame : Frame) : Frame :=
  {frame with
    pc := frame.pc + 1
    gas := frame.gas - 2
    stack := BitVec.ofNat 256 (frame.gas-2) :: frame.stack}

theorem sample (cfg : Config) (world : Journal) (frame : Frame)
    (opcode : frame.code[frame.pc]? = some 0x5a)
    (schema : available cfg 0x5a = ⟨true,2,0,0,1⟩)
    (space : frame.stack.length < 1024) :
    instruction cfg world frame =
      if frame.gas < 2 then stepExhausted world else .ok (.next world (afterGas frame)) := by
  obtain ⟨within,_⟩ := List.getElem?_eq_some_iff.mp opcode
  simp [instruction, instructionWithImage, opcode, schema, Nat.not_le_of_gt within,
    Nat.not_lt.mpr (by omega : frame.stack.length + 1 ≤ 1024),
    pureOp, afterGas, next]

def staticAction (cfg : Config) (world : Journal) (frame : Frame)
    (requested : Nat) (target offset length resultOffset resultSize : Word)
    (rest : List Word) : Action :=
  let destination := target.setWidth 160
  let (code,resolution,entered) := executionCode cfg.fork world destination
  if cfg.unknownCode == some code.address then .error "MIR root runtime code is unknown" else
  withMemory world frame
    (callBoundaryCharge cfg world ⟨.staticcall,destination,0,[]⟩ 0xfa resolution 0 0)
    offset.toNat length.toNat fun frame =>
  withMemory world frame 0 resultOffset.toNat resultSize.toNat fun frame =>
    .ok (.call ⟨.staticcall,destination,0,readBytes frame.memory offset.toNat length.toNat⟩
      resultOffset.toNat resultSize.toNat requested code entered
      {frame with stack := rest, pc := frame.pc+1})

theorem staticCall (cfg : Config) (world : Journal) (frame : Frame) (base : Nat)
    (requested target offset length resultOffset resultSize : Word) (rest : List Word)
    (stack : frame.stack = requested :: target :: offset :: length :: resultOffset :: resultSize :: rest)
    (opcode : frame.code[frame.pc]? = some 0xfa)
    (schema : available cfg 0xfa = ⟨true,base,0,6,1⟩)
    (old : cfg.fork.atLeast .amsterdam = false)
    (space : rest.length+6 ≤ 1024) :
    instruction cfg world frame =
      staticAction cfg world frame requested.toNat target offset length resultOffset resultSize rest := by
  obtain ⟨within,_⟩ := List.getElem?_eq_some_iff.mp opcode
  simp [instruction, instructionWithImage, opcode, schema, Nat.not_le_of_gt within,
    stack, old, pureOp, staticAction,
    show ¬ rest.length+1+1+1+1+1+1 < 6 by omega,
    show ¬ 1024 < rest.length+1 by omega]

/-- The immediately following request uses exactly the balance left after GAS.
An unaffordable GAS produces an exceptional halt before any message starts. -/
theorem recovery (cfg : Config) (world : Journal) (frame : Frame) (base : Nat)
    (offset length : Word) (rest : List Word)
    (stack : frame.stack = 1 :: offset :: length :: 0 :: 32 :: rest)
    (gasOpcode : frame.code[frame.pc]? = some 0x5a)
    (callOpcode : frame.code[frame.pc+1]? = some 0xfa)
    (gasSchema : available cfg 0x5a = ⟨true,2,0,0,1⟩)
    (callSchema : available cfg 0xfa = ⟨true,base,0,6,1⟩)
    (old : cfg.fork.atLeast .amsterdam = false)
    (space : rest.length+6 ≤ 1024) :
    instruction cfg world frame =
      (if frame.gas < 2 then stepExhausted world else .ok (.next world (afterGas frame))) ∧
    instruction cfg world (afterGas frame) =
      staticAction cfg world (afterGas frame) ((BitVec.ofNat 256 (frame.gas-2)).toNat)
        1 offset length 0 32 rest := by
  refine ⟨sample cfg world frame gasOpcode gasSchema ?_, ?_⟩
  · simp only [stack, List.length_cons]
    omega
  exact staticCall cfg world (afterGas frame) base _ _ _ _ _ _ _
    (by simp only [afterGas, stack]) callOpcode callSchema old space

end SolarTV.EVM.MessageBoundary
