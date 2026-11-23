import SolarTV.EVMIR.StackGraph
import SolarTV.EVMIR.ContinuationFlow

/-! A height invariant for a single EVM control graph. Both branch directions
are possible. Only successful architectural stack transitions reach a
successor. The invariant does not constrain values or compare the two roots'
effects. -/

namespace SolarTV.EVM.StackReachability
open StackGraph SourceGraph

/-- Stack transitions permitted by `StackGraph.execute`. A handler can stop
execution or select a branch, but cannot change the architectural height. -/
inductive Step (graph : ProofTable.View Node) (signatures : Signatures) :
    Nat → Nat → Nat → Nat → Prop where
  | operation {pc h label name next inputs outputs}
      (node : graph.get? pc = some (.operation label name next))
      (arity : signatures name = some (inputs,outputs)) (enough : inputs ≤ h)
      (room : h - inputs + outputs ≤ 1024) :
      Step graph signatures pc h next (h - inputs + outputs)
  | jump {pc h label growth next}
      (node : graph.get? pc = some (.jump label growth next)) (room : h + growth ≤ 1024) :
      Step graph signatures pc h next h
  | branch {pc h label yes no}
      (node : graph.get? pc = some (.branch label yes no)) (enough : 0 < h)
      (room : h + 1 ≤ 1024) (taken : Bool) :
      Step graph signatures pc h (if taken then yes else no) (h-1)

inductive Reaches (graph : ProofTable.View Node) (signatures : Signatures) :
    Nat → Nat → Prop where
  | entry : Reaches graph signatures 0 0
  | step {pc h next height} :
      Reaches graph signatures pc h → Step graph signatures pc h next height →
      Reaches graph signatures next height

/-- Check the continuation projection and height transfer in the same pass
over the provenance points. A malformed source edge fails the certificate. -/
def project (nodes : ProofTable.View ContinuationFlow.Node)
    (points : ProofTable.View ContinuationFlow.Point) (edges : ProofTable.View ControlFlow.Edges) :
    ProofTable.View Node := ⟨points.size, fun i => ContinuationFlow.projectRow nodes points edges i⟩

def closedAt (graph : ProofTable.View Node) (signatures : Signatures)
    (bounds : Nat → Nat) (limit pc : Nat) : Bool :=
  (bounds pc ≤ limit) && (match graph.get? pc with
  | some (.operation _ name next) =>
    next < graph.size && match signatures name with
    | some (inputs,outputs) =>
      bounds pc < inputs || postHeight (bounds pc) inputs outputs ≤ bounds next
    | none => false
  | some (.jump _ growth next) =>
    next < graph.size && min (bounds pc) (1024-growth) ≤ bounds next
  | some (.branch _ yes no) =>
    yes < graph.size && no < graph.size &&
      min (bounds pc) 1023 - 1 ≤ bounds yes && min (bounds pc) 1023 - 1 ≤ bounds no
  | some (.terminal ..) => true
  | none => false)

def CheckedRange (check : Nat → Bool) (start count : Nat) : Prop :=
  ∀ point, start ≤ point → point < start + count → check point = true

theorem checkedRange (check : Nat → Bool) (start count : Nat)
    (checked : (List.range' start count).all check = true) :
    CheckedRange check start count := by
  intro point first last
  exact List.all_eq_true.mp checked point (by
    rw [List.mem_range'_1]
    exact ⟨first,last⟩)

theorem joinRanges {check : Nat → Bool} {start left right : Nat}
    (first : CheckedRange check start left)
    (second : CheckedRange check (start+left) right) :
    CheckedRange check start (left+right) := by
  intro point low high
  by_cases inside : point < start+left
  · exact first point low inside
  · exact second point (by omega) (by omega)

theorem invariant {graph : ProofTable.View Node} {signatures : Signatures} {bounds : Nat → Nat}
    {limit : Nat} (entry : 0 < graph.size)
    (closed : ∀ pc < graph.size, closedAt graph signatures bounds limit pc = true)
    {pc height : Nat} (reaches : Reaches graph signatures pc height) :
    pc < graph.size ∧ height ≤ bounds pc := by
  induction reaches with
  | entry => exact ⟨entry, Nat.zero_le _⟩
  | @step pc h next height prior executed ih =>
    obtain ⟨inside, fits⟩ := ih
    have allChecks := closed pc inside
    unfold closedAt at allChecks
    rw [Bool.and_eq_true] at allChecks
    have checked := allChecks.2
    cases executed with
    | operation node arity enough room =>
      simp [node, arity] at checked
      have post := post_height_bound h (bounds pc) _ _ fits enough room
      refine ⟨checked.1, ?_⟩
      rcases checked.2 with tooShort | after
      · omega
      · exact Nat.le_trans post after
    | jump node room =>
      simp [node] at checked
      exact ⟨checked.1, by omega⟩
    | branch node enough room taken =>
      simp [node] at checked
      cases taken with
      | false => exact ⟨checked.1.1.2, by simp only [Bool.false_eq_true, ite_false]; omega⟩
      | true => exact ⟨checked.1.1.1, by simp only [ite_true]; omega⟩

theorem cap {graph : ProofTable.View Node} {signatures : Signatures} {bounds : Nat → Nat}
    {limit : Nat} (entry : 0 < graph.size)
    (closed : ∀ pc < graph.size, closedAt graph signatures bounds limit pc = true)
    {pc height : Nat} (reaches : Reaches graph signatures pc height) : height ≤ limit := by
  obtain ⟨inside, fits⟩ := invariant entry closed reaches
  have checked := closed pc inside
  unfold closedAt at checked
  rw [Bool.and_eq_true] at checked
  exact Nat.le_trans fits (of_decide_eq_true checked.1)

/-- The source at index zero is bound to the raw entry with an empty shadow
stack. Every successor of every admitted point closes under `projectRow`. -/
theorem projected_cap {nodes : ProofTable.View ContinuationFlow.Node}
    {points : ProofTable.View ContinuationFlow.Point} {edges : ProofTable.View ControlFlow.Edges}
    {source : Nat} {signatures : Signatures} {bounds : Nat → Nat} {limit : Nat}
    (entry : 0 < points.size) (_root : points.get? 0 = some ⟨source,[]⟩)
    (closed : ∀ pc < points.size, closedAt (project nodes points edges) signatures bounds limit pc = true)
    {pc height : Nat} (reaches : Reaches (project nodes points edges) signatures pc height) :
    height ≤ limit :=
  cap entry closed reaches

end SolarTV.EVM.StackReachability
