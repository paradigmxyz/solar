/-! A checked boundary between a control-flow graph and an expanded execution tree.
The theorem is polymorphic in machine state and observations. Instruction effects
are parameters, not assumed pure: either a next state or a terminal observation.
Missing nodes and insufficient expansion fuel are unsupported, never outcomes.

This establishes structural correspondence for the declared node semantics. It
does not prove a Rust parser, operation implementation or backend layout correct.
Those remain separate obligations, even when concrete replay agrees. -/

namespace SolarTV.Correspondence

inductive Node (State Observation : Type) where
  | terminal (finish : State → Observation)
  | step (effect : State → Except Observation State) (next : Nat)
  | branch (condition : State → Except Observation (Bool × State)) (yes no : Nat)

inductive Tree (State Observation : Type) where
  | terminal (finish : State → Observation)
  | step (effect : State → Except Observation State) (next : Tree State Observation)
  | branch (condition : State → Except Observation (Bool × State))
      (yes no : Tree State Observation)

abbrev Graph (State Observation : Type) := Nat → Option (Node State Observation)

def expand (graph : Graph State Observation) : Nat → Nat → Option (Tree State Observation)
  | 0, _ => none
  | fuel+1, entry => do
    match ← graph entry with
    | .terminal finish => some (.terminal finish)
    | .step effect next => return .step effect (← expand graph fuel next)
    | .branch condition yes no =>
      return .branch condition (← expand graph fuel yes) (← expand graph fuel no)

def evalTree : Tree State Observation → State → Observation
  | .terminal finish, state => finish state
  | .step effect next, state => match effect state with
    | .error observation => observation
    | .ok state => evalTree next state
  | .branch condition yes no, state => match condition state with
    | .error observation => observation
    | .ok (true,state) => evalTree yes state
    | .ok (false,state) => evalTree no state

def evalGraph (graph : Graph State Observation) : Nat → Nat → State → Option Observation
  | 0, _, _ => none
  | fuel+1, entry, state => do
    match ← graph entry with
    | .terminal finish => some (finish state)
    | .step effect next => match effect state with
      | .error observation => some observation
      | .ok state => evalGraph graph fuel next state
    | .branch condition yes no => match condition state with
      | .error observation => some observation
      | .ok (true,state) => evalGraph graph fuel yes state
      | .ok (false,state) => evalGraph graph fuel no state

/-- A successfully expanded graph has exactly the tree's observations for every
initial state, including arbitrary memory, accounts, aliases and gas resources. -/
theorem expansion_correct (graph : Graph State Observation) (fuel entry : Nat)
    (tree : Tree State Observation) (expanded : expand graph fuel entry = some tree)
    (state : State) :
    evalGraph graph fuel entry state = some (evalTree tree state) := by
  induction fuel generalizing entry tree state with
  | zero => simp [expand] at expanded
  | succ fuel ih =>
    cases node : graph entry with
    | none => simp [expand, node] at expanded
    | some instruction =>
      cases instruction with
      | terminal finish =>
        simp [expand, node] at expanded
        subst tree
        simp [evalGraph, node, evalTree]
      | step effect next =>
        cases child : expand graph fuel next with
        | none => simp [expand, node, child] at expanded
        | some subtree =>
          simp [expand, node, child] at expanded
          subst tree
          simp only [evalGraph, node, Bind.bind, Option.bind, evalTree]
          cases result : effect state with
          | error observation => rfl
          | ok state => exact ih next subtree child state
      | branch condition yes no =>
        cases left : expand graph fuel yes with
        | none => simp [expand, node, left] at expanded
        | some yesTree =>
          cases right : expand graph fuel no with
          | none => simp [expand, node, left, right] at expanded
          | some noTree =>
            simp [expand, node, left, right] at expanded
            subst tree
            simp only [evalGraph, node, Bind.bind, Option.bind, evalTree]
            cases result : condition state with
            | error observation => rfl
            | ok pair =>
              obtain ⟨value,state⟩ := pair
              cases value with
              | false => exact ih no noTree right state
              | true => exact ih yes yesTree left state

end SolarTV.Correspondence
