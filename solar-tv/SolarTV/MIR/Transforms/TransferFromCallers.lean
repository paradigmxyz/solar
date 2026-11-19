import SolarTV.MIR.Transforms.TransferFromReturn

/-! Caller continuations of the captured Morpho transferFrom helper. The
physical entry stack carries the live return words across the helper. The
semantic caller returns a typed product or no values. A propagated revert
exits the owning EVM frame, restoring its checkpoint without dispatching
a caller continuation. -/

namespace SolarTV.MIR.TransferFromCallers
open EvmRules SolarTV.Execution MIR.State
open MIR.TransferFromReturn (Exit)

inductive Caller where
  | supply | repay | supplyCollateral | liquidate | flashLoan
  deriving DecidableEq

inductive Publication where
  | stop
  | pair (first second : Nat)
  deriving DecidableEq

structure Suffix where
  publication : Publication
  gas : Nat
  deriving DecidableEq

def reference : Caller → Suffix
  | .supply | .repay => ⟨.pair 1 5,25⟩
  | .liquidate => ⟨.pair 5 2,28⟩
  | .supplyCollateral | .flashLoan => ⟨.stop,1⟩

def liveOperands (caller : Caller) (amount other : Word) : List Word :=
  match caller with
  | .supply | .repay => [amount,other]
  | .liquidate => [other,amount]
  | .supplyCollateral | .flashLoan => []

def Publication.words (form : Publication) (stack : List Word) : Except String (List Word) := do
  match form with
  | .stop => return []
  | .pair first second =>
    return [← required stack[first]? "missing first ABI return word",
      ← required stack[second]? "missing second ABI return word"]

/-- The checked MIR continuation either returns no values or fills both
fields of the i256 product before returning it. -/
def typedWords (words : List Word) : Except String (List Word) := do
  match words with
  | [] => return []
  | [first,second] =>
    let ty := ValueType.product [.word 256,.word 256]
    let value ← insertField ty .undefined 0 (.scalar (Scalar.ofWord 256 first))
    let value ← insertField ty value 1 (.scalar (Scalar.ofWord 256 second))
    value.flatten 34 ty
  | _ => .error "unexpected Morpho return type"

theorem typed_live (caller : Caller) (amount other : Word) :
    typedWords (liveOperands caller amount other) = .ok (liveOperands caller amount other) := by
  cases caller <;> rfl

def emitWords (memory : List Byte) (first second : Word) : List Byte :=
  writeBytes (writeBytes memory 0 (wordBytes first)) 32 (wordBytes second)

private theorem readPair (memory : List Byte) (first second : Word)
    (live : 64 ≤ memory.length) :
    readBytes (emitWords memory first second) 0 64 = wordBytes first ++ wordBytes second := by
  have parts (mem : List Byte) :
      readBytes mem 0 64 = readBytes mem 0 32 ++ readBytes mem 32 32 := by
    unfold readBytes
    rw [show List.range 64 = List.range 32 ++ (List.range 32).map (32+·) by decide]
    simp [List.map_append,List.map_map]
  rw [parts,emitWords]
  rw [readBytes_writeBytes_disjoint _ _ 32 0 32 (Or.inr (by decide))]
  have head := readBytes_writeBytes_same memory (wordBytes first) 0 (by simp; omega)
  have tail := readBytes_writeBytes_same (writeBytes memory 0 (wordBytes first))
    (wordBytes second) 32 (by simpa using live)
  simp only [wordBytes_length] at head tail
  rw [head,tail]

private theorem expanded (memory active : List Byte) (ok : expand memory 0 64 = .ok active) :
    64 ≤ active.length := by
  unfold expand requiredWords at ok
  simp only [show ¬ (64 : Nat) = 0 by decide,ite_false] at ok
  split at ok
  · contradiction
  · cases ok
    simp only [List.length_append,List.length_replicate]
    have more := Nat.le_max_right (memoryWords memory) 2
    omega

def pricedReturn (checkpoint world : Journal) (fixed gas : Nat) (start active bytes : List Byte) :
    EVM.State.Result :=
  let charge := fixed + EVM.State.memoryCost (memoryWords active) -
    EVM.State.memoryCost (memoryWords start)
  if gas < charge then ⟨.outOfGas,[],checkpoint,0,0⟩
  else ⟨.success,bytes,world,gas-charge,0⟩

/-- Physical stores at 0 and 32 publish the selected stack words. STOP and
RETURN end this message. The caller-local gas check can also exhaust a
budget that was sufficient to leave the helper. -/
def byteContinue (suffix : Suffix) (stack : List Word) (checkpoint : Journal)
    (helper : Except String Exit) : EVM.State.Run := do
  match ← helper with
  | .reverted data gas => return ⟨.reverted,data,checkpoint,gas,0⟩
  | .outOfGas => return ⟨.outOfGas,[],checkpoint,0,0⟩
  | .returned memory world _ gas =>
    match ← suffix.publication.words stack with
    | [] => return pricedReturn checkpoint world suffix.gas gas memory memory []
    | [first,second] =>
      let active ← expand memory 0 64
      let bytes := readBytes (emitWords active first second) 0 64
      return pricedReturn checkpoint world suffix.gas gas memory active bytes
    | _ => .error "unexpected Morpho stack publication"

/-- Interpret the caller's return after the void internal call. Apply the
matched physical publication cost to the semantic result. The checkpoint is
from before the owning external invocation, including writes before the helper. -/
def mirContinue (caller : Caller) (amount other : Word) (checkpoint : Journal)
    (helper : Except String Exit) : EVM.State.Run := do
  match ← helper with
  | .reverted data gas => return ⟨.reverted,data,checkpoint,gas,0⟩
  | .outOfGas => return ⟨.outOfGas,[],checkpoint,0,0⟩
  | .returned memory world _ gas =>
    let words ← typedWords (liveOperands caller amount other)
    let active ← if words.isEmpty then pure memory else expand memory 0 64
    return pricedReturn checkpoint world (reference caller).gas gas memory active
      (words.flatMap wordBytes)

theorem resume_eq (caller : Caller) (suffix : Suffix) (stack : List Word)
    (amount other : Word) (checkpoint : Journal) (helper : Except String Exit)
    (shape : suffix = reference caller)
    (live : suffix.publication.words stack = .ok (liveOperands caller amount other)) :
    mirContinue caller amount other checkpoint helper = byteContinue suffix stack checkpoint helper := by
  subst suffix
  cases helper with
  | error _ => rfl
  | ok exit =>
    cases exit with
    | reverted _ _ | outOfGas => rfl
    | returned memory world _ gas =>
      simp only [mirContinue,byteContinue,Bind.bind,Except.bind,Pure.pure,Except.pure,
        typed_live,live]
      cases caller <;> simp only [liveOperands,List.isEmpty_cons,List.isEmpty_nil,ite_true]
      all_goals
        first
        | rfl
        | cases h : expand memory 0 64 with
          | error _ => rfl
          | ok active =>
            simp only [Bool.false_eq_true,ite_false,List.flatMap_cons,List.flatMap_nil,List.append_nil,
              readPair _ _ _ (expanded _ _ h)]

end SolarTV.MIR.TransferFromCallers
