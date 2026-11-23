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

/-- Each logical successor selects a source point on both sides of the
comparison. A source point can occur more than once when tails are shared. -/
inductive Edges where
  | next (target : Nat)
  | branch (yes no : Nat)
  | terminal

def resolve (points : Array Nat) (source logical : Nat) : Option Nat := do
  if (← points[logical]?) == source then some (logical * 2) else none

theorem resolve_edge (points : Array Nat) (source logical : Nat)
    (found : resolve points source logical = some (logical * 2)) :
    points[logical]? = some source := by
  cases point : points[logical]? <;>
    simp_all [resolve, Bind.bind, Option.bind]

/-- Check every proposed edge against the actual successor, even if two
different logical edges use the same physical point. -/
def project (program : Program) (points : Array Nat) (edges : Array Edges)
    (jumpLimit : Nat) : Option Program := do
  let entry ← resolve points program.entry 0
  if edges.size != points.size then none else
  let rows ← (points.toList.zipIdx).mapM fun (pc,index) => do
    let path ← follow program jumpLimit pc
    let action ← match ← program.nodes[path.destination]?, ← edges[index]? with
    | .operation _ name next, .next target => pure (.operation 0 name (← resolve points next target))
    | .branch _ yes no, .branch yesTarget noTarget =>
      pure (.branch 0 (← resolve points yes yesTarget) (← resolve points no noTarget))
    | .terminal _ name, .terminal => pure (.terminal 0 name)
    | _, _ => none
    return [.jump 0 path.growth (2*index+1),action]
  return ⟨entry, rows.flatten.toArray⟩

/-- Equality after checking the projection from both independent source
graphs. A logical step covers a finite transfer chain and its first
non-jump operation. Position-independent handlers carry the concrete
ordinary effects across the preserved action sequence. -/
theorem contextual (before after old new : Program) (sourcePoints targetPoints : Array Nat)
    (edges : Array Edges) (jumpLimit : Nat) (signatures : Signatures) (bounds : Nat → Nat)
    (oldProjection : project before sourcePoints edges jumpLimit = some old)
    (newProjection : project after targetPoints edges jumpLimit = some new)
    (valid : Valid old new id bounds signatures)
    (handlers : Handlers World Observation) (work : Nat) (state : State World) :
    (project before sourcePoints edges jumpLimit).map
        (fun p => execute p signatures handlers work p.entry state) =
      (project after targetPoints edges jumpLimit).map
        (fun p => execute p signatures handlers work p.entry state) := by
  rw [oldProjection,newProjection]
  exact congrArg some (StackGraph.contextual old new id bounds signatures valid handlers work state)

end SolarTV.EVM.ControlFlow
