import EvmRules.Bitblast
import SolarTV.EVMIR.Suffix

/-! A checked stack recipe for low masks, composed through unchanged scalar
operations, static guards and stateful requests. The proof retains the
source stack and admission checks and compares each paid interval. -/

namespace SolarTV.EVM.LateWord
open EvmRules Effects

/-- Both forms keep the same bits, including for out-of-range shift counts. -/
theorem retainedLowBits (count word : Word) :
    Evm.and (Evm.shr count (Evm.not (0 : Word))) word =
      Evm.shr count (Evm.shl count word) := by
  apply BitVec.eq_of_getLsbD_eq_iff.mpr
  intro index _
  have ones : Evm.not (0 : Word) = BitVec.allOnes 256 := by decide
  rw [ones]
  simp only [Evm.and, Evm.shl_eq, Evm.shr_eq, BitVec.shiftLeft_eq',
    BitVec.ushiftRight_eq', BitVec.getLsbD_and, BitVec.getLsbD_ushiftRight,
    BitVec.getLsbD_allOnes, BitVec.getLsbD_shiftLeft]
  have inside : ¬ count.toNat + index < count.toNat := by omega
  simp [inside]

inductive Edit where
  | same (op : Effects.Op)
  | lowMask (count : Word)

def instructions (target : Bool) : List Edit → List Effects.Op
  | [] => []
  | .same op :: edits => op :: instructions target edits
  | .lowMask count :: edits =>
    if target then
      .scalar (.push count) :: .scalar .shl :: .scalar (.push count) ::
        .scalar .shr :: instructions target edits
    else
      .scalar (.push 0) :: .scalar .not :: .scalar (.push count) ::
        .scalar .shr :: .scalar .and :: instructions target edits

private theorem fault (after : Tree) : Refines after .fault true := by
  cases after <;> simp [Refines]

private theorem same (ctx : Context) (fork : Fork) (op : Effects.Op)
    (before after : List Effects.Op)
    (rest : ∀ stack upstream downstream, upstream ≤ downstream →
      Refines (expand ctx fork after stack upstream) (expand ctx fork before stack downstream) true)
    (stack : Stack) (upstream downstream : Nat) (le : upstream ≤ downstream) :
    Refines (expand ctx fork (op :: after) stack upstream)
      (expand ctx fork (op :: before) stack downstream) true := by
  cases op with
  | scalar op =>
    cases step : stepChecked ctx op stack with
    | error => simp [expand, step, Refines]
    | ok next => simpa only [expand, step] using rest next _ _ (Nat.add_le_add_right le _)
  | effect opcode inputs outputs =>
    simp only [expand]
    split
    · simp [Refines]
    · exact ⟨le, rfl, fun _ => rest _ 0 0 (Nat.le_refl 0)⟩
  | reserve scratch =>
    simp only [expand]
    split
    · simp [Refines]
    · exact rest stack upstream downstream le
  | guard label pushCharge jumpCharge =>
    cases stack with
    | nil => simp [expand, Refines]
    | cons condition next =>
      simp only [expand]
      split
      · simp [Refines]
      · split
        · exact rest next _ _ (Nat.add_le_add_right le _)
        · exact ⟨Nat.add_le_add_right le _, rfl⟩

private theorem mask (ctx : Context) (fork : Fork) (count : Word) (edits : List Edit)
    (rest : ∀ stack upstream downstream, upstream ≤ downstream →
      Refines (expand ctx fork (instructions true edits) stack upstream)
        (expand ctx fork (instructions false edits) stack downstream) true)
    (stack : Stack) (upstream downstream : Nat) (le : upstream ≤ downstream) :
    Refines (expand ctx fork (instructions true (.lowMask count :: edits)) stack upstream)
      (expand ctx fork (instructions false (.lowMask count :: edits)) stack downstream) true := by
  cases stack with
  | nil => simp [instructions, expand, stepChecked, step, Refines]
  | cons word tail =>
    by_cases fits : tail.length + 3 ≤ 1024
    · have h1 : ¬1024 < tail.length + 1 := by omega
      have h2 : ¬1024 < tail.length + 1 + 1 := by omega
      have h3 : ¬1024 < tail.length + 1 + 1 + 1 := by omega
      simp only [instructions, Bool.false_eq_true, ite_false, ite_true, expand,
        stepChecked, step, List.length_cons, h1, h2, h3]
      rw [retainedLowBits]
      apply rest
      simp only [Gas.pureCost]
      split <;> split <;> omega
    · have sourceFault : expand ctx fork (instructions false (.lowMask count :: edits))
          (word :: tail) downstream = .fault := by
        by_cases h1 : 1024 < tail.length + 1
        · simp [instructions, expand, stepChecked, h1]
        · by_cases h2 : 1024 < tail.length + 1 + 1
          · simp [instructions, expand, stepChecked, step, h1, h2]
          · have h3 : 1024 < tail.length + 1 + 1 + 1 := by omega
            simp [instructions, expand, stepChecked, step, h1, h2, h3]
      rw [sourceFault]
      exact fault _

theorem refines (ctx : Context) (fork : Fork) (edits : List Edit)
    (stack : Stack) (upstream downstream : Nat) (le : upstream ≤ downstream) :
    Refines (expand ctx fork (instructions true edits) stack upstream)
      (expand ctx fork (instructions false edits) stack downstream) true := by
  induction edits generalizing stack upstream downstream with
  | nil =>
    by_cases full : stack.length > 1024 <;> simp [instructions, expand, Refines, full, le]
  | cons edit edits ih =>
    cases edit with
    | same op => exact same ctx fork op _ _ ih stack upstream downstream le
    | lowMask count => exact mask ctx fork count edits ih stack upstream downstream le

end SolarTV.EVM.LateWord
