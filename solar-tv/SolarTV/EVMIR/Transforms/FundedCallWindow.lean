import SolarTV.EVMIR.State.CallRefinement
import SolarTV.EVMIR.Transforms.MessageBoundary
import SolarTV.EVMIR.Suffix

/-! A cheaper stack window can feed a GAS and a zero-value message.
The prelude retains the complete live stack and the shared effect world.
The real Cancun instructions read GAS, expand the input and output ranges,
charge account access and produce an EIP-150 request. The child policy
requires the same reply and actual debit above an explicit budget floor.
The continuation after resuming the parent requires its own certificate. -/

namespace SolarTV.EVM.FundedCallWindow
open EvmRules SolarTV.Execution State

inductive Kind where
  | call
  | staticcall

def Kind.opcode : Kind → Byte
  | .call => 0xf1
  | .staticcall => 0xfa

def Kind.message : Kind → Calls.Kind
  | .call => .call
  | .staticcall => .staticcall

def Kind.inputs : Kind → Nat
  | .call => 7
  | .staticcall => 6

structure Operands where
  target : Word
  inputOffset : Word
  inputSize : Word
  outputOffset : Word
  outputSize : Word
  rest : Stack

def Operands.stack (site : Operands) (kind : Kind) : Stack :=
  let ranges := [site.inputOffset,site.inputSize,site.outputOffset,site.outputSize]
  site.target :: (match kind with
    | .call => 0 :: ranges ++ site.rest
    | .staticcall => ranges ++ site.rest)

def Operands.destination (site : Operands) : Address := site.target.setWidth 160

def Operands.message (site : Operands) (kind : Kind) (memory : List Byte) : Calls.Message :=
  ⟨kind.message,site.destination,0,readBytes memory site.inputOffset.toNat site.inputSize.toNat⟩

def Operands.charge (site : Operands) (cfg : Config) (world : Journal) (kind : Kind)
    (memory : List Byte) (resolution : Nat) : Nat :=
  callBoundaryCharge cfg world ⟨kind.message,site.destination,0,[]⟩
      kind.opcode.toNat resolution 0 0 +
    memoryCharge memory site.inputOffset.toNat site.inputSize.toNat

def Operands.outputCharge (site : Operands) (expandedInput : List Byte) : Nat :=
  memoryCharge expandedInput site.outputOffset.toNat site.outputSize.toNat

def Operands.prepared (site : Operands) (frame : Frame) (memory : List Byte)
    (charge : Nat) : Frame :=
  {frame with pc := frame.pc+2, stack := site.rest, memory, gas := frame.gas-2-charge}

def cap (gas : Nat) : Nat := gas - gas/64

theorem cap_mono {before after : Nat} (more : before ≤ after) : cap before ≤ cap after := by
  simp only [cap]
  omega

def execute (cfg : Config) (child : ChildExecutor) (world : Journal)
    (frame : Frame) : Except String (State.Result ⊕ (Nat × Journal × Frame)) := do
  let sampled ← instruction cfg world frame
  match sampled with
  | .done result => return .inl result
  | .next nextWorld nextFrame =>
    advance cfg child (← instruction cfg nextWorld nextFrame)
  | _ => .error "expected GAS before message"

private theorem sampled_action (cfg : Config) (world : Journal) (frame : Frame)
    (site : Operands) (kind : Kind) (base : Nat)
    (fork : cfg.fork = .cancun)
    (opcode : frame.code[frame.pc+1]? = some kind.opcode)
    (schema : available cfg kind.opcode.toNat = ⟨true,base,0,kind.inputs,1⟩)
    (stack : frame.stack = site.stack kind) (space : frame.stack.length < 1024) :
    instruction cfg world (MessageBoundary.afterGas frame) =
      let (code,resolution,entered) := executionCode cfg.fork world site.destination
      let current := MessageBoundary.afterGas frame
      if cfg.unknownCode == some code.address then .error "MIR root runtime code is unknown" else
      withMemory world current
          (callBoundaryCharge cfg world ⟨kind.message,site.destination,0,[]⟩
            kind.opcode.toNat resolution 0 0)
          site.inputOffset.toNat site.inputSize.toNat fun current =>
        withMemory world current 0 site.outputOffset.toNat site.outputSize.toNat fun current =>
          .ok (.call (site.message kind current.memory) site.outputOffset.toNat site.outputSize.toNat
            (BitVec.ofNat 256 (frame.gas-2)).toNat code entered
            {current with stack := site.rest, pc := current.pc+1}) := by
  obtain ⟨within,_⟩ := List.getElem?_eq_some_iff.mp opcode
  cases kind
  all_goals
    simp [Kind.opcode, Kind.inputs] at opcode schema
    simp [stack, Operands.stack] at space
    have fit : ¬ 1024 < site.rest.length + 1 := by omega
    simp [instruction, instructionWithImage, MessageBoundary.afterGas, opcode, schema,
      Kind.opcode, Kind.message, Operands.stack, Operands.message, Operands.destination,
      Nat.not_le_of_gt within, stack, fork, Fork.atLeast, Fork.ordinal, fit, pureOp] <;> omega

private theorem memory_paid (world : Journal) (frame : Frame) (charge offset size : Nat)
    (next : Frame → Action) (memory : List Byte)
    (enough : ¬frame.gas < charge + memoryCharge frame.memory offset size)
    (expansion : expand frame.memory offset size = .ok memory) :
    withMemory world frame charge offset size next =
      next {frame with memory, gas := frame.gas - (charge + memoryCharge frame.memory offset size)} := by
  simp [withMemory,enough,expansion,Bind.bind,Except.bind]

def Operands.resumed (site : Operands) (frame : Frame) (memory : List Byte) (charge : Nat)
    (reply : MessageOutcome) (debit : Nat) : Frame :=
  let prepared := site.prepared frame memory charge
  {prepared with
    stack := reply.value :: prepared.stack
    memory := writeBytes prepared.memory site.outputOffset.toNat
      (reply.data.take site.outputSize.toNat)
    returnData := reply.data
    gas := prepared.gas-debit}

/-- Successful GAS and boundary preparation, followed by a child whose
outcome and actual debit are stable in the admitted budget range. -/
theorem paid (cfg : Config) (child : ChildExecutor) (world : Journal) (frame : Frame)
    (site : Operands) (kind : Kind) (base : Nat)
    (code : ExecutionCode) (resolution : Nat) (checkpoint : Journal)
    (inputMemory outputMemory : List Byte) (reply : MessageOutcome) (debit : Nat)
    (fork : cfg.fork = .cancun)
    (gasOp : frame.code[frame.pc]? = some 0x5a)
    (messageOp : frame.code[frame.pc+1]? = some kind.opcode)
    (gasSchema : available cfg 0x5a = ⟨true,2,0,0,1⟩)
    (messageSchema : available cfg kind.opcode.toNat = ⟨true,base,0,kind.inputs,1⟩)
    (stack : frame.stack = site.stack kind) (space : frame.stack.length < 1024)
    (known : cfg.unknownCode ≠ some code.address)
    (resolve : executionCode cfg.fork world site.destination = (code,resolution,checkpoint))
    (input : expand frame.memory site.inputOffset.toNat site.inputSize.toNat = .ok inputMemory)
    (output : expand inputMemory site.outputOffset.toNat site.outputSize.toNat = .ok outputMemory)
    (depth : frame.environment.depth < 1024)
    (gasWord : frame.gas < 2^256)
    (afford : 2 + site.charge cfg world kind frame.memory resolution +
      site.outputCharge inputMemory ≤ frame.gas)
    (within : debit ≤ cap (frame.gas-2-site.charge cfg world kind frame.memory resolution-
      site.outputCharge inputMemory))
    (stable : ∀ gas, debit ≤ gas →
      executeCall child cfg (warmAddress checkpoint site.destination) frame.environment
        (site.message kind outputMemory) code gas = .ok {reply with gas := gas-debit}) :
    execute cfg child world frame =
      .ok (.inr (reply.work,reply.world,site.resumed frame outputMemory
        (site.charge cfg world kind frame.memory resolution + site.outputCharge inputMemory)
        reply debit)) := by
  let charge := site.charge cfg world kind frame.memory resolution
  let extra := site.outputCharge inputMemory
  have enoughGas : ¬ frame.gas < 2 := by omega
  have enoughInput : ¬ frame.gas-2 < charge := by omega
  have enoughOutput : ¬ frame.gas-2-charge < extra := by omega
  have requested : (BitVec.ofNat 256 (frame.gas-2)).toNat = frame.gas-2 := by
    simp [Nat.mod_eq_of_lt (by omega : frame.gas-2 < 2^256)]
  have boundary :
      instruction cfg world (MessageBoundary.afterGas frame) =
        .ok (.call (site.message kind outputMemory) site.outputOffset.toNat site.outputSize.toNat
          (frame.gas-2) code checkpoint (site.prepared frame outputMemory (charge+extra))) := by
    rw [sampled_action cfg world frame site kind base fork messageOp messageSchema stack space]
    rw [resolve]
    simp only [beq_iff_eq, ite_eq_right (fun h => False.elim (known h))]
    rw [memory_paid world (MessageBoundary.afterGas frame) _ _ _ _ inputMemory enoughInput input]
    let expanded := {MessageBoundary.afterGas frame with
      memory := inputMemory
      gas := frame.gas-2-charge}
    have outPaid : ¬expanded.gas < 0 +
        memoryCharge expanded.memory site.outputOffset.toNat site.outputSize.toNat := by
      change ¬frame.gas-2-charge < 0 + extra
      simpa only [Nat.zero_add] using enoughOutput
    change withMemory world expanded 0 _ _ _ = _
    rw [memory_paid world expanded 0 _ _ _ outputMemory outPaid output]
    simp [expanded, MessageBoundary.afterGas, requested, Operands.prepared, charge, extra,
      Operands.charge, Operands.outputCharge, Nat.sub_add_eq, Nat.add_assoc]
  have admitted : (decide ((site.prepared frame outputMemory (charge+extra)).environment.depth ≥ 1024) ||
      (((site.message kind outputMemory).kind == .call ||
        (site.message kind outputMemory).kind == .callcode) &&
        (account checkpoint frame.environment.address).balance.ult
          (site.message kind outputMemory).value)) = false := by
    simp [Operands.prepared, Operands.message, Nat.not_le_of_gt depth, BitVec.ult]
  have paidCap : debit ≤
      min (frame.gas-2) (cap (site.prepared frame outputMemory (charge+extra)).gas) := by
    have bound : cap (site.prepared frame outputMemory (charge+extra)).gas ≤ frame.gas-2 :=
      Nat.le_trans (Nat.sub_le ..) (Nat.sub_le ..)
    rw [Nat.min_eq_right bound]
    simpa [cap, Operands.prepared, Nat.sub_add_eq] using within
  unfold execute
  rw [MessageBoundary.sample cfg world frame gasOp gasSchema space,
    ite_eq_right (fun hit => False.elim (enoughGas hit))]
  simp only [Bind.bind, Except.bind]
  rw [boundary]
  exact CallRefinement.stable_zero_value_call cfg child (site.message kind outputMemory)
    site.outputOffset.toNat site.outputSize.toNat (frame.gas-2) code checkpoint
    (site.prepared frame outputMemory (charge+extra)) reply debit fork
    (by simp [Calls.hasValue, Operands.message]) admitted paidCap stable

structure PhysicalCut (kind : Kind) where
  image : List Byte
  pc : Nat
  gasOpcode : image[pc]? = some 0x5a
  messageOpcode : image[pc+1]? = some kind.opcode

def PhysicalCut.frame (cut : PhysicalCut kind) (live : Frame) (site : Operands)
    (gas : Nat) : Frame :=
  {live with code := cut.image, pc := cut.pc, stack := site.stack kind, gas}

/-- Shared physical inputs for the two boundaries. Gas and the image position
are supplied separately for each side. -/
structure Placement (cfg : Config) (world : Journal) (live : Frame)
    (site : Operands) (kind : Kind) where
  base : Nat
  code : ExecutionCode
  resolution : Nat
  checkpoint : Journal
  inputMemory : List Byte
  outputMemory : List Byte
  fork : cfg.fork = .cancun
  gasSchema : available cfg 0x5a = ⟨true,2,0,0,1⟩
  messageSchema : available cfg kind.opcode.toNat = ⟨true,base,0,kind.inputs,1⟩
  space : (site.stack kind).length < 1024
  known : cfg.unknownCode ≠ some code.address
  resolve : executionCode cfg.fork world site.destination = (code,resolution,checkpoint)
  input : expand live.memory site.inputOffset.toNat site.inputSize.toNat = .ok inputMemory
  output : expand inputMemory site.outputOffset.toNat site.outputSize.toNat = .ok outputMemory
  depth : live.environment.depth < 1024

def Placement.debit (place : Placement cfg world live site kind) : Nat :=
  site.charge cfg world kind live.memory place.resolution + site.outputCharge place.inputMemory

structure ChildPolicy (place : Placement cfg world live site kind) (child : ChildExecutor) where
  reply : MessageOutcome
  debit : Nat
  stable : ∀ gas, debit ≤ gas →
    executeCall child cfg (warmAddress place.checkpoint site.destination) live.environment
      (site.message kind place.outputMemory) place.code gas =
        .ok {reply with gas := gas-debit}

theorem resumes (place : Placement cfg world live site kind)
    (policy : ChildPolicy place child) (cut : PhysicalCut kind) (gas : Nat)
    (word : gas < 2^256) (afford : 2 + place.debit ≤ gas)
    (enough : policy.debit ≤ cap (gas-2-place.debit)) :
    execute cfg child world (cut.frame live site gas) =
      .ok (.inr (policy.reply.work,policy.reply.world,
        site.resumed (cut.frame live site gas) place.outputMemory place.debit
          policy.reply policy.debit)) := by
  apply paid cfg child world (cut.frame live site gas) site kind place.base place.code
    place.resolution place.checkpoint place.inputMemory place.outputMemory policy.reply policy.debit
    place.fork cut.gasOpcode cut.messageOpcode place.gasSchema place.messageSchema
    rfl place.space place.known place.resolve place.input place.output place.depth word
  · simpa only [Placement.debit, PhysicalCut.frame, Nat.add_assoc] using afford
  · simpa only [Placement.debit, PhysicalCut.frame, Nat.sub_add_eq] using enough
  · exact policy.stable

/-- Compose the checked window with both actual GAS/message transitions.
Only a reached fallthrough calls the child. Branches taken before the cut
retain their exits by the window proof. Parent frames at resume differ in
image/PC and the monotone gas balance. The complete child reply is shared. -/
theorem contextual {Halt : Type} (kind : Kind) (common : List String)
    (before after : List Effects.Op) {allowSourceFault : Bool}
    (middle : ∀ ctx stack, Effects.Refines (Effects.expand ctx .cancun after stack 0)
      (Effects.expand ctx .cancun before stack 0) allowSourceFault)
    (enter : List String → Suffix.PrefixResult (Journal × Frame) Halt)
    (handle : Effects.Handler (Journal × Frame))
    (cfg : Config) (child : ChildExecutor) (world : Journal) (live : Frame) (site : Operands)
    (place : Placement cfg world live site kind) (policy : ChildPolicy place child)
    (oldCut newCut : PhysicalCut kind) (gas : Nat)
    (reached : Suffix.execute enter handle .cancun common before =
      .done (.continuation (site.stack kind)) gas (world,live))
    (oldWord : gas < 2^256)
    (wordDomain : ∀ more, Suffix.execute enter handle .cancun common after =
      .done (.continuation (site.stack kind)) more (world,live) → more < 2^256)
    (afford : 2 + place.debit ≤ gas)
    (enough : policy.debit ≤ cap (gas-2-place.debit)) :
    ∃ more, gas ≤ more ∧
      Suffix.execute enter handle .cancun common after =
        .done (.continuation (site.stack kind)) more (world,live) ∧
      execute cfg child world (oldCut.frame live site gas) =
        .ok (.inr (policy.reply.work,policy.reply.world,
          site.resumed (oldCut.frame live site gas) place.outputMemory place.debit
            policy.reply policy.debit)) ∧
      execute cfg child world (newCut.frame live site more) =
        .ok (.inr (policy.reply.work,policy.reply.world,
          site.resumed (newCut.frame live site more) place.outputMemory place.debit
            policy.reply policy.debit)) ∧
      (site.resumed (oldCut.frame live site gas) place.outputMemory place.debit
          policy.reply policy.debit).gas ≤
        (site.resumed (newCut.frame live site more) place.outputMemory place.debit
          policy.reply policy.debit).gas := by
  obtain ⟨more,reached',moreGas⟩ := Suffix.resource enter handle .cancun common before after
    middle (.continuation (site.stack kind)) gas (world,live) reached
  have bigger : gas-2-place.debit ≤ more-2-place.debit := by omega
  refine ⟨more,moreGas,reached',resumes place policy oldCut gas oldWord afford enough,
    resumes place policy newCut more (wordDomain more reached') (by omega)
      (Nat.le_trans enough (cap_mono bigger)),?_⟩
  simp only [Operands.resumed, Operands.prepared, PhysicalCut.frame]
  omega

end SolarTV.EVM.FundedCallWindow
