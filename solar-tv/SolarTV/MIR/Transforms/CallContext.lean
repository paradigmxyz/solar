import SolarTV.MIR.State.Interpreter

/-! Relate two actual MIR function tables outside a set of replacement bodies.
The machine runs unchanged code with its regular semantics. A cut identifies
a replacement obligation and preserves the entire machine and remaining work
budget, including caller continuations, memory, returndata and the journal. -/

namespace SolarTV.MIR.CallContext
open MIR.State

/-- Only the body can change at a cut. A call can still check arguments,
result types and return-buffer placement before it reaches that cut. -/
def signature (body : Function) : Function := {body with blocks := []}

inductive Agreement (changed : List Nat) : Functions → Functions → Prop where
  | nil : Agreement changed [] []
  | same (id : Nat) (body : Function) {left right} :
      Agreement changed left right →
      Agreement changed ((id,body)::left) ((id,body)::right)
  | replaced (id : Nat) (old new : Function) {left right} :
      id ∈ changed → signature old = signature new → Agreement changed left right →
      Agreement changed ((id,old)::left) ((id,new)::right)

theorem Agreement.signature_eq {changed left right} (pair : Agreement changed left right)
    (id : Nat) :
    (function left id).map signature = (function right id).map signature := by
  induction pair with
  | nil => rfl
  | same key body rest ih =>
    by_cases key == id <;> simp_all [function]
  | replaced key old new allowed same rest ih =>
    by_cases key == id <;> simp_all [function, required, Except.map]

theorem Agreement.body_eq {changed left right} (pair : Agreement changed left right)
    (id : Nat) (common : id ∉ changed) : function left id = function right id := by
  induction pair with
  | nil => rfl
  | same key body rest ih =>
    by_cases key == id <;> simp_all [function]
  | replaced key old new allowed same rest ih =>
    have different : key ≠ id := by
      intro equal
      exact common (equal ▸ allowed)
    simp_all [function]

private theorem signature_bind {changed left right} (pair : Agreement changed left right)
    (id : Nat) {α : Type} (continuation : Function → Except String α) :
    (function left id >>= fun f => continuation (signature f)) =
      (function right id >>= fun f => continuation (signature f)) := by
  have eq := pair.signature_eq id
  cases h : function left id <;> cases j : function right id <;>
    simp_all [Except.map, Except.bind, Bind.bind]

theorem Agreement.start_eq {changed left right} (pair : Agreement changed left right)
    (callee : Nat) (args : List Value) (frame : Frame) :
    start left callee args frame = start right callee args frame :=
  signature_bind (α := Frame) pair callee fun sig => do
    typedWords sig.params args
    return {frame with arguments := args, locals := []}

theorem Agreement.step_eq {changed left right} (pair : Agreement changed left right)
    (fuel : Nat) (cfg : Config) (machine : Machine)
    (common : machine.cursor.function ∉ changed) :
    transition fuel cfg left machine = transition fuel cfg right machine := by
  rcases machine with ⟨cursor, stack, frame⟩
  change cursor.function ∉ changed at common
  simp only [transition]
  rw [pair.body_eq _ common]
  cases function right cursor.function with
  | error _ => rfl
  | ok body =>
    simp only [Bind.bind, Except.bind]
    cases required body.blocks[cursor.block]? "unknown MIR block" with
    | error _ => rfl
    | ok block =>
      simp only []
      by_cases entered : (!cursor.entered) = true
      · simp only [entered, ite_true]
      simp only [entered, Bool.false_eq_true, ite_false]
      by_cases instruction : cursor.next < block.instructions.length
      · simp only [instruction, ite_true]
        cases required block.instructions[cursor.next]? "unknown MIR instruction" with
        | error _ => rfl
        | ok inst =>
          simp only []
          cases inst.operation <;> try rfl
          case call callee =>
            simp only []
            cases inst.operands.mapM (readValue frame) with
            | error _ => rfl
            | ok args =>
              simp only [pair.start_eq callee args frame]
              exact signature_bind (α := Transition) pair callee fun sig => do
                callSignature sig inst.result inst.ty
                if stack.length + 1 ≥ cfg.callDepth then .error "MIR internal call depth limit" else
                  let child ← start right callee args frame
                  next {function := callee} (⟨advance cursor,frame,inst.result,inst.ty⟩ :: stack) child
      · simp only [instruction, ite_false]
        cases block.terminator <;> try rfl
        case tailCall callee operands =>
          simp only []
          cases operands.mapM (readValue frame) with
          | error _ => rfl
          | ok args =>
            simp only [pair.start_eq callee args frame]
        case returned operands =>
          simp only []
          cases operands.mapM (readValue frame) with
          | error _ => rfl
          | ok values =>
            simp only []
            cases typedWords body.returns values with
            | error _ => rfl
            | ok _ =>
              simp only []
              cases cursor.returnOwner with
              | none => rfl
              | some id =>
                exact signature_bind (α := Transition) pair id fun sig => do
                  let sig ← if !(sameTypes sig.returns body.returns) then
                      .error "MIR tail call result mismatch"
                    else pure sig
                  match stack with
                  | [] =>
                    let words ← (body.returns.zip values).mapM (fun (ty,value) => value.flatten 34 ty)
                    stop (.returned words.flatten) frame
                  | caller :: rest =>
                    let frame ← publishReturns cfg sig values frame
                    let parent := {caller.frame with
                      world := frame.world, memory := frame.memory, returnData := frame.returnData,
                      creationIndex := frame.creationIndex, builtinIndex := frame.builtinIndex,
                      messageGas := frame.messageGas}
                    if caller.result.isNone then next caller.cursor rest parent else
                      let value ← required values.head? "call without a result"
                      match ← assignValue parent caller.result caller.ty value with
                      | .returned (_,parent) => next caller.cursor rest parent
                      | _ => .error "MIR call return assignment failed"

abbrev Cut := Result ⊕ (Nat × Machine)

/-- Stop before running a replacement, or return the original machine outcome.
Unsupported execution produces an error, including exhaustion of model fuel. -/
def runTo (changed : List Nat) (fuel : Nat) (cfg : Config) (functions : Functions)
    (machine : Machine) : Except String Cut := do
  if machine.cursor.function ∈ changed then return .inr (fuel,machine)
  else
    match fuel with
    | 0 => .error "MIR evaluation work limit"
    | fuel+1 =>
      match ← transition fuel cfg functions machine with
      | .finished result => return .inl result
      | .next extra after =>
        if extra > fuel then .error "MIR evaluation work limit"
        else runTo changed (fuel-extra) cfg functions after
termination_by fuel
decreasing_by all_goals omega

theorem Agreement.prefix_eq {changed left right} (pair : Agreement changed left right)
    (fuel : Nat) (cfg : Config) (state : Machine) :
    runTo changed fuel cfg left state = runTo changed fuel cfg right state := by
  induction fuel using Nat.strongRecOn generalizing state with
  | ind fuel ih =>
    by_cases cut : state.cursor.function ∈ changed
    · cases fuel <;> simp [runTo, cut]
    cases fuel with
    | zero => simp [runTo, cut]
    | succ fuel =>
      rw [runTo, runTo]
      simp only [cut, ite_false, pair.step_eq fuel cfg state cut, Bind.bind, Except.bind]
      cases transition fuel cfg right state with
      | error _ => rfl
      | ok progress =>
        cases progress with
        | finished _ => rfl
        | next extra after => simp only [ih (fuel-extra) (by omega) after]

/-- A certified prefix can be factored out of the ordinary evaluator. The
resumption receives the original fuel, with the actual prefix work deducted. -/
theorem run_split (changed : List Nat) (fuel : Nat) (cfg : Config) (functions : Functions)
    (state : Machine) :
    run fuel cfg functions state = (do
      match ← runTo changed fuel cfg functions state with
      | .inl result => return result
      | .inr (left,reached) => run left cfg functions reached) := by
  induction fuel using Nat.strongRecOn generalizing state with
  | ind fuel ih =>
    by_cases cut : state.cursor.function ∈ changed
    · cases fuel <;> simp [runTo, cut, Pure.pure, Except.pure, Bind.bind, Except.bind]
    cases fuel with
    | zero => simp [run, runTo, cut, Bind.bind, Except.bind]
    | succ fuel =>
      rw [run_one, runTo]
      simp only [cut, ite_false, Bind.bind, Except.bind]
      cases transition fuel cfg functions state with
      | error _ => rfl
      | ok progress =>
        cases progress with
        | finished _ => rfl
        | next extra after =>
          by_cases within : extra > fuel
          · simp [within]
          · simpa only [within, ite_false, Bind.bind, Except.bind] using
              ih (fuel-extra) (by omega) after

/-- Reuse the same prefix proof for any caller. The obligation concerns only
reached replacements. A terminal prefix needs no replacement proof. -/
theorem Agreement.compose {changed left right} (pair : Agreement changed left right)
    (fuel : Nat) (cfg : Config) (state : Machine)
    (replace : ∀ remaining reached,
      runTo changed fuel cfg left state = .ok (.inr (remaining,reached)) →
      run remaining cfg left reached = run remaining cfg right reached) :
    run fuel cfg left state = run fuel cfg right state := by
  rw [run_split changed fuel cfg left, run_split changed fuel cfg right,
    ← pair.prefix_eq fuel cfg state]
  cases h : runTo changed fuel cfg left state with
  | error _ => rfl
  | ok value =>
    cases value with
    | inl _ => rfl
    | inr next =>
      obtain ⟨remaining,reached⟩ := next
      exact replace remaining reached h

/-- A checked local replacement can resume with a different amount of work
fuel. Each transition must pay its own work, including any child execution. -/
inductive Segment (cfg : Config) (functions : Functions) :
    Nat → Machine → Nat → Machine → Prop where
  | here (fuel : Nat) (machine : Machine) : Segment cfg functions fuel machine fuel machine
  | more {fuel extra remaining : Nat} {initial next reached : Machine} :
      transition fuel cfg functions initial = .ok (.next extra next) →
      extra ≤ fuel →
      Segment cfg functions (fuel-extra) next remaining reached →
      Segment cfg functions (fuel+1) initial remaining reached

theorem Segment.resume {cfg functions fuel initial remaining reached}
    (checked : Segment cfg functions fuel initial remaining reached) :
    run fuel cfg functions initial = run remaining cfg functions reached := by
  induction checked with
  | here => rfl
  | more h paid _ ih =>
    simpa only [run_next _ h, Nat.not_lt_of_ge paid, ite_false] using ih

theorem Segment.append {cfg functions fuel initial remaining reached finalFuel final}
    (first : Segment cfg functions fuel initial remaining reached)
    (last : Segment cfg functions remaining reached finalFuel final) :
    Segment cfg functions fuel initial finalFuel final := by
  induction first with
  | here => exact last
  | more step paid _ ih => exact .more step paid (ih last)

/-- Bind a replacement to the real interpreter on both sides. The caller may
use another context proof for the reached continuations at their own budgets. -/
theorem join_segments {cfg left right leftFuel rightFuel leftStart rightStart
    leftRest rightRest leftReached rightReached}
    (old : Segment cfg left leftFuel leftStart leftRest leftReached)
    (new : Segment cfg right rightFuel rightStart rightRest rightReached)
    (continuations : run leftRest cfg left leftReached = run rightRest cfg right rightReached) :
    run leftFuel cfg left leftStart = run rightFuel cfg right rightStart :=
  old.resume.trans (continuations.trans new.resume.symm)

end SolarTV.MIR.CallContext
