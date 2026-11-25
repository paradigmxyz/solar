import SolarTV.EVMIR.State.Interpreter
import Lean.Data.Json

/-! Observable transaction effects for explicit-world entry checks. Private memory,
transient storage and access warmth die with the root frame. Persistent accounts,
nonces, balances, deployed code and logs remain visible. The running root's code
is compared through execution, not as an output byte-string, so ordinary code
rewrites do not count as state changes. CODECOPY/EXTCODEHASH still read the actual
before/after root image during execution.

Model errors are excluded on both sides of a positive proof. Their textual
observations let the host reject a proposed witness before claiming a compiler
counterexample. Gas/refund observations are optional, but execution always pays
gas from the explicit fixture budget. -/

namespace SolarTV.EVM.State
open EvmRules

structure Observation where
  stop : Stop
  data : List Byte
  gas : Nat
  refund : Int
  accounts : List Account
  logs : List World.Log
  deriving DecidableEq

inductive Policy where
  | effects | exact | refine
  deriving DecidableEq

export SolarTV.Execution (orderedInsert ordered normalizedSlots)

def observe (cfg : Config) (policy : Policy) (result : Result) : Observation :=
  let accounts := result.world.accounts.map (fun a => {a with
    code := if a.address == cfg.root then [] else a.code
    storage := normalizedSlots a.storage
    transient := []
    created := false
    destroyed := false})
  ⟨result.stop, result.data, if policy = .effects then 0 else result.gas,
    if policy = .effects then 0 else result.world.refund,
    ordered (fun a b => a.address.ule b.address) accounts, result.world.logs⟩

def equivalent (cfgBefore cfgAfter : Config) (policy : Policy) (before after : Run) : Prop :=
  match before, after with
  | .ok a, .ok b =>
    if policy = .refine then
      (a.stop = .outOfGas ∨
        (observe cfgBefore .effects a = observe cfgAfter .effects b ∧
          a.world.refund = b.world.refund ∧ a.gas ≤ b.gas))
    else observe cfgBefore policy a = observe cfgAfter policy b
  | _, _ => False

instance (a b : Config) (policy : Policy) (before after : Run) :
    Decidable (equivalent a b policy before after) := by
  unfold equivalent
  split <;> infer_instance

private def wordJson (value : Word) : Lean.Json := .str (toString value.toNat)
private def byteJson (value : Byte) : Lean.Json := Lean.toJson value.toNat

def describe (cfg : Config) (policy : Policy) (result : Run) : String :=
  match result with
  | .error reason => "unsupported: " ++ reason
  | .ok result =>
    let result := observe cfg policy result
    let accounts := result.accounts.map fun a => Lean.Json.arr #[
      .str (toString a.address.toNat), .str (toString a.nonce), wordJson a.balance,
      .arr (a.code.map byteJson).toArray,
      .arr (a.storage.map (fun entry => Lean.Json.arr #[wordJson entry.1, wordJson entry.2])).toArray]
    let logs := result.logs.map fun log => Lean.Json.arr #[
      .str (toString log.address.toNat), .arr (log.topics.map wordJson).toArray,
      .arr (log.data.map byteJson).toArray]
    let stop := match result.stop with
      | .success => "success" | .reverted => "revert"
      | .exception => "exception" | .outOfGas => "out-of-gas"
    (Lean.Json.arr #[.str stop, .arr (result.data.map byteJson).toArray,
      Lean.toJson result.gas, Lean.toJson result.refund, .arr accounts.toArray, .arr logs.toArray]).compress

end SolarTV.EVM.State
