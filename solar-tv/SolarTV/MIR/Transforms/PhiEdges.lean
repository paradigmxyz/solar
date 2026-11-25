import SolarTV.MIR.State.Interpreter

/-! Split a logical CFG edge with an empty jump block. All occurrences of the
same edge move together, including switch defaults and repeated case targets.
The forwarding block leaves the frame unchanged. Rekeying the successor's
phis preserves the simultaneous assignments for every frame at the boundary.
The certificate is local to unpriced MIR; it counts the two hops as one
logical edge and makes no assertion about work limits or physical gas. -/

namespace SolarTV.MIR.PhiEdges
open State

def replace (old fresh candidate : Nat) : Nat :=
  if candidate == old then fresh else candidate

def redirect (old fresh : Nat) : Terminator → Terminator
  | .jump next => .jump (replace old fresh next)
  | .branch condition yes no => .branch condition (replace old fresh yes) (replace old fresh no)
  | .switch key fallback cases =>
    .switch key (replace old fresh fallback) (cases.map fun (value, dest) =>
      (value, replace old fresh dest))
  | term => term

def rekey (old fresh : Nat) (phi : Phi) : Phi :=
  { phi with incoming := phi.incoming.map fun (label, operand) =>
      (replace old fresh label, operand) }

def split (body : Function) (edge : Nat × Nat) : Function :=
  let (pred, succ) := edge
  let fresh := body.blocks.length
  let blocks := body.blocks.zipIdx |>.map fun (block, index) =>
    { block with
      terminator := if index == pred then redirect succ fresh block.terminator else block.terminator
      phis := if index == succ then block.phis.map (rekey pred fresh) else block.phis }
  { body with blocks := blocks ++ [⟨[], [], .jump succ⟩] }

def destinations : Terminator → List Nat
  | .jump dest => [dest]
  | .branch _ yes no => [yes, no]
  | .switch _ fallback cases => fallback :: cases.map Prod.snd
  | _ => []

/-- Each split must start at a real edge and supply a fresh predecessor label. -/
def admissible (body : Function) (edge : Nat × Nat) : Bool :=
  match body.blocks[edge.1]?, body.blocks[edge.2]? with
  | some pred, some succ =>
    (destinations pred.terminator).contains edge.2 &&
      succ.phis.all (fun phi => phi.incoming.all (fun arm => arm.1 != body.blocks.length))
  | _, _ => false

private theorem selected (incoming : List (Nat × Operand)) (old fresh pred : Nat)
    (unused : incoming.all (fun arm => arm.1 != fresh) = true) (valid : pred ≠ fresh) :
    (incoming.map (fun (label, operand) => (replace old fresh label, operand))).filter
      (fun arm => arm.1 == replace old fresh pred) =
    (incoming.filter (fun arm => arm.1 == pred)).map
      (fun (_, operand) => (replace old fresh pred, operand)) := by
  induction incoming with
  | nil => rfl
  | cons arm rest ih =>
    obtain ⟨label, operand⟩ := arm
    have tail : rest.all (fun arm => arm.1 != fresh) = true := by
      exact (Bool.and_eq_true_iff.mp (by simpa only [List.all_cons] using unused)).2
    have notFresh : label ≠ fresh := by
      have h := (Bool.and_eq_true_iff.mp (by simpa only [List.all_cons] using unused)).1
      simpa using h
    have same : (replace old fresh label == replace old fresh pred) = (label == pred) := by
      by_cases labelOld : label = old <;> by_cases predOld : pred = old
      · simp [replace, labelOld, predOld]
      · subst label
        simp [replace, predOld, beq_false_of_ne (Ne.symm valid), beq_false_of_ne (Ne.symm predOld)]
      · subst pred
        simp [replace, beq_false_of_ne notFresh, beq_false_of_ne labelOld]
      · simp [replace, labelOld, predOld]
    simp only [List.map_cons, List.filter_cons]
    rw [same, ih tail]
    split
    · rename_i equal
      have equal : label = pred := by simpa using equal
      subst label
      rfl
    · rfl

private def binding (predecessor : Option Nat) (frame : Frame) (phi : Phi) :
    Except String (Nat × Value) := do
  let some predecessor := predecessor | .error "phi at MIR entry"
  let [(_, operand)] := phi.incoming.filter (·.1 == predecessor) |
    .error "ambiguous or missing MIR phi edge"
  let value ← readValue frame operand
  if !(phi.ty.same operand.ty) then .error "MIR phi type mismatch" else return (phi.result, value)

private theorem rekey_binding (old fresh pred : Nat) (frame : Frame) (phi : Phi)
    (unused : phi.incoming.all (fun arm => arm.1 != fresh) = true) (valid : pred ≠ fresh) :
    binding (some (replace old fresh pred)) frame (rekey old fresh phi) =
      binding (some pred) frame phi := by
  simp only [binding, rekey]
  rw [selected _ _ _ _ unused valid]
  cases phi.incoming.filter (fun x => x.1 == pred) with
  | nil => rfl
  | cons head rest =>
    cases head
    cases rest <;> rfl

theorem enter_rekey (old fresh pred : Nat) (frame : Frame) (phis : List Phi)
    (unused : phis.all (fun phi => phi.incoming.all (fun arm => arm.1 != fresh)) = true)
    (valid : pred ≠ fresh) :
    enter (some (replace old fresh pred)) (phis.map (rekey old fresh)) frame =
      enter (some pred) phis frame := by
  have bindings :
    (phis.map (rekey old fresh)).mapM (binding (some (replace old fresh pred)) frame) =
      phis.mapM (binding (some pred) frame) := by
    induction phis with
    | nil => rfl
    | cons phi rest ih =>
      simp only [List.all_cons, Bool.and_eq_true_iff] at unused
      simp only [List.map_cons, List.mapM_cons, rekey_binding old fresh pred frame phi unused.1 valid, ih unused.2]
  change (do
    let values ← (phis.map (rekey old fresh)).mapM (binding (some (replace old fresh pred)) frame)
    pure {frame with locals := values.foldl (fun locals (id,value) => put locals id value) frame.locals}) = _
  rw [bindings]
  rfl

/-- An empty forwarding block followed by the rekeyed phi group gives the
original edge action, including failed reads. It uses the original live frame
rather than reading values already written by a sibling phi. -/
theorem forward (old fresh : Nat) (frame : Frame) (phis : List Phi)
    (unused : phis.all (fun phi => phi.incoming.all (fun arm => arm.1 != fresh)) = true)
    (valid : old ≠ fresh) :
    (do
      let frame ← enter (some old) [] frame
      enter (some fresh) (phis.map (rekey old fresh)) frame) =
      enter (some old) phis frame := by
  have same : enter (some old) [] frame = .ok frame := by cases frame; rfl
  rw [same]
  simpa [replace, Except.bind, Bind.bind] using enter_rekey old fresh old frame phis unused valid

theorem choose_redirect (frame : Frame) (key : EvmRules.Word) (fallback old fresh : Nat)
    (cases : List (Operand × Nat)) :
    State.choose frame key (replace old fresh fallback)
      (cases.map fun (value, dest) => (value, replace old fresh dest)) =
    (State.choose frame key fallback cases).map (replace old fresh) := by
  induction cases with
  | nil => rfl
  | cons head rest ih =>
    obtain ⟨operand, target⟩ := head
    unfold List.map State.choose
    cases read frame operand with
    | error reason => rfl
    | ok value =>
      by_cases same : value = key <;>
        simp [same, ih, Except.bind, Bind.bind, Except.map, Pure.pure, Except.pure]

/-- Resolve control using the same typed operands and ordered switch tests as
the direct interpreter. A terminal or call terminator has no local successor. -/
def destination (frame : Frame) : Terminator → Except String (Option Nat)
  | .jump target => .ok (some target)
  | .branch condition yes no => do
    let value ← readBits 1 frame condition
    return some (if value = 1 then yes else no)
  | .switch value fallback cases => do
    let key ← read frame value
    return some (← State.choose frame key fallback cases)
  | _ => .ok none

theorem redirect_destination (frame : Frame) (term : Terminator) (old fresh : Nat) :
    destination frame (redirect old fresh term) =
      (destination frame term).map (Option.map (replace old fresh)) := by
  cases term <;> try rfl
  case branch condition yes no =>
    simp only [destination, redirect]
    cases readBits 1 frame condition with
    | error _ => rfl
    | ok condition => by_cases test : condition = 1#1 <;>
      simp [test, Except.bind, Bind.bind, Except.map, Pure.pure, Except.pure]
  case switch value fallback cases =>
    simp only [destination, redirect]
    cases read frame value with
    | error _ => rfl
    | ok key =>
      simp only [Except.bind, Bind.bind, choose_redirect]
      cases State.choose frame key fallback cases <;> rfl

end SolarTV.MIR.PhiEdges
