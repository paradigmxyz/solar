/-! Removing unreachable named function declarations.

Retained bodies and their direct call targets are identical. Execution may stay
inside a body, call or tail-call a declared target, return to its saved caller,
or halt with an arbitrary observation. Primitive instruction semantics and the
complete mutable state are parameters, so cycles, failures and side effects are
not assumed away. Equal execution holds for every step bound, without a fixed
unrolling or termination assumption.

The source parser/display, reference extraction and binding to this named-call
machine remain trusted. This is structural function-table correspondence, not
an instruction-semantics, physical gas, function-pointer or layout certificate.
-/

namespace SolarTV.MIR.FunctionDCE

structure Body where
  text : List String
  targets : List String
  root : Bool
  deriving DecidableEq

abbrev Program := List (String × Body)

def lookup (program : Program) (name : String) : Option Body :=
  (program.find? (fun item => item.1 == name)).map Prod.snd

inductive Action (targets : List String) (State Saved Observation : Type) where
  | next (state : State)
  | call (target : Fin targets.length) (state : State) (saved : Saved)
  | tailCall (target : Fin targets.length) (state : State)
  | returned (state : State)
  | halt (observation : Observation)

structure Machine (State Saved : Type) where
  function : String
  state : State
  stack : List (String × Saved)

def Valid (kept : List String) (machine : Machine State Saved) : Prop :=
  machine.function ∈ kept ∧ ∀ caller ∈ machine.stack, caller.1 ∈ kept

def run (program : Program)
    (step : (body : Body) → State → Action body.targets State Saved Observation)
    (resume : Saved → State → State) (finish : State → Observation) :
    Nat → Machine State Saved → Option Observation
  | 0, _ => none
  | fuel + 1, machine => do
    let body ← lookup program machine.function
    (match step body machine.state with
    | .next state => run program step resume finish fuel {machine with state}
    | .call target state saved => run program step resume finish fuel
        ⟨body.targets[target], state, (machine.function,saved) :: machine.stack⟩
    | .tailCall target state => run program step resume finish fuel
        ⟨body.targets[target], state, machine.stack⟩
    | .returned state => (match machine.stack with
      | [] => some (finish state)
      | (caller,saved) :: rest => run program step resume finish fuel
          ⟨caller, resume saved state, rest⟩)
    | .halt observation => some observation)

def Closed (program : Program) (kept : List String) : Prop :=
  ∀ name ∈ kept, ∀ body, lookup program name = some body →
    ∀ target ∈ body.targets, target ∈ kept

/-- Restricting the table cannot change any execution rooted in the retained
closed set, including recursive calls and every finite observation prefix. -/
theorem preserve (before after : Program) (kept : List String)
    (same : ∀ name ∈ kept, lookup before name = lookup after name)
    (closed : Closed before kept)
    (step : (body : Body) → State → Action body.targets State Saved Observation)
    (resume : Saved → State → State) (finish : State → Observation)
    (fuel : Nat) (machine : Machine State Saved) (valid : Valid kept machine) :
    run before step resume finish fuel machine = run after step resume finish fuel machine := by
  induction fuel generalizing machine with
  | zero => rfl
  | succ fuel ih =>
    have names := same machine.function valid.1
    cases found : lookup before machine.function with
    | none => simp [run,← names,found]
    | some body =>
      simp only [run,← names,found,Bind.bind,Option.bind]
      cases action : step body machine.state with
      | next state => exact ih _ ⟨valid.1,valid.2⟩
      | call target state saved =>
        apply ih
        refine ⟨closed _ valid.1 body found _ (List.getElem_mem _), ?_⟩
        intro caller present
        simp only [List.mem_cons] at present
        rcases present with rfl | present
        · exact valid.1
        · exact valid.2 caller present
      | tailCall target state =>
        exact ih _ ⟨closed _ valid.1 body found _ (List.getElem_mem _), valid.2⟩
      | returned state =>
        cases stack : machine.stack with
        | nil => rfl
        | cons caller rest =>
          apply ih
          refine ⟨valid.2 caller (by simp [stack]), ?_⟩
          intro other present
          exact valid.2 other (by simp [stack,present])
      | halt observation => rfl

/-- Executable obligations bind every surviving body and its target set. -/
def unchanged (before after : Program) : Bool :=
  after.all (fun (name,body) => decide (lookup before name = some body))

def closed (program : Program) : Bool :=
  program.all (fun (_,body) => body.targets.all (fun name => decide (name ∈ program.map Prod.fst)))

def rootsKept (before after : Program) : Bool :=
  before.all (fun (name,body) => if body.root then decide (name ∈ after.map Prod.fst) else true)

@[simp] theorem lookup_nil (name : String) : lookup [] name = none := rfl

@[simp] theorem lookup_cons (key name : String) (body : Body) (rest : Program) :
    lookup ((key,body) :: rest) name = if key = name then some body else lookup rest name := by
  by_cases equal : key = name <;> simp [lookup,equal]

theorem lookup_exists (program : Program) (name : String) (present : name ∈ program.map Prod.fst) :
    ∃ body, lookup program name = some body := by
  induction program with
  | nil => simp at present
  | cons item rest ih =>
    obtain ⟨key,body⟩ := item
    by_cases equal : key = name
    · exact ⟨body,by simp [equal]⟩
    · have tail : name ∈ rest.map Prod.fst := by simpa [Ne.symm equal] using present
      obtain ⟨found,h⟩ := ih tail
      exact ⟨found,by simp [equal,h]⟩

theorem lookup_mem (program : Program) (name : String) (body : Body)
    (found : lookup program name = some body) : (name,body) ∈ program := by
  induction program with
  | nil => simp at found
  | cons item rest ih =>
    obtain ⟨key,head⟩ := item
    by_cases equal : key = name
    · simp [equal] at found
      simp [equal,found]
    · have tail : lookup rest name = some body := by simpa [equal] using found
      exact List.mem_cons_of_mem _ (ih tail)

theorem unchanged_lookup (before after : Program) (checked : unchanged before after = true)
    (name : String) (present : name ∈ after.map Prod.fst) :
    lookup before name = lookup after name := by
  obtain ⟨body, found⟩ : ∃ body, lookup after name = some body := by
    exact lookup_exists after name present
  have member := lookup_mem after name body found
  simp only [unchanged,List.all_eq_true] at checked
  have beforeFound := of_decide_eq_true (checked (name,body) member)
  exact beforeFound.trans found.symm

theorem closed_before (before after : Program) (same : unchanged before after = true)
    (checked : closed after = true) : Closed before (after.map Prod.fst) := by
  intro name present body found target member
  have equal := unchanged_lookup before after same name present
  have afterFound : lookup after name = some body := equal.symm.trans found
  have inTable := lookup_mem after name body afterFound
  simp only [closed,List.all_eq_true] at checked
  exact of_decide_eq_true (checked (name,body) inTable target member)

theorem roots_preserved (before after : Program) (checked : rootsKept before after = true)
    (name : String) (body : Body) (present : (name,body) ∈ before) (root : body.root = true) :
    name ∈ after.map Prod.fst := by
  simp only [rootsKept,List.all_eq_true] at checked
  have h := checked (name,body) present
  simp only [root,ite_true] at h
  exact of_decide_eq_true h

theorem certificate (before after : Program)
    (same : unchanged before after = true) (closed : closed after = true)
    (step : (body : Body) → State → Action body.targets State Saved Observation)
    (resume : Saved → State → State) (finish : State → Observation)
    (fuel : Nat) (machine : Machine State Saved) (valid : Valid (after.map Prod.fst) machine) :
    run before step resume finish fuel machine = run after step resume finish fuel machine :=
  preserve before after _ (unchanged_lookup before after same) (closed_before before after same closed)
    step resume finish fuel machine valid

end SolarTV.MIR.FunctionDCE
