import SolarTV.EVMIR.Transforms.StackSchedule
import SolarTV.State.MemoryLaws

/-! Elide a repeated word store after the last known write to that slot.
Only effects that leave byte memory alone may retain the fact. The
certificate is for funded executions with shared surrounding handlers. -/

namespace SolarTV.EVM.RepeatedStore
open EvmRules SolarTV.Execution StackSchedule

abbrev WordAt := Word × Word

def written (request : Effects.Request) : Option WordAt :=
  if request.opcode = 0x52 ∧ request.returnsWord = false then
    match request.arguments with
    | [offset, value] => some (offset,value)
    | _ => none
  else none

def update (fact : Option WordAt) (request : Effects.Request) : Option WordAt :=
  match written request with
  | some slot =>
    match fact with
    | some old =>
      if slot.1.toNat + 32 ≤ old.1.toNat ∨ old.1.toNat + 32 ≤ slot.1.toNat
      then fact else some slot
    | none => some slot
  | none =>
    if [0x20,0x42,0x51,0x54,0x55].contains request.opcode then fact else none

def redundant (fact : Option WordAt) (request : Effects.Request) : Bool :=
  fact.isSome && fact == written request

def erase : Trace → Option WordAt → Trace
  | .fault, _ => .fault
  | .done exit, _ => .done exit
  | .request request next, fact =>
    if redundant fact request then erase (next 0) fact
    else .request request fun reply => erase (next reply) (update fact request)

@[simp] theorem erase_ite (condition : Prop) [Decidable condition] (yes no : Trace)
    (fact : Option WordAt) :
    erase (if condition then yes else no) fact =
      if condition then erase yes fact else erase no fact := by
  split <;> rfl

def Holds (memory : List Byte) (slot : WordAt) : Prop :=
  slot.1.toNat + 32 ≤ memory.length ∧ readBytes memory slot.1.toNat 32 = wordBytes slot.2

theorem repeated_write (memory : List Byte) (slot : WordAt) (holds : Holds memory slot) :
    writeBytes memory slot.1.toNat (wordBytes slot.2) = memory := by
  apply writeBytes_existing
  simpa using holds.2

theorem first_write (memory : List Byte) (slot : WordAt)
    (active : slot.1.toNat + 32 ≤ memory.length) :
    Holds (writeBytes memory slot.1.toNat (wordBytes slot.2)) slot := by
  constructor
  · simpa [writeBytes_length] using active
  · simpa using readBytes_writeBytes_same memory (wordBytes slot.2) slot.1.toNat (by simpa using active)

theorem disjoint_write (memory : List Byte) (slot other : WordAt)
    (holds : Holds memory slot)
    (separate : other.1.toNat + 32 ≤ slot.1.toNat ∨ slot.1.toNat + 32 ≤ other.1.toNat) :
    Holds (writeBytes memory other.1.toNat (wordBytes other.2)) slot := by
  refine ⟨by simpa using holds.1, ?_⟩
  rw [readBytes_writeBytes_disjoint memory (wordBytes other.2) other.1.toNat slot.1.toNat 32
    (by simpa using separate)]
  exact holds.2

structure Policy {World : Type} (handler : Handler World) where
  memory : World → List Byte
  repeated : ∀ (fact : Option WordAt) request world,
    (∀ slot ∈ fact, Holds (memory world) slot) →
    redundant fact request = true → handler request world = some (0,world)
  establishes : ∀ (fact : Option WordAt) request world reply next,
    (∀ slot ∈ fact, Holds (memory world) slot) →
    handler request world = some (reply,next) →
    ∀ slot ∈ update fact request, Holds (memory next) slot

theorem preserves {World : Type} (handler : Handler World) (policy : Policy handler)
    (tree : Trace) (fact : Option WordAt) (world : World)
    (holds : ∀ slot ∈ fact, Holds (policy.memory world) slot) :
    StackSchedule.execute handler (erase tree fact) world = StackSchedule.execute handler tree world := by
  induction tree generalizing fact world with
  | fault => rfl
  | done exit => rfl
  | request request next ih =>
    cases remove : redundant fact request with
    | true =>
      rw [erase, ite_eq_left (by exact remove), StackSchedule.execute,
        policy.repeated fact request world holds remove]
      exact ih 0 fact world holds
    | false =>
      rw [erase, ite_eq_right (by simp [remove])]
      cases handled : handler request world with
      | none => simp [StackSchedule.execute, handled]
      | some result =>
        obtain ⟨reply, middle⟩ := result
        simpa only [StackSchedule.execute, handled, Bind.bind, Option.bind] using
          ih reply (update fact request) middle (policy.establishes fact request world reply middle holds handled)

def trace (ctx : Context) (fork : Fork) (aborts : Nat → Option (List Effects.Op))
    (ops : List Effects.Op) (stack : Stack) : Trace :=
  erase (StackSchedule.trace ctx fork aborts ops stack) none

theorem contextual {World Halt : Type} (enter : List String → Suffix.PrefixResult World Halt)
    (handler : Handler World) (policy : Policy handler) (fork : Fork) (common : List String)
    (aborts : Nat → Option (List Effects.Op)) (before after : List Effects.Op)
    (supported : Stack → Prop)
    (binding : ∀ ctx stack gas world, enter common = .next ctx stack gas world → supported stack)
    (same : ∀ ctx stack, supported stack →
      Refines (trace ctx fork aborts after stack) (trace ctx fork aborts before stack))
    (value : Exit) (final : World)
    (ran : observe enter handler fork common aborts before = .done value final) :
    observe enter handler fork common aborts after = .done value final := by
  cases entry : enter common with
  | stopped halt => simp [observe, entry] at ran
  | next ctx stack gas world =>
    have fits := binding ctx stack gas world entry
    have unchanged (ops : List Effects.Op) :
        StackSchedule.execute handler (trace ctx fork aborts ops stack) world =
          StackSchedule.execute handler (StackSchedule.trace ctx fork aborts ops stack) world :=
      preserves handler policy _ none world (by simp)
    simp only [observe, entry, ← unchanged] at ran ⊢
    cases run : StackSchedule.execute handler (trace ctx fork aborts before stack) world with
    | none => simp [run] at ran
    | some result =>
      have next := effects handler _ _ (same ctx stack fits) world result run
      simpa only [run, next] using ran

end SolarTV.EVM.RepeatedStore
