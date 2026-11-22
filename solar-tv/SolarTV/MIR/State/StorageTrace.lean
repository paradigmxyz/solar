import SolarTV.MIR.State.Observe

/-! Ordered completed SLOAD and SSTORE operations for concrete MIR replay. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

abbrev StorageAccess := Nat × Word × Word

private def storageAt (cfg : Config) (functions : Functions) (machine : Machine) :
    Option StorageAccess := do
  let cursor := machine.cursor
  if !cursor.entered then none else
    let body ← (function functions cursor.function).toOption
    let block ← body.blocks[cursor.block]?
    let inst ← block.instructions[cursor.next]?
    match inst.operation, inst.operands with
    | .storage false false, [slot] =>
      let key ← (read machine.frame slot).toOption
      let slots := (account machine.frame.world cfg.environment.address).storage
      return (0x54,key,slotsRead slots key)
    | .storage false true, [slot,value] =>
      let key ← (read machine.frame slot).toOption
      let word ← (read machine.frame value).toOption
      return (0x55,key,word)
    | _,_ => none

/-- Record completed operations while sharing the direct interpreter's transitions
and work accounting. Records do not affect the execution result. -/
private def runStorage (fuel : Nat) (cfg : Config) (functions : Functions) (machine : Machine) :
    Except String (List StorageAccess × Result) := do
  match fuel with
  | 0 => .error "MIR evaluation work limit"
  | fuel + 1 =>
    match ← transition fuel cfg functions machine with
    | .finished result => return ([],result)
    | .next extra nextMachine =>
      if extra > fuel then .error "MIR evaluation work limit"
      else
        let (trace,result) ← runStorage (fuel-extra) cfg functions nextMachine
        return ((storageAt cfg functions machine).toList ++ trace,result)
termination_by fuel
decreasing_by all_goals omega

private theorem runStorage_result (fuel : Nat) (cfg : Config) (functions : Functions) (machine : Machine) :
    (runStorage fuel cfg functions machine).map Prod.snd = run fuel cfg functions machine := by
  induction fuel using Nat.strongRecOn generalizing machine with
  | ind fuel ih =>
    cases fuel with
    | zero => simp [runStorage,run,Except.map]
    | succ fuel =>
      rw [runStorage,run]
      cases step : transition fuel cfg functions machine with
      | error _ => rfl
      | ok effect =>
        cases effect with
        | finished _ => rfl
        | next extra nextMachine =>
          by_cases limited : extra > fuel
          · simp [Bind.bind,Except.bind,Except.map,limited]
          · have same := ih (fuel-extra) (by omega) nextMachine
            simp only [Bind.bind,Except.bind,limited,ite_false]
            cases result : runStorage (fuel-extra) cfg functions nextMachine <;>
              simp [result,Except.map,Pure.pure,Except.pure] at same ⊢ <;> exact same

def executeStorage (fuel : Nat) (cfg : Config) (functions : Functions) (entry : Nat)
    (frame : Frame) : Except String (List StorageAccess × Result) := do
  if cfg.callDepth = 0 then .error "MIR internal call depth limit" else
  let initial ← start functions entry frame.arguments frame
  let (trace,result) ← runStorage fuel cfg functions ⟨{function := entry},[],initial⟩
  return (trace,← conclude cfg frame (.ok result))

theorem executeStorage_result (fuel : Nat) (cfg : Config) (functions : Functions) (entry : Nat)
    (frame : Frame) :
    (executeStorage fuel cfg functions entry frame).map Prod.snd =
      execute fuel cfg functions entry frame := by
  unfold executeStorage execute
  split
  · rfl
  simp only [Bind.bind,Except.bind]
  cases start functions entry frame.arguments frame
  · rfl
  rename_i initial
  dsimp only
  rw [← runStorage_result fuel cfg functions]
  cases runStorage fuel cfg functions ⟨{function := entry},[],initial⟩
  · rfl
  rename_i traced
  obtain ⟨trace,result⟩ := traced
  simp only [Except.map]
  cases conclude cfg frame (.ok result) <;> rfl

end SolarTV.MIR.State
