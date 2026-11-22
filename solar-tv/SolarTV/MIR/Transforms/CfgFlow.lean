import SolarTV.MIR.State.Interpreter

/-! An unpriced MIR action graph after coalescing or threading unconditional
jumps. The import makes each predecessor-selected parallel phi an action.
Every ordered operand, result binder, instruction and nonforwarding transfer
stays in the graph. Interpreting a preserved action can read, modify, or halt
arbitrary shared state. The certificate counts logical actions. -/

namespace SolarTV.MIR.CfgFlow
open SolarTV.Execution

inductive Node where
  | action (key next : Nat)
  | select (key : Nat) (targets : List Nat)
  | terminal (key : Nat)
  | forward (next : Nat)
  deriving DecidableEq

abbrev Graph := Array Node

def follow (graph : Graph) : Nat → Nat → Option Node
  | 0, _ => none
  | fuel+1, point => do
    match ← graph[point]? with
    | .forward next => follow graph fuel next
    | node => some node

def project (graph : Graph) (points : List Nat) (jumpLimit : Nat) : Option Graph := do
  let resolve := points.idxOf?
  return (← points.mapM fun point => do
    match ← follow graph jumpLimit point with
    | .action key next => return .action key (← resolve next)
    | .select key targets => return .select key (← targets.mapM resolve)
    | .terminal key => return .terminal key
    | .forward _ => none).toArray

structure Handlers (State Terminal : Type) where
  action : Nat → State → Except Terminal State
  select : Nat → State → Except Terminal (Nat × State)
  terminal : Nat → State → Terminal

inductive Result (State Terminal : Type) where
  | halted (terminal : Terminal)
  | exhausted (state : State)
  | invalid (state : State)

def run (graph : Graph) (handlers : Handlers Payload Terminal) :
    Nat → Nat → Payload → Result Payload Terminal
  | 0, _, state => .exhausted state
  | work+1, point, state =>
    match graph[point]? with
    | some (.action key target) => match handlers.action key state with
      | .error observation => .halted observation
      | .ok state => run graph handlers work target state
    | some (.select key targets) => match handlers.select key state with
      | .error observation => .halted observation
      | .ok (arm,state) => match targets[arm]? with
        | none => .invalid state
        | some target => run graph handlers work target state
    | some (.terminal key) => .halted (handlers.terminal key state)
    | _ => .invalid state

theorem contextual (before after : Graph) (left right : List Nat) (jumpLimit : Nat)
    (same : project before left jumpLimit = project after right jumpLimit)
    (handlers : Handlers Payload Terminal) (work : Nat) (state : Payload) :
    (project before left jumpLimit).map (fun graph => run graph handlers work 0 state) =
    (project after right jumpLimit).map (fun graph => run graph handlers work 0 state) :=
  congrArg (Option.map (fun graph => run graph handlers work 0 state)) same

/-- Moving across an actual typed MIR unconditional jump changes only the
cursor and its predecessor. Phi entry remains a separate operation. -/
theorem typed_jump (cfg : State.Config) (functions : State.Functions) (machine : State.Machine)
    (body : State.Function) (block : State.Block) (target fuel : Nat)
    (resolved : State.function functions machine.cursor.function = .ok body)
    (selected : body.blocks[machine.cursor.block]? = some block)
    (entered : machine.cursor.entered = true)
    (endOfBlock : block.instructions.length ≤ machine.cursor.next)
    (transfer : block.terminator = .jump target) :
    State.transition fuel cfg functions machine =
      .ok (.next 0 ⟨State.branch machine.cursor target,machine.stack,machine.frame⟩) := by
  cases machine with
  | mk cursor stack frame =>
    change cursor.entered = true at entered
    simp [State.transition,resolved,selected,State.required,entered,transfer,
      show ¬cursor.next < block.instructions.length from Nat.not_lt.mpr endOfBlock,
      State.next,Bind.bind,Except.bind]

theorem typed_enter_empty (cfg : State.Config) (functions : State.Functions)
    (machine : State.Machine) (body : State.Function) (block : State.Block) (fuel : Nat)
    (resolved : State.function functions machine.cursor.function = .ok body)
    (selected : body.blocks[machine.cursor.block]? = some block)
    (entered : machine.cursor.entered = false) (empty : block.phis = []) :
    State.transition fuel cfg functions machine =
      .ok (.next 0 ⟨{machine.cursor with entered := true},machine.stack,machine.frame⟩) := by
  cases machine with
  | mk cursor stack frame =>
    change cursor.entered = false at entered
    simp [State.transition,resolved,selected,State.required,entered,empty,
      State.enter,Bind.bind,Except.bind,Pure.pure,Except.pure]

end SolarTV.MIR.CfgFlow
