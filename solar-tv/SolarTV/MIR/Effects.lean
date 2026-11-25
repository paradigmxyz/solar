import SolarTV.MIR.Ops
import SolarTV.State.ByteMemory

/-! Compositional certificates for acyclic scalar MIR with explicit effects.

Requests retain opcode identity, ordered operands and whether a result is live.
Their implementations are arbitrary stateful handlers, including handlers that
fail immediately. Equality of the trees therefore preserves every interaction,
not just final slot values in an empty world. Scalar computations use MIR's
existing word semantics. The constructor importer is trusted, like the direct
SSA importer; this theorem does not certify that importer or physical gas costs.
Acyclic internal calls expand their supplied bodies, with isolated SSA locals
and shared request state. Allocation requests retain their size, zeroing and
checked-failure policy. No allocation success, fresh address, implicit FMP or
return-buffer placement is assumed. Constructor-range queries retain their
base/end discriminator as ordered requests; no range is chosen or assumed to
exist. The selected ABI adapter can retain GAS and message operations as
ordered requests. This remains an unpriced interaction certificate.
SSTORE remains an opaque request, including possible sentry failure; differences
in physical scalar gas costs and their effect on that sentry are not certified.
This certificate is deliberately distinct from full-state interpreter proofs.
-/

namespace SolarTV.MIR.Effects
open EvmRules

inductive Operation where
  | opcode (code : Nat)
  | allocate (zeroed checked : Bool)
  | constructorRange (isEnd : Bool)
  | mappingSlot
  | abiWordObject (hasSelector : Bool)
  | abiWordScratch (hasBase hasSelector : Bool)
  | abiTreeScratch (layout : String)
  | abiTreeScratchSize (layout : String)
  | immutableRead (slot : Nat)
  | immutableStore (slot : Nat)
  | internalCall (callee : Nat)
  /-- Retain a semantic check or message with its exact typed schema and operand widths. -/
  | typedRequest (descriptor : String) (widths : List Nat)
  deriving DecidableEq

structure Request where
  operation : Operation
  arguments : List Word
  returnsWord : Bool
  deriving DecidableEq

structure Boundary where
  kind : String
  values : List (Nat × Word)
  deriving DecidableEq

inductive Tree where
  | done (boundary : Boundary)
  | request (effect : Request) (next : Word → Tree)

/-- Stage the static public return words in private memory. -/
def writeAbiWords (offset : Nat) (words : List Word) (tail : Tree) : Tree :=
  match words with
  | [] => tail
  | word :: rest =>
    .request ⟨.opcode 0x52, [BitVec.ofNat 256 offset, word], false⟩
      (fun _ => writeAbiWords (offset + 32) rest tail)

def abiReturn (words : List Word) : Tree :=
  match words with
  | [] => .done ⟨"stop", []⟩
  | _ => writeAbiWords 0 words
    (.done ⟨"returndata", [(256, 0), (256, BitVec.ofNat 256 (32 * words.length))]⟩)

def abiDecodeFailure : Tree := .done ⟨"revert", [(256, 0), (256, 0)]⟩

/-- A static tuple occupies contiguous words in freshly allocated memory.
Address and other narrow fields reject noncanonical encodings before the store. -/
def stageCalldataFields (input : List Execution.Byte) (head : Nat) (destination : Word)
    (widths : List Nat) (tail : Tree) : Tree :=
  match widths with
  | [] => tail
  | width :: rest =>
    let word := Execution.readWord input head
    if width < 256 ∧ 2^width - 1 < word.toNat then abiDecodeFailure else
      .request ⟨.opcode 0x52, [destination, word], false⟩ (fun _ =>
        stageCalldataFields input (head + 32) (destination + 32) rest tail)

def decodeCalldataTuple (input : List Execution.Byte) (head : Nat) (widths : List Nat)
    (next : Word → Tree) : Tree :=
  .request ⟨.allocate false false, [BitVec.ofNat 256 (widths.length * 32)], true⟩
    (fun pointer => stageCalldataFields input head pointer widths (next pointer))

theorem highHalfNat (word : Word) :
    (word.toNat >>> 128) % 2^128 = word.toNat >>> 128 :=
  Nat.mod_eq_of_lt (by simpa using BitVec.toNat_ushiftRight_lt word 128 (by decide))

theorem highHalfZero (word : Word) :
    (word >>> 128).setWidth 128 = 0#128 ↔ ¬ (2^128 - 1) < word.toNat := by
  simp only [← BitVec.toNat_inj, BitVec.toNat_setWidth, BitVec.toNat_ushiftRight,
    BitVec.toNat_ofNat, Nat.zero_mod, Nat.shiftRight_eq_div_pow]
  omega

theorem highHalfWord (word : Word) :
    ((word >>> 128).setWidth 128).setWidth 256 = word >>> 128 := by
  have fits : word.toNat >>> 128 < 2^128 := by
    simpa using BitVec.toNat_ushiftRight_lt word 128 (by decide)
  have wide : word.toNat >>> 128 < 2^256 := Nat.lt_of_lt_of_le fits (by decide)
  apply BitVec.eq_of_toNat_eq
  simp only [BitVec.toNat_setWidth, BitVec.toNat_ushiftRight]
  rw [Nat.mod_eq_of_lt fits, Nat.mod_eq_of_lt wide]

theorem highHalfInputEq (word : Word) (input : List Execution.Byte) (offset : Nat) :
    (word >>> 128 = Execution.readWord input offset) ↔
      (Execution.readWord input offset = word >>> 128) := eq_comm

theorem bitOrTruth (p q : Prop) [Decidable p] [Decidable q] :
    ((if p then 1#1 else 0#1) ||| (if q then 1#1 else 0#1)) = 1#1 ↔ p ∨ q := by
  by_cases p <;> by_cases q <;> simp_all


/-- Materialize a MIR revert of the current message returndata. -/
def abiRevertData : Tree :=
  .request ⟨.opcode 0x3d, [], true⟩ fun size =>
    .request ⟨.opcode 0x3e, [0, 0, size], false⟩ fun _ =>
      .request ⟨.opcode 0x3d, [], true⟩ fun finalSize =>
        .done ⟨"revert", [(256, 0), (256, finalSize)]⟩

/-- A one-word memory boolean decoder reads the object header before its
payload. The second read only occurs for a valid head length. -/
def decodeMemoryBool (pointer : Word) (next : BitVec 1 → Tree) : Tree :=
  .request ⟨.opcode 0x51, [pointer], true⟩ fun length =>
    if length.toNat ≥ 2^255 ∨ length.toNat < 32 then abiDecodeFailure else
    .request ⟨.opcode 0x51, [pointer + 32], true⟩ fun word =>
      if 1 < word.toNat then abiDecodeFailure else next (MIR.compare .ne word 0)

theorem signedBoolHead (length : Word) :
    length.slt (32#256) = true ↔ length.toNat ≥ 2^255 ∨ length.toNat < 32 := by
  simp only [BitVec.slt_eq_decide, decide_eq_true_eq, BitVec.toInt]
  split <;> simp_all <;> omega

/-- A handler can return an arbitrary word and next state, or stop execution.
No successful memory access, distinct storage key or non-reverting call is assumed. -/
def interpret (handler : Request → σ → Except α (Word × σ))
    (finish : Boundary → σ → α) : Tree → σ → α
  | .done boundary, state => finish boundary state
  | .request effect next, state => match handler effect state with
    | .error outcome => outcome
    | .ok (word,state) => interpret handler finish (next word) state

/-- A certificate holds for every stateful interpretation and initial state,
including aliasing, faults and all terminal boundary implementations. -/
theorem contextual (before after : Tree) (same : before = after)
    (handler : Request → σ → Except α (Word × σ))
    (finish : Boundary → σ → α) (state : σ) :
    interpret handler finish before state = interpret handler finish after state := by
  rw [same]

/-- Internal returns resume their caller; frame-ending outcomes do not. -/
def resume (continuation : List (Nat × Word) → Tree) : Tree → Tree
  | .done boundary =>
    if boundary.kind = "return" then continuation boundary.values else .done boundary
  | .request effect next => .request effect (fun word => resume continuation (next word))

/-- Resuming a supplied body preserves failures and the complete shared state. -/
theorem interpret_resume (handler : Request → σ → Except α (Word × σ))
    (finish : Boundary → σ → α) (continuation : List (Nat × Word) → Tree)
    (body : Tree) (state : σ) :
    interpret handler finish (resume continuation body) state =
      interpret handler (fun boundary state =>
        if boundary.kind = "return" then interpret handler finish (continuation boundary.values) state
        else finish boundary state) body state := by
  induction body generalizing state with
  | done boundary => simp [resume,interpret]; split <;> rfl
  | request effect next ih =>
    simp only [resume,interpret]
    cases handled : handler effect state with
    | error outcome => rfl
    | ok result => exact ih result.1 result.2

/-- Dropping even a void store is observable to a handler. -/
theorem store_is_observable :
    interpret (fun _ (n : Nat) => .ok (0,n+1)) (fun _ n => n)
      (.request ⟨.opcode 85,[0,1],false⟩ (fun _ => .done ⟨"stop",[]⟩)) 0 ≠
    interpret (fun _ (n : Nat) => .ok (0,n+1)) (fun _ n => n)
      (.done ⟨"stop",[]⟩) 0 := by decide

example : (⟨.allocate false true,[64],true⟩ : Request) ≠
    ⟨.allocate false false,[64],true⟩ := by decide

example : interpret (fun _ (_ : Nat) => .error 7) (fun _ _ => 0)
    (resume (fun _ => .done ⟨"return",[]⟩)
      (.request ⟨.allocate true false,[64],true⟩ (fun _ => .done ⟨"return",[]⟩))) 0 = 7 := by decide

end SolarTV.MIR.Effects
