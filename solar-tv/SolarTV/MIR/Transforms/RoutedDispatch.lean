import SolarTV.MIR.Transforms.CallContext
import SolarTV.MIR.Transforms.TransferFromDispatch
import SolarTV.MIR.State.Normalize

/-! Implicit runtime routing and the corresponding cut of ordinary typed MIR
execution. The cut stops before any ABI wrapper instructions. A successful route
must preserve the entire frame and begin at the wrapper's entry. A rejected
route must preserve the journal and produce an empty revert. -/

namespace SolarTV.MIR.RoutedDispatch
open EvmRules SolarTV.Execution State CallContext

structure Routes where
  selectors : List (Word × Nat)
  receive : Option Nat
  fallback : Option (Nat × Bool)
  rejectValue : Bool

def lookup (key : Word) : List (Word × Nat) → Option Nat
  | [] => none
  | (selector,id) :: rest => if selector = key then some id else lookup key rest

/-- Short calldata can never select a function, even when its padded prefix
equals a selector. Receive precedes fallback for empty calldata. -/
def select (routes : Routes) (input : List Byte) (value : Word) : Option Nat :=
  if routes.rejectValue && value != 0 then none
  else if input.isEmpty && routes.receive.isSome then routes.receive
  else
    let selected := if input.length < 4 then none
      else lookup (TransferFromDispatch.selector input) routes.selectors
    selected.orElse fun _ => routes.fallback.bind fun (id,payable) =>
      if payable || value == 0 then some id else none

private theorem lookup_target (key : Word) (routes : List (Word × Nat)) (wrapper : Nat)
    (selected : lookup key routes = some wrapper) : wrapper ∈ routes.map Prod.snd := by
  induction routes with
  | nil => simp [lookup] at selected
  | cons route rest ih =>
    by_cases equal : route.1 = key
    · simp [lookup, equal] at selected
      simp [← selected]
    · simp [lookup, equal] at selected
      exact List.mem_cons_of_mem _ (ih selected)

theorem selector_target (selectors : List (Word × Nat)) (rejectValue : Bool)
    (input : List Byte) (value : Word) (wrapper : Nat)
    (selected : select ⟨selectors, none, none, rejectValue⟩ input value = some wrapper) :
    wrapper ∈ selectors.map Prod.snd := by
  simp only [select, Option.isSome_none, Bool.and_false, Bool.false_eq_true,
    ite_false, Option.bind_none] at selected
  split at selected
  · contradiction
  split at selected
  · contradiction
  apply lookup_target (TransferFromDispatch.selector input)
  cases found : lookup (TransferFromDispatch.selector input) selectors
  · simp [found, Option.orElse] at selected
  · simpa [found, Option.orElse] using selected

def initial (entry : Nat) (frame : Frame) : Machine :=
  ⟨{function := entry}, [], {frame with arguments := [], locals := []}⟩

def Agrees (entry : Nat) (frame : Frame) (expected : Option Nat)
    (actual : Except String Cut) : Prop :=
  match expected, actual with
  | none, .ok (.inl result) => result = finish (.reverted []) frame
  | some id, .ok (.inr (_, machine)) =>
    machine.cursor = {function := id, returnOwner := some entry} ∧
    machine.stack = [] ∧ machine.frame = {frame with arguments := [], locals := []}
  | _, _ => False

/-- The interpreter consults only signatures at the cut. Keep a smaller
function table in the symbolic routing proof, with kernel-checked agreement
against the original bodies. -/
def cutBodies (cuts : List Nat) (functions : Functions) : Functions :=
  functions.map fun (id,body) => (id,if id ∈ cuts then signature body else body)

theorem cut_agreement (cuts : List Nat) (functions : Functions) :
    Agreement cuts functions (cutBodies cuts functions) := by
  induction functions with
  | nil => exact .nil
  | cons pair rest ih =>
    obtain ⟨id,body⟩ := pair
    by_cases h : id ∈ cuts
    · simpa [cutBodies, h] using Agreement.replaced id body (signature body) h rfl ih
    · simpa [cutBodies, h] using Agreement.same id body ih

theorem callvalue (cfg : Config) (frame : Frame) :
    contextOperation cfg frame 0x34 [] = value frame cfg.environment.value := rfl

/-- The empty-revert block introduces no memory expansion in the modeled domain. -/
theorem expand_zero (memory : List Byte) (bounded : memory.length ≤ 65536) :
    expand memory 0 0 = .ok memory := by
  have h : 32 * memoryWords memory ≤ memory.length := by
    simpa [memoryWords, Nat.mul_comm] using Nat.div_mul_le_self memory.length 32
  simp [expand, requiredWords, Nat.not_lt.mpr (Nat.le_trans h bounded), Nat.sub_eq_zero_of_le h]

theorem short_selector (input : List Byte) (short : input.length < 4) :
    (TransferFromDispatch.selector input).setWidth 8 = 0 := by
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  have index : 3 - i / 8 = 3 := by omega
  have missing : input[3]? = none := by
    exact List.getElem?_eq_none (by omega)
  simp [TransferFromDispatch.selector, ← BitVec.flattenList_eq_flattenListFast,
    BitVec.getLsbD_flattenList, readBytes, hi, index, missing]

theorem runTo_one (cuts : List Nat) (fuel : Nat) (cfg : Config) (functions : Functions)
    (machine : Machine) :
    runTo cuts (fuel + 1) cfg functions machine =
      if machine.cursor.function ∈ cuts then .ok (.inr (fuel+1,machine)) else (do
        match ← transition fuel cfg functions machine with
        | .finished result => return .inl result
        | .next extra after =>
          if extra > fuel then .error "MIR evaluation work limit"
          else runTo cuts (fuel-extra) cfg functions after) := by
  rw [runTo]
  rfl

theorem runTo_next (cuts : List Nat) (fuel : Nat) (cfg : Config) (functions : Functions)
    (machine : Machine) (extra : Nat) (after : Machine)
    (h : transition fuel cfg functions machine = .ok (.next extra after)) :
    runTo cuts (fuel + 1) cfg functions machine =
      if machine.cursor.function ∈ cuts then .ok (.inr (fuel+1,machine))
      else if extra > fuel then .error "MIR evaluation work limit"
      else runTo cuts (fuel-extra) cfg functions after := by
  simp only [runTo_one, h, Except.bind, Bind.bind]

theorem runTo_finished (cuts : List Nat) (fuel : Nat) (cfg : Config) (functions : Functions)
    (machine : Machine) (result : Result)
    (h : transition fuel cfg functions machine = .ok (.finished result)) :
    runTo cuts (fuel + 1) cfg functions machine =
      if machine.cursor.function ∈ cuts then .ok (.inr (fuel+1,machine)) else .ok (.inl result) := by
  simp only [runTo_one, h, Except.bind, Bind.bind, Except.pure, Pure.pure]

theorem runTo_error (cuts : List Nat) (fuel : Nat) (cfg : Config) (functions : Functions)
    (machine : Machine) (reason : String)
    (h : transition fuel cfg functions machine = .error reason) :
    runTo cuts (fuel + 1) cfg functions machine =
      if machine.cursor.function ∈ cuts then .ok (.inr (fuel+1,machine)) else .error reason := by
  simp only [runTo_one, h, Except.bind, Bind.bind]

simproc_decl reduceRunTo (runTo _ _ _ _ _) := fun expression =>
  reduceRunStep expression ``runTo_error ``runTo_next ``runTo_finished ``runTo_one 1

end SolarTV.MIR.RoutedDispatch
