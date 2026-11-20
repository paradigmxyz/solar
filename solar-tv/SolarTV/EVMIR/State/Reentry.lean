import SolarTV.EVMIR.State.Interpreter

/-! Run a separately certified region at a reached bytecode position,
including in callbacks into an already active contract. Ordinary bytecode
still selects the position and constructs the entered environment and
checkpoint. Each admitted region returns actual gas, work, effects and
outcome to its owning frame. -/

namespace SolarTV.EVM.State.Reentry
open SolarTV.Execution

abbrev Route := Nat → ChildExecutor → Journal → Frame → Option Run

/-- A region may delegate children at the strictly smaller work budget. The
recursive driver supplies the correspondence needed by its local proof. -/
def Sound (cfg : Config) (route : Route) : Prop :=
  ∀ fuel execute, execute = run fuel cfg → ∀ world frame result,
    route (fuel+1) execute world frame = some result → result = run (fuel+1) cfg world frame

def runWith (cfg : Config) (route : Route) (fuel : Nat) (world : Journal) (frame : Frame) : Run := do
  match fuel with
  | 0 => .error "world instruction/depth evaluation limit"
  | fuel+1 =>
    let execute := runWith cfg route fuel
    match route (fuel+1) execute world frame with
    | some result => result
    | none =>
      match ← advance cfg execute (← instruction cfg world frame) with
      | .inl result => return {result with work := result.work + 1}
      | .inr (spent,nextWorld,nextFrame) =>
        let result ← runWith cfg route (fuel-spent) nextWorld nextFrame
        return {result with work := result.work + spent + 1}
termination_by fuel
decreasing_by all_goals omega

theorem correspondence (cfg : Config) (route : Route) (sound : Sound cfg route) (fuel : Nat) :
    runWith cfg route fuel = run fuel cfg := by
  induction fuel using Nat.strongRecOn with
  | ind fuel ih =>
    funext world frame
    cases fuel with
    | zero => rw [runWith, run]
    | succ fuel =>
      rw [runWith, ih fuel (by omega)]
      cases routed : route (fuel+1) (run fuel cfg) world frame with
      | some result => exact sound fuel _ rfl world frame result routed
      | none =>
        rw [run]
        cases instruction cfg world frame with
        | error _ => rfl
        | ok signal =>
          simp only [Bind.bind, Except.bind]
          cases advance cfg (run fuel cfg) signal with
          | error _ => rfl
          | ok step =>
            cases step with
            | inl _ => rfl
            | inr pending =>
              obtain ⟨spent,world,frame⟩ := pending
              simp only [ih (fuel-spent) (by omega : fuel-spent < fuel+1)]

def executeWith (cfg : Config) (route : Route) (fuel : Nat) (world : Journal) (frame : Frame) : Run := do
  let result ← runWith cfg route fuel world frame
  return if result.stop = .success then {result with world := finalize result.world}
    else {result with world, gas := if result.stop = .exception || result.stop = .outOfGas then 0 else result.gas}

theorem execute_correspondence (cfg : Config) (route : Route) (sound : Sound cfg route)
    (fuel : Nat) (world : Journal) (frame : Frame) :
    executeWith cfg route fuel world frame = execute fuel cfg world frame := by
  unfold executeWith execute
  rw [correspondence cfg route sound fuel]

end SolarTV.EVM.State.Reentry
