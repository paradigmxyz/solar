import SolarTV.EVMIR.State.Selfdestruct
import SolarTV.EVMIR.State.Delegation
import SolarTV.EVMIR.Precompiles.Execute

/-! Shared execution of prepared call and creation messages. The caller owns
memory access and parent gas pricing/forwarding; this module owns child entry,
state checkpoints and completion. A child is executed by the supplied evaluator,
never an unconstrained result. MIR can use the bytecode evaluator for fixture
accounts without lowering its own program to bytecode. -/

namespace SolarTV.EVM.State
open EvmRules

structure MessageOutcome where
  value : Word
  data : List Byte
  world : Journal
  gas : Nat
  work : Nat

abbrev ChildExecutor := Journal → Frame → Run

def executeCall (execute : ChildExecutor) (cfg : Config) (checkpoint : Journal)
    (parent : World.Environment) (message : Calls.Message) (code : ExecutionCode)
    (supplied : Nat) : Except String MessageOutcome := do
  if cfg.unknownCode == some code.address then .error "MIR root runtime code is unknown" else
  if parent.depth ≥ 1024 ||
      ((message.kind == .call || message.kind == .callcode) &&
        (account checkpoint parent.address).balance.ult message.value) then
    return ⟨0,[],checkpoint,supplied,0⟩
  else
    let entered := if message.kind == .call then
      (transfer checkpoint parent.address message.destination message.value).getD checkpoint
      else checkpoint
    let child ← if !code.delegated && precompile cfg.fork message.destination then do
        let (stop,data,gas) ← Precompiles.execute cfg.fork message.destination message.input supplied
        pure ⟨stop,data,entered,gas,0⟩
      else execute entered {
        environment := {Calls.childEnvironment parent message with codeAddress := code.address}
        code := code.bytes
        gas := supplied}
    let failed := child.stop = .exception || child.stop = .outOfGas
    return ⟨if child.stop = .success then 1 else 0,
      if failed then [] else child.data,
      if child.stop = .success then child.world else checkpoint,
      if failed then 0 else child.gas,child.work⟩

def executeCreation (execute : ChildExecutor) (cfg : Config) (world : Journal)
    (parent : World.Environment) (salt : Option Word) (value : Word)
    (initcode : List Byte) (supplied : Nat) : Except String MessageOutcome := do
  let creator := account world parent.address
  if parent.depth ≥ 1024 || creator.balance.ult value || creator.nonce ≥ 2^64 - 1 then
    return ⟨0,[],world,supplied,0⟩
  else
    let target := match salt with
      | none => createAddress creator.address creator.nonce
      | some salt => create2Address creator.address salt initcode
    let checkpoint := warmAddress (putAccount world {creator with nonce := creator.nonce + 1}) target
    if collision checkpoint target then return ⟨0,[],checkpoint,0,0⟩ else
    let old := account checkpoint target
    let fresh := {old with
      nonce := if cfg.fork.atLeast .spuriousDragon then 1 else 0
      code := []
      storage := []
      transient := []
      created := true
      destroyed := false}
    let entered := putAccount checkpoint fresh
    let entered := {entered with original := fresh :: entered.original.filter (fun a => a.address != target)}
    let entered := (transfer entered creator.address target value).getD entered
    let child ← execute entered {
      environment := {parent with
        address := target
        codeAddress := target
        caller := creator.address
        value
        input := []
        depth := parent.depth + 1}
      code := initcode,gas := supplied}
    let deposit := 200 * child.data.length
    let success := child.stop == .success && validDeployedCode cfg.fork child.data && deposit ≤ child.gas
    return ⟨if success then target.setWidth 256 else 0,
      if child.stop = .reverted then child.data else [],
      if success then putAccount child.world {(account child.world target) with code := child.data} else checkpoint,
      if success then child.gas - deposit else if child.stop = .reverted then child.gas else 0,child.work⟩

end SolarTV.EVM.State
