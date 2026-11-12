import SolarTV.EVMIR.State.Decode

/-! Explicit environment reads for complete-entry execution. BLOCKHASH accepts
only the previous 256 blocks, excluding the current block, without word-wrap.
The Paris transition selects randomness instead of difficulty; opcode activation
is checked by the reflected fork table before reading context. Blob indices are
full words, so enormous indices return zero rather than truncating to a host index.
SLOTNUM widens the consensus uint64 value. History and blobs are fixture inputs,
not hash oracles inferred from the IR being validated.
https://eips.ethereum.org/EIPS/eip-4399
https://eips.ethereum.org/EIPS/eip-4844
https://eips.ethereum.org/EIPS/eip-7843 -/

namespace SolarTV.EVM.State
open EvmRules

def blockHash (context : Block) (number : Word) : Word :=
  if number.toNat < context.number.toNat && context.number.toNat - number.toNat ≤ 256 then
    slotsRead context.hashes number
  else 0

def environmentValue (cfg : Config) (opcode : Nat) (argument : Word) : Word :=
  match opcode with
  | 0x40 => blockHash cfg.block argument
  | 0x41 => cfg.block.beneficiary.setWidth 256
  | 0x42 => cfg.block.timestamp
  | 0x43 => cfg.block.number
  | 0x44 => if cfg.fork.atLeast .paris then cfg.block.randomness else cfg.block.difficulty
  | 0x45 => cfg.block.gasLimit
  | 0x46 => cfg.block.chainId
  | 0x48 => cfg.block.baseFee
  | 0x49 => cfg.block.blobs[argument.toNat]?.getD 0
  | 0x4a => cfg.block.blobBaseFee
  | 0x4b => cfg.block.slot.setWidth 256
  | _ => 0

end SolarTV.EVM.State
