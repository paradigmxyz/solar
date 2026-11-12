import SolarTV.State.Journal

/-! Shared byte-addressed memory with zero extension, aligned active size and
overlap-safe copies. The 64 KiB bound is a prover work limit, never an EVM trap
or an assumption that permits ignoring a live byte. Gas is modeled separately. -/

namespace SolarTV.Execution
open EvmRules

def memoryWords (memory : List Byte) : Nat := memory.length / 32

def requiredWords (memory : List Byte) (offset count : Nat) : Nat :=
  if count = 0 then memoryWords memory else max (memoryWords memory) ((offset + count + 31) / 32)

def expand (memory : List Byte) (offset count : Nat) : Except String (List Byte) :=
  let size := 32 * requiredWords memory offset count
  if size > 65536 then .error "world memory limit of 64 KiB"
  else .ok (memory ++ List.replicate (size - memory.length) 0)

def readBytes (bytes : List Byte) (offset count : Nat) : List Byte :=
  (List.range count).map (fun i => bytes[offset + i]?.getD 0)

def writeBytes (memory : List Byte) (offset : Nat) (bytes : List Byte) : List Byte :=
  memory.mapIdx (fun i old => if offset ≤ i ∧ i < offset + bytes.length then
    bytes[i - offset]?.getD 0 else old)

def wordBytes (word : Word) : List Byte :=
  (List.range 32).map (fun i => (word >>> (8 * (31 - i))).setWidth 8)

def readWord (bytes : List Byte) (offset : Nat) : Word :=
  (BitVec.flattenListFast (readBytes bytes offset 32)).setWidth 256


end SolarTV.Execution
