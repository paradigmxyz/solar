import SolarTV.EVMIR.State.Transactions.SetCode
import SolarTV.EVMIR.State.Transactions.Storage

/-! Decoded Amsterdam EIP-1559 message transactions. The outer signer and the
initial accounts are fixture inputs. The admitted transaction owns the upfront
fee purchase and nonce increment. Its message owns value transfer, EIP-7708
logs, storage, and the refundable reservation for a newly funded account. -/

namespace SolarTV.EVM.Transaction.AmsterdamMessage
open EvmRules SolarTV.Execution EVM.State Reservoir

structure Envelope where
  signer : Address
  destination : Address
  chain : Word
  nonce : Nat
  gasLimit : Nat
  maxFee : Word
  priorityFee : Word
  value : Word
  input : List Byte
  accessList : List (Address × List Word)

def baseCost (tx : Envelope) : Nat :=
  12000 + if tx.destination = tx.signer then 0 else
    3000 + if tx.value = 0 then 0 else 6000

def slotCount (tx : Envelope) : Nat :=
  (tx.accessList.map (·.2.length)).sum

def intrinsic (tx : Envelope) : Nat :=
  baseCost tx + 4 * SetCode.tokens tx.input +
    (2900 + 20 * 64) * tx.accessList.length +
    (2000 + 32 * 64) * slotCount tx

def floorGas (tx : Envelope) : Nat :=
  baseCost tx + 64 * (tx.input.length + 20 * tx.accessList.length + 32 * slotCount tx)

structure Prepared where
  world : Journal
  meter : Meter
  price : Nat

def prepare (cfg : Config) (world : Journal) (tx : Envelope) : Option Prepared := do
  let sender := account world tx.signer
  if tx.chain ≠ cfg.block.chainId || tx.priorityFee.toNat > tx.maxFee.toNat ||
      tx.maxFee.toNat < cfg.block.baseFee.toNat || tx.gasLimit > cfg.block.gasLimit.toNat ||
      tx.gasLimit < floorGas tx || floorGas tx > 2^24 ||
      sender.nonce ≠ tx.nonce || tx.nonce ≥ 2^64-1 ||
      !SetCode.eligible cfg.fork sender ||
      sender.balance.toNat < tx.gasLimit * tx.maxFee.toNat + tx.value.toNat then none else
  let meter ← begin tx.gasLimit (intrinsic tx)
  let price := min tx.maxFee.toNat (cfg.block.baseFee.toNat + tx.priorityFee.toNat)
  let updated := putAccount world {sender with
    nonce := sender.nonce+1
    balance := sender.balance - BitVec.ofNat 256 (tx.gasLimit*price)}
  let warmed := warmAddress (warmAddress (warmAddress updated tx.signer) tx.destination)
    cfg.block.beneficiary
  let precompiles := ((List.range 18).map (BitVec.ofNat 160) ++ [256]).filter (precompile cfg.fork)
  let world := tx.accessList.foldl SetCode.access (precompiles.foldl warmAddress warmed)
  return ⟨world,meter,price⟩

def accountStateGas : Nat := 120 * 1530

structure Dispatch where
  world : Journal
  code : ExecutionCode
  meter : Meter
  reserved : Nat

/-- EIP-2780 charges a new recipient reservation and delegation target access
before forwarding the transaction's remaining execution and reservoir pools. -/
def dispatch (cfg : Config) (tx : Envelope) (prepared : Prepared) : Option Dispatch := do
  let world := prepared.world
  let bytes := (account world tx.destination).code
  let reserved := if tx.value ≠ 0 && dead world tx.destination then accountStateGas else 0
  let meter ← prepared.meter.spendState reserved
  match delegationTarget cfg.fork bytes with
  | none => return ⟨world,⟨tx.destination,bytes,false⟩,meter,reserved⟩
  | some target =>
    let price := if world.warmAddresses.contains target then 100 else 3000
    let meter ← meter.spendExecution price
    return ⟨warmAddress world target,⟨target,(account world target).code,true⟩,meter,reserved⟩

def transferValue (world : Journal) (tx : Envelope) : Option Journal := do
  let world ← transfer world tx.signer tx.destination tx.value
  if tx.signer = tx.destination || tx.value == 0 then return world
  let topic : Word := 0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef
  let log : World.Log := ⟨0xfffffffffffffffffffffffffffffffffffffffe,
    [topic,tx.signer.setWidth 256,tx.destination.setWidth 256],wordBytes tx.value⟩
  return {world with logs := world.logs ++ [log]}

def exitKind : Stop → Exit
  | .success => .success
  | .reverted => .revert
  | .exception | .outOfGas => .halt

/-- At the transaction boundary an exceptional halt burns any execution gas
made available by releasing the failed frame's upfront account reservation. -/
def mergeRoot (parent child : FrameMeter) (reserved : Nat) (exit : Exit) : FrameMeter :=
  let parent := {parent with meter := {parent.meter with execution := 0}}
  let merged := parent.merge child exit
  match exit with
  | .success => merged
  | .revert => {merged with meter := merged.meter.refill reserved}
  | .halt => {merged with meter := {(merged.meter.refill reserved) with execution := 0}}

theorem halt_burns_restored_execution (parent child : FrameMeter) (reserved : Nat) :
    (mergeRoot parent child reserved .halt).meter.execution = 0 := rfl

theorem revert_discards_child_refund (parent child : FrameMeter) (reserved : Nat) :
    (mergeRoot parent child reserved .revert).refund = parent.refund := rfl

def runMessage (fuel : Nat) (cfg : Config) (tx : Envelope) (price : Nat) (entry : Dispatch) :
    Except String (Result × FrameMeter) := do
  let some entered := transferValue entry.world tx
    | .error "validated transaction cannot fund its value transfer"
  let meter : Meter := ⟨entry.meter.execution,entry.meter.reservoir,0,0⟩
  let env : World.Environment := ⟨tx.destination,entry.code.address,tx.signer,tx.signer,tx.value,
    BitVec.ofNat 256 price,tx.input,false,0⟩
  let outcome ← if !entry.code.delegated && precompile cfg.fork tx.destination then do
      -- The pinned Amsterdam registry uses Osaka's precompile activation and tariffs.
      let (stop,data,remaining) ← Precompiles.execute .osaka tx.destination tx.input meter.execution
      pure (⟨⟨stop,data,entered,remaining,0⟩,{meter with execution := remaining}⟩ : EVM.State.Amsterdam.Outcome)
    else EVM.State.Amsterdam.run cfg fuel entered ⟨env,entry.code.bytes,0,[],[],[],meter.execution⟩ meter
  let exit := exitKind outcome.result.stop
  let root := mergeRoot ⟨entry.meter,entry.world.refund⟩
    ⟨outcome.meter,outcome.result.world.refund - entry.world.refund⟩ entry.reserved exit
  let world := if exit = .success then finalize outcome.result.world else entry.world
  return ({outcome.result with world, gas := root.meter.execution},root)

def executePrepared (fuel : Nat) (cfg : Config) (tx : Envelope) (prepared : Prepared) :
    Except String (Result × Settlement.Gas) := do
  let (result,root) ← match dispatch cfg tx prepared with
    | none => pure (⟨.outOfGas,[],prepared.world,0,0⟩,
        ⟨{prepared.meter with execution := 0},prepared.world.refund⟩)
    | some entry => runMessage fuel cfg tx prepared.price entry
  let gas := Settlement.finish tx.gasLimit (floorGas tx) root.meter root.refund 0 0
  let world := Settlement.fees result.world tx.signer cfg.block.beneficiary gas
    tx.gasLimit (BitVec.ofNat 256 prepared.price) cfg.block.baseFee
  return ({result with world},gas)

def execute (fuel : Nat) (cfg : Config) (accounts : List Account) (tx : Envelope) :
    Except String (Option (Result × Settlement.Gas)) := do
  if cfg.fork ≠ .amsterdam then .error "expected an Amsterdam dynamic-fee transaction"
  else if tx.gasLimit ≥ 2^32 then .error "Amsterdam transaction fixture exceeds the gas model bound"
  else
    let world : Journal := ⟨accounts,accounts,[],[],[],0⟩
    match prepare cfg world tx with
    | none => return none
    | some prepared => return some (← executePrepared fuel cfg tx prepared)

end SolarTV.EVM.Transaction.AmsterdamMessage
