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

end SolarTV.EVM.ProofTable
