import SolarTV.MIR.State.Normalize

/-! Ordered whole-word stores for direct MIR execution. The instrumented
machine uses the same transitions and work accounting as the interpreter.
Only an explicit MSTORE that completes its transition emits an event. -/

namespace SolarTV.MIR.State
open EvmRules

abbrev Store := Word × Word

def storeAt (functions : Functions) (machine : Machine) : Option Store := do
  let cursor := machine.cursor
  if !cursor.entered then none else
    let body ← (function functions cursor.function).toOption
    let block ← body.blocks[cursor.block]?
    let inst ← block.instructions[cursor.next]?
    let .memory .store := inst.operation | none
    let [address, value] := inst.operands | none
    return (← (read machine.frame address).toOption, ← (read machine.frame value).toOption)

def runStores (fuel : Nat) (cfg : Config) (functions : Functions) (machine : Machine) :
    Except String (List Store × Result) := do
  match fuel with
  | 0 => .error "MIR evaluation work limit"
  | fuel + 1 =>
    match ← transition fuel cfg functions machine with
    | .finished result => return ([],result)
    | .next extra nextMachine =>
      if extra > fuel then .error "MIR evaluation work limit"
      else
        let (stores,result) ← runStores (fuel-extra) cfg functions nextMachine
        return ((storeAt functions machine).toList ++ stores,result)
termination_by fuel
decreasing_by all_goals omega

theorem runStores_result (fuel : Nat) (cfg : Config) (functions : Functions) (machine : Machine) :
    (runStores fuel cfg functions machine).map Prod.snd = run fuel cfg functions machine := by
  induction fuel using Nat.strongRecOn generalizing machine with
  | ind fuel ih =>
    cases fuel with
    | zero => simp [runStores,run,Except.map]
    | succ fuel =>
      rw [runStores,run]
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
            cases result : runStores (fuel-extra) cfg functions nextMachine <;>
              simp [result,Except.map,Pure.pure,Except.pure] at same ⊢ <;> exact same

theorem runStores_next (fuel : Nat) {cfg functions machine extra nextMachine}
    (step : transition fuel cfg functions machine = .ok (.next extra nextMachine)) :
    runStores (fuel+1) cfg functions machine =
      if extra > fuel then .error "MIR evaluation work limit"
      else (runStores (fuel-extra) cfg functions nextMachine).map
        (fun (stores,result) => ((storeAt functions machine).toList ++ stores,result)) := by
  simp only [runStores,step,Bind.bind,Except.bind,Except.map]
  split
  · rfl
  · cases runStores (fuel-extra) cfg functions nextMachine <;> rfl

theorem runStores_finished (fuel : Nat) {cfg functions machine result}
    (step : transition fuel cfg functions machine = .ok (.finished result)) :
    runStores (fuel+1) cfg functions machine = .ok ([],result) := by
  simp only [runStores,step,Bind.bind,Except.bind,Pure.pure,Except.pure]

theorem runStores_error (fuel : Nat) {cfg functions machine reason}
    (step : transition fuel cfg functions machine = .error reason) :
    runStores (fuel+1) cfg functions machine = .error reason := by
  simp only [runStores,step,Bind.bind,Except.bind]

theorem runStores_one (fuel : Nat) (cfg : Config) (functions : Functions) (machine : Machine) :
    runStores (fuel+1) cfg functions machine = (do
      match ← transition fuel cfg functions machine with
      | .finished result => return ([],result)
      | .next extra nextMachine =>
        if extra > fuel then .error "MIR evaluation work limit"
        else
          let (stores,result) ← runStores (fuel-extra) cfg functions nextMachine
          return ((storeAt functions machine).toList ++ stores,result)) := by
  rw [runStores]

open Lean Meta Simp in
simproc_decl reduceStoreRun (runStores _ _ _ _) := fun expression =>
  reduceRunStep expression ``runStores_error ``runStores_next ``runStores_finished ``runStores_one

open Lean Meta Simp in
dsimproc_decl reduceStoreAt (storeAt _ _) := fun expression => do
  let event ← withTransparency .default (whnf expression)
  if event.isAppOf ``Option.some || event.isAppOf ``Option.none then
    return .visit event
  return .continue

end SolarTV.MIR.State
