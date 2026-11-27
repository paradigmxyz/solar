import SolarTV.MIR.Transforms.DeadSSA
import SolarTV.MIR.State.Ops
import SolarTV.MIR.Transforms.ContinuedCounter

/-! Check permutations of the existing SSA action machine. Only a total
state-independent action may cross an arbitrary effect. The two actions must
preserve each other's inputs and write disjoint results. Induction covers all
logical block bounds, with cycles and possible shared-handler failures. The
importer supplies the typed action correspondence. Physical gas is unpriced. -/

namespace SolarTV.MIR.Schedule
open DeadSSA

theorem calldata_load (cfg : MIR.State.Config) (frame : MIR.State.Frame) (offset : EvmRules.Word) :
    MIR.State.memoryOperation cfg frame .calldataLoad [offset] =
      MIR.State.value frame (Execution.readWord cfg.environment.input offset.toNat) := rfl

theorem calldata_size (cfg : MIR.State.Config) (frame : MIR.State.Frame) :
    MIR.State.memoryOperation cfg frame .calldataSize [] =
      MIR.State.value frame (BitVec.ofNat 256 cfg.environment.input.length) := rfl

theorem address (cfg : MIR.State.Config) (frame : MIR.State.Frame) :
    MIR.State.contextOperation cfg frame 0x30 [] =
      MIR.State.value frame (cfg.environment.address.setWidth 256) := rfl

/-- Normalize an aligned memory access without expanding its symbolic bytes. -/
theorem expand_aligned_end (memory : List Execution.Byte) (offset count : Nat)
    (present : 0 < count) (ending : (offset+count)%32 = 0)
    (bounded : memory.length ≤ 65536) (fits : offset+count ≤ 65536) :
    Execution.expand memory offset count = .ok (MIR.ContinuedCounter.padTo memory (offset+count)) := by
  unfold Execution.expand Execution.requiredWords Execution.memoryWords MIR.ContinuedCounter.padTo
  rw [ite_eq_right (by omega : count ≠ 0)]
  by_cases covered : offset + count ≤ memory.length
  · rw [Nat.max_eq_left (by omega : (offset+count+31)/32 ≤ memory.length/32)]
    have active : 32*(memory.length/32) ≤ memory.length := by omega
    simp [Nat.not_lt.mpr (Nat.le_trans active bounded), Nat.sub_eq_zero_of_le active,
      Nat.sub_eq_zero_of_le covered]
  · rw [Nat.max_eq_right (by omega : memory.length/32 ≤ (offset+count+31)/32)]
    simp [show 32*((offset+count+31)/32) = offset+count by omega, Nat.not_lt.mpr fits]

theorem warm_account (journal : Execution.Journal) (owner address : Execution.Address)
    (key : EvmRules.Word) :
    Execution.account (Execution.warmSlot journal owner key) address =
      Execution.account journal address := rfl

theorem warm_accounts (journal : Execution.Journal) (owner : Execution.Address)
    (key : EvmRules.Word) :
    (Execution.warmSlot journal owner key).accounts = journal.accounts := rfl

theorem warm_logs (journal : Execution.Journal) (owner : Execution.Address)
    (key : EvmRules.Word) :
    (Execution.warmSlot journal owner key).logs = journal.logs := rfl

def disjoint (left right : List Nat) : Bool :=
  left.all (!right.contains ·)

def commutes (left right : Action) : Bool :=
  (left.silent || right.silent) &&
    disjoint left.reads right.writes &&
    disjoint right.reads left.writes &&
    disjoint left.writes right.writes

private theorem outside (left right : List Nat) (h : disjoint left right = true)
    (id : Nat) (member : id ∈ left) : id ∉ right := by
  simpa using (List.all_eq_true.mp h) id member

private theorem disjoint_sym (left right : List Nat) (h : disjoint left right = true) :
    disjoint right left = true := by
  apply List.all_eq_true.mpr
  intro id member
  have notLeft : id ∉ left := fun inLeft => outside left right h id inLeft member
  simpa using notLeft

private theorem map_update (registers reply : Registers Value) (reads writes : List Nat)
    (h : disjoint reads writes = true) :
    reads.map (update registers writes reply) = reads.map registers := by
  apply List.map_congr_left
  intro id member
  simp [update,outside reads writes h id member]

private theorem updates_commute (registers a b : Registers Value) (left right : List Nat)
    (h : disjoint left right = true) :
    update (update registers left a) right b = update (update registers right b) left a := by
  funext id
  by_cases first : id ∈ left
  · simp [update,first,outside left right h id first]
  · by_cases second : id ∈ right
    · simp [update,first,second]
    · simp [update,first,second]

private def invoke (handlers : Handlers Value State Observation)
    (pred : Option Nat) (action : Action) (registers : Registers Value) (state : State) :
    Except Observation (Registers Value × State) :=
  if action.silent then .ok (handlers.pure action.key pred (action.reads.map registers),state)
  else handlers.effect action.key pred (action.reads.map registers) state

private theorem actions_cons (handlers : Handlers Value State Observation)
    (pred : Option Nat) (action : Action) (tail : List Action)
    (registers : Registers Value) (state : State) :
    actions handlers pred (action :: tail) registers state =
      (invoke handlers pred action registers state).bind
        (fun (reply,next) => actions handlers pred tail (update registers action.writes reply) next) := by
  cases silent : action.silent <;>
    simp [actions, invoke, silent, Bind.bind, Except.bind, Pure.pure, Except.pure]

private theorem invoke_update (handlers : Handlers Value State Observation) (pred : Option Nat)
    (action : Action) (registers reply : Registers Value) (writes : List Nat) (state : State)
    (h : disjoint action.reads writes = true) :
    invoke handlers pred action (update registers writes reply) state =
      invoke handlers pred action registers state := by
  simp only [invoke,map_update registers reply action.reads writes h]

private theorem swap_silent_left (handlers : Handlers Value State Observation)
    (pred : Option Nat) (left right : Action) (tail : List Action)
    (registers : Registers Value) (state : State)
    (silent : left.silent = true)
    (readsLeft : disjoint left.reads right.writes = true)
    (readsRight : disjoint right.reads left.writes = true)
    (writes : disjoint left.writes right.writes = true) :
    actions handlers pred (left :: right :: tail) registers state =
      actions handlers pred (right :: left :: tail) registers state := by
  simp only [actions_cons]
  have first (r : Registers Value) (s : State) : invoke handlers pred left r s =
      .ok (handlers.pure left.key pred (left.reads.map r),s) := by simp [invoke,silent]
  rw [first]
  simp only [Except.bind]
  rw [invoke_update handlers pred right _ _ _ _ readsRight]
  cases output : invoke handlers pred right registers state with
  | error failure => rfl
  | ok pair =>
    obtain ⟨reply,next⟩ := pair
    simp only [first,map_update registers reply left.reads right.writes readsLeft]
    rw [updates_commute _ _ _ _ _ writes]

theorem swap (handlers : Handlers Value State Observation)
    (pred : Option Nat) (left right : Action) (tail : List Action)
    (registers : Registers Value) (state : State) (safe : commutes left right = true) :
    actions handlers pred (left :: right :: tail) registers state =
      actions handlers pred (right :: left :: tail) registers state := by
  simp only [commutes,Bool.and_eq_true,Bool.or_eq_true] at safe
  rcases safe with ⟨⟨⟨h,readsLeft⟩,readsRight⟩,writes⟩
  rcases h with leftSilent | rightSilent
  · exact swap_silent_left handlers pred _ _ _ _ _ leftSilent readsLeft readsRight writes
  · exact (swap_silent_left handlers pred _ _ _ _ _ rightSilent readsRight readsLeft
      (disjoint_sym _ _ writes)).symm

def pull (desired : Action) : List Action → Option (List Action)
  | [] => none
  | head :: tail =>
    if head = desired then some tail
    else if commutes head desired then (pull desired tail).map (head :: ·) else none

def checks : List Action → List Action → Bool
  | [], before => before.isEmpty
  | head :: tail, before =>
    match pull head before with
    | none => false
    | some rest => checks tail rest

private theorem cons_actions (handlers : Handlers Value State Observation)
    (pred : Option Nat) (action : Action) (left right : List Action)
    (same : ∀ registers state, actions handlers pred left registers state =
      actions handlers pred right registers state) (registers : Registers Value) (state : State) :
    actions handlers pred (action :: left) registers state =
      actions handlers pred (action :: right) registers state := by
  simp only [actions_cons]
  cases output : invoke handlers pred action registers state with
  | error failure => rfl
  | ok pair =>
    obtain ⟨reply,next⟩ := pair
    exact same _ _

private theorem pull_sound (handlers : Handlers Value State Observation) (pred : Option Nat)
    (desired : Action) (before tail : List Action) (found : pull desired before = some tail)
    (registers : Registers Value) (state : State) :
    actions handlers pred before registers state = actions handlers pred
      (desired :: tail) registers state := by
  induction before generalizing tail registers state with
  | nil => simp [pull] at found
  | cons head rest ih =>
    by_cases same : head = desired
    · have : rest = tail := by simpa [pull,same] using found
      subst head
      subst tail
      rfl
    · by_cases safe : commutes head desired = true
      · cases foundTail : pull desired rest with
        | none => simp [pull,same,safe,foundTail] at found
        | some moved =>
          have : head :: moved = tail := by simpa [pull,same,safe,foundTail] using found
          subst tail
          exact (cons_actions handlers pred head _ _ (ih moved foundTail) registers state).trans
            (swap handlers pred head desired moved registers state safe)
      · simp [pull,same,safe] at found

theorem actions_eq (handlers : Handlers Value State Observation) (pred : Option Nat)
    (after before : List Action) (valid : checks after before = true)
    (registers : Registers Value) (state : State) :
    actions handlers pred before registers state = actions handlers pred after registers state := by
  induction after generalizing before registers state with
  | nil =>
    have : before = [] := by simpa [checks,List.isEmpty_iff] using valid
    subst before
    rfl
  | cons head tail ih =>
    cases found : pull head before with
    | none => simp [checks,found] at valid
    | some rest =>
      have validTail : checks tail rest = true := by simpa [checks,found] using valid
      exact (pull_sound handlers pred head before rest found registers state).trans
        (cons_actions handlers pred head rest tail (ih rest validTail) registers state)

def blockPair (before after : Block) : Bool :=
  decide (before.key = after.key) && decide (before.reads = after.reads) &&
    checks after.actions before.actions

def programPair : List Block → List Block → Bool
  | [], [] => true
  | a :: old, b :: new => blockPair a b && programPair old new
  | _, _ => false

def matchBlocks : Option Block → Option Block → Prop
  | none, none => True
  | some a, some b => blockPair a b = true
  | _, _ => False

theorem blocks_match (before after : List Block) (valid : programPair before after = true) (index : Nat) :
    matchBlocks before[index]? after[index]? := by
  induction before generalizing after index with
  | nil =>
    cases after
    · simp [matchBlocks]
    · simp [programPair] at valid
  | cons old rest ih =>
    cases after with
    | nil => simp [programPair] at valid
    | cons new next =>
      have parts : blockPair old new = true ∧ programPair rest next = true := by
        simpa [programPair, Bool.and_eq_true] using valid
      cases index with
      | zero => simpa [matchBlocks] using parts.1
      | succ index => simpa using ih next parts.2 index

theorem run_eq (handlers : Handlers Value State Observation) (before after : List Block)
    (valid : programPair before after = true) (fuel index : Nat) (pred : Option Nat)
    (registers : Registers Value) (state : State) :
    run handlers before fuel index pred registers state =
      run handlers after fuel index pred registers state := by
  induction fuel generalizing index pred registers state with
  | zero => rfl
  | succ fuel ih =>
    have bodies := blocks_match before after valid index
    cases old : before[index]? with
    | none =>
      cases new : after[index]? with
      | none => simp [run,old,new]
      | some b => simp [old,new,matchBlocks] at bodies
    | some a =>
      cases new : after[index]? with
      | none => simp [old,new,matchBlocks] at bodies
      | some b =>
        have eqs : (a.key = b.key ∧ a.reads = b.reads) ∧ checks b.actions a.actions = true := by
          simpa [old,new,matchBlocks,blockPair,Bool.and_eq_true] using bodies
        simp only [run,old,new]
        rw [actions_eq handlers pred b.actions a.actions eqs.2 registers state, eqs.1.1, eqs.1.2]
        cases reply : actions handlers pred b.actions registers state with
        | error failure => rfl
        | ok pair =>
          obtain ⟨next,shared⟩ := pair
          dsimp only
          cases edge : handlers.transfer b.key pred (b.reads.map next) shared with
          | inl observation => rfl
          | inr target => exact ih target (some index) next shared

end SolarTV.MIR.Schedule
