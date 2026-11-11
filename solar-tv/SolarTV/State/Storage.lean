import SolarTV.State.Fork
import EvmRules.Word

/-! Persistent and transient storage with transaction-original values, per-address
access warmth and signed refunds. Stores are total functions, so proofs quantify
over all slots rather than assuming an initially empty or finitely sized store.

Pricing follows EIP-2200 (including its <=2300 gas sentry), EIP-2929's cold
surcharge and reset price, and EIP-3529's clear refund. Constantinople uses
EIP-1283 without the sentry; Petersburg restores the legacy rules. Refunds
are signed because an individual frame may remove a refund earned by its caller.
A failed store leaves all state to its enclosing frame's rollback rule.
https://eips.ethereum.org/EIPS/eip-2200#specification
https://eips.ethereum.org/EIPS/eip-2929#sstore-changes
https://eips.ethereum.org/EIPS/eip-3529#specification -/

namespace SolarTV.Storage
open EvmRules

abbrev Address := BitVec 160
abbrev Store := Address → Word → Word
abbrev WarmSlots := Address → Word → Bool

def update (store : Store) (address : Address) (slot value : Word) : Store :=
  fun a k => if a = address ∧ k = slot then value else store a k

def warm (slots : WarmSlots) (address : Address) (slot : Word) : WarmSlots :=
  fun a k => (a == address && k == slot) || slots a k

structure State where
  persistent : Store
  original : Store
  transient : Store
  accessed : WarmSlots
  refund : Int

inductive Error where
  | outOfGas | staticViolation | unavailable
  deriving DecidableEq

structure Price where
  gas : Nat
  refund : Int
  deriving DecidableEq

def readPrice (fork : Fork) (warm : Bool) : Nat :=
  if fork.atLeast .berlin then if warm then 100 else 2100
  else if fork.atLeast .istanbul then 800
  else if fork.atLeast .tangerineWhistle then 200 else 50

def netMetered (fork : Fork) : Bool :=
  fork == .constantinople || fork.atLeast .istanbul

/-- Pricing alone does not enforce the sentry or static permission. -/
def writePrice (fork : Fork) (warm : Bool) (original current value : Word) : Price :=
  let cold := if fork.atLeast .berlin && !warm then 2100 else 0
  let read := readPrice fork true
  let reset := if fork.atLeast .berlin then 2900 else 5000
  let clear : Int := if fork.atLeast .london then 4800 else 15000
  if !netMetered fork then
    ⟨if current == 0 && value != 0 then 20000 else 5000,
      if current != 0 && value == 0 then clear else 0⟩
  else if current == value then ⟨cold + read, 0⟩
  else if original == current then
    if original == 0 then ⟨cold + 20000, 0⟩
    else ⟨cold + reset, if value == 0 then clear else 0⟩
  else
    let remove := if original != 0 && current == 0 then -clear else 0
    let add := if original != 0 && value == 0 then clear else 0
    let restored := if original == value then
      (if original == 0 then (20000 : Int) else (reset : Int)) - (read : Int) else 0
    ⟨cold + read, remove + add + restored⟩

def load (fork : Fork) (state : State) (address : Address) (slot : Word) (gas : Nat) :
    Except Error (Word × State × Nat) :=
  let charge := readPrice fork (state.accessed address slot)
  if gas < charge then .error .outOfGas else
    .ok (state.persistent address slot,
      {state with accessed := warm state.accessed address slot}, gas - charge)

def store (fork : Fork) (state : State) (address : Address) (slot value : Word)
    (gas : Nat) (isStatic : Bool) : Except Error (State × Nat) :=
  if isStatic then .error .staticViolation
  else if fork.atLeast .istanbul && gas ≤ 2300 then .error .outOfGas
  else
    let price := writePrice fork (state.accessed address slot)
      (state.original address slot) (state.persistent address slot) value
    if gas < price.gas then .error .outOfGas else
      .ok ({state with
        persistent := update state.persistent address slot value
        accessed := warm state.accessed address slot
        refund := state.refund + price.refund},
        gas - price.gas)

def tload (fork : Fork) (state : State) (address : Address) (slot : Word) (gas : Nat) :
    Except Error (Word × Nat) :=
  if !fork.atLeast .cancun then .error .unavailable
  else if gas < 100 then .error .outOfGas
  else .ok (state.transient address slot, gas - 100)

/-- Transient stores have no EIP-2200 sentry and no refund. -/
def tstore (fork : Fork) (state : State) (address : Address) (slot value : Word)
    (gas : Nat) (isStatic : Bool) : Except Error (State × Nat) :=
  if !fork.atLeast .cancun then .error .unavailable
  else if isStatic then .error .staticViolation
  else if gas < 100 then .error .outOfGas
  else .ok ({state with transient := update state.transient address slot value}, gas - 100)

@[simp] theorem update_same (state : Store) (a : Address) (k value : Word) :
    update state a k value a k = value := by simp [update]

@[simp] theorem update_other (state : Store) (a b : Address) (k j value : Word)
    (different : b ≠ a ∨ j ≠ k) : update state a k value b j = state b j := by
  simp only [update]
  rcases different with h | h <;> simp_all

theorem overwrite (state : Store) (a : Address) (k first last : Word) :
    update (update state a k first) a k last = update state a k last := by
  funext b j
  simp only [update]
  split <;> simp_all

end SolarTV.Storage
