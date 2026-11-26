import SolarTV.MIR.Transforms.RecursiveCalls

/-! Argument projection in the logical MIR call machine. Register IDs separate
SSA locals, source argument slots and typed constants. Each callee retains its
original slot names even when the caller supplies a shorter argument vector.
The checker requires every original read of a parameter to use a retained slot
and every projected call to keep exactly the declared operands. -/

namespace SolarTV.MIR.DeadArguments
open CfgFlow RecursiveCalls

abbrev Slots := Nat → Option (List Nat)

def select (slots : Option (List Nat)) (defaultValue : Value) (values : List Value) : List Value :=
  match slots with
  | none => values
  | some keep => keep.map fun i => values[i]?.getD defaultValue

def project (slots : Slots) (action : Action) : Action :=
  match action.key with
  | .directCall callee _ _ => {action with reads := select (slots callee) 0 action.reads}
  | _ => action

def projectNode (slots : Slots) : Node → Node
  | .action action => .action (project slots action)
  | other => other

def live (keep : Option (List Nat)) (register : Nat) : Bool :=
  register % 3 != 1 || (keep.all fun slots => slots.contains (register / 3))

def validAction (slots : Slots) (caller : Nat) (action : Action) : Bool :=
  action.reads.all (live (slots caller)) &&
    match action.key with
    | .directCall callee _ _ =>
      (slots callee).all fun keep => keep.all (· < action.reads.length)
    | _ => true

def validNode (slots : Slots) (caller : Nat) : Node → Bool
  | .action action => validAction slots caller action
  | _ => true

def seeded (keep : Option (List Nat)) (literal : Nat → Value) (d : Value)
    (args : List Value) : Locals Value :=
  fun register =>
    if register % 3 = 1 then
      let index := register / 3
      match keep with
      | none => args[index]?.getD d
      | some kept => ((kept.idxOf? index).bind (fun i => args[i]?)).getD d
    else if register % 3 = 2 then literal (register / 3)
    else d

def Agrees (keep : Option (List Nat)) (a b : Locals Value) : Prop :=
  ∀ r, live keep r = true → a r = b r

theorem select_reads (keep : Option (List Nat)) (reads : List Nat) (d : Value) (r : Locals Value)
    (valid : keep.all (fun keep => keep.all (· < reads.length)) = true) :
    (select keep 0 reads).map r = select keep d (reads.map r) := by
  cases keep with
  | none => rfl
  | some keep =>
    simp only [select, List.map_map]
    apply List.map_congr_left
    intro i member
    have bound : i < reads.length := of_decide_eq_true (List.all_eq_true.mp valid i member)
    simp [List.getElem?_eq_getElem bound]

theorem seed_agrees (keep : Option (List Nat)) (literal : Nat → Value) (d : Value)
    (args : List Value) :
    Agrees keep (seeded none literal d args) (seeded keep literal d (select keep d args)) := by
  intro r isLive
  cases keep with
  | none => rfl
  | some keep =>
    by_cases tag : r % 3 = 1
    · have member : r / 3 ∈ keep := by simpa [live, tag] using isLive
      cases index : keep.idxOf? (r / 3) with
      | none => exact False.elim (List.idxOf?_eq_none_iff.mp index member)
      | some i =>
        obtain ⟨bound, slot, _⟩ := List.idxOf?_eq_some_iff.mp index
        simp [seeded, tag, index, select, List.getElem?_map, List.getElem?_eq_getElem bound, slot]
    · simp [seeded, tag]

theorem follow_project (slots : Slots) (graph : Graph) (limit index : Nat) :
    follow (graph.map (projectNode slots)) limit index =
      (follow graph limit index).map (project slots) := by
  induction limit generalizing index with
  | zero => rfl
  | succ n ih =>
    simp only [follow, Array.getElem?_map]
    cases h : graph[index]? with
    | none => rfl
    | some node => cases node <;> simp [projectNode, Option.bind, ih]

theorem follow_valid (slots : Slots) (caller : Nat) (graph : Graph) (limit index : Nat)
    (valid : graph.all (validNode slots caller) = true) (a : Action)
    (found : follow graph limit index = some a) : validAction slots caller a = true := by
  induction limit generalizing index with
  | zero => simp [follow] at found
  | succ n ih =>
    simp only [follow] at found
    cases h : graph[index]? with
    | none => simp [h, Option.bind] at found
    | some node =>
      cases node with
      | action action =>
        have eq : action = a := by simpa [h, Option.bind] using found
        subst a
        exact Array.all_eq_true_iff_forall_mem.mp valid _ (Array.mem_of_getElem? h)
      | forward next => exact ih next (by simpa [h, Option.bind] using found)
      | literalBranch condition yes no =>
        exact ih (if condition then yes else no) (by simpa [h, Option.bind] using found)

/-- A direct call enters a new logical activation with the operands it reads.
The shared resume handler propagates halts, results and ordered effects. -/
def handlers
    (ordinary : Nat → Handler Value World Observation)
    (resume : Nat → Nat → Nat → Bool → Result Observation → World →
      Except Observation (Nat × Locals Value × World)) :
    Handlers Value World Observation where
  ordinary := fun caller _ => ordinary caller
  operands := fun _ _ _ _ _ inputs => .ok inputs
  enter := fun _ _ _ _ _ world => .ok world
  resume := resume

theorem run_project (slots : Slots) (caller : Nat) (graph : Graph) (limit : Nat)
    (valid : graph.all (validNode slots caller) = true)
    (before after : Handler Value World Observation) (d : Value)
    (sameHandler : ∀ (a : Action) (inputs : List Value) (world : World),
      before a.key.normalized inputs world =
        after (project slots a).key.normalized
          (match a.key with
           | .directCall callee _ _ => select (slots callee) d inputs
           | _ => inputs) world)
    (work index : Nat) (a b : Locals Value) (world : World)
    (same : Agrees (slots caller) a b) :
    CfgFlow.run graph limit before work index a world =
      CfgFlow.run (graph.map (projectNode slots)) limit after work index b world := by
  induction work generalizing index a b world with
  | zero => rfl
  | succ work ih =>
    simp only [CfgFlow.run, follow_project]
    cases found : follow graph limit index with
    | none => rfl
    | some act =>
      have validAct := follow_valid slots caller graph limit index valid act found
      simp only [validAction, Bool.and_eq_true_iff] at validAct
      obtain ⟨readOK, callsOK⟩ := validAct
      have readsEq : act.reads.map a = act.reads.map b :=
        List.map_congr_left fun r h => same r (List.all_eq_true.mp readOK r h)
      have projected : (project slots act).reads.map b =
          (match act.key with
           | .directCall callee _ _ => select (slots callee) d (act.reads.map a)
           | _ => act.reads.map a) := by
        rw [readsEq]
        cases key : act.key <;> simp only [project, key]
        exact select_reads _ _ _ _ (by simpa only [key] using callsOK)
      simp only [Option.map_some]
      rw [projected, ← sameHandler act (act.reads.map a) world]
      have targets : (project slots act).targets = act.targets := by
        cases key : act.key <;> simp [project, key]
      have writes : (project slots act).writes = act.writes := by
        cases key : act.key <;> simp [project, key]
      rw [targets, writes]
      split
      · rfl
      · next arm reply state _ =>
        split
        · rfl
        · next successor _ =>
          apply ih
          intro r live
          simp only [CfgFlow.write]
          cases h : act.writes.idxOf? r
          · exact same r live
          · rfl

structure Pair (slots : Slots) where
  id : Nat
  before : Procedure
  after : Procedure
  safe : before.graph.all (validNode slots id) = true
  graph : after.graph = before.graph.map (projectNode slots)
  limit : after.jumpLimit = before.jumpLimit

def procedures (pairs : List (Pair slots)) (after : Bool) (id : Nat) : Option Procedure :=
  (pairs.find? (·.id == id)).map fun p => if after then p.after else p.before

/-- Preserve the whole logical call execution, including all finite work and
activation bounds, without assuming that any callee returns normally. -/
theorem simulation (slots : Slots) (pairs : List (Pair slots))
    (ordinary : Nat → Handler Value World Observation)
    (resume : Nat → Nat → Nat → Bool → Result Observation → World →
      Except Observation (Nat × Locals Value × World))
    (literal : Nat → Value) (d : Value) (work depth id : Nat)
    (args : List Value) (world : World) :
    runRecursive (procedures pairs false) (handlers ordinary resume)
      (fun _ => seeded none literal d) work depth id args world =
    runRecursive (procedures pairs true) (handlers ordinary resume)
      (fun id => seeded (slots id) literal d) work depth id (select (slots id) d args) world := by
  induction depth generalizing id args world with
  | zero => rfl
  | succ depth ih =>
    cases found : pairs.find? (·.id == id) with
    | none => simp [runRecursive, procedures, found]
    | some pair =>
      have idEq : pair.id = id := by
        have := List.find?_some found
        simpa using this
      simp only [runRecursive, procedures, found, Option.map_some, Bool.false_eq_true,
        ite_false, ite_true, pair.graph, pair.limit]
      have safe : pair.before.graph.all (validNode slots id) = true := idEq ▸ pair.safe
      apply run_project slots id _ _ safe _ _ d
      · intro a inputs state
        cases key : a.key <;>
          simp [project, key, ActionKey.normalized, Handlers.dispatch, handlers,
            Except.bind, Bind.bind]
        congr 1
        exact ih _ _ _
      · exact seed_agrees _ _ _ _

end SolarTV.MIR.DeadArguments
