/-! Indexed tables for checking concrete CFG certificates in the kernel.

An array literal reduces to a list in the kernel. Repeated access to the
end of that list is expensive. A certificate can instead supply a balanced
tree with checked subtree sizes. Its leaves are the original ordered rows.
The lookup theorem connects all uses of the tree to the ordinary array. -/

namespace SolarTV.EVM.ProofTable

structure View (α : Type) where
  size : Nat
  get? : Nat → Option α

def ofArray (rows : Array α) : View α := ⟨rows.size, (rows[·]?)⟩

inductive Tree (α : Type) where
  | leaf (rows : List α)
  | branch (size : Nat) (left right : Tree α)

def Tree.rows : Tree α → List α
  | .leaf rows => rows
  | .branch _ left right => left.rows ++ right.rows

def Tree.size : Tree α → Nat
  | .leaf rows => rows.length
  | .branch size _ _ => size

def Tree.checked : Tree α → Bool
  | .leaf _ => true
  | .branch size left right => size == left.size + right.size && left.checked && right.checked

def Tree.get? : Tree α → Nat → Option α
  | .leaf rows, i => rows[i]?
  | .branch _ left right, i =>
    if i < left.size then left.get? i else right.get? (i-left.size)

def Tree.view (tree : Tree α) : View α := ⟨tree.size, tree.get?⟩

theorem Tree.size_eq (tree : Tree α) (checked : tree.checked = true) :
    tree.size = tree.rows.length := by
  induction tree with
  | leaf => rfl
  | branch size left right leftIH rightIH =>
    simp only [Tree.checked, Bool.and_eq_true, beq_iff_eq] at checked
    change size = (left.rows ++ right.rows).length
    rw [checked.1.1, List.length_append, leftIH checked.1.2, rightIH checked.2]

theorem Tree.get_eq (tree : Tree α) (checked : tree.checked = true) (i : Nat) :
    tree.get? i = tree.rows[i]? := by
  induction tree generalizing i with
  | leaf => rfl
  | branch size left right leftIH rightIH =>
    simp only [Tree.checked, Bool.and_eq_true] at checked
    by_cases inside : i < left.size
    · simp only [Tree.get?, inside, ite_true, Tree.rows, leftIH checked.1.2]
      rw [List.getElem?_append_left (by simpa [left.size_eq checked.1.2] using inside)]
    · simp only [Tree.get?, inside, ite_false, Tree.rows, rightIH checked.2]
      rw [left.size_eq checked.1.2, List.getElem?_append_right (by
        simpa [left.size_eq checked.1.2] using Nat.le_of_not_gt inside)]

theorem Tree.view_eq (tree : Tree α) (checked : tree.checked = true) :
    tree.view = ofArray tree.rows.toArray := by
  have get : tree.get? = fun (i : Nat) => tree.rows.toArray[i]? := by
    funext i
    simpa only [List.getElem?_toArray] using tree.get_eq checked i
  simp only [Tree.view, ofArray, List.size_toArray, tree.size_eq checked, get]


theorem mapM_get (f : Nat → Option α) (expected : List α)
    (agrees : ∀ i < expected.length, f i = expected[i]?) :
    (List.range expected.length).mapM f = some expected := by
  induction expected generalizing f with
  | nil => rfl
  | cons head tail ih =>
    have first : f 0 = some head := by simpa using agrees 0 (by simp)
    have rest : (List.range tail.length).mapM (f ∘ Nat.succ) = some tail := by
      apply ih
      intro i inside
      have atSucc := agrees (i+1) (by simpa using inside)
      simpa using atSucc
    simp [List.range_succ_eq_map, first, rest, Bind.bind, Option.bind]

def checkedRow [DecidableEq α] (expected : View α) (f : Nat → Option α) (pc : Nat) : Bool :=
  decide (f pc = expected.get? pc)

theorem gather [DecidableEq α] (expected : List α) (view : View α)
    (viewEq : view = ofArray expected.toArray) (f : Nat → Option α)
    (checked : ∀ pc < view.size, checkedRow view f pc = true) :
    (List.range view.size).mapM f = some expected := by
  rw [viewEq] at checked ⊢
  simp only [ofArray, List.size_toArray] at checked ⊢
  apply mapM_get
  intro pc inside
  have agrees := of_decide_eq_true (checked pc inside)
  simpa [ofArray] using agrees


def readPair (lookup : Nat → Option α) (i : Nat) : Option (List α) := do
  return [← lookup (2*i), ← lookup (2*i+1)]

theorem gatherPairs (size : Nat) (expected : List α) (f : Nat → Option (List α))
    (length : expected.length = 2*size)
    (agrees : ∀ j < size, f j = readPair (fun i => expected[i]?) j) :
    ((List.range size).mapM f).map List.flatten = some expected := by
  induction size generalizing expected f with
  | zero =>
    have empty : expected = [] := by simpa using length
    simp [empty]
  | succ size ih =>
    rcases expected with _ | ⟨a, _ | ⟨b, rest⟩⟩
    · simp at length
    · simp at length
      omega
    have head : f 0 = some [a,b] := by
      simpa [readPair, Bind.bind, Option.bind] using agrees 0 (Nat.zero_lt_succ _)
    have tailLen : rest.length = 2*size := by
      simp at length
      omega
    obtain tail := ih rest (f ∘ Nat.succ) tailLen
    have good : ((List.range size).mapM (f ∘ Nat.succ)).map List.flatten = some rest := by
      apply tail
      intro j inside
      have row := agrees (j+1) (by omega)
      simpa [readPair, Nat.mul_succ, Nat.add_assoc] using row
    cases tailRows : (List.range size).mapM (f ∘ Nat.succ) with
    | none => simp [tailRows] at good
    | some rows =>
      simp only [tailRows, Option.map_some, Option.some.injEq] at good
      simp [List.range_succ_eq_map, head, tailRows, Bind.bind, Option.bind, good]

end SolarTV.EVM.ProofTable
