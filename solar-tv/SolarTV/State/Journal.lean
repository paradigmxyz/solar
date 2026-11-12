import SolarTV.State.World

/-! Shared finite journals for explicit-world MIR and EVM execution. Account and
slot updates preserve unrelated state; both evaluators use the same address-qualified
store. No instruction, backend layout, gas policy or stack convention lives here. -/

namespace SolarTV.Execution
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


/-- Structural sorting keeps concrete observation proofs reducible in the kernel.
The finite fixture/evaluator bounds keep these comparison-only lists small. -/
def orderedInsert (le : α → α → Bool) (value : α) : List α → List α
  | [] => [value]
  | head :: rest => if le value head then value :: head :: rest
    else head :: orderedInsert le value rest

def ordered (le : α → α → Bool) : List α → List α
  | [] => []
  | head :: rest => orderedInsert le head (ordered le rest)

def normalizedSlots (slots : Slots) : Slots :=
  ordered (fun a b => a.1.ule b.1) (slots.filter (fun entry => entry.2 != 0))

/-- Same-key overwrite is the state identity used by storage-DSE, without alias assumptions. -/
@[simp] theorem slotsWrite_overwrite (slots : Slots) (key old value : Word) :
    slotsWrite (slotsWrite slots key old) key value = slotsWrite slots key value := by
  simp [slotsWrite, List.filter_filter]

/-- Reading the cell just written returns its value independently of the initial journal. -/
@[simp] theorem slotsRead_write (slots : Slots) (key value : Word) :
    slotsRead (slotsWrite slots key value) key = value := by
  simp [slotsRead, slotsWrite]

end SolarTV.Execution
