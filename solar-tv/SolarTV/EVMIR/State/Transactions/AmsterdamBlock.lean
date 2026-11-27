import SolarTV.EVMIR.State.Transactions.AmsterdamMessage
import SolarTV.EVMIR.State.Transactions.BlockGas

/-! Amsterdam gas accounting across a sequence of decoded dynamic-fee
messages. Each transaction starts with the previous committed accounts and
a fresh transaction journal. Receipt gas and the header counters have
separate definitions. -/

namespace SolarTV.EVM.Transaction.AmsterdamBlock
open SolarTV.Execution EVM.State AmsterdamMessage
open Settlement (BlockGas Gas)

structure Receipt where
  stop : Stop
  data : List Byte
  logs : List World.Log
  gas : Gas
  cumulativeGas : Nat
  deriving DecidableEq

structure Result where
  accounts : List Account
  gas : BlockGas
  receipts : List Receipt
  deriving DecidableEq

private def runFrom (fuel : Nat) (cfg : Config) :
    List Envelope → List Account → BlockGas → List Receipt → Except String (Option Result)
  | [],accounts,used,receipts => .ok (some ⟨accounts,used,receipts.reverse⟩)
  | tx :: rest,accounts,used,receipts => do
    if ¬ used.CanInclude cfg.block.gasLimit.toNat tx.gasLimit then return none
    let some (outcome,gas) ← AmsterdamMessage.execute fuel cfg accounts tx | return none
    let used := used.record gas
    let receipt := ⟨outcome.stop,outcome.data,outcome.world.logs,gas,used.receipts⟩
    runFrom fuel cfg rest outcome.world.accounts used (receipt :: receipts)

def execute (fuel : Nat) (cfg : Config) (accounts : List Account) (transactions : List Envelope) :
    Except String (Option Result) := do
  if cfg.fork ≠ .amsterdam then .error "expected an Amsterdam dynamic-fee block"
  else runFrom fuel cfg transactions accounts {} []

def verifyHeader (result : Result) (expectedGasUsed : Nat) : Bool :=
  result.gas.header == expectedGasUsed

end SolarTV.EVM.Transaction.AmsterdamBlock
