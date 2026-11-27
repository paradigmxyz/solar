import SolarTV.EVMIR.StackGraph
import SolarTV.EVMIR.Ops

/-! A sparse constant invariant over the independently imported graph.
Only literal pushes introduce facts. Stack shuffles and the untouched suffix
of an instruction preserve them. All predecessors must justify a requested
fact, including both directions of a conditional. -/

namespace SolarTV.EVM.StackWords
open EvmRules SourceGraph

abbrev Facts := List (Nat × Word)

inductive Movement
  | effect (inputs outputs : Nat)
  | literal (value : Word)
  | dup (depth : Nat)
  | swap (depth : Nat)

inductive Origin
  | unknown
  | literal (value : Word)
  | stack (depth : Nat)

def Movement.origin : Movement → Nat → Origin
  | .effect inputs outputs, depth =>
    if depth < outputs then .unknown else .stack (depth - outputs + inputs)
  | .literal word, 0 => .literal word
  | .literal _, depth+1 => .stack depth
  | .dup depth, 0 => .stack depth
  | .dup _, depth+1 => .stack depth
  | .swap depth, index =>
    .stack (if index = 0 then depth else if index = depth then 0 else index)

def Movement.relates (move : Movement) (before after : Stack) : Prop :=
  ∀ depth, match move.origin depth with
  | .unknown => True
  | .literal word => after[depth]? = some word
  | .stack prior => after[depth]? = before[prior]?

theorem effect_relates (stack results : Stack) (inputs : Nat) :
    (Movement.effect inputs results.length).relates stack (results ++ stack.drop inputs) := by
  intro depth
  by_cases head : depth < results.length
  · simp [Movement.origin, head]
  · simp [Movement.origin, head, List.getElem?_append_right (Nat.le_of_not_gt head),
      List.getElem?_drop, Nat.add_comm]

theorem literal_relates (stack : Stack) (word : Word) :
    (Movement.literal word).relates stack (word :: stack) := by
  intro depth
  cases depth <;> rfl

theorem dup_relates (stack : Stack) (depth : Nat) (word : Word)
    (value : stack[depth]? = some word) : (Movement.dup depth).relates stack (word :: stack) := by
  intro index
  cases index <;> simp [Movement.origin, value]

theorem swap_relates (stack : Stack) (depth : Nat) (first other : Word)
    (different : depth ≠ 0) (top : stack[0]? = some first) (atDepth : stack[depth]? = some other) :
    (Movement.swap depth).relates stack ((stack.set 0 other).set depth first) := by
  have topFits := (List.getElem?_eq_some_iff.mp top).1
  have depthFits := (List.getElem?_eq_some_iff.mp atDepth).1
  intro index
  by_cases zero : index = 0
  · subst index
    change ((stack.set 0 other).set depth first)[0]? = stack[depth]?
    rw [List.getElem?_set_ne different, List.getElem?_set_self topFits, atDepth]
  · by_cases equal : index = depth
    · subst index
      simp [Movement.origin, zero, depthFits, top]
    · simp [Movement.origin, zero, equal,
        List.getElem?_set_ne (Ne.symm equal), List.getElem?_set_ne (Ne.symm zero)]

abbrev Specials := String → Option Movement

def movement (name : String) (signatures : StackGraph.Signatures) (specials : Specials)
    (relocated : Bool) :
    Option Movement := do
  let (inputs,outputs) ← signatures name
  match specials name with
  | some (.literal word) =>
    if relocated then some (.effect inputs outputs)
    else if name == s!"push {word.toNat}" then some (.literal word) else none
  | some (.dup depth) =>
    if depth < 16 && name == s!"dup {depth + 1}" then some (.dup depth) else none
  | some (.swap depth) =>
    if 0 < depth && depth ≤ 16 && name == s!"swap {depth}" then some (.swap depth) else none
  | some (.effect ..) => none
  | none => some (.effect inputs outputs)

def successors (node : Node) (signatures : StackGraph.Signatures) (specials : Specials)
    (relocated : Bool) :
    Option (List (Nat × Movement)) := do
  match node with
  | .operation _ name next => return [(next, ← movement name signatures specials relocated)]
  | .jump _ _ next => return [(next,.effect 0 0)]
  | .branch _ yes no => return [(yes,.effect 1 0),(no,.effect 1 0)]
  | .terminal .. => return []

def has (facts : Facts) (move : Movement) (fact : Nat × Word) : Bool :=
  match move.origin fact.1 with
  | .unknown => false
  | .literal word => word == fact.2
  | .stack depth => facts.contains (depth,fact.2)

def Holds (facts : Facts) (stack : Stack) : Prop :=
  ∀ fact ∈ facts, stack[fact.1]? = some fact.2

theorem transfers {before after : Stack} {facts : Facts} {move : Movement}
    (related : move.relates before after) (known : Holds facts before)
    (fact : Nat × Word) (valid : has facts move fact = true) :
    after[fact.1]? = some fact.2 := by
  have step := related fact.1
  cases source : move.origin fact.1 with
  | unknown => simp [has, source] at valid
  | literal word => simpa [has, source, (by simpa [has, source] using valid : word = fact.2)] using step
  | stack depth =>
    simp only [source] at step
    exact step.trans (known (depth,fact.2) (by simpa [has, source] using valid))

def closedAt (nodes : ProofTable.View Node) (signatures : StackGraph.Signatures)
    (specials : Specials) (relocated : Nat → Bool) (facts : Nat → Facts) (pc : Nat) : Bool :=
  match (nodes.get? pc).bind (fun node => successors node signatures specials (relocated pc)) with
  | none => false
  | some edges => edges.all fun (next,move) =>
    next < nodes.size && (facts next).all (has (facts pc) move)

inductive Reaches (nodes : ProofTable.View Node) (signatures : StackGraph.Signatures)
    (specials : Specials) (relocated : Nat → Bool) (entry : Nat) : Nat → Stack → Prop where
  | start (stack : Stack) : Reaches nodes signatures specials relocated entry entry stack
  | step {pc next : Nat} {before after : Stack} {edges : List (Nat × Movement)} {move : Movement}
      (reached : Reaches nodes signatures specials relocated entry pc before)
      (source : (nodes.get? pc).bind
        (fun node => successors node signatures specials (relocated pc)) = some edges)
      (edge : (next,move) ∈ edges) (related : move.relates before after) :
      Reaches nodes signatures specials relocated entry next after

theorem invariant {nodes : ProofTable.View Node} {signatures : StackGraph.Signatures}
    {specials : Specials} {relocated : Nat → Bool} {facts : Nat → Facts} {entry : Nat}
    (admitted : entry < nodes.size) (initial : facts entry = [])
    (closed : ∀ pc < nodes.size, closedAt nodes signatures specials relocated facts pc = true)
    {pc : Nat} {stack : Stack} (reach : Reaches nodes signatures specials relocated entry pc stack) :
    pc < nodes.size ∧ Holds (facts pc) stack := by
  induction reach with
  | start stack => exact ⟨admitted, by simp [initial, Holds]⟩
  | @step pc next before after edges move _ source edge related ih =>
    have valid := closed pc ih.1
    simp only [closedAt, source] at valid
    have arrived := List.all_eq_true.mp valid (next,move) edge
    simp only [Bool.and_eq_true, decide_eq_true_eq] at arrived
    exact ⟨arrived.1, fun word found =>
      transfers related ih.2 word (List.all_eq_true.mp arrived.2 word found)⟩

end SolarTV.EVM.StackWords
