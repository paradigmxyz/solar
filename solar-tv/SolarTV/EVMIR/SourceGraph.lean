import SolarTV.Correspondence

/-! Structural identity of independently exported scheduled source graphs.

Every operation, terminal, logical label, edge and transfer stack-growth check
is retained. Handlers may change arbitrary machine state or stop with any
observation. Equality is quantified over every execution bound, not established
by accepting two exhausted executions at one bound. Cycles need no termination
assumption when the source graphs are identical.

The parser/normalizer and constructor binding remain trusted. This certificate
does not establish opcode semantics or physical transfer prices. The importer
excludes code-image observers and calls; no backend-layout claim follows.
-/

namespace SolarTV.EVM.SourceGraph

inductive Node where
  | operation (label : Nat) (instruction : String) (next : Nat)
  | jump (label growth next : Nat)
  | branch (label yes no : Nat)
  | terminal (label : Nat) (instruction : String)
  deriving DecidableEq

structure Program where
  entry : Nat
  nodes : Array Node
  deriving DecidableEq

structure Handlers (State Observation : Type) where
  operation : Nat → Nat → String → State → Except Observation State
  jump : Nat → Nat → Nat → State → Except Observation State
  branch : Nat → Nat → State → Except Observation (Bool × State)
  terminal : Nat → Nat → String → State → Observation

def graph (program : Program) (handlers : Handlers State Observation) :
    Correspondence.Graph State Observation := fun index => do
  match ← program.nodes[index]? with
  | .operation label instruction next =>
    pure (.step (handlers.operation index label instruction) next)
  | .jump label growth next => pure (.step (handlers.jump index label growth) next)
  | .branch label yes no => pure (.branch (handlers.branch index label) yes no)
  | .terminal label instruction => pure (.terminal (handlers.terminal index label instruction))

/-- Identical source data preserves every finite execution under the same
interpreter, including stateful effects, early failures and resource state. -/
theorem identity (before after : Program) (same : before = after)
    (handlers : Handlers State Observation) (fuel : Nat) (state : State) :
    Correspondence.evalGraph (graph before handlers) fuel before.entry state =
      Correspondence.evalGraph (graph after handlers) fuel after.entry state := by
  cases same
  rfl

end SolarTV.EVM.SourceGraph
