import SolarTV.State.Storage

/-! Account state and journaled transaction effects. Storage includes the address
of its owner, so CALL and DELEGATECALL cannot accidentally share a global slot
map. Snapshots include transient storage, logs, refund and access sets; restoring
only persistent storage would be incorrect after a child reverts.

These definitions describe state and message boundaries, not an oracle for
executing unknown code. A child result must eventually come from the instruction
semantics; it is not admitted as an unconstrained successful call by the CLI. -/

namespace SolarTV.World
open EvmRules

abbrev Address := Storage.Address
abbrev Byte := BitVec 8

structure Account where
  present : Bool
  nonce : Nat
  balance : Word
  code : List Byte
  deriving DecidableEq

structure Log where
  address : Address
  topics : List Word
  data : List Byte
  deriving DecidableEq

structure State where
  accounts : Address → Account
  storage : Storage.State
  logs : List Log
  accessedAccounts : Address → Bool
  created : Address → Bool
  destructed : Address → Bool

structure Environment where
  address : Address
  codeAddress : Address
  caller : Address
  origin : Address
  value : Word
  gasPrice : Word
  input : List Byte
  isStatic : Bool
  depth : Nat
  deriving DecidableEq

def setAccount (state : State) (address : Address) (account : Account) : State :=
  {state with accounts := fun a => if a = address then account else state.accounts a}

def warmAccount (state : State) (address : Address) : State :=
  {state with accessedAccounts := fun a => a == address || state.accessedAccounts a}

/-- The caller checks sufficient funds. Self-transfers do not mint or destroy funds. -/
def transfer (state : State) (source destination : Address) (value : Word) : Option State :=
  if (state.accounts source).balance.ult value then none
  else if source = destination then some state
  else
    let debited := setAccount state source
      {(state.accounts source) with balance := (state.accounts source).balance - value}
    let credited := {(debited.accounts destination) with
      present := (debited.accounts destination).present || value != 0
      balance := (debited.accounts destination).balance + value}
    some (setAccount debited destination credited)

/-- A log is forbidden in static mode, including LOG0 with zero bytes. -/
def emitLog (state : State) (environment : Environment) (topics : List Word) (data : List Byte) :
    Except Storage.Error State :=
  if environment.isStatic then .error .staticViolation
  else .ok {state with logs := state.logs ++ [⟨environment.address, topics, data⟩]}

@[simp] theorem setAccount_same (state : State) (address : Address) (account : Account) :
    (setAccount state address account).accounts address = account := by simp [setAccount]

@[simp] theorem setAccount_other (state : State) (address other : Address) (account : Account)
    (different : other ≠ address) :
    (setAccount state address account).accounts other = state.accounts other := by
  simp [setAccount, different]

theorem transfer_to_self (state : State) (address : Address) (value : Word)
    (funds : (state.accounts address).balance.ult value = false) :
    transfer state address address value = some state := by simp [transfer, funds]

end SolarTV.World
