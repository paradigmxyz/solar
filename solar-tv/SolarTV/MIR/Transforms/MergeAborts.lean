import SolarTV.MIR.Transforms.Schedule
import SolarTV.MIR.Transforms.WidthTestFacts

/-! A chain of total, state-independent stages may defer rejection to the
last guard. Success retains the completed local assignments. Rejection
selects the same terminal continuation. Its inputs must be available before
the speculated assignments; the importer checks that boundary and the common
control-flow graph. The number of ordinary MIR steps and physical gas remain
unpriced. -/

namespace SolarTV.MIR.MergeAborts
open EvmRules

abbrev Stage (Frame : Type) := Frame → Frame × Bool

def early : List (Stage Frame) → Frame → Option Frame
  | [], frame => some frame
  | stage :: rest, frame =>
    let (next, reject) := stage frame
    if reject then none else early rest next

def collect : List (Stage Frame) → Frame → Frame × List Bool
  | [], frame => (frame, [])
  | stage :: rest, frame =>
    let (next, reject) := stage frame
    let (completed, guards) := collect rest next
    (completed, reject :: guards)

def late (stages : List (Stage Frame)) (frame : Frame) (reject : Bool) : Option Frame :=
  if reject then none else some (collect stages frame).1

theorem early_collect (stages : List (Stage Frame)) (frame : Frame) :
    early stages frame = late stages frame ((collect stages frame).2.any id) := by
  induction stages generalizing frame with
  | nil => rfl
  | cons stage rest ih =>
    cases result : stage frame with
    | mk next reject =>
      cases reject <;> simp [early, collect, late, result, ih next]

theorem context (stages : List (Stage Frame)) (frame : Frame) (reject : Bool)
    (equal : (collect stages frame).2.any id = reject)
    (observe : Option Frame → Observation) :
    observe (early stages frame) = observe (late stages frame reject) := by
  rw [early_collect, equal]

theorem callvalue (cfg : State.Config) (frame : State.Frame) :
    State.contextOperation cfg frame 0x34 [] = State.value frame cfg.environment.value := rfl

theorem heap_floor (cfg : State.Config) (frame : State.Frame) (owner : String)
    (placement : State.FunctionPlacement) (floor : Word)
    (bound : State.placementBinding cfg owner = .ok placement)
    (available : placement.heapFloor = some floor) :
    State.layoutOperation cfg frame (.heapFloor owner) [] = State.value frame floor := by
  simp [State.layoutOperation, bound, available, State.required, State.value,
    Bind.bind, Except.bind]

theorem constructor_range (cfg : State.Config) (frame : State.Frame) (isEnd : Bool)
    (base size : Nat) (available : cfg.layout.constructor = some (base, size)) :
    State.layoutOperation cfg frame (.constructorRange isEnd) [] =
      State.value frame (BitVec.ofNat 256 (base + if isEnd then size else 0)) := by
  simp [State.layoutOperation, available, State.required, State.value, Bind.bind, Except.bind]

end SolarTV.MIR.MergeAborts
