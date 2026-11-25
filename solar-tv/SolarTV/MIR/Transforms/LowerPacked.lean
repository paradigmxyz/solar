import SolarTV.MIR.State.Ops

/-! The packed signing envelope writes a length word and three overlapping
payload words. Relate the semantic packed object to the explicit allocation
and stores, including active memory, dirty padding and the free-memory word. -/

namespace SolarTV.MIR.LowerPacked
open EvmRules SolarTV.Execution MIR.State

def signingParts : List PackedInput :=
  [.literal [0x19,0x01],.scalar 0 32 true,.scalar 1 32 true]

def signingPrefix : Word := 0x1901 <<< 240

private theorem readWord_wordBytes (word : Word) : readWord (wordBytes word) 0 = word := by
  have all (bytes : List Byte) : readBytes bytes 0 bytes.length = bytes := by
    apply List.ext_getElem
    · simp [readBytes]
    · intro i hi hj
      simp [readBytes, List.getElem?_eq_getElem hj]
  unfold readWord
  rw [← wordBytes_length word, all, ← BitVec.flattenList_eq_flattenListFast]
  exact flatten_wordBytes word

private theorem signingPlan (domain digest : Word) :
    (do
      let pieces ← packedPieces signingParts [domain,digest]
      pure (packedStorePlan pieces)) =
      .ok ⟨66,66,[(34,digest),(2,domain),(0,signingPrefix)]⟩ := by
  have nonempty (word : Word) : wordBytes word ≠ [] := by
    intro h
    have := wordBytes_length word
    simp [h] at this
  have whole (word : Word) : (wordBytes word).take 32 = wordBytes word :=
    List.take_of_length_le (by simp)
  have marker : readWord [0x19,0x01] 0 = signingPrefix := by
    unfold readWord signingPrefix
    rw [← BitVec.flattenList_eq_flattenListFast]
    decide
  simp [signingParts, packedPieces, packedStorePlan, packedFlush, packedEmit, required, nonempty,
    whole, readWord_wordBytes, Except.bind, Bind.bind, Pure.pure, Except.pure]
  exact marker

/-- A full-word store at each of the four offsets in the explicit lowering. -/
def store (memory : List Byte) (address word : Word) : Except String (List Byte) := do
  let memory ← expand memory address.toNat 32
  pure (writeBytes memory address.toNat (wordBytes word))

def loweredSigningEnvelope (frame : Frame) (domain digest : Word) : Step := do
  match ← allocate frame 128 false false with
  | .returned (some base,frame) =>
    let memory ← store frame.memory base 66
    let memory ← store memory (base+32) signingPrefix
    let memory ← store memory (base+34) domain
    let memory ← store memory (base+66) digest
    return .returned (some base,{frame with memory})
  | _ => .error "packed allocation precondition"

theorem signingEnvelope (frame : Frame) (domain digest : Word) :
    abiPacked frame signingParts false [domain,digest] =
      loweredSigningEnvelope frame domain digest := by
  have plan := signingPlan domain digest
  cases pieces : packedPieces signingParts [domain,digest] with
  | error _ =>
    simp [pieces, Except.bind, Bind.bind] at plan
  | ok piecesValue =>
    simp only [pieces, Except.bind, Bind.bind, Pure.pure, Except.pure, Except.ok.injEq] at plan
    simp only [abiPacked, pieces, plan, Bool.false_eq_true, ite_false,
      Except.bind, Bind.bind, Pure.pure, Except.pure, loweredSigningEnvelope]
    cases alloc : allocate frame 128 false false with
    | error _ => rfl
    | ok result =>
      cases result with
      | invalid | reverted _ => rfl
      | returned pair =>
        obtain ⟨pointer,allocated⟩ := pair
        cases pointer with
        | none => rfl
        | some base =>
          simp only [store, Except.bind, Bind.bind, Pure.pure, Except.pure]
          have off0 : base + (32 : Word) + BitVec.ofNat 256 0 = base + 32 := by simp
          have off2 : base + (32 : Word) + BitVec.ofNat 256 2 = base + 34 := by
            rw [BitVec.add_assoc]
            rfl
          have off34 : base + (32 : Word) + BitVec.ofNat 256 34 = base + 66 := by
            rw [BitVec.add_assoc]
            rfl
          simp only [List.reverse_cons, List.reverse_nil, List.nil_append, List.cons_append,
            List.forIn_cons, List.forIn_nil, off0, off2, off34,
            Except.bind, Bind.bind, Pure.pure, Except.pure]
          cases expand allocated.memory base.toNat 32 with
          | error _ => rfl
          | ok header =>
            simp only []
            cases expand (writeBytes header base.toNat (wordBytes 66)) (base+32).toNat 32 with
            | error _ => rfl
            | ok first =>
              simp only []
              cases expand (writeBytes first (base+32).toNat (wordBytes signingPrefix))
                  (base+34).toNat 32 with
              | error _ => rfl
              | ok second =>
                simp only []
                cases expand (writeBytes second (base+34).toNat (wordBytes domain))
                    (base+66).toNat 32 <;> rfl

end SolarTV.MIR.LowerPacked
