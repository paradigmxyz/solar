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

export SolarTV.Execution (blockHash)

def environmentValue (cfg : Config) (opcode : Nat) (argument : Word) : Word :=
  SolarTV.Execution.environmentValue cfg.fork cfg.block opcode argument

end SolarTV.EVM.State
