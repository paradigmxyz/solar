import SolarTV.MIR.Transforms.ContinuedCounter
import SolarTV.MIR.Transforms.FreshHeap
import SolarTV.MIR.Transforms.LowerChecks

/-! Checked and infallible allocations lower to a cursor load or a named heap
floor, a bounded word addition, a cursor store, and optional initialization.
A checked invalid bump stages the exact panic payload. The floor substitution
requires an active FMP slot and a placement bound to its live word. -/
namespace SolarTV.MIR.LowerAlloc.Stage
open EvmRules SolarTV.Execution MIR.ContinuedCounter MIR.State
open MIR.LowerAbiEncode.ZeroCopy (expand_view read_view)

def invalidBump (cursor size : Word) : Bool :=
  (cursor + size).ult cursor || (18446744073709551615#256).ult (cursor + size)

theorem bump_guard (cursor size : Word) :
    MIR.eval .or
      (MIR.compare .lt (cursor + size) cursor)
      (MIR.compare .gt (cursor + size) 18446744073709551615#256) =
      if invalidBump cursor size then 1 else 0 := by
  by_cases wrap : (cursor + size).ult cursor = true <;>
    by_cases cap : (18446744073709551615#256).ult (cursor + size) = true <;>
    simp [MIR.compare, invalidBump, wrap, cap] <;> rfl

private theorem invalid_spec (cursor size : Word) :
    invalidBump cursor size =
      (decide ((cursor + size).toNat < cursor.toNat) ||
        decide ((cursor + size).toNat > 2^64 - 1)) := by
  simp [invalidBump, BitVec.ult, BitVec.toNat_ofNat]

private def afterEffect (step : Step) : Except String Frame := do
  match ← step with
  | .returned (none, next) => pure next
  | _ => .error "allocation memory effect did not return a frame"

def bump (cfg : Config) (frame : Frame) (cursor size : Word) (zeroed checked : Bool) : Step := do
  if checked && invalidBump cursor size then
    let bytes ← MIR.LowerChecks.scratchPayload frame.memory 0x4e487b71 [65]
    return .reverted (RevertData.fromBytes bytes)
  else
    let frame ← afterEffect (memoryOperation cfg frame .store [64,cursor + size])
    let frame ← if zeroed then afterEffect (memoryOperation cfg frame .zero [cursor,size]) else pure frame
    return .returned (some cursor, frame)

private theorem bump_panic (cfg : Config) (frame : Frame) (cursor size : Word) (zeroed : Bool)
    (bounded : frame.memory.length ≤ 65536) (invalid : invalidBump cursor size = true) :
    bump cfg frame cursor size zeroed true = .ok (.reverted (.panic 65)) := by
  rw [bump]
  simp only [invalid, Bool.and_self, ite_true]
  rw [MIR.LowerChecks.scratch_payload _ _ _ bounded (by decide)]
  rfl

private theorem active_bump (cfg : Config) (frame : Frame) (cursor size : Word)
    (zeroed checked : Bool) (bounded : frame.memory.length ≤ 65536)
    (slot : expand frame.memory 64 32 = .ok frame.memory)
    (live : readWord frame.memory 64 = cursor)
    (valid : checked = false → invalidBump cursor size = false) :
    allocate frame size zeroed checked = bump cfg frame cursor size zeroed checked := by
  cases invalid : invalidBump cursor size with
  | false =>
    have source := (invalid_spec cursor size).symm.trans invalid
    unfold allocate bump
    simp only [slot, Bind.bind, Except.bind, live, source, invalid, Bool.and_false, Bool.false_eq_true, ite_false]
    have stored : memoryOperation cfg frame .store [64, cursor + size] =
        effect {frame with memory := writeBytes frame.memory 64 (wordBytes (cursor + size))} := by
      simp only [memoryOperation, wordBytes_length, show (64 : Word).toNat = 64 from rfl,
        slot, Bind.bind, Except.bind]
    rw [stored]
    cases zeroed
    · rfl
    · simp only [ite_true, afterEffect, memoryOperation, effect, Bind.bind,
        Except.bind, Pure.pure, Except.pure]
      cases expand (writeBytes frame.memory 64 (wordBytes (cursor + size))) cursor.toNat size.toNat <;> rfl
  | true =>
    have checkedNow : checked = true := by
      cases checked
      · simp_all
      · rfl
    subst checked
    have source := (invalid_spec cursor size).symm.trans invalid
    rw [bump_panic cfg frame cursor size zeroed bounded invalid]
    simp only [allocate, slot, Bind.bind, Except.bind, live, source, ite_true,
      Pure.pure, Except.pure]

def fromLoad (cfg : Config) (frame : Frame) (size : Word) (zeroed checked : Bool) : Step := do
  match ← memoryOperation cfg frame .load [64] with
  | .returned (some cursor, next) => bump cfg next cursor size zeroed checked
  | _ => .error "allocation cursor load did not return a word"

theorem load_bump (cfg : Config) (frame : Frame) (size : Word) (zeroed checked : Bool)
    (aligned : frame.memory.length % 32 = 0) (bounded : frame.memory.length ≤ 65536)
    (valid : checked = false → invalidBump (readWord frame.memory 64) size = false) :
    allocate frame size zeroed checked = fromLoad cfg frame size zeroed checked := by
  let active := padTo frame.memory 96
  have expanded : expand frame.memory 64 32 = .ok active :=
    expandAligned _ 64 32 (by decide) aligned (by decide) bounded (by decide)
  have activeBound : active.length ≤ 65536 := by
    simp only [active, padLength]
    omega
  have activeAligned : active.length % 32 = 0 := by
    simp only [active, padLength]
    exact maxAligned _ _ aligned (by decide)
  have activeSlot : expand active 64 32 = .ok active :=
    MIR.FreshHeap.active_slot active activeBound activeAligned (by simp only [active, padLength]; omega)
  have cursor : readWord active 64 = readWord frame.memory 64 := by
    unfold readWord
    rw [← read_view active, expand_view _ _ _ _ expanded, read_view]
  unfold fromLoad
  simp only [memoryOperation, show (64 : Word).toNat = 64 from rfl, expanded,
    Bind.bind, Except.bind, value]
  rw [← active_bump cfg {frame with memory := active} (readWord active 64) size zeroed checked
    activeBound activeSlot rfl (by rwa [cursor])]
  unfold allocate
  rw [expanded, activeSlot]

def fromFloor (cfg : Config) (frame : Frame) (name : String) (size : Word)
    (zeroed checked : Bool) : Step := do
  match ← layoutOperation cfg frame (.heapFloor name) [] with
  | .returned (some cursor, next) => bump cfg next cursor size zeroed checked
  | _ => .error "allocation heap floor did not return a word"

theorem floor_bump (cfg : Config) (frame : Frame) (name : String) (floor size : Word)
    (zeroed checked : Bool) (aligned : frame.memory.length % 32 = 0)
    (bounded : frame.memory.length ≤ 65536) (active : 96 ≤ frame.memory.length)
    (binding : MIR.FreshHeap.Binding cfg.layout name floor) (live : readWord frame.memory 64 = floor)
    (valid : checked = false → invalidBump floor size = false) :
    allocate frame size zeroed checked = fromFloor cfg frame name size zeroed checked := by
  unfold fromFloor
  rw [MIR.FreshHeap.query cfg frame name floor binding]
  exact active_bump cfg frame floor size zeroed checked bounded
    (MIR.FreshHeap.active_slot _ bounded aligned active) live valid

end SolarTV.MIR.LowerAlloc.Stage
