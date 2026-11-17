import SolarTV.MIR.State.Ops
import SolarTV.EVMIR.State.Interpreter

/-! Direct MIR message boundaries. Optional caller budgets price memory growth,
account access, value and creation charges before EIP-150 forwarding. Omission
retains the sufficient-caller-gas mode and explicit CREATE child budgets. Neither
mode assigns physical gas to an unpriced MIR body.
Implicit typed calls require EIP-150 and a caller budget. Builtin lowering's
GAS sample precedes CALL's charges, so clipping yields the cap; it is not a
stand-in for an arbitrary GAS instruction elsewhere in the MIR body.
Children execute real fixture bytecode with fork-specific gas, precompiles and
journaling. The current MIR runtime's code is unknown, so inspecting or executing
it from a child remains unsupported instead of compiling it through the backend.
Missing creation budgets make a scenario unsupported; an unused suffix is allowed. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

/-- An absent result is caller out-of-gas; evaluator errors remain unsupported. -/
abbrev MessageStep := Except String (Option (Outcome (Option Word × Frame) × Nat))

def external (fuel : Nat) (cfg : Config) (frame : Frame) (opcode : Nat)
    (args : List Word) (gas : Option Word) : MessageStep := do
  let externalConfig ← required cfg.externalConfig "missing MIR external execution configuration"
  if cfg.fork.atLeast .amsterdam then .error "Amsterdam MIR message gas is not modeled" else
  match opcode,args with
  | 0xf1,_ | 0xf2,_ | 0xf4,_ | 0xfa,_ =>
    let explicitValue := opcode = 0xf1 || opcode = 0xf2
    if args.length != (if explicitValue then 6 else 5) then
      .error "MIR call operand count" else
    let destination := args[0]?.getD 0
    let start := if explicitValue then 2 else 1
    let inputOffset := args[start]?.getD 0
    let inputSize := args[start + 1]?.getD 0
    let outputOffset := args[start + 2]?.getD 0
    let outputSize := args[start + 3]?.getD 0
    let amount := if explicitValue then args[1]?.getD 0 else 0
    let kind := if opcode = 0xf1 then Calls.Kind.call else if opcode = 0xf2 then .callcode
      else if opcode = 0xf4 then .delegatecall else .staticcall
    if cfg.environment.isStatic && kind == .call && amount != 0 then return some (.invalid,0) else
    if gas.any (fun gas => gas.toNat > 30000000) then .error "MIR requested child gas exceeds 30000000" else
    if gas.isNone && (cfg.messageGas.isNone || !cfg.fork.atLeast .tangerineWhistle) then
      .error "implicit address-call gas requires caller pricing" else
    let memory ← expand frame.memory inputOffset.toNat inputSize.toNat
    let memory ← expand memory outputOffset.toNat outputSize.toNat
    let (code,resolution,world) := EVM.State.executionCode cfg.fork frame.world (destination.setWidth 160)
    let message := ⟨kind,destination.setWidth 160,amount,readBytes memory inputOffset.toNat inputSize.toNat⟩
    let budget ← match cfg.messageGas with
      | none => do
        let gas ← required gas "implicit address-call gas requires caller pricing"
        pure (some (gas.toNat + if Calls.hasValue message then 2300 else 0,0))
      | some budgets => do
        let available ← required budgets[frame.messageGas.length]? "missing MIR caller gas budget"
        let charge := EVM.State.callBoundaryCharge externalConfig frame.world message opcode resolution
          (memoryWords frame.memory) (memoryWords memory)
        if available < charge then pure none else
          let requested := gas.map BitVec.toNat |>.getD (available-charge)
          pure ((Calls.forwardGas cfg.fork (available-charge) requested (Calls.hasValue message)).map
            (fun budget => (budget.supplied,available-charge-budget.charged)))
    let some (supplied,retained) := budget | return none
    let child ← EVM.State.executeCall (EVM.State.run fuel externalConfig) externalConfig world
      cfg.environment message code supplied
    let next := {frame with
      world := child.world
      memory := writeBytes memory outputOffset.toNat (child.data.take outputSize.toNat)
      returnData := child.data
      messageGas := if cfg.messageGas.isSome then frame.messageGas ++ [retained+child.gas] else frame.messageGas}
    return some (.returned (some child.value,next),child.work)
  | 0xf0,[amount,offset,size] | 0xf5,[amount,offset,size,_] =>
    if cfg.environment.isStatic then return some (.invalid,0) else
    if cfg.fork.atLeast .shanghai && size.toNat > 49152 then return none else
    let memory ← expand frame.memory offset.toNat size.toNat
    let salt := if opcode = 0xf5 then args[3]? else none
    let budget ← match cfg.messageGas with
      | none => do
        let supplied ← required cfg.creationGas[frame.creationIndex]? "missing MIR creation gas budget"
        pure (some (supplied,0))
      | some budgets => do
        let available ← required budgets[frame.messageGas.length]? "missing MIR caller gas budget"
        let charge := EVM.State.createBoundaryCharge externalConfig (opcode = 0xf5) size.toNat
          (memoryWords frame.memory) (memoryWords memory)
        if available < charge then pure none else
          let remaining := available-charge
          let supplied := if cfg.fork.atLeast .tangerineWhistle then remaining-remaining/64 else remaining
          pure (some (supplied,remaining-supplied))
    let some (supplied,retained) := budget | return none
    let child ← EVM.State.executeCreation (EVM.State.run fuel externalConfig) externalConfig frame.world
      cfg.environment salt amount (readBytes memory offset.toNat size.toNat) supplied
    return some (.returned (some child.value,{frame with
      world := child.world
      memory
      returnData := child.data
      creationIndex := frame.creationIndex + 1
      messageGas := if cfg.messageGas.isSome then frame.messageGas ++ [retained+child.gas] else frame.messageGas}),child.work)
  | _,_ => .error "unmodeled MIR external message or operand count"

/-- Reading an object's header precedes the message; a slice needs no memory read. -/
def messageArguments (cfg : Config) (frame : Frame) (op : MessageOp) (operands : List Operand) :
    Except String (Nat × List Word × Frame × Option Word) := do
  match op,operands with
  | .raw opcode,_ =>
    let args ← operands.mapM (read frame)
    if opcode == 0xf1 || opcode == 0xf2 || opcode == 0xf4 || opcode == 0xfa then
      return (opcode,args.drop 1,frame,some (← required args[0]? "missing call gas"))
    else return (opcode,args,frame,none)
  | .address opcode hasGas hasValue hasInput outputWord clearWord,address :: operands =>
    if operands.length != (if hasInput then 1 else 0) + (if hasGas then 1 else 0) + (if hasValue then 1 else 0) ||
        (opcode != 0xf1 && hasValue) then .error "MIR address-call operand count" else
    let address ← read frame address
    let input ← if hasInput then some <$> readValue frame (← required operands[0]? "missing call input") else pure none
    let rest := operands.drop (if hasInput then 1 else 0)
    let gas ← if hasGas then some <$> read frame (← required rest[0]? "missing call gas") else pure none
    let amount ← if hasValue then read frame (← required rest[if hasGas then 1 else 0]? "missing call value") else pure 0
    let (offset,length,frame) ← match input with
      | none => pure (0,0,frame)
      | some (.scalar object) => do
        let memory ← expand frame.memory object.word.toNat 32
        pure (object.word + 32,readWord memory object.word.toNat,{frame with memory})
      | some (.aggregate [pointer,length]) => pure (← pointer.word,← length.word,frame)
      | _ => .error "undefined address-call input"
    let frame ← if outputWord && clearWord then do
        let memory ← expand frame.memory 0 32
        pure {frame with memory := writeBytes memory 0 (wordBytes 0)}
      else pure frame
    let opcode ← if opcode == 0xfa && !cfg.available.contains 0xfa then
        if 1 ≤ address.toNat && address.toNat ≤ 4 then pure 0xf1
        else .error "pre-Byzantium static extcall requires a precompile"
      else pure opcode
    let args := [address] ++ (if opcode = 0xf1 then [amount] else []) ++ [offset,length,0,if outputWord then 32 else 0]
    return (opcode,args,frame,gas)
  | _,_ => .error "MIR address-call operand count"

end SolarTV.MIR.State
