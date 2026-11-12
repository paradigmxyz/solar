import SolarTV.State.Calls
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
abbrev Byte := BitVec 8
abbrev Address := BitVec 160
abbrev Slots := List (Word × Word)

structure Account where
  address : Address
  nonce : Nat := 0
  balance : Word := 0
  code : List Byte := []
  storage : Slots := []
  transient : Slots := []
  created : Bool := false
  destroyed : Bool := false
  deriving DecidableEq

structure Journal where
  accounts : List Account
  original : List Account
  warmAddresses : List Address
  warmSlots : List (Address × Word)
  logs : List World.Log := []
  refund : Int := 0
  deriving DecidableEq

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
  deriving DecidableEq

/-- Model errors are not execution outcomes and cannot be equated by the prover. -/
abbrev Run := Except String Result

def slotsRead (slots : Slots) (key : Word) : Word :=
  (slots.find? (fun entry => entry.1 == key)).map Prod.snd |>.getD 0

def slotsWrite (slots : Slots) (key value : Word) : Slots :=
  (key, value) :: slots.filter (fun entry => entry.1 != key)

def account (world : Journal) (address : Address) : Account :=
  (world.accounts.find? (fun a => a.address == address)).getD {address}

def putAccount (world : Journal) (account : Account) : Journal :=
  {world with accounts := account :: world.accounts.filter (fun a => a.address != account.address)}

def warmAddress (world : Journal) (address : Address) : Journal :=
  {world with warmAddresses := if world.warmAddresses.contains address then world.warmAddresses
    else address :: world.warmAddresses}

def warmSlot (world : Journal) (address : Address) (key : Word) : Journal :=
  {world with warmSlots := if world.warmSlots.contains (address, key) then world.warmSlots
    else (address, key) :: world.warmSlots}

def transfer (world : Journal) (source destination : Address) (value : Word) : Option Journal :=
  let sender :=  account world source
  if sender.balance.ult value then none
  else if source = destination || value == 0 then some world
  else
    let world := putAccount world {sender with balance := sender.balance - value}
    let to := account world destination
    some (putAccount world {to with balance := to.balance + value})

def memoryWords (memory : List Byte) : Nat := memory.length / 32

def memoryCost (words : Nat) : Nat := 3 * words + words * words / 512

def requiredWords (memory : List Byte) (offset count : Nat) : Nat :=
  if count = 0 then memoryWords memory else max (memoryWords memory) ((offset + count + 31) / 32)

def memoryCharge (memory : List Byte) (offset count : Nat) : Nat :=
  memoryCost (requiredWords memory offset count) - memoryCost (memoryWords memory)

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
  (readBytes bytes offset 32).foldl (fun word byte => (word <<< 8) ||| byte.setWidth 256) 0

def failed (world : Journal) : Run := .ok ⟨.exception, [], world, 0⟩

def exhausted (world : Journal) : Run := .ok ⟨.outOfGas, [], world, 0⟩

def finished (world : Journal) (frame : Frame) (reverted : Bool) (data : List Byte := []) : Run :=
  .ok ⟨if reverted then .reverted else .success, data, world, frame.gas⟩

structure Opcode where
  available : Bool
  base : Nat
  dynamic : Nat
  inputs : Nat
  outputs : Nat
  deriving DecidableEq

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

structure Config where
  fork : Fork
  opcodes : List Opcode
  root : Address
  block : Block := {}

end SolarTV.EVM.State
