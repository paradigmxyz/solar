import SolarTV.MIR.State.Interpreter

/-! A runtime with no selector, receive, or fallback entry rejects every
message with empty returndata. Prove that the materialized dispatch implements
that implicit behavior in the ordinary MIR interpreter. The work bound counts
block entries and terminators; physical gas and ABI wrapper dispatch are outside
this certificate. -/

namespace SolarTV.MIR.LowerDispatch
open EvmRules SolarTV.Execution State

def implicitRuntime (routes : List Nat) (frame : Frame) : Run :=
  if routes.isEmpty then .ok (finish (.reverted []) frame)
  else .error "runtime has implicit dispatch targets"

/-- The text printer places the dispatcher before the ordinary functions.
Reparsing that order shifts the IDs of the functions after its position. -/
def insertedId (entry id : Nat) : Nat := if id < entry then id else id + 1

def insertDispatcher (functions : Functions) (entry : Nat) (dispatcher : Function) : Functions :=
  let rename := insertedId entry
  let retained := functions.map fun (id, body) =>
    (rename id, {body with blocks := body.blocks.map fun block => {block with
      instructions := block.instructions.map fun inst =>
        match inst.operation with
        | .call callee => {inst with operation := .call (rename callee)}
        | _ => inst
      terminator := match block.terminator with
        | .tailCall callee args => .tailCall (rename callee) args
        | term => term}})
  retained.take entry ++ [(entry, dispatcher)] ++ retained.drop entry

private def zero : Operand := ⟨.constant ⟨256, 0⟩, .word 256⟩

/-- Each reachable block must be free of instructions and phis. The finite
path certificate binds its block IDs to the actual resolved function. -/
inductive RejectPath (body : Function) : Nat → Nat → Prop where
  | revert (block : Nat) (code : body.blocks[block]? = some ⟨[], [], .reverted zero zero⟩) :
      RejectPath body block 2
  | jump (block target steps : Nat) (code : body.blocks[block]? = some ⟨[], [], .jump target⟩)
      (rest : RejectPath body target steps) : RejectPath body block (steps + 2)

private theorem expand_empty (memory : List Byte) (bounded : memory.length ≤ 65536) :
    expand memory 0 0 = .ok memory := by
  have h : 32 * memoryWords memory ≤ memory.length := by
    simpa [memoryWords, Nat.mul_comm] using Nat.div_mul_le_self memory.length 32
  simp [expand, requiredWords, Nat.not_lt.mpr (Nat.le_trans h bounded), Nat.sub_eq_zero_of_le h]

private theorem run_enter (cfg : Config) (functions : Functions) (entry block fuel : Nat)
    (frame : Frame) (pred : Option Nat) {body code}
    (lookup : function functions entry = .ok body)
    (found : body.blocks[block]? = some ⟨[], [], code⟩) :
    run (fuel + 1) cfg functions ⟨{function := entry, block, predecessor := pred}, [], frame⟩ =
      run fuel cfg functions
        ⟨{function := entry, block, predecessor := pred, entered := true}, [], frame⟩ := by
  rw [run_next fuel
    (nextMachine := ⟨{function := entry, block, predecessor := pred, entered := true}, [], frame⟩)
    (extra := 0) (by simp [transition, lookup, required, found, enter, Except.bind, Bind.bind,
      Pure.pure, Except.pure])]
  simp

private theorem run_path {body : Function} {block steps : Nat} (path : RejectPath body block steps)
    (cfg : Config) (functions : Functions) (entry fuel : Nat) (frame : Frame)
    (lookup : function functions entry = .ok body) (bounded : frame.memory.length ≤ 65536)
    (pred : Option Nat) :
    run (fuel + steps) cfg functions ⟨{function := entry, block, predecessor := pred}, [], frame⟩ =
      .ok (finish (.reverted []) frame) := by
  induction path generalizing pred with
  | revert block code =>
    rw [show fuel + 2 = (fuel + 1) + 1 by omega]
    rw [run_enter cfg functions entry block (fuel + 1) frame pred lookup code]
    exact run_finished fuel (by
      have h : read frame zero = .ok 0 := rfl
      simp [transition, lookup, required, code, h, expand_empty _ bounded, readBytes,
        stop, Except.bind, Bind.bind])
  | jump block target steps code rest ih =>
    rw [show fuel + (steps + 2) = (fuel + steps + 1) + 1 by omega]
    rw [run_enter cfg functions entry block (fuel + steps + 1) frame pred lookup code]
    rw [run_next (fuel + steps)
      (nextMachine := ⟨{function := entry, block := target, predecessor := some block}, [], frame⟩)
      (extra := 0) (by simp [transition, lookup, required, code, next, State.branch, Except.bind, Bind.bind])]
    simpa using ih (some block)

/-- For every runtime context, the dispatcher reverts and leaves memory and
the journal untouched. Additional work fuel does not affect the outcome. -/
theorem certificate (routes : List Nat) (noRoutes : routes = [])
    (functions : Functions) (entry : Nat) (body : Function)
    (lookup : function functions entry = .ok body) (paramless : body.params = [])
    (steps : Nat) (path : RejectPath body 0 steps)
    (cfg : Config) (fuel : Nat) (frame : Frame)
    (depth : cfg.callDepth ≠ 0) (args : frame.arguments = [])
    (bounded : frame.memory.length ≤ 65536) :
    implicitRuntime routes frame = execute (fuel + steps) cfg functions entry frame := by
  have startEntry : start functions entry frame.arguments frame =
      .ok {frame with arguments := [], locals := []} := by
    simp [start, lookup, paramless, args, typedWords, Except.bind, Bind.bind, Pure.pure, Except.pure]
  have runEntry := run_path path cfg functions entry fuel
    {frame with arguments := [], locals := []} lookup bounded none
  simp [implicitRuntime, noRoutes, execute, depth, startEntry, runEntry, conclude, finish,
    Except.bind, Bind.bind, Pure.pure, Except.pure]

end SolarTV.MIR.LowerDispatch
