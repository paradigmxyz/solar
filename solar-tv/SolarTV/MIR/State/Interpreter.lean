import SolarTV.MIR.State.Ops

/-! Small-step direct MIR execution with simultaneous edge phis. Each block is
executed in instruction order against one immutable incoming SSA snapshot.
The frame carries shared memory and a journal; a revert or exception rolls the
journal back at the selected boundary. Fuel exhaustion is a model error and
cannot prove equivalence, even when both executions exhaust their bound.
No instruction selection, stack scheduling or backend memory record is used. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

def finish (completion : Completion) (frame : Frame) : Result :=
  ⟨completion,frame.world,frame.memory⟩

def body (cfg : Config) (frame : Frame) : List Instruction → Step
  | [] => .ok (.returned (none,frame))
  | inst :: rest => do
    match ← instruction cfg frame inst with
    | .returned (_,next) => body cfg next rest
    | .reverted data => return .reverted data
    | .invalid => return .invalid

def choose (frame : Frame) (key : Word) (fallback : Nat) : List (Operand × Nat) → Except String Nat
  | [] => .ok fallback
  | (operand,target) :: rest => do
    let test ← read frame operand
    if test = key then return target else choose frame key fallback rest

def run (fuel : Nat) (cfg : Config) (blocks : List Block) (target : Nat)
    (predecessor : Option Nat) (frame : Frame) : Run := do
  match fuel with
  | 0 => .error "MIR evaluation work limit"
  | fuel + 1 =>
    let block ← required blocks[target]? "unknown MIR block"
    if block.phis.length + block.instructions.length > fuel then .error "MIR evaluation work limit" else
    let incoming ← enter predecessor block.phis frame
    match ← body cfg incoming block.instructions with
    | .reverted data => return finish (.reverted (failureData data)) incoming
    | .invalid => return finish .invalid incoming
    | .returned (_,frame) =>
      let remaining := fuel - block.phis.length - block.instructions.length
      match block.terminator with
      | .jump next => run remaining cfg blocks next (some target) frame
      | .branch condition yes no =>
        let condition ← read frame condition
        run remaining cfg blocks (if condition = 1 then yes else no) (some target) frame
      | .switch value fallback cases =>
        let key ← read frame value
        let next ← choose frame key fallback cases
        run remaining cfg blocks next (some target) frame
      | .returned operands =>
        let values ← operands.mapM (read frame)
        if cfg.entry then
          if values.isEmpty then return finish (.success []) frame
          else .error "MIR entry returned word values"
        else return finish (.returned values) frame
      | .stop => return finish (.success []) frame
      | .invalid => return finish .invalid frame
      | .returndata offset size | .reverted offset size =>
        let offset ← read frame offset
        let size ← read frame size
        let memory ← expand frame.memory offset.toNat size.toNat
        let data := readBytes memory offset.toNat size.toNat
        let reverted := match block.terminator with | .reverted .. => true | _ => false
        return finish (if reverted then .reverted data else .success data) {frame with memory}
termination_by fuel
decreasing_by all_goals omega

def execute (fuel : Nat) (cfg : Config) (blocks : List Block) (frame : Frame) : Run := do
  let result ← run fuel cfg blocks 0 none frame
  return match result.completion with
    | .reverted _ | .invalid => {result with world := frame.world}
    | _ => result

end SolarTV.MIR.State
