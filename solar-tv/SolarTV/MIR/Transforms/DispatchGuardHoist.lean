import SolarTV.MIR.Transforms.RoutedDispatch

/-! A common ABI head-size guard can move from the selected wrapper to the
dispatcher. Bind the removed checks to the original typed instructions and run
those instructions in both wrappers. Success must reach the same cursor with
the same locals and frame, and the original short path must revert empty. -/

namespace SolarTV.MIR.DispatchGuardHoist
open EvmRules State CallContext
open SolarTV.Execution (readBytes)

structure Guard where
  wrapper : Nat
  size : Nat
  condition : Nat
  reject : Nat
  accept : Nat

private def word (value : Nat) : Operand :=
  ⟨.constant ⟨256, BitVec.ofNat 256 value⟩, .word 256⟩

private def localWord (id width : Nat) : Operand := ⟨.local id, .word width⟩

private def instructions (guard : Guard) (minimum : Nat) : List Instruction :=
  [⟨some guard.size, .word 256, some 0x36, .memory .calldataSize, []⟩,
   ⟨some guard.condition, .word 1, some 0x10, .comparison .lt 256,
     [localWord guard.size 256, word minimum]⟩]

private def sourceBlock (guard : Guard) (minimum : Nat) : Block :=
  ⟨[], instructions guard minimum, .branch (localWord guard.condition 1) guard.reject guard.accept⟩

private def loweredBlock (guard : Guard) (minimum : Nat) : Block :=
  {sourceBlock guard minimum with terminator := .jump guard.accept}

private def rejectBlock : Block := ⟨[], [], .reverted (word 0) (word 0)⟩

/-- Check the complete resolved tables. The dispatcher is already present in
both tables so the same function IDs can describe the wrapper continuation. -/
def lower (guards : List Guard) (functions : Functions) : Functions :=
  functions.map fun (id, body) =>
    (id, match guards.find? (·.wrapper == id) with
      | none => body
      | some guard =>
        let blocks := body.blocks.modify 0 (fun block => {block with terminator := .jump guard.accept})
        {body with blocks})

private def wrapperInitial (entry wrapper : Nat) (frame : Frame) : Machine :=
  ⟨{function := wrapper, returnOwner := some entry}, [], {frame with arguments := [], locals := []}⟩

private def enteredAt (entry wrapper index : Nat) (frame : Frame) : Machine :=
  ⟨{function := wrapper, returnOwner := some entry, entered := true, next := index}, [], frame⟩

private def withSize (cfg : Config) (guard : Guard) (frame : Frame) : Frame :=
  {frame with arguments := [], locals := [(guard.size, .scalar ⟨256, BitVec.ofNat 256 cfg.environment.input.length⟩)]}

private def withTest (cfg : Config) (guard : Guard) (frame : Frame) (short : Bool) : Frame :=
  let sized := withSize cfg guard frame
  {sized with locals := put sized.locals guard.condition (.scalar ⟨1, if short then 1 else 0⟩)}

private def atSuccessor (entry wrapper block : Nat) (frame : Frame) : Machine :=
  ⟨{function := wrapper, returnOwner := some entry, block, predecessor := some 0}, [], frame⟩

private theorem evaluate_guard (cfg : Config) (functions : Functions) (entry fuel minimum : Nat)
    (guard : Guard) (frame : Frame) (body : Function) (term : Terminator)
    (lookup : function functions guard.wrapper = .ok body)
    (start : body.blocks[0]? = some ⟨[], instructions guard minimum, term⟩)
    (sizeOp : 0x36 ∈ cfg.available) (ltOp : 0x10 ∈ cfg.available)
    (inputBound : cfg.environment.input.length < 2^256) (minimumBound : minimum < 2^256) :
    Segment cfg functions (fuel + 3) (wrapperInitial entry guard.wrapper frame) fuel
      (enteredAt entry guard.wrapper 2 (withTest cfg guard frame (decide (cfg.environment.input.length < minimum)))) := by
  refine .more (fuel := fuel + 2)
    (next := enteredAt entry guard.wrapper 0 {frame with arguments := [], locals := []}) ?_ (Nat.zero_le _) ?_
  · simp [transition, wrapperInitial, enteredAt, lookup, required, start, enter,
      Bind.bind, Except.bind, Pure.pure, Except.pure]
  refine .more (fuel := fuel + 1) (next := enteredAt entry guard.wrapper 1 (withSize cfg guard frame))
    ?_ (Nat.zero_le _) ?_
  · simp [transition, enteredAt, lookup, State.required, start, instructions, State.next, State.advance,
      instruction, sizeOp, operation, memoryOperation, State.value, assign, assignValue, effect,
      withSize, put, ↓reduceFits, Scalar.ofWord, ValueType.width, Bind.bind, Except.bind, Pure.pure,
      Except.pure]
  refine .more (fuel := fuel) (extra := 0) ?_ (Nat.zero_le _) (.here _ _)
  · by_cases short : cfg.environment.input.length < minimum <;>
      simp [transition, enteredAt, lookup, State.required, start, instructions, State.next, State.advance,
        instruction, ltOp, operation, State.value, assign, assignValue, effect, localWord, word,
        withTest, withSize, State.read, readValue, Value.word, ↓reduceFits, put, MIR.compare,
        Scalar.ofWord, ValueType.width,
        BitVec.ult_eq_decide, Nat.mod_eq_of_lt inputBound, Nat.mod_eq_of_lt minimumBound, short,
        Bind.bind, Except.bind, Pure.pure, Except.pure]

private theorem source_path (cfg : Config) (functions : Functions) (entry fuel minimum : Nat)
    (guard : Guard) (frame : Frame) (body : Function)
    (lookup : function functions guard.wrapper = .ok body)
    (start : body.blocks[0]? = some (sourceBlock guard minimum))
    (sizeOp : 0x36 ∈ cfg.available) (ltOp : 0x10 ∈ cfg.available)
    (inputBound : cfg.environment.input.length < 2^256) (minimumBound : minimum < 2^256) :
    let short := decide (cfg.environment.input.length < minimum)
    Segment cfg functions (fuel + 4) (wrapperInitial entry guard.wrapper frame) fuel
      (atSuccessor entry guard.wrapper (if short then guard.reject else guard.accept)
        (withTest cfg guard frame short)) := by
  have prepared := evaluate_guard cfg functions entry (fuel + 1) minimum guard frame body
    _ lookup start sizeOp ltOp inputBound minimumBound
  apply prepared.append
  refine .more (fuel := fuel) (extra := 0) ?_ (Nat.zero_le _) (.here _ _)
  by_cases short : cfg.environment.input.length < minimum <;>
    simp [transition, enteredAt, lookup, State.required, start, sourceBlock, instructions,
      State.next, State.branch, atSuccessor, withTest, readBits, readValue, ↓reduceFits,
      put, localWord, short, Bind.bind, Except.bind, Pure.pure, Except.pure]

private theorem lowered_path (cfg : Config) (functions : Functions) (entry fuel minimum : Nat)
    (guard : Guard) (frame : Frame) (body : Function)
    (lookup : function functions guard.wrapper = .ok body)
    (start : body.blocks[0]? = some (loweredBlock guard minimum))
    (sizeOp : 0x36 ∈ cfg.available) (ltOp : 0x10 ∈ cfg.available)
    (inputBound : cfg.environment.input.length < 2^256) (minimumBound : minimum < 2^256) :
    Segment cfg functions (fuel + 4) (wrapperInitial entry guard.wrapper frame) fuel
      (atSuccessor entry guard.wrapper guard.accept
        (withTest cfg guard frame (decide (cfg.environment.input.length < minimum)))) := by
  have prepared := evaluate_guard cfg functions entry (fuel + 1) minimum guard frame body
    _ lookup start sizeOp ltOp inputBound minimumBound
  apply prepared.append
  refine .more (fuel := fuel) (extra := 0) ?_ (Nat.zero_le _) (.here _ _)
  simp [transition, enteredAt, lookup, State.required, start, loweredBlock, sourceBlock,
    instructions, State.next, State.branch, atSuccessor, Bind.bind, Except.bind]

private theorem rejects (cfg : Config) (functions : Functions) (entry fuel minimum : Nat)
    (guard : Guard) (frame : Frame) (body : Function)
    (lookup : function functions guard.wrapper = .ok body)
    (start : body.blocks[0]? = some (sourceBlock guard minimum))
    (reject : body.blocks[guard.reject]? = some rejectBlock)
    (sizeOp : 0x36 ∈ cfg.available) (ltOp : 0x10 ∈ cfg.available)
    (inputBound : cfg.environment.input.length < 2^256) (minimumBound : minimum < 2^256)
    (short : cfg.environment.input.length < minimum) (bounded : frame.memory.length ≤ 65536) :
    run (fuel + 6) cfg functions (wrapperInitial entry guard.wrapper frame) =
      .ok (finish (.reverted []) frame) := by
  rw [(source_path cfg functions entry (fuel + 2) minimum guard frame body lookup start
    sizeOp ltOp inputBound minimumBound).resume]
  simp only [short, decide_true, ite_true]
  rw [run_next (fuel + 1) (extra := 0) (nextMachine :=
    ⟨{function := guard.wrapper, returnOwner := some entry, block := guard.reject,
      predecessor := some 0, entered := true}, [], withTest cfg guard frame true⟩)
    (by simp [transition, atSuccessor, lookup, State.required, reject, rejectBlock, State.enter,
      Bind.bind, Except.bind, Pure.pure, Except.pure])]
  simp only [Nat.not_lt_zero, ite_false, Nat.sub_zero]
  apply run_finished
  simp [transition, lookup, State.required, reject, rejectBlock, State.read, readValue, Value.word,
    word, ↓reduceFits, RoutedDispatch.expand_zero _ bounded, withTest, withSize, readBytes,
    State.stop, State.finish, Bind.bind, Except.bind, Pure.pure, Except.pure]

/-- Independently resolved shapes for one original wrapper and its lowering. -/
def Matches (source target : Functions) (minimum : Nat) (guard : Guard) : Prop :=
  ∃ old new,
    function source guard.wrapper = .ok old ∧
    function target guard.wrapper = .ok new ∧
    old.blocks[0]? = some (sourceBlock guard minimum) ∧
    old.blocks[guard.reject]? = some rejectBlock ∧
    new.blocks[0]? = some (loweredBlock guard minimum)

private def CommonContinuation (cfg : Config) (source target : Functions) (entry wrapper : Nat)
    (frame : Frame) : Prop :=
  ∀ fuel, ∃ reached,
    Segment cfg source (fuel + 4) (wrapperInitial entry wrapper frame) fuel reached ∧
    Segment cfg target (fuel + 4) (wrapperInitial entry wrapper frame) fuel reached

/-- Compare the actually executed target dispatcher with implicit routing
followed by the original wrapper. A selected wrapper must either reject with
the same result or reach the exact same continuation on both sides. -/
def Agrees (cfg : Config) (source target : Functions) (entry : Nat) (frame : Frame)
    (original : Option Nat) (actual : Except String Cut) : Prop :=
  match original, actual with
  | none, .ok (.inl result) => result = finish (.reverted []) frame
  | some wrapper, .ok (.inl result) =>
    ∀ fuel, run (fuel + 6) cfg source (wrapperInitial entry wrapper frame) = .ok result
  | some wrapper, .ok (.inr (_, machine)) =>
    machine = wrapperInitial entry wrapper frame ∧ CommonContinuation cfg source target entry wrapper frame
  | _, _ => False

private theorem Agrees.of_routes (cfg : Config) (source target : Functions) (entry minimum : Nat)
    (frame : Frame) (original : Option Nat) (actual : Except String Cut)
    (routing : RoutedDispatch.Agrees entry frame
      (if cfg.environment.input.length < minimum then none else original) actual)
    (reject : ∀ wrapper, original = some wrapper → cfg.environment.input.length < minimum →
      ∀ fuel, run (fuel + 6) cfg source (wrapperInitial entry wrapper frame) =
        .ok (finish (.reverted []) frame))
    (resume : ∀ wrapper, original = some wrapper → minimum ≤ cfg.environment.input.length →
      CommonContinuation cfg source target entry wrapper frame) :
    Agrees cfg source target entry frame original actual := by
  cases original with
  | none =>
    simp only [ite_self] at routing
    cases actual with
    | error => exact routing
    | ok cut => cases cut <;> exact routing
  | some wrapper =>
    by_cases short : cfg.environment.input.length < minimum
    · simp only [short, ite_true] at routing
      cases actual with
      | error => exact routing.elim
      | ok cut =>
        cases cut with
        | inl result =>
          change result = finish (.reverted []) frame at routing
          rw [routing]
          exact reject wrapper rfl short
        | inr _ => exact routing.elim
    · simp only [short, ite_false] at routing
      cases actual with
      | error => exact routing
      | ok cut =>
        cases cut with
        | inl _ => exact routing.elim
        | inr state =>
          obtain ⟨_, ⟨_, _, _⟩⟩ := state
          obtain ⟨cursor, stack, localFrame⟩ := routing
          refine ⟨?_, resume wrapper rfl (Nat.le_of_not_gt short)⟩
          simpa only [wrapperInitial, Machine.mk.injEq] using And.intro cursor (And.intro stack localFrame)

/-- The runtime dispatch cut and the source guard proofs share the same
calldata and frame. The source or target never invents a value for a local. -/
theorem certificate (cfg : Config) (source target : Functions) (entry minimum : Nat)
    (frame : Frame) (original : Option Nat) (actual : Except String Cut)
    (routing : RoutedDispatch.Agrees entry frame
      (if cfg.environment.input.length < minimum then none else original) actual)
    (wrappers : ∀ id, original = some id →
      ∃ guard, guard.wrapper = id ∧ Matches source target minimum guard)
    (sizeOp : 0x36 ∈ cfg.available) (ltOp : 0x10 ∈ cfg.available)
    (inputBound : cfg.environment.input.length < 2^256) (minimumBound : minimum < 2^256)
    (bounded : frame.memory.length ≤ 65536) :
    Agrees cfg source target entry frame original actual := by
  refine Agrees.of_routes cfg source target entry minimum frame original actual routing ?_ ?_
  · intro wrapper selected short fuel
    obtain ⟨guard, rfl, old, _, found, _, head, reject, _⟩ := wrappers wrapper selected
    exact rejects cfg source entry fuel minimum guard frame old found head reject
      sizeOp ltOp inputBound minimumBound short bounded
  · intro wrapper selected long fuel
    obtain ⟨guard, rfl, old, new, left, right, before, _, after⟩ := wrappers wrapper selected
    refine ⟨atSuccessor entry guard.wrapper guard.accept (withTest cfg guard frame false), ?_, ?_⟩
    · simpa [Nat.not_lt_of_ge long] using source_path cfg source entry fuel minimum guard frame
        old left before sizeOp ltOp inputBound minimumBound
    · simpa [Nat.not_lt_of_ge long] using lowered_path cfg target entry fuel minimum guard frame
        new right after sizeOp ltOp inputBound minimumBound

end SolarTV.MIR.DispatchGuardHoist
