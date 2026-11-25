import SolarTV.EVMIR.Memory
import SolarTV.Correspondence

/-! Acyclic scheduled regions, expanded from the original CFG by the importer.
Instructions use the existing stack/memory semantics. Transfers retain the
temporary stack growth of their concrete lowering, then execute the successor
with the same live state. Terminal outcomes discard private stack and memory.
Shared tails may appear more than once in the tree; expansion is bounded and
cycles are rejected rather than cut into arbitrary outgoing edges. Gas-priced
regions, dynamic targets and layout-observing operations are not admitted.

Reference: `TerminatorKind::lowering_stack_growth` and EVMYulLean's stack bound.
The importer preserves the original next block, including jump fallthroughs. -/

namespace SolarTV.EVM.Region
open EvmRules

inductive Code where
  | terminal (terminal : Memory.Terminal)
  | instructions (ops : List Memory.Op) (next : Code)
  | jump (growth : Nat) (next : Code)
  | branch (yes no : Code)

def instructions (ctx : Context) : List Memory.Op → Stack → SolarTV.Memory.State →
    Except Failure (Stack × SolarTV.Memory.State)
  | [], stack, memory => .ok (stack,memory)
  | op :: ops, stack, memory => do
    let (stack,memory) ← Memory.step ctx op stack memory
    instructions ctx ops stack memory

def execute (ctx : Context) : Code → Stack → SolarTV.Memory.State → Memory.Execution
  | .terminal terminal, stack, memory => Memory.finish terminal stack memory
  | .instructions ops next, stack, memory =>
    match instructions ctx ops stack memory with
    | .error reason => .failed reason
    | .ok (stack,memory) => execute ctx next stack memory
  | .jump growth next, stack, memory =>
    if stack.length + growth > 1024 then .failed .overflow
    else execute ctx next stack memory
  | .branch yes no, stack, memory =>
    if stack.length + 1 > 1024 then .failed .overflow
    else match stack with
      | condition :: rest => if condition = 0 then execute ctx no rest memory else execute ctx yes rest memory
      | _ => .failed .underflow

/-- Moving a stack swap across an unconditional transfer preserves both branches'
live state when the transfer has enough temporary stack space. -/
theorem swap_across_jump (ctx : Context) (next : Code) (a b : Word) (memory : SolarTV.Memory.State) :
    execute ctx (.instructions [.stack (.swap 0)] (.jump 1 next)) [a,b] memory =
      execute ctx (.jump 1 (.instructions [.stack (.swap 0)] next)) [a,b] memory := rfl

abbrev State := Stack × SolarTV.Memory.State

def effect (ctx : Context) (op : Memory.Op) (state : State) : Except Memory.Execution State :=
  (Memory.step ctx op state.1 state.2).mapError Memory.Execution.failed

def jumpEffect (growth : Nat) (state : State) : Except Memory.Execution State :=
  if state.1.length + growth > 1024 then .error (.failed .overflow) else .ok state

def conditionEffect (state : State) : Except Memory.Execution (Bool × State) :=
  if state.1.length + 1 > 1024 then .error (.failed .overflow)
  else match state.1 with
    | condition :: rest => .ok (decide (condition = 0),(rest,state.2))
    | _ => .error (.failed .underflow)

def operationsTree (ctx : Context) (ops : List Memory.Op)
    (next : Correspondence.Tree State Memory.Execution) : Correspondence.Tree State Memory.Execution :=
  ops.foldr (fun op next => .step (effect ctx op) next) next

def toTree (ctx : Context) : Code → Correspondence.Tree State Memory.Execution
  | .terminal terminal => .terminal (fun state => Memory.finish terminal state.1 state.2)
  | .instructions ops next => operationsTree ctx ops (toTree ctx next)
  | .jump growth next => .step (jumpEffect growth) (toTree ctx next)
  | .branch yes no => .branch conditionEffect (toTree ctx no) (toTree ctx yes)

theorem operationsTree_correct (ctx : Context) (ops : List Memory.Op)
    (next : Correspondence.Tree State Memory.Execution) (state : State) :
    Correspondence.evalTree (operationsTree ctx ops next) state =
      match instructions ctx ops state.1 state.2 with
      | .error reason => .failed reason
      | .ok state => Correspondence.evalTree next state := by
  induction ops generalizing state with
  | nil => rfl
  | cons op ops ih =>
    obtain ⟨stack,memory⟩ := state
    simp only [operationsTree, List.foldr_cons, Correspondence.evalTree, effect, instructions]
    cases result : Memory.step ctx op stack memory with
    | error reason => rfl
    | ok state => simpa only [operationsTree, Bind.bind, Except.bind, Except.mapError] using ih state

/-- The certificate interpreter and region interpreter agree for every projected
live-in state. This does not certify native opcode implementations. -/
theorem toTree_correct (ctx : Context) (code : Code) (state : State) :
    Correspondence.evalTree (toTree ctx code) state = execute ctx code state.1 state.2 := by
  induction code generalizing state with
  | terminal terminal => rfl
  | instructions ops next ih =>
    rw [toTree, operationsTree_correct]
    simp only [execute]
    cases instructions ctx ops state.1 state.2 with
    | error reason => rfl
    | ok state => exact ih state
  | jump growth next ih =>
    by_cases full : state.1.length + growth > 1024
    · simp [toTree, Correspondence.evalTree, jumpEffect, execute, full]
    · simpa [toTree, Correspondence.evalTree, jumpEffect, execute, full] using ih state
  | branch yes no ihYes ihNo =>
    obtain ⟨stack,memory⟩ := state
    by_cases full : stack.length + 1 > 1024
    · simp [toTree, Correspondence.evalTree, conditionEffect, execute, full]
    · cases stack with
      | nil => simp [toTree, Correspondence.evalTree, conditionEffect, execute]
      | cons condition rest =>
        have bound : ¬ rest.length + 1 + 1 > 1024 := by simpa using full
        by_cases zero : condition = (BitVec.ofNat 256 0)
        · simpa [toTree, Correspondence.evalTree, conditionEffect, execute, bound, zero] using ihNo (rest,memory)
        · simpa [toTree, Correspondence.evalTree, conditionEffect, execute, bound, zero] using ihYes (rest,memory)

end SolarTV.EVM.Region
