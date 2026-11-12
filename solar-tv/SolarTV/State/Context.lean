import SolarTV.State.Journal

/-! Explicit transaction and block inputs, shared by direct MIR and EVM execution.
These are immutable inputs, independent of physical instruction selection. -/

namespace SolarTV.Execution
open EvmRules

/-- Explicit block/transaction context, fixed throughout all nested frames.
Missing history entries and out-of-range blob indices denote zero. -/
structure Block where
  number : Word := 0
  beneficiary : Address := 0
  timestamp : Word := 1
  gasLimit : Word := 2^64 - 1
  difficulty : Word := 0
  randomness : Word := 0
  chainId : Word := 1
  baseFee : Word := 0
  blobBaseFee : Word := 1
  slot : BitVec 64 := 0
  hashes : Slots := []
  blobs : List Word := []


def blockHash (context : Block) (number : Word) : Word :=
  if number.toNat < context.number.toNat && context.number.toNat - number.toNat ≤ 256 then
    slotsRead context.hashes number
  else 0

def environmentValue (fork : Fork) (context : Block) (opcode : Nat) (argument : Word) : Word :=
  match opcode with
  | 0x40 => blockHash context argument
  | 0x41 => context.beneficiary.setWidth 256
  | 0x42 => context.timestamp
  | 0x43 => context.number
  | 0x44 => if fork.atLeast .paris then context.randomness else context.difficulty
  | 0x45 => context.gasLimit
  | 0x46 => context.chainId
  | 0x48 => context.baseFee
  | 0x49 => context.blobs[argument.toNat]?.getD 0
  | 0x4a => context.blobBaseFee
  | 0x4b => context.slot.setWidth 256
  | _ => 0

end SolarTV.Execution
