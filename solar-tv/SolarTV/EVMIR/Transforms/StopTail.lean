import SolarTV.EVMIR.StackReachability

/-! Removing POPs immediately before STOP preserves the unpriced exit if
the caller supplies enough stack words. The lower bound must hold at the
first removed POP on every path from physical entry. A block with unknown
entry height does not get to assume the deleted POPs succeed. -/

namespace SolarTV.EVM.StopTail
open StackGraph StackReachability

def tail (nodes : ProofTable.View ContinuationFlow.Node) : Nat → Nat → Bool
  | 0, pc => nodes.get? pc == some (.terminal "stop" 0)
  | count+1, pc =>
    match nodes.get? pc with
    | some (.operation "pop" .pop next) => tail nodes count next
    | _ => false

def floorAtCut (points : ProofTable.View ContinuationFlow.Point) (floors : Nat → Nat)
    (cut count pc : Nat) : Bool :=
  match points.get? pc with
  | some point => point.pc != cut || count ≤ floors pc
  | none => false

theorem discard_then_stop (ctx : Context) (count : Nat) (stack : Stack)
    (enough : count ≤ stack.length) (fits : stack.length ≤ 1024) :
    EVM.execute ctx (List.replicate count .pop) .stop stack = .success := by
  induction count generalizing stack with
  | zero => simp [EVM.execute, finish, Nat.not_lt_of_ge fits]
  | succ n ih =>
    cases stack with
    | nil => simp at enough
    | cons head rest =>
      have room : rest.length ≤ 1023 := by simpa using fits
      have restFits : rest.length ≤ 1024 := by omega
      have enoughRest : n ≤ rest.length := by simpa using enough
      have popped : stepChecked ctx .pop (head :: rest) = .ok rest := by
        simp [stepChecked, step, room, Nat.not_lt_of_ge restFits]
      simpa only [List.replicate_succ, EVM.execute, popped] using ih rest enoughRest restFits

/-- The two cuts stop with the same world. The POPs have no effect to
preserve beyond ensuring they cannot underflow on the original path. -/
theorem from_entry {nodes : ProofTable.View ContinuationFlow.Node}
    {points : ProofTable.View ContinuationFlow.Point} {edges : ProofTable.View ControlFlow.Edges}
    {signatures : Signatures} {floors : Nat → Nat} {source cut count : Nat}
    (entry : 0 < points.size) (_root : points.get? 0 = some ⟨source,[]⟩)
    (empty : floors 0 = 0)
    (closed : ∀ pc < points.size, StackReachability.floorAt
      (project nodes points edges) signatures floors pc = true)
    (cutChecked : ∀ pc < points.size, floorAtCut points floors cut count pc = true)
    (_tail : tail nodes count cut = true) (ctx : Context) (stack : Stack)
    {pc : Nat} {point : ContinuationFlow.Point}
    (reaches : Reaches (project nodes points edges) signatures pc stack.length)
    (pointEq : points.get? pc = some point) (atCut : point.pc = cut) :
    EVM.execute ctx (List.replicate count .pop) .stop stack =
      EVM.execute ctx [] .stop stack := by
  obtain ⟨inside,lower,upper⟩ := floor_invariant entry empty closed reaches
  have needed := cutChecked pc inside
  simp [floorAtCut, pointEq, atCut] at needed
  exact (discard_then_stop ctx count stack (Nat.le_trans needed lower) upper).trans
    (discard_then_stop ctx 0 stack (Nat.zero_le _) upper).symm

end SolarTV.EVM.StopTail
