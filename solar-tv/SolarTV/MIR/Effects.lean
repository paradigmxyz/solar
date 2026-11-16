import SolarTV.MIR.Ops

/-! Compositional certificates for acyclic scalar MIR with explicit effects.

Requests retain opcode identity, ordered operands and whether a result is live.
Their implementations are arbitrary stateful handlers, including handlers that
fail immediately. Equality of the trees therefore preserves every interaction,
not just final slot values in an empty world. Scalar computations use MIR's
existing word semantics. The constructor importer is trusted, like the direct
SSA importer; this theorem does not certify that importer or physical gas costs.
Calls, allocation, layout queries and code/GAS sampling are not imported here.
SSTORE remains an opaque request, including possible sentry failure; differences
in physical scalar gas costs and their effect on that sentry are not certified.
This certificate is deliberately distinct from full-state interpreter proofs.
-/

namespace SolarTV.MIR.Effects
open EvmRules

structure Request where
  opcode : Nat
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

/-- Dropping even a void store is observable to a handler. -/
theorem store_is_observable :
    interpret (fun _ (n : Nat) => .ok (0,n+1)) (fun _ n => n)
      (.request ⟨85,[0,1],false⟩ (fun _ => .done ⟨"stop",[]⟩)) 0 ≠
    interpret (fun _ (n : Nat) => .ok (0,n+1)) (fun _ n => n)
      (.done ⟨"stop",[]⟩) 0 := by decide

end SolarTV.MIR.Effects
