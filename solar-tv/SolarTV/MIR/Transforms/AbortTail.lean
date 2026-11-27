import SolarTV.MIR.State.Interpreter

/-! A straight-line revert helper can discard its incoming continuation.
Its real typed MIR instructions still write memory and the revert still reads
that memory. The proof is independent of caller stack, return owner, input
words, and initial memory. The contextual caller rewrite retains the same
arguments and helper binding. Administrative work and physical gas are
separate from the helper's execution. -/

namespace SolarTV.MIR.AbortTail
open SolarTV.MIR.State SolarTV.Execution

structure Store where
  offset : Operand
  data : Operand

def Store.instruction (store : Store) : Instruction :=
  ⟨none, .word 0, some 82, .memory .store, [store.offset,store.data]⟩

@[simp] theorem Store.operation (store : Store) :
    store.instruction.operation = .memory .store := rfl

theorem store_activation (cfg : Config) (frame : Frame) (store : Store) (depth : Nat) :
    instruction cfg frame store.instruction depth = instruction cfg frame store.instruction 0 := by
  unfold Store.instruction instruction
  rfl

structure Helper where
  stores : List Store
  offset : Operand
  size : Operand

def Helper.block (helper : Helper) : State.Block :=
  ⟨[],helper.stores.map Store.instruction,.reverted helper.offset helper.size⟩

def Helper.body (helper : Helper) (parameters : List ValueType) (name : String) : Function :=
  ⟨parameters,[],[helper.block],some name⟩

def failure (frame : Frame) (offset size : Operand) : Run := do
  let offset ← read frame offset
  let size ← read frame size
  let memory ← expand frame.memory offset.toNat size.toNat
  return finish (.reverted (readBytes memory offset.toNat size.toNat)) {frame with memory}

def execute (helper : Helper) (cfg : Config) : Nat → Nat → Frame → Run
  | 0, _, _ => .error "MIR evaluation work limit"
  | fuel + 1, pc, frame =>
    match helper.stores[pc]? with
    | none => failure frame helper.offset helper.size
    | some store => do
      match ← instruction cfg frame store.instruction with
      | .returned (_, frame) => execute helper cfg fuel (pc+1) frame
      | .reverted data => return finish (.reverted (failureData data)) frame
      | .invalid => return finish .invalid frame

/-- The same helper execution with any saved continuation and return owner. -/
theorem ignores_continuation (helper : Helper) (cfg : Config) (functions : Functions)
    (id : Nat) (parameters : List ValueType) (name : String)
    (resolved : function functions id = .ok (helper.body parameters name))
    (fuel pc : Nat) (stack : List Continuation) (owner : Option Nat) (frame : Frame) :
    run fuel cfg functions ⟨{function := id, next := pc, entered := true, returnOwner := owner},stack,frame⟩ =
      execute helper cfg fuel pc frame := by
  induction fuel generalizing pc frame with
  | zero => simp [run, execute]
  | succ fuel ih =>
    rw [run_one]
    simp only [transition, resolved, Helper.body, Helper.block, List.getElem?_cons,
      required, Except.bind, Bind.bind]
    by_cases nextStore : pc < helper.stores.length
    · simp [nextStore, execute, store_activation cfg frame helper.stores[pc]]
      cases instruction cfg frame helper.stores[pc].instruction with
      | error reason => simp [Except.bind, Bind.bind]
      | ok outcome =>
        cases outcome with
        | returned result =>
          obtain ⟨word,nextFrame⟩ := result
          simp [State.next, advance, ih, Except.bind, Bind.bind]
        | reverted data =>
          simp [stop, Pure.pure, Except.pure, Except.bind, Bind.bind]
        | invalid => simp [stop, Pure.pure, Except.pure, Except.bind, Bind.bind]
    · simp [nextStore, execute, failure, Except.bind, Bind.bind]
      cases read frame helper.offset with
      | error reason => rfl
      | ok offset =>
        cases read frame helper.size with
        | error reason => rfl
        | ok size =>
          cases memory : expand frame.memory offset.toNat size.toNat <;>
            simp [memory,stop,Pure.pure,Except.pure]

theorem at_entry (helper : Helper) (cfg : Config) (functions : Functions)
    (id : Nat) (parameters : List ValueType) (name : String)
    (resolved : function functions id = .ok (helper.body parameters name))
    (fuel : Nat) (stack : List Continuation) (owner : Option Nat) (frame : Frame) :
    run (fuel+1) cfg functions ⟨{function := id, returnOwner := owner},stack,frame⟩ =
      execute helper cfg fuel 0 frame := by
  have entryStep : transition fuel cfg functions
      ⟨{function := id, returnOwner := owner},stack,frame⟩ =
      .ok (.next 0 ⟨{function := id, entered := true, returnOwner := owner},stack,frame⟩) := by
    cases frame
    simp [transition, resolved, Helper.body, Helper.block, enter,
      required, Except.bind, Bind.bind, Pure.pure, Except.pure]
  rw [run_next fuel entryStep]
  simpa using ignores_continuation helper cfg functions id parameters name resolved fuel 0 stack owner frame

theorem execute_never_returns (helper : Helper) (cfg : Config) (fuel pc : Nat)
    (frame : Frame) (values : List EvmRules.Word) :
    ∀ result, execute helper cfg fuel pc frame = .ok result →
      result.completion ≠ .returned values := by
  induction fuel generalizing pc frame with
  | zero => simp [execute]
  | succ fuel ih =>
    rw [execute]
    cases helper.stores[pc]? with
    | none =>
      unfold failure
      cases read frame helper.offset with
      | error reason => simp [ Except.bind, Bind.bind]
      | ok offset =>
        cases read frame helper.size with
        | error reason => simp [Except.bind,Bind.bind]
        | ok size =>
          cases memory : expand frame.memory offset.toNat size.toNat <;>
            simp [memory,Except.bind,Bind.bind,finish,Pure.pure,Except.pure]
    | some store =>
      cases effect : instruction cfg frame store.instruction with
      | error reason => simp [effect, Except.bind, Bind.bind]
      | ok outcome =>
        cases outcome with
        | returned result =>
          obtain ⟨word,nextFrame⟩ := result
          simpa [effect, Except.bind, Bind.bind] using ih (pc+1) nextFrame
        | reverted data => simp [effect, Except.bind, Bind.bind, finish, Pure.pure, Except.pure]
        | invalid => simp [effect, Except.bind, Bind.bind, finish, Pure.pure, Except.pure]

end SolarTV.MIR.AbortTail
