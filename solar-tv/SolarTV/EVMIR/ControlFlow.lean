import SolarTV.EVMIR.StackGraph

/-! Contract finite unconditional jump paths in unpriced EVM IR. A jump can
require a temporary stack slot, so the projection keeps the strongest
growth guard on the path. Ordinary operations and transfers must retain
their operands and all successors. Paths that remain in jump-only cycles
have no projection. -/

namespace SolarTV.EVM.ControlFlow
open SourceGraph (Program Node)
open StackGraph

structure Path where
  destination : Nat
  jumps : Nat
  growth : Nat

def follow (program : Program) : Nat → Nat → Option Path
  | 0, _ => none
  | fuel+1, pc => do
    match ← program.nodes[pc]? with
    | .jump _ growth next =>
      if growth > 1 then none else
      let path ← follow program fuel next
      return ⟨path.destination,path.jumps+1,max growth path.growth⟩
    | _ => return ⟨pc,0,0⟩

/-- A finite chain performs no effects. It can fail only at a stack limit. -/
theorem follow_execute (program : Program) (signatures : Signatures)
    (handlers : Handlers World Observation) (fuel pc : Nat) (path : Path)
    (found : follow program fuel pc = some path) (work : Nat) (state : State World)
    (fits : state.height ≤ 1024) :
    execute program signatures handlers (work+path.jumps) pc state =
      if state.height + path.growth > 1024 then some .overflow
      else execute program signatures handlers work path.destination state := by
  induction fuel generalizing pc path with
  | zero => simp [follow] at found
  | succ fuel ih =>
    have notFull : ¬state.height > 1024 := by omega
    cases node : program.nodes[pc]? with
    | none => simp [follow,node,Bind.bind,Option.bind] at found
    | some instruction =>
      cases instruction with
      | jump label growth next =>
        simp only [follow,node,Bind.bind,Option.bind] at found
        split at found
        · contradiction
        · cases tail : follow program fuel next with
          | none => simp [tail] at found
          | some subpath =>
            simp only [tail,Option.pure_def,Option.some.injEq] at found
            subst path
            change execute program signatures handlers (work+subpath.jumps+1) pc state = _
            simp only [execute,notFull,ite_false,node,Bind.bind,Option.bind]
            by_cases full : state.height + growth > 1024
            · have limit : state.height + max growth subpath.growth > 1024 := by omega
              simp [full,limit]
            · rw [ite_eq_right full, ih next subpath tail]
              have same : (state.height + max growth subpath.growth > 1024) ↔
                  (state.height + subpath.growth > 1024) := by omega
              simp only [same]
      | operation _ _ _ | branch _ _ _ | terminal _ _ =>
        simp [follow,node,Bind.bind,Option.bind,Option.pure_def] at found
        subst path
        simp [notFull]

/-- Selecting a successor is part of the checked projection. Logical point
indices identify the same physical path on both sides of the comparison. -/
def project (program : Program) (points : Array Nat) (jumpLimit : Nat) :
    Option Program := do
  let resolve (pc : Nat) := (points.toList.idxOf? pc).map (· * 2)
  let rows ← (points.toList.zipIdx).mapM fun (pc,index) => do
    let path ← follow program jumpLimit pc
    let action ← match ← program.nodes[path.destination]? with
    | .operation _ name next => pure (.operation 0 name (← resolve next))
    | .branch _ yes no => pure (.branch 0 (← resolve yes) (← resolve no))
    | .terminal _ name => pure (.terminal 0 name)
    | .jump .. => none
    return [.jump 0 path.growth (2*index+1),action]
  return ⟨← resolve program.entry, rows.flatten.toArray⟩

/-- Equality after checking the projection from both independent source
graphs. A logical step covers a finite transfer chain and its first
non-jump operation. Position-independent handlers carry the concrete
ordinary effects across the preserved action sequence. -/
theorem contextual (before after old new : Program) (sourcePoints targetPoints : Array Nat)
    (jumpLimit : Nat) (signatures : Signatures) (bounds : Nat → Nat)
    (oldProjection : project before sourcePoints jumpLimit = some old)
    (newProjection : project after targetPoints jumpLimit = some new)
    (valid : Valid old new id bounds signatures)
    (handlers : Handlers World Observation) (work : Nat) (state : State World) :
    (project before sourcePoints jumpLimit).map
        (fun p => execute p signatures handlers work p.entry state) =
      (project after targetPoints jumpLimit).map
        (fun p => execute p signatures handlers work p.entry state) := by
  rw [oldProjection,newProjection]
  exact congrArg some (StackGraph.contextual old new id bounds signatures valid handlers work state)

end SolarTV.EVM.ControlFlow
