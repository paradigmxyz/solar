import SolarTV.MIR.State.Interpreter
import Lean.Data.Json

/-! Observable direct MIR effects at the selected boundary. A normal internal
return exposes all live memory and transient storage to its caller. STOP/raw
RETURN end the transaction, so private memory and transient storage disappear.
Revert/exception roll back state and expose no memory. No gas placeholders or
backend image hashes are part of this observation. Model errors on either side
exclude a proof, including identical nontermination or resource exhaustion. A source
run that fails an `assume` is undefined and constrains no completed target run. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

structure Observation where
  status : String
  values : List Word
  data : List Byte
  memory : Option (List Byte)
  accounts : List Account
  logs : List World.Log
  returnData : List Byte
  warmAddresses : List Address
  warmSlots : List (Address × Word)
  deriving DecidableEq

def observe (result : Result) : Observation :=
  let (status,values,data,memory,keepTransient) := match result.completion with
    | .returned values => ("return",values,[],some result.memory,true)
    | .success data => ("success",[],data,none,false)
    | .reverted data => ("revert",[],data,none,false)
    | .invalid => ("exception",[],[],none,false)
    | .undefined => ("undefined",[],[],none,false)
  let accounts := result.world.accounts.map fun a => {a with
    storage := normalizedSlots a.storage
    transient := if keepTransient then normalizedSlots a.transient else []
    created := keepTransient && a.created
    destroyed := keepTransient && a.destroyed}
  ⟨status,values,data,memory,ordered (fun a b => a.address.ule b.address) accounts,result.world.logs,
    if keepTransient then result.returnData else [],
    if keepTransient then ordered (fun a b => a.ule b) result.world.warmAddresses.eraseDups else [],
    if keepTransient then ordered (fun a b => if a.1 == b.1 then a.2.ule b.2 else a.1.ule b.1) result.world.warmSlots.eraseDups else []⟩

/-- The target refines the source: equal observations, unless the source run is undefined (a
failed `assume`), which any target run that completes refines. An undefined target run observes
as `undefined`, which no defined source run observes, so it never refines one. -/
def equivalent (before after : Run) : Prop :=
  match before,after with
  | .ok a,.ok b => a.completion = .undefined ∨ observe a = observe b
  | _,_ => False

instance (a b : Run) : Decidable (equivalent a b) := by
  unfold equivalent
  split <;> infer_instance

private def wordJson (v : Word) : Lean.Json := .str (toString v.toNat)
private def bytesJson (bytes : List Byte) : Lean.Json :=
  .arr (bytes.map (fun b => Lean.toJson b.toNat)).toArray
private def slotsJson (slots : Slots) : Lean.Json :=
  .arr (slots.map (fun (k,v) => Lean.Json.arr #[wordJson k,wordJson v])).toArray

def describe (result : Run) : String :=
  match result with
  | .error reason => "unsupported: " ++ reason
  | .ok result =>
    let o := observe result
    (Lean.Json.mkObj [
      ("returndata",bytesJson o.returnData),
      ("warm_addresses",.arr (o.warmAddresses.map (fun a => .str (toString a.toNat))).toArray),
      ("warm_slots",.arr (o.warmSlots.map (fun (a,k) => Lean.Json.arr #[.str (toString a.toNat),wordJson k])).toArray),
      ("outcome",.str o.status), ("values",.arr (o.values.map wordJson).toArray),
      ("data",bytesJson o.data), ("memory",o.memory.map bytesJson |>.getD .null),
      ("accounts",.arr (o.accounts.map (fun a => Lean.Json.arr #[
        .str (toString a.address.toNat),.str (toString a.nonce),wordJson a.balance,
        bytesJson a.code,slotsJson a.storage,slotsJson a.transient,Lean.toJson a.created,Lean.toJson a.destroyed])).toArray),
      ("logs",.arr (o.logs.map (fun l => Lean.Json.arr #[
        .str (toString l.address.toNat),.arr (l.topics.map wordJson).toArray,bytesJson l.data])).toArray)
    ]).compress

end SolarTV.MIR.State
