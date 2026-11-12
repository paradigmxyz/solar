import SolarTV.State.Calls
import SolarTV.State.ByteMemory
import SolarTV.State.Context
import SolarTV.Keccak
import SolarTV.EVMIR.Ops

/-! Executable, journaled account state for explicit-world validation. Lists are
finite representations of total maps with an absent/zero default. Updates replace
keys rather than appending histories, and observations sort/erase zero slots.
The world fixture is an explicit assumption, never an arbitrary-account proof.
The evaluator reports unsupported behavior and work/memory limits separately
from EVM exceptional halts; neither may certify equivalence. -/

namespace SolarTV.EVM.State
open EvmRules
export SolarTV.Execution (Byte Address Slots Account Journal Block slotsRead slotsWrite account putAccount
  warmAddress warmSlot transfer memoryWords requiredWords expand readBytes writeBytes wordBytes readWord)

structure Frame where
  environment : World.Environment
  code : List Byte
  pc : Nat := 0
  stack : List Word := []
  memory : List Byte := []
  returnData : List Byte := []
  gas : Nat

inductive Stop where
  | success | reverted | exception | outOfGas
  deriving DecidableEq

structure Result where
  stop : Stop
  data : List Byte
  world : Journal
  gas : Nat
  work : Nat := 0
  deriving DecidableEq

/-- Model errors are not execution outcomes and cannot be equated by the prover. -/
abbrev Run := Except String Result

def memoryCost (words : Nat) : Nat := 3 * words + words * words / 512

def memoryCharge (memory : List Byte) (offset count : Nat) : Nat :=
  memoryCost (requiredWords memory offset count) - memoryCost (memoryWords memory)

def failed (world : Journal) : Run := .ok ⟨.exception, [], world, 0, 0⟩

def exhausted (world : Journal) : Run := .ok ⟨.outOfGas, [], world, 0, 0⟩

def finished (world : Journal) (frame : Frame) (reverted : Bool) (data : List Byte := []) : Run :=
  .ok ⟨if reverted then .reverted else .success, data, world, frame.gas, 0⟩

structure Opcode where
  available : Bool
  base : Nat
  dynamic : Nat
  inputs : Nat
  outputs : Nat
  deriving DecidableEq

structure Config where
  fork : Fork
  opcodes : List Opcode
  root : Address
  block : Block := {}
  unknownCode : Option Address := none

end SolarTV.EVM.State
