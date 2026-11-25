import SolarTV.EVMIR.State.Interpreter

/-! Stack effects for physical control-flow proofs. A replacement retains the
suffix below the consumed operands. Its outputs carry no facts. PUSH, DUP
and SWAP retain exact words so a continuation can be moved around before JUMP. -/

namespace SolarTV.EVM.State
open EvmRules

inductive StackMotion where
  | push (word : Word) (inputs : Nat)
  | dup (depth : Fin 256)
  | swap (depth : Fin 256)
  | replace (inputs outputs : Nat)

def StackMotion.realizes (motion : StackMotion) (before after : EVM.Stack) : Prop :=
  match motion with
  | .push word inputs => after = word :: before.drop inputs
  | .dup depth => EVM.step ⟨0,0,0,[],0⟩ (.dup depth) before = some after
  | .swap depth => EVM.step ⟨0,0,0,[],0⟩ (.swap depth) before = some after
  | .replace inputs outputs =>
    ∃ words : EVM.Stack, words.length = outputs ∧ after = words ++ before.drop inputs

@[simp] theorem StackMotion.push_realizes (value : Word) (inputs : Nat) (before after : EVM.Stack) :
    (.push value inputs : StackMotion).realizes before after ↔
      after = value :: before.drop inputs :=
  Iff.rfl

@[simp] theorem StackMotion.replace_zero (inputs : Nat) (before after : EVM.Stack) :
    (.replace inputs 0 : StackMotion).realizes before after ↔ after = before.drop inputs := by
  simp [StackMotion.realizes]

@[simp] theorem StackMotion.replace_one (inputs : Nat) (before after : EVM.Stack) :
    (.replace inputs 1 : StackMotion).realizes before after ↔
      ∃ word, after = word :: before.drop inputs := by
  constructor
  · rintro ⟨words,length,rfl⟩
    obtain ⟨word,rfl⟩ := List.length_eq_one_iff.mp length
    exact ⟨word,rfl⟩
  · rintro ⟨word,rfl⟩
    exact ⟨[word],rfl,rfl⟩

abbrev KnownStack := List (Option Word)

def KnownStack.word (words : KnownStack) (depth : Nat) : Option Word :=
  words[depth]?.join

def KnownStack.matches (words : KnownStack) (stack : EVM.Stack) : Prop :=
  ∀ depth word, words.word depth = some word → stack[depth]? = some word

theorem KnownStack.empty_matches (stack : EVM.Stack) : KnownStack.matches [] stack := by
  simp [KnownStack.matches, KnownStack.word]

private def swappedIndex (index depth : Nat) : Nat :=
  if index = 0 then depth else if index = depth then 0 else index

private def StackMotion.move (motion : StackMotion) (words : KnownStack) : KnownStack :=
  match motion with
  | .push word inputs => some word :: words.drop inputs
  | .dup depth => words.word depth.val :: words
  | .swap depth =>
    (List.range (max words.length (depth.val+2))).map
      (fun index => words.word (swappedIndex index (depth.val+1)))
  | .replace inputs outputs => List.replicate outputs none ++ words.drop inputs

/-- A wider estimated stack is sound when an instruction succeeds. The
untracked suffix carries no facts, including about its length. -/
def StackMotion.carry (motion : StackMotion) (words : KnownStack) : KnownStack :=
  (motion.move words).take 32

private theorem moved (motion : StackMotion) (words : KnownStack) (before after : EVM.Stack)
    (related : words.matches before) (realized : motion.realizes before after) :
    (motion.move words).matches after := by
  intro index value known
  cases motion with
  | push word inputs =>
    subst after
    cases index with
    | zero => simpa [StackMotion.move, KnownStack.word] using known
    | succ index =>
      simpa [List.getElem?_drop] using
        related (inputs+index) value (by
          simpa [StackMotion.move, KnownStack.word, List.getElem?_drop] using known)
  | dup depth =>
    simp only [StackMotion.realizes, EVM.step] at realized
    cases copied : before[depth.val]? with
    | none => simp [copied] at realized
    | some word =>
      simp only [copied, Option.map_some, Option.some.injEq] at realized
      subst after
      cases index with
      | zero =>
        have source := related depth.val value (by
          simpa [StackMotion.move, KnownStack.word] using known)
        simpa [copied] using source
      | succ index =>
        apply related
        simpa [StackMotion.move, KnownStack.word] using known
  | swap depth =>
    cases before with
    | nil => simp [StackMotion.realizes, EVM.step] at realized
    | cons top tail =>
      simp only [StackMotion.realizes, EVM.step] at realized
      cases distant : tail[depth.val]? with
      | none => simp [distant] at realized
      | some word =>
        simp only [distant, Option.map_some, Option.some.injEq] at realized
        subst after
        have above : index < max words.length (depth.val+2) := by
          by_cases fits : index < max words.length (depth.val+2)
          · exact fits
          · have absent : (List.range (max words.length (depth.val+2)))[index]? = none := by
              apply List.getElem?_eq_none_iff.mpr
              simpa using Nat.le_of_not_lt fits
            simp [StackMotion.move, KnownStack.word, absent] at known
        have atValue : words.word (swappedIndex index (depth.val+1)) = some value := by
          simpa [StackMotion.move, KnownStack.word, List.getElem?_range above] using known
        have source := related _ _ atValue
        cases index with
        | zero =>
          simpa [swappedIndex, distant] using source
        | succ index =>
          by_cases atDepth : index = depth.val
          · subst index
            have fits : depth.val < tail.length := (List.getElem?_eq_some_iff.mp distant).1
            simpa [swappedIndex, List.getElem?_set_self fits] using source
          · have outside : index + 1 ≠ depth.val + 1 := by omega
            rw [List.getElem?_cons_succ, List.getElem?_set_ne (Ne.symm atDepth)]
            simpa [swappedIndex, outside, atDepth] using source
  | replace inputs outputs =>
    obtain ⟨result,length,rfl⟩ := realized
    simp only [StackMotion.move, KnownStack.word, List.getElem?_append,
      List.length_replicate] at known
    by_cases inside : index < outputs
    · simp [inside] at known
    · simp [inside, List.getElem?_drop] at known
      have kept := related (inputs + (index-outputs)) value known
      simpa [List.getElem?_append, length, inside, List.getElem?_drop] using kept

theorem StackMotion.carry_matches (motion : StackMotion) (words : KnownStack)
    (before after : EVM.Stack) (related : words.matches before)
    (realized : motion.realizes before after) : (motion.carry words).matches after := by
  intro index value known
  have bound : index < 32 := by
    by_cases fits : index < 32
    · exact fits
    · simp [StackMotion.carry, KnownStack.word, List.getElem?_take_eq_none (Nat.le_of_not_lt fits)] at known
  apply moved motion words before after related realized index value
  simpa [StackMotion.carry, KnownStack.word, List.getElem?_take_of_lt bound] using known

end SolarTV.EVM.State
