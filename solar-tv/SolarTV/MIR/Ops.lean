import EvmRules.Word

/-! Pure i256 MIR operations and ordered switch dispatch.
A case stores an SSA value ID. Reading from an arbitrary total environment models
defined MIR operands, including nonconstant cases. The interpreter tests them in
source order and enters the first matching successor from the current block. -/

namespace SolarTV.MIR

open EvmRules

inductive Op where
  | add | sub | mul | and | or | xor

def eval : Op → Word → Word → Word
  | .add => Evm.add
  | .sub => Evm.sub
  | .mul => Evm.mul
  | .and => Evm.and
  | .or => Evm.or
  | .xor => Evm.xor

/-- Ordered switch selection, also used when compiling a pair obligation. -/
abbrev choose (key test : Word) (yes no : α) := if test = key then yes else no

abbrev Case := Nat × Nat -- (SSA value ID, destination block)

def dispatch (read : Nat → EvmRules.Word) (key : EvmRules.Word) (fallback : Nat) :
    List Case → Nat
  | [] => fallback
  | (id, dst) :: rest => choose key (read id) dst (dispatch read key fallback rest)

/-- Phi operands are read in the incoming environment and selected by predecessor.
An absent predecessor is undefined; the importer requires exactly one match. -/
def phi (predecessor : Nat) : List (Nat × EvmRules.Word) → Option EvmRules.Word
  | [] => none
  | (block, value) :: rest =>
    if predecessor = block then some value else phi predecessor rest

end SolarTV.MIR
