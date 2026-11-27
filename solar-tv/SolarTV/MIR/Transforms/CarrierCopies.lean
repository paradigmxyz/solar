/-! Eliminate full-word SSA copies in an unpriced action graph. The importer
identifies full-width carrier copies and encodes all other operations as ordered
requests. A kernel-checked invariant lists the copies whose captured values are
still current at each block. Overwriting a word invalidates copies of that word,
including across a loop. Phi reads occur together before their writes.
This simulation neither interprets the requests nor bills physical gas. -/

namespace SolarTV.MIR.CarrierCopies

inductive Input where
  | reg (id : Nat)
  | fixed (id : Nat)
  deriving DecidableEq, BEq, ReflBEq, LawfulBEq

abbrev Registers (Value : Type) := Nat → Value
abbrev Copies := List (Nat × Input)

def Input.read (fixed regs : Registers Value) : Input → Value
  | .reg id => regs id
  | .fixed id => fixed id

def base (copies : Copies) (input : Input) : Input :=
  match input with
  | .reg id => ((copies.find? (·.1 == id)).map Prod.snd).getD input
  | .fixed _ => input

def activeRead (copies : Copies) (active : List Nat) : Input → Bool
  | .reg id => base copies (.reg id) == .reg id || active.contains id
  | .fixed _ => true

structure Action where
  key : Nat
  reads : List Input
  writes : List Nat
  deriving DecidableEq

inductive Instruction where
  | action (effect : Action)
  | copy (result : Nat) (source : Input)
  deriving DecidableEq

structure Block where
  phis : List (Nat × Action)
  code : List Instruction
  exit : Action
  targets : List Nat
  deriving DecidableEq

def rewrite (copies : Copies) (a : Action) : Action :=
  {a with reads := a.reads.map (base copies)}

def retained (copies : Copies) : List Instruction → List Instruction :=
  List.filterMap fun
    | .action a => some (.action (rewrite copies a))
    | .copy .. => none

def erase (copies : Copies) (block : Block) : Block :=
  ⟨block.phis.map (fun (p,a) => (p,rewrite copies a)), retained copies block.code,
    rewrite copies block.exit, block.targets⟩

def withoutWrites (copies : Copies) (active writes : List Nat) : List Nat :=
  active.filter fun id =>
    match base copies (.reg id) with
    | .reg source => !writes.contains source
    | .fixed _ => true

def safeAction (copies : Copies) (active : List Nat) (a : Action) : Bool :=
  a.reads.all (activeRead copies active) &&
    a.writes.all (fun id => base copies (.reg id) == .reg id)

def step (copies : Copies) (active : List Nat) : Instruction → Option (List Nat)
  | .action a => if safeAction copies active a then some (withoutWrites copies active a.writes)
                 else none
  | .copy id source =>
    if base copies (.reg id) == base copies source &&
        base copies (.reg id) != .reg id && activeRead copies active source
    then some (id :: active) else none

def checkCode (copies : Copies) (code : List Instruction) (active : List Nat) : Option (List Nat) :=
  code.foldlM (step copies) active

def agrees (copies : Copies) (active : List Nat) (fixed a b : Registers Value) : Prop :=
  ∀ id, let input := base copies (.reg id)
        (input = .reg id ∨ id ∈ active) → a id = input.read fixed b

theorem reads_agree {copies active} {fixed a b : Registers Value}
    (h : agrees copies active fixed a b) (input : Input)
    (valid : activeRead copies active input = true) :
    input.read fixed a = (base copies input).read fixed b := by
  cases input with
  | fixed id => rfl
  | reg id =>
    apply h id
    simpa [activeRead] using valid

def update (old : Registers Value) (writes : List Nat) (reply : Registers Value) : Registers Value :=
  fun id => if id ∈ writes then reply id else old id

theorem update_agrees {copies active} {fixed a b : Registers Value}
    (h : agrees copies active fixed a b) (inst : Action)
    (valid : safeAction copies active inst = true) (reply : Registers Value) :
    agrees copies (withoutWrites copies active inst.writes) fixed
      (update a inst.writes reply) (update b inst.writes reply) := by
  intro id
  dsimp only
  intro good
  by_cases written : id ∈ inst.writes
  · have same : base copies (.reg id) = .reg id := by
      simp only [safeAction, Bool.and_eq_true_iff] at valid
      simpa using (List.all_eq_true.mp valid.2) id written
    simp [same, Input.read, update, written]
  · cases good with
    | inl same =>
      simpa [same, Input.read, update, written] using h id (.inl same)
    | inr still =>
      have kept := List.mem_filter.mp still
      cases hb : base copies (.reg id) with
      | fixed i =>
        have equal := h id (.inr kept.1)
        simpa [hb, Input.read, update, written] using equal
      | reg i =>
        have live : i ∉ inst.writes := by simpa [withoutWrites, hb] using kept.2
        have equal := h id (.inr kept.1)
        simpa [hb, Input.read, update, written, live] using equal

theorem copy_agrees {copies active} {fixed a b : Registers Value} (id : Nat) (source : Input)
    (h : agrees copies active fixed a b)
    (alias : base copies (.reg id) = base copies source)
    (valid : activeRead copies active source = true) :
    agrees copies (id :: active) fixed (update a [id] (fun _ => source.read fixed a)) b := by
  intro reg
  dsimp only
  intro good
  by_cases same : reg = id
  · subst reg
    simpa [update, alias] using reads_agree h source valid
  · have preserved : base copies (.reg reg) = .reg reg ∨ reg ∈ active := by
      simpa [same] using good
    simpa [update, same] using h reg preserved

structure Handlers (Value State Terminal : Type) where
  action : Nat → List Value → List Nat → State → Except Terminal (Registers Value × State)
  transfer : Nat → List Value → State → Terminal ⊕ (Nat × State)

def perform (handlers : Handlers Value State Terminal) (fixed : Registers Value)
    (a : Action) (regs : Registers Value) (state : State) :
    Except Terminal (Registers Value × State) := do
  let (reply,state) ← handlers.action a.key (a.reads.map (Input.read fixed regs)) a.writes state
  return (update regs a.writes reply,state)

def execute (handlers : Handlers Value State Terminal) (fixed : Registers Value) :
    List Instruction → Registers Value → State → Except Terminal (Registers Value × State)
  | [], regs, state => .ok (regs,state)
  | .action a :: code, regs, state => do
    let (regs,state) ← perform handlers fixed a regs state
    execute handlers fixed code regs state
  | .copy id source :: code, regs, state =>
    execute handlers fixed code (update regs [id] (fun _ => source.read fixed regs)) state

def related (copies : Copies) (active : List Nat) (fixed : Registers Value) :
    Except Terminal (Registers Value × State) → Except Terminal (Registers Value × State) → Prop
  | .error a, .error b => a = b
  | .ok (a,s), .ok (b,t) => agrees copies active fixed a b ∧ s = t
  | _, _ => False

theorem perform_sound (handlers : Handlers Value State Terminal) {copies active}
    {fixed a b : Registers Value} (state : State) (inst : Action)
    (known : agrees copies active fixed a b) (valid : safeAction copies active inst = true) :
    related copies (withoutWrites copies active inst.writes) fixed
      (perform handlers fixed inst a state) (perform handlers fixed (rewrite copies inst) b state) := by
  have reads : inst.reads.all (activeRead copies active) = true := by
    simp only [safeAction, Bool.and_eq_true_iff] at valid
    exact valid.1
  have equal : inst.reads.map (Input.read fixed a) =
      (rewrite copies inst).reads.map (Input.read fixed b) := by
    simp only [rewrite, List.map_map]
    exact List.map_congr_left fun x hx => reads_agree known x ((List.all_eq_true.mp reads) x hx)
  unfold perform
  simp only [rewrite] at equal ⊢
  rw [equal]
  cases handlers.action inst.key ((inst.reads.map (base copies)).map (Input.read fixed b))
      inst.writes state with
  | error _ => rfl
  | ok pair =>
    obtain ⟨reply, next⟩ := pair
    exact ⟨update_agrees known inst valid reply,rfl⟩

theorem execute_sound (handlers : Handlers Value State Terminal) {copies active next code}
    {fixed a b : Registers Value} (state : State) (checked : checkCode copies code active = some next)
    (known : agrees copies active fixed a b) :
    related copies next fixed
      (execute handlers fixed code a state)
      (execute handlers fixed (retained copies code) b state) := by
  induction code generalizing active a b state with
  | nil =>
    simp [checkCode] at checked
    subst next
    exact ⟨known,rfl⟩
  | cons inst rest ih =>
    cases inst with
    | copy id source =>
      simp only [checkCode, List.foldlM_cons, step] at checked
      split at checked
      · rename_i valid
        simp only [Bool.and_eq_true_iff, beq_iff_eq, bne_iff_ne] at valid
        exact ih state checked (copy_agrees id source known valid.1.1 valid.2)
      · contradiction
    | action inst =>
      simp only [checkCode, List.foldlM_cons, step] at checked
      split at checked
      · rename_i valid
        have proof := perform_sound handlers state inst known valid
        change related copies next fixed
          (do let (regs,state) ← perform handlers fixed inst a state
              execute handlers fixed rest regs state)
          (do let (regs,state) ← perform handlers fixed (rewrite copies inst) b state
              execute handlers fixed (retained copies rest) regs state)
        cases lhs : perform handlers fixed inst a state with
        | error failure =>
          cases rhs : perform handlers fixed (rewrite copies inst) b state with
          | error other =>
            have same : failure = other := by simpa [lhs,rhs,related] using proof
            subst other
            rfl
          | ok pair => simp [lhs,rhs,related] at proof
        | ok pair =>
          obtain ⟨left,leftState⟩ := pair
          cases rhs : perform handlers fixed (rewrite copies inst) b state with
          | error failure => simp [lhs,rhs,related] at proof
          | ok pair =>
            obtain ⟨right,rightState⟩ := pair
            have equal : agrees copies (withoutWrites copies active inst.writes) fixed left right ∧ leftState = rightState := by
              simpa [lhs,rhs,related] using proof
            obtain ⟨agree,equal⟩ := equal
            subst rightState
            exact ih leftState checked agree
      · contradiction

def enter (block : Block) (fromBlock : Option Nat) : Option (List Instruction) :=
  if block.phis.isEmpty then some block.code else do
  let pred ← fromBlock
  let (_,phis) ← block.phis.find? (·.1 == pred)
  return .action phis :: block.code

theorem enter_erase (copies : Copies) (block : Block) (fromBlock : Option Nat) :
    enter (erase copies block) fromBlock = (enter block fromBlock).map (retained copies) := by
  have lookup (pred : Nat) (phis : List (Nat × Action)) :
      (phis.map (fun (p,a) => (p,rewrite copies a))).find? (·.1 == pred) =
        (phis.find? (·.1 == pred)).map (fun (p,a) => (p,rewrite copies a)) := by
    induction phis with
    | nil => rfl
    | cons head rest ih =>
      cases head
      simp only [List.map_cons, List.find?_cons]
      split <;> simp_all
  simp only [enter, erase, List.isEmpty_map]
  split
  · rfl
  · cases fromBlock with
    | none => rfl
    | some pred =>
      simp only [Option.bind, Bind.bind, lookup]
      cases block.phis.find? (·.1 == pred)
      · rfl
      · rfl

def checkPath (copies : Copies) (live : List (List Nat)) (block : Block)
    (position : Nat) (code : List Instruction) : Bool :=
  match checkCode copies code (live[position]?.getD []) with
  | some active =>
    safeAction copies active block.exit &&
    block.targets.all (fun next => next < live.length &&
      (live[next]?.getD []).all active.contains)
  | none => false

def checkBlock (copies : Copies) (live : List (List Nat)) (block : Block) (position : Nat) : Bool :=
  if block.phis.isEmpty then checkPath copies live block position block.code
  else block.phis.all fun (_,phis) => checkPath copies live block position (.action phis :: block.code)

def check (copies : Copies) (live : List (List Nat)) (blocks : List Block) : Bool :=
  live.length == blocks.length && (live.head?.getD []).isEmpty &&
    blocks.zipIdx.all fun (block, position) => checkBlock copies live block position

inductive Result (State Terminal : Type) where
  | terminal (observation : Terminal)
  | exhausted (state : State)
  | missingBlock (state : State)
  | invalidEdge (state : State)

def run (handlers : Handlers Value State Terminal) (fixed : Registers Value) (blocks : List Block) :
    Nat → Nat → Option Nat → Registers Value → State → Result State Terminal
  | 0, _, _, _, state => .exhausted state
  | fuel + 1, index, fromBlock, regs, state =>
    match blocks[index]? with
    | none => .missingBlock state
    | some block =>
      match enter block fromBlock with
      | none => .invalidEdge state
      | some code =>
        match execute handlers fixed code regs state with
        | .error failure => .terminal failure
        | .ok (regs,state) =>
          match handlers.transfer block.exit.key (block.exit.reads.map (Input.read fixed regs)) state with
          | .inl observation => .terminal observation
          | .inr (dest,state) =>
            if block.targets.contains dest then run handlers fixed blocks fuel dest (some index) regs state
            else .invalidEdge state

private theorem checked_path {copies live block position fromBlock code}
    (checked : checkBlock copies live block position = true)
    (entered : enter block fromBlock = some code) :
    checkPath copies live block position code = true := by
  unfold checkBlock at checked
  unfold enter at entered
  split at checked <;> rename_i empty
  · simp [empty] at entered
    subst code
    exact checked
  · simp only [empty] at entered
    cases fromBlock with
    | none => contradiction
    | some pred =>
      simp only [Option.bind, Bind.bind] at entered
      cases found : block.phis.find? (·.1 == pred) with
      | none => simp [found] at entered
      | some row =>
        obtain ⟨index,phis⟩ := row
        have member := List.mem_of_find?_eq_some found
        simp [found, Pure.pure] at entered
        subst code
        exact (List.all_eq_true.mp checked) (index,phis) member

/-- Equal complete traces through any number of logical blocks. The initial
active set is empty. At every later entry the checked invariant is re-established
on the actual edge, rather than assumed for a loop or a phi operand. -/
theorem simulation (handlers : Handlers Value State Terminal) (fixed : Registers Value)
    (copies : Copies) (live : List (List Nat)) (blocks : List Block)
    (checked : check copies live blocks = true) (fuel index : Nat) (fromBlock : Option Nat)
    (a b : Registers Value) (state : State)
    (known : agrees copies (live[index]?.getD []) fixed a b) :
    run handlers fixed blocks fuel index fromBlock a state =
      run handlers fixed (blocks.map (erase copies)) fuel index fromBlock b state := by
  have checks : blocks.zipIdx.all (fun (block,pos) => checkBlock copies live block pos) = true := by
    simpa only [check, Bool.and_eq_true_iff, beq_iff_eq] using (Bool.and_eq_true_iff.mp checked).2
  induction fuel generalizing index fromBlock a b state with
  | zero => rfl
  | succ fuel ih =>
    simp only [run, List.getElem?_map]
    cases found : blocks[index]? with
    | none => rfl
    | some block =>
      simp only [Option.map_some, enter_erase]
      cases entered : enter block fromBlock with
      | none => rfl
      | some code =>
        simp only [Option.map_some]
        have existsBlock : (blocks.zipIdx)[index]? = some (block,index) := by
          simp [List.getElem?_zipIdx, found]
        have legal := (List.all_eq_true.mp checks) (block,index) (List.mem_of_getElem? existsBlock)
        have path := checked_path legal entered
        unfold checkPath at path
        cases flow : checkCode copies code (live[index]?.getD []) with
        | none => simp [flow] at path
        | some active =>
          simp only [flow, Bool.and_eq_true_iff] at path
          have actions := execute_sound handlers state flow known
          cases lhs : execute handlers fixed code a state with
          | error failure =>
            cases rhs : execute handlers fixed (retained copies code) b state with
            | error other =>
              have equal : failure = other := by simpa [lhs,rhs,related] using actions
              simp [equal]
            | ok pair => simp [lhs,rhs,related] at actions
          | ok pair =>
            obtain ⟨left,leftState⟩ := pair
            cases rhs : execute handlers fixed (retained copies code) b state with
            | error failure => simp [lhs,rhs,related] at actions
            | ok pair =>
              obtain ⟨right,rightState⟩ := pair
              have agree : agrees copies active fixed left right ∧ leftState = rightState := by
                simpa [lhs,rhs,related] using actions
              obtain ⟨agree,rfl⟩ := agree
              have reads := (Bool.and_eq_true_iff.mp path.1).1
              have args : block.exit.reads.map (Input.read fixed left) =
                  (block.exit.reads.map (base copies)).map (Input.read fixed right) := by
                simp only [List.map_map]
                exact List.map_congr_left fun x hx => reads_agree agree x ((List.all_eq_true.mp reads) x hx)
              simp only [erase, rewrite]
              rw [args]
              cases handlers.transfer block.exit.key ((block.exit.reads.map (base copies)).map (Input.read fixed right)) leftState with
              | inl observation => rfl
              | inr dest =>
                obtain ⟨dest,nextState⟩ := dest
                by_cases edge : dest ∈ block.targets
                · simp only [List.contains_iff_mem.mpr edge, ite_true]
                  apply ih
                  intro id
                  dsimp only
                  intro good
                  apply agree id
                  cases good with
                  | inl same => exact .inl same
                  | inr present =>
                    have successor := (List.all_eq_true.mp path.2) dest edge
                    have subset := (Bool.and_eq_true_iff.mp successor).2
                    exact .inr (List.contains_iff_mem.mp ((List.all_eq_true.mp subset) id present))
                · simp [edge]

theorem from_entry (handlers : Handlers Value State Terminal) (fixed : Registers Value)
    (copies : Copies) (live : List (List Nat)) (before after : List Block)
    (checked : check copies live before = true) (matched : before.map (erase copies) = after)
    (fuel : Nat) (regs : Registers Value) (state : State) :
    run handlers fixed before fuel 0 none regs state = run handlers fixed after fuel 0 none regs state := by
  have empty : (live[0]?.getD []).isEmpty = true := by
    have h := (Bool.and_eq_true_iff.mp (Bool.and_eq_true_iff.mp checked).1).2
    cases live <;> simp_all
  rw [← matched]
  apply simulation handlers fixed copies live before checked
  intro id
  dsimp only
  intro whereLive
  cases whereLive with
  | inl identity => simp [identity, Input.read]
  | inr member => simp [List.isEmpty_iff.mp empty] at member

/-- Each side may retain different full-word copies. Compare their checked
erasures without assuming that an unused copy's source stays current. -/
theorem paired_entry (handlers : Handlers Value State Terminal) (fixed : Registers Value)
    (left right : List Block) (leftCopies rightCopies : Copies) (leftLive rightLive : List (List Nat))
    (leftChecked : check leftCopies leftLive left = true)
    (rightChecked : check rightCopies rightLive right = true)
    (matched : left.map (erase leftCopies) = right.map (erase rightCopies))
    (fuel : Nat) (regs : Registers Value) (state : State) :
    run handlers fixed left fuel 0 none regs state =
      run handlers fixed right fuel 0 none regs state := by
  calc
    _ = run handlers fixed (left.map (erase leftCopies)) fuel 0 none regs state :=
      from_entry handlers fixed leftCopies leftLive left _ leftChecked rfl fuel regs state
    _ = run handlers fixed (right.map (erase rightCopies)) fuel 0 none regs state :=
      congrArg (fun blocks => run handlers fixed blocks fuel 0 none regs state) matched
    _ = _ :=
      (from_entry handlers fixed rightCopies rightLive right _ rightChecked rfl fuel regs state).symm

-- A redefinition invalidates the old word captured by a copy.
example :
    checkCode [(3,.reg 1)]
      [.copy 3 (.reg 1), .action ⟨0,[],[1]⟩, .action ⟨1,[.reg 3],[]⟩] [] = none := by
  decide

end SolarTV.MIR.CarrierCopies
