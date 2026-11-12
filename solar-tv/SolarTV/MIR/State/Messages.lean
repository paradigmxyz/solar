import SolarTV.MIR.State.Ops
import SolarTV.EVMIR.State.Interpreter

/-! Direct MIR message boundaries. Explicit CALL gas is forwarded without a
caller-side cap under the sufficient-caller-gas assumption. CREATE's child gas
is an explicit boundary input; it is not invented from an unpriced MIR body.
Children execute real fixture bytecode with fork-specific gas, precompiles and
journaling. The current MIR runtime's code is unknown, so inspecting or executing
it from a child remains unsupported instead of compiling it through the backend.
Missing creation budgets make a scenario unsupported; an unused suffix is allowed. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

abbrev MessageStep := Except String (Outcome (Option Word × Frame) × Nat)

def external (fuel : Nat) (cfg : Config) (frame : Frame) (opcode : Nat)
    (args : List Word) : MessageStep := do
  let externalConfig ← required cfg.externalConfig "missing MIR external execution configuration"
  if cfg.fork.atLeast .amsterdam then .error "Amsterdam MIR message gas is not modeled" else
  match opcode,args with
  | 0xf1,_ | 0xf2,_ | 0xf4,_ | 0xfa,_ =>
    let explicitValue := opcode = 0xf1 || opcode = 0xf2
    if args.length != (if explicitValue then 7 else 6) then
      .error "MIR call operand count" else
    let gas := args[0]?.getD 0
    let destination := args[1]?.getD 0
    let start := if explicitValue then 3 else 2
    let inputOffset := args[start]?.getD 0
    let inputSize := args[start + 1]?.getD 0
    let outputOffset := args[start + 2]?.getD 0
    let outputSize := args[start + 3]?.getD 0
    let amount := if opcode = 0xf1 || opcode = 0xf2 then args[2]?.getD 0 else 0
    let kind := if opcode = 0xf1 then Calls.Kind.call else if opcode = 0xf2 then .callcode
      else if opcode = 0xf4 then .delegatecall else .staticcall
    if cfg.environment.isStatic && kind == .call && amount != 0 then return (.invalid,0) else
    if gas.toNat > 30000000 then .error "MIR requested child gas exceeds 30000000" else
    let memory ← expand frame.memory inputOffset.toNat inputSize.toNat
    let memory ← expand memory outputOffset.toNat outputSize.toNat
    let (code,_,world) := EVM.State.executionCode cfg.fork frame.world (destination.setWidth 160)
    let message := ⟨kind,destination.setWidth 160,amount,readBytes memory inputOffset.toNat inputSize.toNat⟩
    let child ← EVM.State.executeCall (EVM.State.run fuel externalConfig) externalConfig world
      cfg.environment message code (gas.toNat + if Calls.hasValue message then 2300 else 0)
    let next := {frame with
      world := child.world
      memory := writeBytes memory outputOffset.toNat (child.data.take outputSize.toNat)
      returnData := child.data}
    return (.returned (some child.value,next),child.work)
  | 0xf0,[amount,offset,size] | 0xf5,[amount,offset,size,_] =>
    if cfg.environment.isStatic then return (.invalid,0) else
    if cfg.fork.atLeast .shanghai && size.toNat > 49152 then return (.invalid,0) else
    let supplied ← required cfg.creationGas[frame.creationIndex]? "missing MIR creation gas budget"
    let memory ← expand frame.memory offset.toNat size.toNat
    let salt := if opcode = 0xf5 then args[3]? else none
    let child ← EVM.State.executeCreation (EVM.State.run fuel externalConfig) externalConfig frame.world
      cfg.environment salt amount (readBytes memory offset.toNat size.toNat) supplied
    return (.returned (some child.value,{frame with
      world := child.world
      memory
      returnData := child.data
      creationIndex := frame.creationIndex + 1}),child.work)
  | _,_ => .error "unmodeled MIR external message or operand count"

end SolarTV.MIR.State
