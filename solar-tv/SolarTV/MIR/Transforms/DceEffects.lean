import SolarTV.MIR.State.Interpreter
import SolarTV.State.MemoryLaws

/-! Local effect laws for dead MIR definitions and empty direct calls. -/
namespace SolarTV.MIR.State.DceEffects
open EvmRules SolarTV.Execution

theorem fixed_calldata_word (cfg : Config) (frame : Frame) (offset : Word) :
    memoryOperation cfg frame .calldataLoad [offset] =
      value frame (readWord cfg.environment.input offset.toNat) := rfl

theorem fixed_calldata_size (cfg : Config) (frame : Frame) :
    memoryOperation cfg frame .calldataSize [] =
      value frame (BitVec.ofNat 256 cfg.environment.input.length) := rfl

theorem insert_undefined (ty : ValueType) (fields : List ValueType) (index : Nat)
    (element : ValueType) (word : Value)
    (shape : ty.fields = some fields) (field : fields[index]? = some element) :
    insertField ty .undefined index word =
      .ok (.aggregate ((List.replicate fields.length Value.undefined).set index word)) := by
  simp [insertField, required, shape, field, Bind.bind, Except.bind, Pure.pure, Except.pure]

theorem insert_existing (ty : ValueType) (fields : List ValueType) (index : Nat)
    (element : ValueType) (word : Value) (previous : List Value)
    (shape : ty.fields = some fields) (field : fields[index]? = some element)
    (arity : previous.length = fields.length) :
    insertField ty (.aggregate previous) index word =
      .ok (.aggregate (previous.set index word)) := by
  simp [insertField, required, shape, field, arity, Bind.bind, Except.bind, Pure.pure, Except.pure]

/-- An empty direct callee enters and returns to the exact saved caller frame. -/
theorem empty_call (cfg : Config) (functions : Functions) (id : Nat)
    (params : List ValueType) (name : Option String) (args : List Value)
    (cursor : Cursor) (stack : List Continuation) (frame : Frame) (ty : ValueType)
    (target : function functions id = .ok ⟨params,[],[⟨[],[],.returned []⟩],name⟩)
    (typed : typedWords params args = .ok ()) :
    (do
      let child ← start functions id args frame
      let .next 0 entered ← transition 0 cfg functions
        ⟨{function := id},⟨cursor,frame,none,ty⟩ :: stack,child⟩
        | .error "empty callee entry"
      transition 0 cfg functions entered) = next cursor stack frame := by
  unfold start
  simp only [target, typed, Bind.bind, Except.bind, Pure.pure, Except.pure]
  simp [transition, target, required, enter, typedWords, publishReturns,
    Bind.bind, Except.bind, Pure.pure, Except.pure]

theorem expanded_size (memory grown : List Byte) (offset count : Nat)
    (aligned : memory.length % 32 = 0)
    (expanded : expand memory offset count = .ok grown) :
    grown.length = 32 * requiredWords memory offset count ∧ grown.length ≤ 65536 := by
  simp only [expand] at expanded
  split at expanded
  · contradiction
  · rename_i bounded
    cases expanded
    simp only [List.length_append, List.length_replicate]
    have words : memoryWords memory ≤ requiredWords memory offset count := by
      unfold requiredWords
      split
      · exact Nat.le_refl _
      · exact Nat.le_max_left _ _
    unfold memoryWords at words
    omega

/-- On a continuing CALL edge its complete output interval is active, even if
the child supplied fewer bytes or returned an unsuccessful status. -/
theorem call_output (fuel : Nat) (cfg : Config) (frame parent : Frame)
    (destination amount inputOffset inputSize : Word) (gas value : Option Word) (work : Nat)
    (aligned : frame.memory.length % 32 = 0)
    (returned : external fuel cfg frame 0xf1 [destination, amount, inputOffset, inputSize, 0, 32] gas =
      .ok (some (.returned (value,parent),work))) :
    parent.memory.length % 32 = 0 ∧ parent.memory.length ≤ 65536 ∧ 32 ≤ parent.memory.length := by
  simp only [external, Bind.bind, Except.bind, Pure.pure, Except.pure] at returned
  split at returned <;> try contradiction
  split at returned <;> try contradiction
  simp at returned
  split at returned
  · cases returned
  split at returned <;> try contradiction
  split at returned <;> try contradiction
  split at returned <;> try contradiction
  rename_i firstMemory first
  split at returned <;> try contradiction
  rename_i outputMemory output
  split at returned <;> try contradiction
  split at returned
  · split at returned <;> try contradiction
    injection returned with equal
    simp only [Option.some.injEq, Prod.mk.injEq, Outcome.returned.injEq] at equal
    rcases equal with ⟨⟨_,rfl⟩,_⟩
    have ⟨firstSize,_⟩ := expanded_size _ _ _ _ aligned first
    have firstAligned : firstMemory.length % 32 = 0 := by omega
    have ⟨outputSize,bounded⟩ := expanded_size _ _ _ _ firstAligned output
    simp [requiredWords] at outputSize
    simp only [writeBytes_length]
    have words : 1 ≤ max (memoryWords firstMemory) 1 := Nat.le_max_right _ _
    exact ⟨by omega, bounded, by omega⟩
  · cases returned

theorem covered_word_read (cfg : Config) (frame : Frame) (offset : Word)
    (aligned : frame.memory.length % 32 = 0) (bounded : frame.memory.length ≤ 65536)
    (covers : offset.toNat + 32 ≤ frame.memory.length) :
    memoryOperation cfg frame .load [offset] =
      value frame (readWord frame.memory offset.toNat) := by
  have rounded : 32 * (frame.memory.length / 32) = frame.memory.length := by omega
  have fits : (offset.toNat + 32 + 31) / 32 ≤ frame.memory.length / 32 := by omega
  simp only [memoryOperation, expand, requiredWords, memoryWords, Nat.reduceEqDiff, ite_false,
    Nat.max_eq_left fits, rounded, Nat.not_lt.mpr bounded, Nat.sub_self,
    List.replicate_zero, List.append_nil, Except.bind, Bind.bind]

end SolarTV.MIR.State.DceEffects
