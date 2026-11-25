import SolarTV.EVMIR.State.Interpreter
import SolarTV.EVMIR.State.Transactions.NestedReservoir

/-! Amsterdam persistent-storage gas on the bytecode interpreter's frame
boundary. Charge execution before state gas, after enforcing the SSTORE
sentry. The two-resource meter is threaded between ordinary bytecode steps.
A failed frame restores its journal and inherited reservoir.

The state byte tariff and execution prices follow the pinned devnet version.
Message, creation and account-access opcodes retain their existing unsupported
status. The starting meter is supplied by the enclosing frame or transaction. -/

namespace SolarTV.EVM.State.Amsterdam
open EvmRules SolarTV.Execution
open SolarTV.EVM.Transaction.Reservoir (Meter FrameMeter Exit)

def slotStateGas : Nat := 64 * 1530

structure StorePrice where
  execution : Nat
  state : Nat
  refill : Nat
  refund : Int
  deriving DecidableEq

def storePrice (warm : Bool) (original current value : Word) : StorePrice :=
  let load := if warm then 100 else 2100
  let write := 10000
  let clear : Int := 11616
  if current = value then ⟨load,0,0,0⟩
  else if original = current then
    ⟨load+write,if original = 0 then slotStateGas else 0,0,
      if value = 0 then clear else 0⟩
  else
    let remove := if original ≠ 0 ∧ current = 0 then -clear else 0
    let add := if original ≠ 0 ∧ value = 0 then clear else 0
    let restored : Int := if original = value then write else 0
    ⟨load,0,if original = 0 ∧ value = 0 then slotStateGas else 0,
      remove + add + restored⟩

def StorePrice.debit (price : StorePrice) (meter : Meter) : Option Meter := do
  let meter ← meter.spendExecution price.execution
  let meter ← meter.spendState price.state
  return meter.refill price.refill

theorem new_cold_slot (value : Word) (changed : value ≠ 0) :
    storePrice false 0 0 value = ⟨12100,97920,0,0⟩ := by
  have nonzero : value ≠ 0#256 := changed
  simp [storePrice,slotStateGas,Ne.symm nonzero,nonzero]

theorem restore_zero_slot (value : Word) (changed : value ≠ 0) :
    storePrice true 0 value 0 = ⟨100,0,97920,10000⟩ := by
  have nonzero : value ≠ 0#256 := changed
  simp [storePrice,slotStateGas,Ne.symm nonzero,nonzero]

theorem cannot_pay_execution_from_reservoir (price : StorePrice) (meter : Meter)
    (short : meter.execution < price.execution) : price.debit meter = none := by
  simp [StorePrice.debit,Meter.spendExecution,short]

inductive Step where
  | next (world : Journal) (frame : Frame) (meter : Meter)
  | done (result : Result) (meter : Meter)

abbrev Action := Except String Step

def fail (world : Journal) (meter : Meter) (stop : Stop) : Action :=
  .ok (.done ⟨stop,[],world,0,0⟩ meter)

private def storageStep (cfg : Config) (opcode : Nat) (world : Journal) (frame : Frame)
    (meter : Meter) : Action := do
  let info := available cfg opcode
  if !info.available || frame.stack.length < info.inputs ||
      frame.stack.length - info.inputs + info.outputs > 1024 then
    fail world meter .exception
  else
    let key := frame.stack.headD 0
    let owner := frame.environment.address
    let old := account world owner
    let current := slotsRead old.storage key
    let warm := world.warmSlots.contains (owner,key)
    if opcode = 0x54 then
      match meter.spendExecution (Storage.readPrice .amsterdam warm) with
      | none => fail world meter .outOfGas
      | some paid =>
        let resumed := {frame with
          pc := frame.pc+1
          stack := current :: frame.stack.drop 1
          gas := paid.execution}
        return .next (warmSlot world owner key) resumed paid
    else if frame.environment.isStatic then fail world meter .exception
    else if meter.execution ≤ 2300 then fail world meter .outOfGas
    else
      let original := (world.original.find? (fun a => a.address == owner)).getD {address := owner}
      let value := frame.stack[1]?.getD 0
      let price := storePrice warm (slotsRead original.storage key) current value
      match price.debit meter with
      | none => fail world meter .outOfGas
      | some paid =>
        let worlds := warmSlot world owner key
        let updated := putAccount {worlds with refund := world.refund + price.refund}
          {old with storage := slotsWrite old.storage key value}
        return .next updated
          {frame with pc := frame.pc+1, stack := frame.stack.drop 2, gas := paid.execution} paid

def instruction (cfg : Config) (world : Journal) (frame : Frame) (meter : Meter) : Action := do
  let frame := {frame with gas := meter.execution}
  let opcode := (frame.code[frame.pc]?.getD 0).toNat
  if opcode = 0x54 || opcode = 0x55 then storageStep cfg opcode world frame meter
  else
    match ← SolarTV.EVM.State.instruction cfg world frame with
    | .next world frame => return .next world frame {meter with execution := frame.gas}
    | .done result => return .done result {meter with execution := result.gas}
    | .call .. | .create .. => .error "Amsterdam message and creation gas is outside this fragment"

structure Outcome where
  result : Result
  meter : Meter
  deriving DecidableEq

def run (cfg : Config) : Nat → Journal → Frame → Meter → Except String Outcome
  | 0, _, _, _ => .error "Amsterdam world instruction evaluation limit"
  | fuel + 1, world, frame, meter => do
    match ← instruction cfg world frame meter with
    | .done result meter => return ⟨{result with work := result.work+1},meter⟩
    | .next world frame meter =>
      let result ← run cfg fuel world frame meter
      return {result with result := {result.result with work := result.result.work+1}}

def execute (cfg : Config) (fuel : Nat) (world : Journal) (frame : Frame) (meter : Meter) :
    Except String Outcome := do
  if cfg.fork ≠ .amsterdam then .error "Amsterdam transaction meter requires Amsterdam" else
    let ⟨result,final⟩ ← run cfg fuel world frame meter
    let exit : Exit := match result.stop with
      | .success => .success
      | .reverted => .revert
      | .exception | .outOfGas => .halt
    let gas := ((⟨final,0⟩ : FrameMeter).settle exit).meter
    let result := {result with
      gas := gas.execution
      world := if exit = .success then finalize result.world else world}
    return ⟨result,gas⟩

end SolarTV.EVM.State.Amsterdam
