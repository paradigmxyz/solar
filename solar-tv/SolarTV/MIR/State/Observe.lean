import SolarTV.MIR.State.Interpreter
import Lean.Data.Json

/-! Observable direct MIR effects at the selected boundary. A normal internal
return exposes all live memory and transient storage to its caller. STOP/raw
RETURN end the transaction, so private memory and transient storage disappear.
Revert/exception roll back state and expose no memory. No gas placeholders or
backend image hashes are part of this observation. Model errors on either side
exclude a proof, including identical nontermination or resource exhaustion. -/

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
  boundaryBudgets : Option (Nat × Nat)
  messageGas : List Nat
  deriving DecidableEq

def observe (result : Result) : Observation :=
  let (status,values,data,memory,keepTransient) := match result.completion with
    | .returned values => ("return",values,[],some result.memory,true)
    | .success data => ("success",[],data,none,false)
    | .reverted data => ("revert",[],data,none,false)
    | .invalid => ("exception",[],[],none,false)
    | .outOfGas => ("out-of-gas",[],[],none,false)
  let accounts := result.world.accounts.map fun a => {a with
    storage := normalizedSlots a.storage
    transient := if keepTransient then normalizedSlots a.transient else []
    created := keepTransient && a.created
    destroyed := keepTransient && a.destroyed}
  ⟨status,values,data,memory,ordered (fun a b => a.address.ule b.address) accounts,result.world.logs,
    if keepTransient then result.returnData else [],
    if keepTransient then ordered (fun a b => a.ule b) result.world.warmAddresses.eraseDups else [],
    if keepTransient then ordered (fun a b => if a.1 == b.1 then a.2.ule b.2 else a.1.ule b.1) result.world.warmSlots.eraseDups else [],
    if keepTransient then some (result.creationIndex,result.builtinIndex) else none,
    result.messageGas⟩

def equivalent (before after : Run) : Prop :=
  match before,after with | .ok a,.ok b => observe a = observe b | _,_ => False

instance (a b : Run) : Decidable (equivalent a b) := by
  unfold equivalent
  split <;> infer_instance

/-- Live memory without the free region: the bytes below the free memory pointer, and always
below `0x60` so the pointer itself counts, with trailing zero bytes dropped because memory not
yet touched reads as zero. Solidity guarantees nothing about memory at or above the free memory
pointer, and only `MSIZE` observes how far memory reaches, so code that never reads `MSIZE` may
leave that region, and the memory size, as it likes. -/
def belowFree (memory : List Byte) : List Byte :=
  ((memory.take (max (readWord memory 64).toNat 0x60)).reverse.dropWhile (· == 0)).reverse

/-- `observe` with the memory of a normal return cut to `belowFree`. -/
def observeBelowFree (result : Result) : Observation :=
  let o := observe result
  {o with memory := o.memory.map belowFree}

/-- `equivalent` for a pair in which neither side reads `MSIZE`: free memory and the memory size
are not observed. Everything else, including every byte below the free memory pointer, is. -/
def equivalentBelowFree (before after : Run) : Prop :=
  match before,after with | .ok a,.ok b => observeBelowFree a = observeBelowFree b | _,_ => False

instance (a b : Run) : Decidable (equivalentBelowFree a b) := by
  unfold equivalentBelowFree
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
      ("boundary_budgets",o.boundaryBudgets.map (fun (creation,builtin) =>
        Lean.Json.arr #[Lean.toJson creation,Lean.toJson builtin]) |>.getD .null),
      ("message_gas",Lean.toJson o.messageGas),
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
