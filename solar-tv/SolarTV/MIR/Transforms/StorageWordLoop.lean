import SolarTV.MIR.State.StorageBytes

/-! The storage clearing helper carries two phis: a word index and a slot
address. The index must reach the end without wrapping. The storage address
may wrap on any iteration. The invariant relates the two phis to the reference
range, preserving the order of stores and the first failing effect. -/

namespace SolarTV.MIR.StorageWordLoop
open EvmRules SolarTV.Execution MIR.State

/-- Each successful transfer consumes one unit. The final guard also needs fuel. -/
def lowered (finish : Word) (step : Word → State → Except Error State) :
    Nat → Word → Word → State → Except (Option Error) State
  | 0, _, _, _ => .error none
  | fuel+1, index, slot, state =>
    if index.toNat < finish.toNat then do
      let state ← (step slot state).mapError some
      lowered finish step fuel (index+1) (slot+1) state
    else .ok state

def reference (step : Word → State → Except Error State)
    (count : Nat) (slot : Word) (state : State) : Except (Option Error) State :=
  (List.range count).foldlM
    (fun state i => (step (slot + BitVec.ofNat 256 i) state).mapError some) state

private theorem offset (slot : Word) (i : Nat) :
    slot + BitVec.ofNat 256 (i+1) = slot + 1 + BitVec.ofNat 256 i := by
  rw [BitVec.ofNat_add, BitVec.add_comm (BitVec.ofNat 256 i), ← BitVec.add_assoc]
  rfl

private theorem reference_succ (step : Word → State → Except Error State)
    (count : Nat) (slot : Word) (state : State) :
    reference step (count+1) slot state =
      ((step slot state).mapError some >>= reference step count (slot+1)) := by
  simp only [reference, List.range_succ_eq_map, List.foldlM_cons,
    List.foldlM_map, BitVec.add_zero, Nat.succ_eq_add_one, offset]
  rfl

theorem corresponds (finish : Word) (step : Word → State → Except Error State)
    (fuel : Nat) (index slot : Word) (state : State)
    (enough : finish.toNat - index.toNat < fuel) :
    lowered finish step fuel index slot state =
      reference step (finish.toNat-index.toNat) slot state := by
  induction fuel generalizing index slot state with
  | zero => omega
  | succ fuel ih =>
    by_cases active : index.toNat < finish.toNat
    · have room : index.toNat + 1 < 2^256 := by
        have := finish.isLt
        omega
      have nextIndex : (index+1).toNat = index.toNat+1 := by
        rw [BitVec.toNat_add]
        exact Nat.mod_eq_of_lt room
      have count : finish.toNat - index.toNat = (finish.toNat - (index+1).toNat)+1 := by
        rw [nextIndex]
        omega
      have nextFuel : finish.toNat - (index+1).toNat < fuel := by omega
      simp only [lowered, active, ite_true, count, reference_succ, Bind.bind, Except.bind]
      cases (step slot state).mapError some with
      | error _ => rfl
      | ok next => exact ih (index+1) (slot+1) next nextFuel
    · have empty : finish.toNat-index.toNat = 0 := by omega
      simp [lowered, active, empty, reference, Pure.pure, Except.pure]

private theorem foldlM_mapError (step : State → Nat → Except Error State)
    (indices : List Nat) (state : State) :
    indices.foldlM (fun state i => (step state i).mapError some) state =
      (indices.foldlM step state).mapError some := by
  induction indices generalizing state with
  | nil => rfl
  | cons index rest ih =>
    simp only [List.foldlM_cons, Except.bind, Bind.bind]
    cases step state index with
    | error _ => rfl
    | ok next => exact ih next

/-- The index loop emitted for the clear helper, under the source model's work
and static-state checks. Hashing still happens when the range is empty. -/
def clear (cfg : Config) (frame : Frame) (slot first finish : Word) (fuel : Nat) : Step := do
  let (base,frame) ← fixedSlot frame [slot]
  let count := finish.toNat - first.toNat
  if count > 2048 then .error "MIR storage work limit of 2048 words" else
  if count > 0 && cfg.environment.isStatic then return .invalid else
  match lowered finish (fun address frame => storageWrite cfg frame address 0)
      fuel first (base+first) frame with
  | .ok frame => return .returned (none,frame)
  | .error (some reason) => .error reason
  | .error none => .error "MIR storage loop fuel exhausted"

theorem clear_corresponds (cfg : Config) (frame : Frame) (slot first finish : Word) (fuel : Nat)
    (enough : finish.toNat - first.toNat < fuel) :
    clear cfg frame slot first finish fuel = clearStorageWords cfg frame slot first finish := by
  simp only [clear, clearStorageWords, Except.bind, Bind.bind, Pure.pure, Except.pure]
  cases fixedSlot frame [slot] with
  | error _ => rfl
  | ok result =>
    obtain ⟨base,next⟩ := result
    by_cases limited : finish.toNat - first.toNat > 2048
    · simp [limited]
    · simp only [limited, ite_false]
      by_cases static : (decide (finish.toNat - first.toNat > 0) && cfg.environment.isStatic) = true
      · simp [static]
      · simp only [static, corresponds _ _ _ _ _ _ enough, reference, foldlM_mapError]
        cases (List.range (finish.toNat-first.toNat)).foldlM
          (fun state i => storageWrite cfg state (base+first+BitVec.ofNat 256 i) 0) next <;> rfl

/-- The builtin stores a rounded-up count of words, masking the last word.
Lowering stores complete words in the loop and uses a separate partial store.
The lists retain the same order, offsets and number of bytes to keep. -/
theorem fullWordsThenPartial (length : Nat) :
    (List.range ((length+31)/32)).map (fun i => (i,min 32 (length-32*i))) =
      (List.range (length/32)).map (fun i => (i,32)) ++
        (if length % 32 = 0 then [] else [(length/32,length % 32)]) := by
  have full : (List.range (length/32)).map (fun i => (i,min 32 (length-32*i))) =
      (List.range (length/32)).map (fun i => (i,32)) := by
    apply List.map_congr_left
    intro i member
    have index := List.mem_range.mp member
    have keep : 32 ≤ length-32*i := by omega
    rw [Nat.min_eq_left keep]
  by_cases aligned : length % 32 = 0
  · have count : (length+31)/32 = length/32 := by omega
    simpa [aligned,count] using full
  · have count : (length+31)/32 = length/32+1 := by omega
    have keep : min 32 (length-32*(length/32)) = length % 32 := by omega
    rw [count, List.range_succ, List.map_append, full]
    simp [aligned, keep]

end SolarTV.MIR.StorageWordLoop
