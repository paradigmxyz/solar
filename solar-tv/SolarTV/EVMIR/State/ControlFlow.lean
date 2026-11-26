import SolarTV.EVMIR.State.StackMotion

/-! The bytecode interpreter's program-counter effects. Child execution may
change the world and return data. Its parent's continuation PC is fixed by
the CALL or CREATE instruction before the child runs. -/

namespace SolarTV.EVM.State
open EvmRules

def Step.continuation : Step → Option Frame
  | .next _ frame | .call _ _ _ _ _ _ frame | .create _ _ _ _ frame => some frame
  | .done _ => none

def Step.resumedStack (signal : Step) (stack : EVM.Stack) : Prop :=
  match signal with
  | .next _ frame => stack = frame.stack
  | .call _ _ _ _ _ _ frame | .create _ _ _ _ frame => ∃ reply, stack = reply :: frame.stack
  | .done _ => False

/-- The immediate pushed by an instruction, with PUSH0 handled separately
from the ordinary PUSH payload width. -/
def pushedWord (opcode : Nat) (immediate : Nat → Word) : Option Word :=
  if opcode = 0x5f then some 0
  else if 0x60 ≤ opcode ∧ opcode ≤ 0x7f then some (immediate (opcode - 0x5f))
  else none

def stopsControl (opcode : Nat) : Bool :=
  [0x00, 0xf3, 0xfd, 0xfe, 0xff].contains opcode

private def pureInputs : EVM.Op → Nat
  | .not | .iszero | .clz => 1
  | .addmod | .mulmod => 3
  | _ => 2

private def pureMotion : EVM.Op → StackMotion
  | .add => .binary Evm.add
  | .sub => .binary Evm.sub
  | op => .replace (pureInputs op) 1

private theorem pure_motion (opcode : Nat) (op : EVM.Op) (ctx : EVM.Context)
    (before after : EVM.Stack) (found : pureOp opcode = some op)
    (ran : EVM.step ctx op before = some after) :
    (pureMotion op).realizes before after := by
  unfold pureOp at found
  split at found <;> try contradiction
  all_goals
    cases found
    rcases before with _ | ⟨a,_ | ⟨b,_ | ⟨c,rest⟩⟩⟩ <;>
      simp_all [EVM.step, pureInputs, pureMotion]
  all_goals
    cases ran
    first | exact ⟨_,_,_,rfl,rfl⟩ | exact ⟨_,rfl⟩

private def noResult (opcode : Nat) : Bool :=
  [0x3c,0x37,0x39,0x3e,0x50,0x52,0x53,0x55,0x5d,0x56,0x57,0x5b,0x5e].contains opcode ||
    (0xa0 ≤ opcode && opcode ≤ 0xa4)

/-- Literals, PC, and arithmetic on known words can provide an address.
The ordinary opcode cases retain the untouched suffix under the operands. -/
def stackMotion (cfg : Config) (pc opcode : Nat) (immediate : Nat → Word) : StackMotion :=
  if let some word := pushedWord opcode immediate then
    .push word (if opcode = 0x5f then (available cfg opcode).inputs else 0)
  else if let some op := pureOp opcode then pureMotion op
  else if range : 0x80 ≤ opcode ∧ opcode ≤ 0x9f then
    if opcode < 0x90 then .dup ⟨opcode - 0x80, by omega⟩
    else .swap ⟨opcode - 0x90, by omega⟩
  else if opcode = 0x58 then .push (BitVec.ofNat 256 pc) (available cfg opcode).inputs
  else .replace (available cfg opcode).inputs (if noResult opcode then 0 else 1)

/-- Every potentially resuming instruction has one of three kinds of PC
effect. The finite gas, stack, and world checks only remove transitions
from this projection. -/
def ControlEffect (frame after : Frame) (opcode inputs : Nat)
    (immediate : Nat → Word)
    (jump : Word → Bool) : Prop :=
  if (pushedWord opcode immediate).isSome then
    after.pc = frame.pc + 1 + pushWidth opcode
  else if opcode = 0x56 ∨ opcode = 0x57 then
    let address := (frame.stack.take inputs)[0]?.getD 0
    (jump address = true ∧ after.pc = address.toNat) ∨
      (opcode = 0x57 ∧ after.pc = frame.pc + 1)
  else
    stopsControl opcode = false ∧ after.pc = frame.pc + 1

def InstructionEffect (cfg : Config) (frame after : Frame) (signal : Step)
    (opcode : Nat) (immediate : Nat → Word) (jump : Word → Bool) : Prop :=
  ControlEffect frame after opcode (available cfg opcode).inputs immediate jump ∧
    ∀ stack, signal.resumedStack stack →
      (stackMotion cfg frame.pc opcode immediate).realizes frame.stack stack

private def ResumesWith (action : Action) (property : Step → Frame → Prop) : Prop :=
  ∀ signal after, action = .ok signal → signal.continuation = some after → property signal after

private theorem resumesWith_next (world : Journal) (frame : Frame) (stack : List Word)
    (property : Step → Frame → Prop) (holds : property (.next world {frame with pc := frame.pc + 1, stack})
      {frame with pc := frame.pc + 1, stack}) :
    ResumesWith (next world frame stack) property := by
  intro signal after action follows
  cases action
  cases follows
  exact holds

private theorem resumesWith_failed (world : Journal) (property : Step → Frame → Prop) :
    ResumesWith (stepFailed world) property := by
  intro signal after action follows
  cases action
  contradiction

private theorem resumesWith_exhausted (world : Journal) (property : Step → Frame → Prop) :
    ResumesWith (stepExhausted world) property := by
  intro signal after action follows
  cases action
  contradiction

private theorem resumesWith_memory (world : Journal) (frame : Frame)
    (charge offset count : Nat) (continuation : Frame → Action) (property : Step → Frame → Prop)
    (holds : ∀ memory, ResumesWith
      (continuation {frame with
        memory := memory
        gas := frame.gas - (charge + memoryCharge frame.memory offset count)}) property) :
    ResumesWith (withMemory world frame charge offset count continuation) property := by
  simp only [withMemory]
  split
  · exact resumesWith_exhausted _ _
  · cases expand frame.memory offset count with
    | error => simp [ResumesWith, Bind.bind, Except.bind]
    | ok memory => exact holds memory

private theorem resumesWith_ok (signal : Step) (property : Step → Frame → Prop)
    (holds : ∀ frame, signal.continuation = some frame → property signal frame) :
    ResumesWith (.ok signal) property := by
  intro _ after action follows
  cases action
  exact holds after follows

private theorem resumesWith_error (message : String) (property : Step → Frame → Prop) :
    ResumesWith (.error message) property := by
  intro signal after action
  contradiction

private theorem resumesWith_bind {α : Type} (value : Except String α)
    (continuation : α → Action) (property : Step → Frame → Prop)
    (continues : ∀ result, ResumesWith (continuation result) property) :
    ResumesWith (value >>= continuation) property := by
  cases value with
  | error message => exact resumesWith_error message property
  | ok result => exact continues result

set_option maxHeartbeats 4000000 in
set_option maxRecDepth 4096 in
theorem instruction_control (cfg : Config) (world : Journal) (frame : Frame)
    (codeLength : Nat) (byte : Byte) (immediate : Nat → Word) (jump : Word → Bool)
    (signal : Step) (after : Frame)
    (action : instructionWithImage cfg world frame codeLength byte immediate jump = .ok signal)
    (follows : signal.continuation = some after) :
    InstructionEffect cfg frame after signal byte.toNat immediate jump := by
  refine (show ResumesWith (instructionWithImage cfg world frame codeLength byte immediate jump)
    (fun signal after => InstructionEffect cfg frame after signal byte.toNat immediate jump)
      from ?_) _ _ action follows
  clear action follows signal after
  simp only [instructionWithImage]
  by_cases pastEnd : codeLength ≤ frame.pc
  · rw [ite_eq_left pastEnd]
    exact resumesWith_ok _ _ (by intro _ impossible; contradiction)
  rw [ite_eq_right pastEnd]
  by_cases unavailable : (!(available cfg byte.toNat).available) = true
  · rw [ite_eq_left unavailable]
    exact resumesWith_failed _ _
  rw [ite_eq_right unavailable]
  by_cases unsupported : (cfg.fork.atLeast .amsterdam &&
      ([0x31,0x3b,0x3c,0x3f,0x47,0x54,0x55,0x5c,0x5d,0xf0,0xf1,0xf2,0xf4,0xf5,0xfa,0xff].contains byte.toNat ||
        (0xa0 ≤ byte.toNat && byte.toNat ≤ 0xa4))) = true
  · rw [ite_eq_left unsupported]
    exact resumesWith_error _ _
  rw [ite_eq_right unsupported]
  by_cases stackFault : (decide (frame.stack.length < (available cfg byte.toNat).inputs) ||
      decide (frame.stack.length - (available cfg byte.toNat).inputs +
        (available cfg byte.toNat).outputs > 1024)) = true
  · rw [ite_eq_left stackFault]
    exact resumesWith_failed _ _
  rw [ite_eq_right stackFault]
  clear pastEnd unavailable unsupported stackFault
  cases pure : pureOp byte.toNat
  case none =>
    dsimp only
    by_cases pushRange : 0x60 ≤ byte.toNat ∧ byte.toNat ≤ 0x7f
    · rw [ite_eq_left pushRange]
      split
      · exact resumesWith_exhausted _ _
      · apply resumesWith_ok
        intro result follows
        cases follows
        have notZero : byte.toNat ≠ 0x5f := by omega
        simp [InstructionEffect, ControlEffect, pushedWord, pushWidth, stackMotion,
          StackMotion.realizes, Step.resumedStack, notZero, pushRange]
    rw [ite_eq_right pushRange]
    by_cases stackRange : 0x80 ≤ byte.toNat ∧ byte.toNat ≤ 0x9f
    · rw [ite_eq_left stackRange]
      have range : 0x80 ≤ byte.toNat ∧ byte.toNat ≤ 0x9f := stackRange
      repeat' first
        | apply resumesWith_failed
        | apply resumesWith_exhausted
        | apply resumesWith_next
        | change ResumesWith _ _; split <;> try dsimp only
      all_goals
        have h₁ : byte.toNat ≠ 0x5f := by omega
        have h₂ : ¬ (byte.toNat = 0x56 ∨ byte.toNat = 0x57) := by omega
        have h₃ : stopsControl byte.toNat = false := by
          simp [stopsControl]
          omega
        simp [InstructionEffect, ControlEffect, pushedWord, h₁, h₂, h₃, pushRange, stackRange,
          stackMotion, pure, Step.resumedStack]
        split <;> simp_all [StackMotion.realizes]
    rw [ite_eq_right stackRange]
    generalize eq : byte.toNat = opcode at *
    repeat' first
      | apply resumesWith_failed
      | apply resumesWith_exhausted
      | apply resumesWith_error
      | apply resumesWith_next
      | apply resumesWith_memory; intro memory
      | apply resumesWith_bind; intro result
      | apply resumesWith_ok; intro result follows; cases follows
      | change ResumesWith _ _; split <;> try dsimp only
    all_goals try (simp [InstructionEffect, ControlEffect, pushedWord, pushWidth, stopsControl])
    all_goals try (simp_all (config := { maxSteps := 1000 }) only [InstructionEffect, ControlEffect, pushedWord,
      pushWidth, stopsControl, Nat.reduceEq, Nat.reduceLe, if_true, if_false, List.head?_cons,
      Bool.false_eq_true, decide_true, decide_false, Bool.or_false, Bool.false_or,
      Bool.not_eq_true, Bool.not_eq_false, Bool.false_and,
      List.contains_cons, List.contains_nil, beq_iff_eq, List.getD_cons_zero,
      List.getElem?_zero])
    all_goals try exact ⟨by assumption, rfl⟩
    all_goals try exact Or.inl ⟨by assumption, rfl⟩
    all_goals try
      have notZero : opcode ≠ 0x5f := by omega
      have notJump : ¬(opcode = 0x56 ∨ opcode = 0x57) := by omega
      simp [notZero, pushRange, notJump] <;> omega
    all_goals simp_all [Step.resumedStack, stackMotion, pushedWord, noResult]
    all_goals
      have noPermutation : ¬(0x80 ≤ opcode ∧ opcode ≤ 0x9f) := by omega
      simp [noPermutation] <;> omega
  case some op =>
    dsimp only
    have small : 1 ≤ byte.toNat ∧ byte.toNat ≤ 0x1e := by
      unfold pureOp at pure
      split at pure <;> first | contradiction | omega
    repeat' first
      | apply resumesWith_failed
      | apply resumesWith_exhausted
      | apply resumesWith_next
      | change ResumesWith _ _; split <;> try dsimp only
    all_goals
      have h₁ : byte.toNat ≠ 0x5f := by omega
      have h₂ : ¬ (0x60 ≤ byte.toNat ∧ byte.toNat ≤ 0x7f) := by omega
      have h₃ : ¬ (byte.toNat = 0x56 ∨ byte.toNat = 0x57) := by omega
      have h₄ : stopsControl byte.toNat = false := by
        simp [stopsControl]
        omega
      simp [InstructionEffect, ControlEffect, pushedWord, h₁, h₂, h₃, h₄, pure,
        stackMotion, Step.resumedStack]
      exact pure_motion _ _ _ _ _ pure (by assumption)

/-- Resolving a child call or creation keeps the parent's continuation PC.
Child execution supplies one reply to the caller's remaining stack. -/
theorem advance_control (cfg : Config) (execute : ChildExecutor) (signal : Step)
    (planned resumed : Frame) (cost : Nat) (world : Journal)
    (continues : signal.continuation = some planned)
    (advanced : advance cfg execute signal = .ok (.inr (cost,world,resumed))) :
    resumed.pc = planned.pc ∧ signal.resumedStack resumed.stack := by
  cases signal <;> simp only [Step.continuation] at continues
  case done => contradiction
  case next =>
    cases continues
    cases advanced
    exact ⟨rfl,rfl⟩
  all_goals
    cases continues
    simp only [advance, Bind.bind, Except.bind, Pure.pure, Except.pure] at advanced
    repeat' first
      | contradiction
      | injection advanced with result; contradiction
      | injection advanced with result; injection result with pair; cases pair
        exact ⟨rfl,⟨_,rfl⟩⟩
      | split at advanced

end SolarTV.EVM.State
