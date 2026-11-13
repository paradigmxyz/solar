import SolarTV.MIR.State.Messages

/-! A direct SSA abstract machine with explicit caller continuations. Each body
has its own arguments/locals; internal calls share byte memory and the journal.
Returning restores only the caller's SSA context, while STOP, raw RETURN, revert
and exception terminate the whole frame. Tail calls replace the activation.
Phis read their incoming values simultaneously. One global work budget covers
all calls and CFG edges; call depth has a separate model bound. Exhaustion is
unsupported, never program failure or a reason to accept equal executions.
No stack scheduling, backend return protocol or frame initialization is used. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

def finish (completion : Completion) (frame : Frame) : Result :=
  ⟨completion,frame.world,frame.memory,frame.returnData,frame.creationIndex⟩

def function (functions : Functions) (id : Nat) : Except String Function :=
  required ((functions.find? (·.1 == id)).map Prod.snd) "unknown MIR function"

def typedWords (types : List ValueType) (values : List Value) : Except String Unit :=
  if types.length != values.length then .error "MIR call arity mismatch"
  else if (types.zip values).all (fun (ty,value) => value.fits 34 ty) then .ok ()
  else .error "unmodeled or mismatched MIR call type"

def choose (frame : Frame) (key : Word) (fallback : Nat) : List (Operand × Nat) → Except String Nat
  | [] => .ok fallback
  | (operand,target) :: rest => do
    let test ← read frame operand
    if test = key then return target else choose frame key fallback rest

def branch (cursor : Cursor) (target : Nat) : Cursor :=
  {function := cursor.function,block := target,predecessor := some cursor.block}

def advance (cursor : Cursor) : Cursor := {cursor with next := cursor.next + 1}

def start (functions : Functions) (callee : Nat) (args : List Value) (frame : Frame) : Except String Frame := do
  let body ← function functions callee
  typedWords body.params args
  return {frame with arguments := args,locals := []}

def callSignature (callee : Function) (result : Option Nat) (ty : ValueType) : Except String Unit :=
  if callee.returns.length > 1 then .error "physical multi-return protocol is not modeled"
  else if result.isNone || sameTypes callee.returns [ty] then .ok ()
  else .error "MIR call result signature mismatch"


structure Machine where
  cursor : Cursor
  stack : List Continuation
  frame : Frame

inductive Transition where
  | next (extraWork : Nat) (machine : Machine)
  | finished (result : Result)

def next (cursor : Cursor) (stack : List Continuation) (frame : Frame) : Except String Transition :=
  .ok (.next 0 ⟨cursor,stack,frame⟩)

def stop (completion : Completion) (frame : Frame) : Except String Transition :=
  .ok (.finished (finish completion frame))

/-- One MIR transition. Internal calls use continuations; external children spend the shared fuel. -/
def transition (fuel : Nat) (cfg : Config) (functions : Functions) (machine : Machine) : Except String Transition := do
  let ⟨cursor,stack,frame⟩ := machine
  let body ← function functions cursor.function
  let block ← required body.blocks[cursor.block]? "unknown MIR block"
  if !cursor.entered then
    let frame ← enter cursor.predecessor block.phis frame
    return .next block.phis.length ⟨{cursor with entered := true},stack,frame⟩
  else if cursor.next < block.instructions.length then
    let inst ← required block.instructions[cursor.next]? "unknown MIR instruction"
    match inst.operation with
    | .external opcode =>
      if inst.opcode.any (fun opcode => !cfg.available.contains opcode) then stop .invalid frame else
      let args ← inst.operands.mapM (read frame)
      let (effect,work) ← external fuel cfg frame opcode args
      match effect with
      | .returned (value,parent) =>
        match ← assign parent inst.result inst.ty value with
        | .returned (_,parent) => return .next work ⟨advance cursor,stack,parent⟩
        | _ => .error "MIR message result assignment failed"
      | .reverted data => stop (.reverted (failureData data)) frame
      | .invalid => stop .invalid frame
    | .assume fact =>
      let [lhs,rhs] := inst.operands | .error "MIR assume operand count"
      let lhs ← read frame lhs
      let rhs ← read frame rhs
      if fact.holds lhs rhs then next (advance cursor) stack frame else stop .undefined frame
    | .call callee =>
      let args ← inst.operands.mapM (readValue frame)
      let target ← function functions callee
      callSignature target inst.result inst.ty
      if stack.length + 1 ≥ cfg.callDepth then .error "MIR internal call depth limit" else
        let child ← start functions callee args frame
        let caller := ⟨advance cursor,frame,inst.result,inst.ty⟩
        next {function := callee} (caller :: stack) child
    | _ =>
      match ← instruction cfg frame inst with
      | .returned (_,frame) => next (advance cursor) stack frame
      | .reverted data => stop (.reverted (failureData data)) frame
      | .invalid => stop .invalid frame
  else
    match block.terminator with
    | .jump target => next (branch cursor target) stack frame
    | .branch condition yes no =>
      let condition ← read frame condition
      if condition = 1 then next (branch cursor yes) stack frame
      else next (branch cursor no) stack frame
    | .switch value fallback cases =>
      let key ← read frame value
      let target ← choose frame key fallback cases
      next (branch cursor target) stack frame
    | .tailCall callee operands =>
      let args ← operands.mapM (readValue frame)
      let target ← function functions callee
      if !(sameTypes target.returns body.returns) then .error "MIR tail call result mismatch" else
        let child ← start functions callee args frame
        next {function := callee} stack child
    | .returned operands =>
      let values ← operands.mapM (readValue frame)
      typedWords body.returns values
      match stack with
      | [] =>
        let words ← (body.returns.zip values).mapM (fun (ty,value) => value.flatten 34 ty)
        stop (.returned words.flatten) frame
      | caller :: rest =>
        let parent := {caller.frame with
          world := frame.world
          memory := frame.memory
          returnData := frame.returnData
          creationIndex := frame.creationIndex}
        if caller.result.isNone then next caller.cursor rest parent else
          let [value] := values | .error "physical multi-return protocol is not modeled"
          match ← assignValue parent caller.result caller.ty value with
          | .returned (_,parent) => next caller.cursor rest parent
          | _ => .error "MIR call return assignment failed"
    | .stop => stop (.success []) frame
    | .invalid => stop .invalid frame
    | .returndata offset size | .reverted offset size =>
      let offset ← read frame offset
      let size ← read frame size
      let memory ← expand frame.memory offset.toNat size.toNat
      let data := readBytes memory offset.toNat size.toNat
      let reverted := match block.terminator with | .reverted .. => true | _ => false
      stop (if reverted then .reverted data else .success data) {frame with memory}

def run (fuel : Nat) (cfg : Config) (functions : Functions) (machine : Machine) : Run := do
  match fuel with
  | 0 => .error "MIR evaluation work limit"
  | fuel + 1 =>
    match ← transition fuel cfg functions machine with
    | .finished result => return result
    | .next extra machine =>
      if extra > fuel then .error "MIR evaluation work limit"
      else run (fuel - extra) cfg functions machine
termination_by fuel
decreasing_by all_goals omega

theorem run_one (fuel : Nat) (cfg : Config) (functions : Functions) (machine : Machine) :
    run (fuel + 1) cfg functions machine = (do
      match ← transition fuel cfg functions machine with
      | .finished result => return result
      | .next extra nextMachine =>
        if extra > fuel then .error "MIR evaluation work limit"
        else run (fuel - extra) cfg functions nextMachine) := by
  rw [run]

theorem run_next (fuel : Nat) {cfg functions machine extra nextMachine}
    (h : transition fuel cfg functions machine = .ok (.next extra nextMachine)) :
    run (fuel + 1) cfg functions machine =
      if extra > fuel then .error "MIR evaluation work limit"
      else run (fuel - extra) cfg functions nextMachine := by
  simp only [run, h, Except.bind, Bind.bind]

theorem run_finished (fuel : Nat) {cfg functions machine result}
    (h : transition fuel cfg functions machine = .ok (.finished result)) :
    run (fuel + 1) cfg functions machine = .ok result := by
  simp only [run, h, Except.bind, Bind.bind, Pure.pure, Except.pure]

theorem run_error (fuel : Nat) {cfg functions machine reason}
    (h : transition fuel cfg functions machine = .error reason) :
    run (fuel + 1) cfg functions machine = .error reason := by
  simp only [run, h, Except.bind, Bind.bind]

def execute (fuel : Nat) (cfg : Config) (functions : Functions) (entry : Nat) (frame : Frame) : Run := do
  if cfg.callDepth = 0 then .error "MIR internal call depth limit" else
  let initial ← start functions entry frame.arguments frame
  let result ← run fuel cfg functions ⟨{function := entry},[],initial⟩
  match result.completion with
  | .reverted _ | .invalid | .undefined => return {result with world := frame.world}
  | .returned values =>
    if cfg.entry then
      if values.isEmpty then return {result with completion := .success [],world := EVM.State.finalize result.world}
      else .error "MIR entry returned word values"
    else return result
  | .success _ => return {result with world := EVM.State.finalize result.world}

end SolarTV.MIR.State
