import SolarTV.EVMIR.Precompiles.Execute
import Lean.Data.Json

/-! Direct precompile observations for boundary tests and independent replay.
These observations do not bypass the kernel proof required for a world-entry
verification. An inactive address is identified separately: its account code,
not a precompile, must be executed by the caller's world evaluator. -/

namespace SolarTV.Precompiles
open EVM.State

def describe (fork : Fork) (address : Address) (input : List Byte) (gas : Nat) : String :=
  let result := if precompile fork address then execute fork address input gas
    else .ok (.success,[],gas)
  match result with
  | .error reason => (Lean.Json.mkObj [("unsupported",.str reason)]).compress
  | .ok (stop,data,remaining) =>
    let name := if !precompile fork address then "inactive" else match stop with
      | .success => "success" | .exception => "exception"
      | .outOfGas => "out-of-gas" | .reverted => "revert"
    (Lean.Json.arr #[.str name,
      .arr (data.map (fun b => Lean.toJson b.toNat)).toArray, Lean.toJson remaining]).compress

end SolarTV.Precompiles
