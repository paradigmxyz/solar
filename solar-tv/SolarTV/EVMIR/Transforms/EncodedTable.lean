import SolarTV.EVMIR.Transforms.IndexedJump
import SolarTV.EVMIR.Transforms.RelocatedReturn

/-! Verify an indexed transfer against its emitted byte image. Decode the
table code from an instruction boundary, follow only valid JUMPDEST targets,
and stop before executing the selected logical successor. Each in-range
selector is checked, with a common arbitrary live stack underneath it.
The table's physical gas is charged, including intermediate stubs. -/

namespace SolarTV.EVM.EncodedTable
open EvmRules

private def ctx : Context := ⟨0,0,0,[],0⟩

private def tableOp : Op → Bool
  | .add | .mul | .and | .iszero | .byte | .shl | .shr | .not | .push _ => true
  | .dup i | .swap i => i.val == 0
  | _ => false

private theorem step_append (op : Op) (active next tail : Stack)
    (allowed : tableOp op = true) (found : step ctx op active = some next) :
    step ctx op (active ++ tail) = some (next ++ tail) := by
  cases op <;> simp [tableOp] at allowed
  case dup i =>
    subst i
    cases active <;> simp_all [step] <;> subst next <;> rfl
  case swap i =>
    subst i
    cases active with
    | nil => simp [step] at found
    | cons head rest => cases rest <;> simp_all [step, List.set] <;> subst next <;> rfl
  all_goals
    cases active with
    | nil => simp_all [step] <;> subst next <;> rfl
    | cons word rest => cases rest <;> simp_all [step] <;> subst next <;> rfl

inductive Action
  | word (op : Op)
  | marker
  | jump

private def action (inst : RelocatedReturn.ByteOp) : Option Action := do
  if let some value := inst.immediate then return .word (.push value)
  else if inst.opcode = 0x5b then return .marker
  else if inst.opcode = 0x56 then return .jump
  else if inst.opcode = 0x80 then return .word (.dup 0)
  else if inst.opcode = 0x90 then return .word (.swap 0)
  else
    let op ← State.pureOp inst.opcode
    if tableOp op then some (.word op) else none

private theorem action_allowed (inst : RelocatedReturn.ByteOp) (op : Op)
    (decoded : action inst = some (.word op)) : tableOp op = true := by
  unfold action at decoded
  split at decoded
  · simp at decoded
    subst op
    rfl
  · split at decoded
    · simp at decoded
    · split at decoded
      · simp at decoded
      · split at decoded
        · simp at decoded
          subst op
          rfl
        · split at decoded
          · simp at decoded
            subst op
            rfl
          · cases h : State.pureOp inst.opcode with
            | none => simp [h, Bind.bind, Option.bind] at decoded
            | some instruction =>
              by_cases legal : tableOp instruction = true <;>
                simp [h, Bind.bind, Option.bind, legal] at decoded
              subst op
              exact legal

private def boundary (code : State.CodeChunks) (target : Nat) : Nat → Nat → Bool
  | 0, _ => false
  | fuel+1, pc =>
    if pc = target then pc < code.size else if pc > target then false else
    match RelocatedReturn.decode code pc with
    | none => false
    | some inst => boundary code target fuel inst.next

/-- The current image decoder handles PUSH payloads. On forks where the
extended stack opcodes have immediates, reject those layouts until the
fork-specific decoder can distinguish their payloads from JUMPDESTs. -/
private def legacyBoundaries (code : State.CodeChunks) : Nat → Nat → Bool
  | 0, pc => pc == code.size
  | fuel+1, pc =>
    if pc == code.size then true else
    match RelocatedReturn.decode code pc with
    | none => false
    | some inst =>
      if [0xe6,0xe7,0xe8].contains inst.opcode then false
      else legacyBoundaries code fuel inst.next

private def decodedBoundaries (cfg : State.Config) (code : State.CodeChunks) : Bool :=
  !(cfg.fork.atLeast .amsterdam) || legacyBoundaries code code.size 0

structure Summary where
  pc : Nat
  stack : Stack
  peak : Nat
  gas : Nat
  deriving DecidableEq

private def Summary.prepend (length price : Nat) (s : Summary) : Summary :=
  {s with peak := max length s.peak, gas := price + s.gas}

private def Summary.frame (tail : Stack) (s : Summary) : Summary :=
  {s with stack := s.stack ++ tail, peak := s.peak + tail.length}

private theorem frame_prepend (tail : Stack) (s : Summary) (length price : Nat) :
    (s.prepend length price).frame tail =
      (s.frame tail).prepend (length + tail.length) price := by
  simp only [Summary.frame, Summary.prepend, Summary.mk.injEq, true_and, and_true]
  omega

private def route (cfg : State.Config) (code : State.CodeChunks) :
    Nat → Nat → Nat → Stack → Option Summary
  | _, 0, pc, stack => some ⟨pc,stack,stack.length,0⟩
  | 0, _+1, _, _ => none
  | fuel+1, jumps+1, pc, stack => do
    let inst ← RelocatedReturn.decode code pc
    let price := State.available cfg inst.opcode
    if !price.available then none else
    let extra := match ← action inst with
      | .word op => do route cfg code fuel (jumps+1) inst.next (← step ctx op stack)
      | .marker => route cfg code fuel (jumps+1) inst.next stack
      | .jump => do
        let target ← stack.head?
        if (code.jumps 0).contains target.toNat then
          route cfg code fuel jumps target.toNat stack.tail
        else none
    extra.map (Summary.prepend stack.length price.base)

private theorem route_frame (cfg : State.Config) (code : State.CodeChunks)
    (fuel jumps pc : Nat) (active tail : Stack) (summary : Summary)
    (checked : route cfg code fuel jumps pc active = some summary) :
    route cfg code fuel jumps pc (active ++ tail) = some (summary.frame tail) := by
  induction fuel generalizing jumps pc active summary with
  | zero =>
    cases jumps with
    | zero =>
      simp [route] at checked
      subst summary
      simp [route, Summary.frame]
    | succ => contradiction
  | succ fuel ih =>
    cases jumps with
    | zero =>
      simp [route] at checked
      subst summary
      simp [route, Summary.frame]
    | succ jumps =>
      simp only [route, Bind.bind, Option.bind] at checked ⊢
      cases codeEq : RelocatedReturn.decode code pc with
      | none => simp [codeEq] at checked
      | some inst =>
        simp only [codeEq] at checked ⊢
        by_cases available : (State.available cfg inst.opcode).available = true
        · simp only [available, Bool.not_true, Bool.false_eq_true, ite_false] at checked ⊢
          cases semantics : action inst with
            | none => simp [semantics] at checked
            | some operation =>
              have lift (remaining dest : Nat) (work : Stack) :
                  (route cfg code fuel remaining dest work).map
                      (Summary.prepend active.length (State.available cfg inst.opcode).base) =
                    some summary →
                  (route cfg code fuel remaining dest (work ++ tail)).map
                      (Summary.prepend (active ++ tail).length (State.available cfg inst.opcode).base) =
                    some (summary.frame tail) := by
                intro success
                cases runEq : route cfg code fuel remaining dest work with
                | none => simp [runEq] at success
                | some result =>
                  simp [runEq] at success
                  subst summary
                  rw [ih _ _ _ _ runEq, Option.map_some, frame_prepend, List.length_append]
              cases operation with
              | marker =>
                simpa [semantics, Option.map, Bind.bind, Option.bind, Option.pure_def] using
                  lift (jumps+1) inst.next active
                    (by simpa [semantics, Option.map, Bind.bind, Option.bind, Option.pure_def] using checked)
              | word op =>
                cases opEq : step ctx op active with
                | none => simp [semantics, opEq] at checked
                | some next =>
                  have stepped := step_append op active next tail (action_allowed inst op semantics) opEq
                  simpa [semantics, stepped, Option.map, Bind.bind, Option.bind, Option.pure_def] using
                    lift (jumps+1) inst.next next
                      (by simpa [semantics, opEq, Option.map, Bind.bind, Option.bind,
                        Option.pure_def] using checked)
              | jump =>
                cases active with
                | nil => simp [semantics] at checked
                | cons word rest =>
                  by_cases dest : word.toNat ∈ code.jumps 0
                  · simpa [semantics, dest, Option.map, Bind.bind, Option.bind, Option.pure_def] using
                      lift jumps word.toNat rest
                        (by simpa [semantics, dest, Option.map, Bind.bind, Option.bind,
                          Option.pure_def] using checked)
                  · simp [semantics, dest] at checked
        · simp [available] at checked

/-- Propose the numerical limits by evaluating each selector on its local
stack. The certificate checks the proposal against the independent target map. -/
def limits (cfg : State.Config) (code : State.CodeChunks) (entry fuel jumps count : Nat) :
    List (Nat × Nat) :=
  (List.range count).map fun i =>
    match route cfg code fuel jumps entry [BitVec.ofNat 256 i] with
    | none => (0,0)
    | some summary => (summary.peak,summary.gas)

/-- Reduce an effect-free table route to its transfer, peak height and exact
static charge. Failure of either limit prevents transfer to the successor. -/
def execute (cfg : State.Config) (code : State.CodeChunks) (entry fuel jumps : Nat)
    (stack : Stack) (gas : Nat) : Option (Nat × Stack × Nat) := do
  if !boundary code entry (entry+1) 0 then none else
  let summary ← route cfg code fuel jumps entry stack
  if summary.peak ≤ 1024 && summary.gas ≤ gas then
    some (summary.pc, summary.stack, gas - summary.gas)
  else none

def check (cfg : State.Config) (code : State.CodeChunks) (entry fuel jumps : Nat)
    (targets : List Nat) (rows : List (Nat × Nat)) : Bool :=
  decodedBoundaries cfg code && !targets.isEmpty &&
    targets.length == rows.length && 0 < jumps &&
    boundary code entry (entry+1) 0 && (List.range targets.length).all fun i =>
    match targets[i]?, rows[i]? with
    | some target, some (peak,gas) =>
      route cfg code fuel jumps entry [BitVec.ofNat 256 i] == some ⟨target,[],peak,gas⟩
    | _, _ => false

/-- Cover all selector words with an in-range logical edge, not just one
execution or a chosen constant. The caller supplies the logical destinations
in table order. Locally the selector is the only consumed stack word. -/
private theorem checked_route (cfg : State.Config) (code : State.CodeChunks) (entry fuel jumps : Nat)
    (targets : List Nat) (rows : List (Nat × Nat))
    (checked : check cfg code entry fuel jumps targets rows = true)
    (selector : Word) (pc peak charge : Nat)
    (logical : IndexedJump.target targets selector = some pc)
    (limits : rows[selector.toNat]? = some (peak,charge)) :
    boundary code entry (entry+1) 0 = true ∧
      route cfg code fuel jumps entry [selector] = some ⟨pc,[],peak,charge⟩ := by
  simp only [check, Bool.and_eq_true, decide_eq_true_eq] at checked
  obtain ⟨⟨⟨⟨⟨_,_⟩,_⟩,_⟩,start⟩,each⟩ := checked
  have selected := (List.getElem?_eq_some_iff.mp logical).1
  have row := List.all_eq_true.mp each selector.toNat (List.mem_range.mpr selected)
  unfold IndexedJump.target at logical
  exact ⟨start, by simpa [logical, limits, beq_iff_eq] using row⟩

theorem correspondence (cfg : State.Config) (code : State.CodeChunks) (entry fuel jumps : Nat)
    (targets : List Nat) (rows : List (Nat × Nat))
    (checked : check cfg code entry fuel jumps targets rows = true)
    (selector : Word) (tail : Stack) (gas pc peak charge : Nat)
    (logical : IndexedJump.target targets selector = some pc)
    (limits : rows[selector.toNat]? = some (peak,charge)) :
    execute cfg code entry fuel jumps (selector :: tail) gas =
      if peak + tail.length ≤ 1024 && charge ≤ gas then some (pc,tail,gas-charge) else none := by
  obtain ⟨start, computed⟩ := checked_route cfg code entry fuel jumps targets rows checked
    selector pc peak charge logical limits
  have frame := route_frame cfg code fuel jumps entry [selector] tail _ computed
  have run : route cfg code fuel jumps entry (selector :: tail) =
      some (Summary.frame tail ⟨pc,[],peak,charge⟩) := frame
  simp [execute, start, run, Summary.frame, Bind.bind, Option.bind]

/-- Bind a range-producing word mask immediately before the table. The
provider is part of the same byte image and must end at the table's entry. -/
def maskPrefix (cfg : State.Config) (code : State.CodeChunks) (start entry : Nat) :
    Option (Word × Nat) := do
  if !boundary code start (start+1) 0 then none else
  let push ← RelocatedReturn.decode code start
  let mask ← push.immediate
  let andOp ← RelocatedReturn.decode code push.next
  let lhs := State.available cfg push.opcode
  let rhs := State.available cfg andOp.opcode
  if andOp.opcode == 0x16 && andOp.immediate.isNone && andOp.next == entry &&
      lhs.available && rhs.available then some (mask,lhs.base + rhs.base)
  else none

private theorem masked_route (cfg : State.Config) (code : State.CodeChunks) (start entry fuel jumps : Nat)
    (mask input : Word) (price : Nat) (summary : Summary)
    (provider : maskPrefix cfg code start entry = some (mask,price))
    (next : route cfg code fuel (jumps+1) entry [Evm.and mask input] = some summary) :
    boundary code start (start+1) 0 = true ∧
      route cfg code (fuel+2) (jumps+1) start [input] =
        some (summary.prepend 2 price) := by
  cases first : boundary code start (start+1) 0 <;>
    simp only [maskPrefix, first, Bool.not_false, Bool.not_true, ite_true,
      Bind.bind, Option.bind] at provider
  case false => contradiction
  cases head : RelocatedReturn.decode code start with
  | none => simp [head] at provider
  | some push =>
    cases value : push.immediate with
    | none => simp [head, value] at provider
    | some loaded =>
      cases following : RelocatedReturn.decode code push.next with
      | none => simp [head, value, following] at provider
      | some andOp =>
        simp only [head, value, following] at provider
        by_cases guards : (andOp.opcode == 0x16 && andOp.immediate.isNone &&
            andOp.next == entry && (State.available cfg push.opcode).available &&
            (State.available cfg andOp.opcode).available) = true
        · simp only [guards, ite_true] at provider
          simp only [Bool.and_eq_true, beq_iff_eq] at guards
          obtain ⟨⟨⟨⟨opcode, imm⟩, destination⟩, pushAvailable⟩, andAvailable⟩ := guards
          obtain ⟨rfl, rfl⟩ := Option.some.inj provider
          have noImmediate := Option.isNone_iff_eq_none.mp imm
          rw [opcode] at andAvailable
          simp [route, head, value, following, action, opcode, noImmediate, pushAvailable,
            andAvailable, State.pureOp, tableOp, step, destination, next, Bind.bind,
            Option.bind, Summary.prepend, Nat.add_assoc]
          omega
        · simp [guards] at provider

/-- A producer mask below the table length discharges the selector range
for every word, including dirty high bits. The proof charges both producer
instructions and retains the stack peak needed by the PUSH. -/
theorem masked_correspondence (cfg : State.Config) (code : State.CodeChunks)
    (start entry fuel jumps : Nat) (targets : List Nat) (rows : List (Nat × Nat))
    (checked : check cfg code entry fuel (jumps+1) targets rows = true)
    (input mask : Word) (tail : Stack) (gas price : Nat)
    (provider : maskPrefix cfg code start entry = some (mask,price))
    (bounded : mask.toNat < targets.length) :
    ∃ pc peak charge, IndexedJump.target targets (Evm.and mask input) = some pc ∧
      rows[(Evm.and mask input).toNat]? = some (peak,charge) ∧
      execute cfg code start (fuel+2) (jumps+1) (input :: tail) gas =
        if max 2 peak + tail.length ≤ 1024 && price + charge ≤ gas
        then some (pc,tail,gas-(price+charge)) else none := by
  have index : (Evm.and mask input).toNat < targets.length := by
    exact Nat.lt_of_le_of_lt (by simpa [Evm.and] using (Nat.and_le_left :
      mask.toNat &&& input.toNat ≤ mask.toNat)) bounded
  have rowCount : targets.length = rows.length := by
    simp only [check, Bool.and_eq_true, beq_iff_eq] at checked
    exact checked.1.1.1.2
  have rowIndex : (Evm.and mask input).toNat < rows.length := rowCount ▸ index
  let bounds := rows[(Evm.and mask input).toNat]'rowIndex
  let peak := bounds.1
  let charge := bounds.2
  let pc := targets[(Evm.and mask input).toNat]'index
  have logical : IndexedJump.target targets (Evm.and mask input) = some pc := by
    simp [IndexedJump.target, index, pc]
  have limits : rows[(Evm.and mask input).toNat]? = some (peak,charge) := by
    simp [rowIndex, peak, charge, bounds]
  refine ⟨pc,peak,charge,logical,limits,?_⟩
  obtain ⟨_, table⟩ := checked_route cfg code entry fuel (jumps+1) targets rows checked _ _ _ _
    logical limits
  obtain ⟨boundary, run⟩ := masked_route cfg code start entry fuel jumps mask input price _ provider table
  have frame := route_frame cfg code (fuel+2) (jumps+1) start [input] tail _ run
  have full : route cfg code (fuel+2) (jumps+1) start (input :: tail) =
      some ((Summary.prepend 2 price ⟨pc,[],peak,charge⟩).frame tail) := frame
  simp [execute, boundary, full, Summary.prepend, Summary.frame, Bind.bind, Option.bind]

def bindTargets (positions : List (Nat × Nat)) (labels : List Nat) : Option (List Nat) :=
  labels.mapM (fun label => positions.lookup label)

/-- A table preserves the order of logical edges, including repeated labels.
Resolve them against label definitions measured during byte emission. -/
theorem bound_target (positions : List (Nat × Nat)) (labels targets : List Nat)
    (bound : bindTargets positions labels = some targets) (index label : Nat)
    (selected : labels[index]? = some label) :
    targets[index]? = positions.lookup label := by
  induction labels generalizing index targets with
  | nil => simp at selected
  | cons head rest ih =>
    cases pos : positions.lookup head with
    | none => simp [bindTargets, pos, Bind.bind, Option.bind] at bound
    | some pc =>
      cases tail : bindTargets positions rest with
      | none => simp [bindTargets, pos, show rest.mapM (fun label => positions.lookup label) = none from tail,
          Bind.bind, Option.bind] at bound
      | some actual =>
        have same : pc :: actual = targets := by
          simpa [bindTargets, pos, show rest.mapM (fun label => positions.lookup label) = some actual from tail,
            Bind.bind, Option.bind, Option.pure_def] using bound
        subst targets
        cases index with
        | zero =>
          simp at selected
          subst label
          exact pos.symm
        | succ index => exact ih actual tail index selected

def validDestinations (code : State.CodeChunks) (targets : List Nat) : Bool :=
  targets.all (fun pc => pc < code.size && (code.jumps 0).contains pc)

/-- Resolved targets are instruction boundaries in the actual image, including
when arbitrary opaque data follows the executable prefix. -/
theorem destination_in_image (code : State.CodeChunks) (targets : List Nat)
    (checked : validDestinations code targets = true) (data : List Execution.Byte)
    (target : Word) (member : target.toNat ∈ targets) :
    State.validJump (code.bytes ++ data) target = true := by
  have valid := List.all_eq_true.mp checked target.toNat member
  rw [Bool.and_eq_true] at valid
  rw [← code.jump_eq_with_data data target (of_decide_eq_true valid.1)]
  exact valid.2

/-- Discover a differing route only after checking the same decoder boundary
obligations as a positive proof. The runner extracts a specific witness and
checks it with a separate kernel theorem. -/
def mismatch (cfg : State.Config) (code : State.CodeChunks) (entry fuel jumps : Nat)
    (targets : List Nat) : Option (Nat × Nat × Nat × Nat) :=
  if !decodedBoundaries cfg code || !boundary code entry (entry+1) 0 || jumps == 0
  then none
  else (targets.zipIdx).findSome? fun (expected,index) => do
    let actual ← route cfg code fuel jumps entry [BitVec.ofNat 256 index]
    if actual.pc != expected && actual.stack.isEmpty && actual.peak ≤ 1024 then
      some (index,expected,actual.pc,actual.gas)
    else none

end SolarTV.EVM.EncodedTable
