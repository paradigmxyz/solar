import SolarTV.MIR.State.Interpreter
import SolarTV.MIR.Transforms.LoopSimulation

/-! Index-to-pointer induction for strided, stateful MIR loops. The
header tests either the matching index or a pointer sentinel. Both versions
use the same address for each effect and preserve the first failure. -/

namespace SolarTV.MIR.AffineLoop
open EvmRules LoopSimulation

def address (base stride index : Word) : Word := base + index*stride

theorem addressStep (base stride index : Word) :
    address base stride (index+1) = address base stride index + stride := by
  simp only [address, BitVec.add_mul, BitVec.add_assoc]
  rw [show (1 : Word) * stride = stride from BitVec.one_mul stride]

theorem addressInjective (base stride index limit : Word)
    (within : index.toNat ≤ limit.toNat) (positive : 0 < stride.toNat)
    (fits : limit.toNat * stride.toNat < 2^256) :
    address base stride index = address base stride limit ↔ index = limit := by
  constructor
  · intro equal
    have same : index * stride = limit * stride := by
      have := congrArg (fun slot : Word => slot - base) equal
      simpa [address, BitVec.add_comm base, BitVec.add_sub_cancel] using this
    have indexFits := Nat.lt_of_le_of_lt (Nat.mul_le_mul_right stride.toNat within) fits
    have words := congrArg BitVec.toNat same
    rw [BitVec.toNat_mul, BitVec.toNat_mul, Nat.mod_eq_of_lt indexFits,
      Nat.mod_eq_of_lt fits] at words
    exact BitVec.eq_of_toNat_eq (Nat.mul_right_cancel positive words)
  · intro equal
    rw [equal]

/-- The effect can access or change any live state. Its returned event list
includes effects that occurred before a failure. -/
abbrev Effect (State Event Failure : Type) :=
  Word → State → List Event × Except Failure State

/-- A header guard can read live state, or fail before a body effect. -/
abbrev Guard (State Event Failure : Type) :=
  Word → State → List Event × Except Failure (Bool × State)

inductive Exit State Failure where
  | success (state : State)
  | failure (reason : Failure)

structure Frame (State : Type) where
  index : Word
  cursor : Word
  state : State

def iteration (guard : Guard State Event Failure) (effect : Effect State Event Failure)
    (index slot : Word) (state : State)
    (advance : State → Frame State) : Step (Frame State) Event (Exit State Failure) :=
  match guard index state with
  | (events, .error reason) => .done events (.failure reason)
  | (events, .ok (false,next)) => .done events (.success next)
  | (events, .ok (true,next)) =>
    match effect slot next with
    | (more,.error reason) => .done (events ++ more) (.failure reason)
    | (more,.ok next) => .again (events ++ more) (advance next)

def indexed (base stride : Word) (guard : Guard State Event Failure) (effect : Effect State Event Failure) :
    Loop (Frame State) Event (Exit State Failure) :=
  fun f => iteration guard effect f.index (address base stride f.index) f.state
    (fun state => ⟨f.index+1,0,state⟩)

def carried (stride : Word) (guard : Guard State Event Failure) (effect : Effect State Event Failure) :
    Loop (Frame State) Event (Exit State Failure) :=
  fun f => iteration guard effect f.index f.cursor f.state
    (fun state => ⟨f.index+1,f.cursor+stride,state⟩)

def carries (base stride : Word) (a b : Frame State) : Prop :=
  a.index = b.index ∧ b.cursor = address base stride a.index ∧ a.state = b.state

def carrySimulation (base stride : Word) (guard : Guard State Event Failure)
    (effect : Effect State Event Failure) :
    Simulation (indexed base stride guard effect) (carried stride guard effect) where
  invariant := carries base stride
  step := by
    intro ⟨i,_,state⟩ ⟨j,ptr,other⟩ h
    obtain ⟨rfl,rfl,rfl⟩ := h
    simp only [indexed,carried,iteration]
    split
    · simp [Step.related]
    · simp [Step.related]
    · split
      · simp [Step.related]
      · simp [Step.related,carries]
        exact (addressStep base stride i).symm

def below (limit index : Word) (state : State) : List Event × Except Failure (Bool × State) :=
  ([],.ok (decide (index.toNat < limit.toNat),state))

/-- This version keeps only a cursor in the emitted loop. The index in the
simulation frame records the logical iteration and has no runtime observer. -/
def sentinel (stride finish : Word) (effect : Effect State Event Failure) :
    Loop (Frame State) Event (Exit State Failure) :=
  fun f =>
    if f.cursor = finish then .done [] (.success f.state)
    else iteration (fun _ state => ([],.ok (true,state))) effect f.index f.cursor f.state
      (fun state => ⟨f.index+1,f.cursor+stride,state⟩)

private theorem nextIndex (index limit : Word) (small : index.toNat < limit.toNat) :
    (index+1).toNat = index.toNat+1 := by
  rw [BitVec.toNat_add]
  change (index.toNat+1) % 2^256 = index.toNat+1
  have := limit.isLt
  exact Nat.mod_eq_of_lt (by omega)

theorem indexedTerminates (base stride limit : Word) (guard : Guard State Event Failure)
    (effect : Effect State Event Failure)
    (active : ∀ index state events next,
      guard index state = (events,.ok (true,next)) → index.toNat < limit.toNat)
    (fuel : Nat) (frame : Frame State)
    (enough : limit.toNat-frame.index.toNat < fuel) :
    (observe (indexed base stride guard effect) fuel frame).2.isSome = true := by
  apply LoopSimulation.terminates (rank := fun f => limit.toNat-f.index.toNat) (enough := enough)
  intro start events next step
  simp only [indexed,iteration] at step
  split at step
  · contradiction
  · contradiction
  · rename_i emitted checked passed
    split at step
    · contradiction
    · simp only [Step.again.injEq] at step
      rw [← step.2]
      have advances := active start.index start.state emitted checked passed
      change limit.toNat-(start.index+1).toNat < limit.toNat-start.index.toNat
      rw [nextIndex start.index limit advances]
      omega

def sentinelSimulation (base stride limit : Word) (positive : 0 < stride.toNat)
    (fits : limit.toNat * stride.toNat < 2^256) (effect : Effect State Event Failure) :
    Simulation (indexed base stride (below limit) effect)
      (sentinel stride (address base stride limit) effect) where
  invariant a b := carries base stride a b ∧ a.index.toNat ≤ limit.toNat
  step := by
    intro ⟨i,_,state⟩ ⟨j,ptr,other⟩ ⟨same,within⟩
    obtain ⟨rfl,rfl,rfl⟩ := same
    change i.toNat ≤ limit.toNat at within
    have equal := addressInjective base stride i limit within positive fits
    by_cases active : i.toNat < limit.toNat
    · have different : i ≠ limit := by intro h; simp [h] at active
      simp only [indexed,sentinel,iteration,below,active,decide_true,equal,different,ite_false]
      split
      · simp [Step.related]
      · simp only [Step.related, carries, and_true, true_and,
          ← addressStep]
        change (i+1).toNat ≤ limit.toNat
        rw [nextIndex i limit active]
        omega
    · have last : i = limit := BitVec.eq_of_toNat_eq (by omega)
      simp [indexed,sentinel,iteration,below,last,Step.related]

/-- The loop walks two modular addresses. Only the first cursor determines
the exit, so the second stride needs no injectivity premise. -/
structure CursorPair where
  first : Word
  second : Word

def CursorPair.address (bases strides : CursorPair) (index : Word) : CursorPair :=
  ⟨AffineLoop.address bases.first strides.first index,
    AffineLoop.address bases.second strides.second index⟩

def CursorPair.advance (cursors strides : CursorPair) : CursorPair :=
  ⟨cursors.first + strides.first,cursors.second + strides.second⟩

theorem CursorPair.addressStep (bases strides : CursorPair) (index : Word) :
    bases.address strides (index+1) = (bases.address strides index).advance strides := by
  simp only [CursorPair.address, CursorPair.advance, CursorPair.mk.injEq]
  exact ⟨AffineLoop.addressStep _ _ _, AffineLoop.addressStep _ _ _⟩

structure PairedFrame (State : Type) where
  index : Word
  cursors : CursorPair
  state : State

abbrev PairedEffect (State Event Failure : Type) :=
  CursorPair → State → List Event × Except Failure State

private def pairedStep (active : Bool) (effect : PairedEffect State Event Failure)
    (cursors : CursorPair) (state : State) (next : State → PairedFrame State) :
    Step (PairedFrame State) Event (Exit State Failure) :=
  if active then
    match effect cursors state with
    | (events,.error reason) => .done events (.failure reason)
    | (events,.ok updated) => .again events (next updated)
  else .done [] (.success state)

def indexedPair (bases strides : CursorPair) (limit : Word)
    (effect : PairedEffect State Event Failure) :
    Loop (PairedFrame State) Event (Exit State Failure) :=
  fun f => pairedStep (decide (f.index.toNat < limit.toNat)) effect
    (bases.address strides f.index) f.state
    (fun state => ⟨f.index+1,f.cursors,state⟩)

def walkedPair (strides : CursorPair) (finish : Word)
    (effect : PairedEffect State Event Failure) :
    Loop (PairedFrame State) Event (Exit State Failure) :=
  fun f =>
    if f.cursors.first = finish then .done [] (.success f.state)
    else pairedStep true effect f.cursors f.state
      (fun state => ⟨f.index+1,f.cursors.advance strides,state⟩)

def pairedSimulation (bases strides : CursorPair) (limit : Word)
    (positive : 0 < strides.first.toNat)
    (fits : limit.toNat * strides.first.toNat < 2^256)
    (effect : PairedEffect State Event Failure) :
    Simulation (indexedPair bases strides limit effect)
      (walkedPair strides (address bases.first strides.first limit) effect) where
  invariant left right := left.index = right.index ∧
    right.cursors = bases.address strides left.index ∧
    left.state = right.state ∧ left.index.toNat ≤ limit.toNat
  step := by
    intro ⟨index,left,state⟩ ⟨other,right,otherState⟩ ⟨same,atCursor,shared,within⟩
    obtain rfl := same
    obtain rfl := atCursor
    obtain rfl := shared
    change index.toNat ≤ limit.toNat at within
    have sameIndex := addressInjective bases.first strides.first index limit within positive fits
    by_cases active : index.toNat < limit.toNat
    · have different : index ≠ limit := by intro equal; simp [equal] at active
      have moved : address bases.first strides.first index ≠ address bases.first strides.first limit :=
        fun eq => different (sameIndex.mp eq)
      unfold indexedPair walkedPair
      have walking : (bases.address strides index).first ≠ address bases.first strides.first limit := moved
      rw [ite_eq_right (fun hit => False.elim (walking hit)), decide_eq_true active]
      simp only [pairedStep, ite_true]
      split
      · simp [Step.related]
      · simp only [Step.related, true_and]
        refine ⟨(CursorPair.addressStep bases strides index).symm,?_⟩
        rw [nextIndex index limit active]
        omega
    · have equal : index = limit := BitVec.eq_of_toNat_eq (by omega)
      simp [indexedPair,walkedPair,pairedStep,CursorPair.address,equal,Step.related]

theorem pairedEntry (bases strides : CursorPair) (limit : Word)
    (positive : 0 < strides.first.toNat)
    (fits : limit.toNat * strides.first.toNat < 2^256)
    (effect : PairedEffect State Event Failure) (fuel : Nat) (state : State) :
    observe (indexedPair bases strides limit effect) fuel ⟨0,bases,state⟩ =
      observe (walkedPair strides (address bases.first strides.first limit) effect)
        fuel ⟨0,bases,state⟩ := by
  apply (pairedSimulation bases strides limit positive fits effect).observe
  simp [pairedSimulation,CursorPair.address,address]

theorem calldataStrideBound (remaining length : Word)
    (admitted : MIR.compare .lt (MIR.shift .shr remaining 5) length = 0) :
    length.toNat * 32 < 2^256 := by
  simp [MIR.compare, MIR.shift, Word.shift, BitVec.ult_iff_toNat_lt,
    BitVec.toNat_ushiftRight] at admitted
  have := remaining.isLt
  omega

end SolarTV.MIR.AffineLoop
