import SolarTV.EVMIR.Transforms.EqIszeroJumpi
import SolarTV.EVMIR.ContinuationFlow

/-! The layout pass can replace the test at a conditional transfer and swap
its destinations, without changing the live words or a stack failure. -/

namespace SolarTV.EVM.BranchLayout
open EvmRules

theorem eq_sub_swapped (ctx : Context) (stack : Stack) (yes no : Word) :
    execute ctx [.eq] (.branch yes no) stack =
      execute ctx [.sub] (.branch no yes) stack := by
  rcases stack with _ | ⟨a, _ | ⟨b, rest⟩⟩ <;> try rfl
  by_cases overfull : 1024 < rest.length + 1 + 1
  · simp [execute, stepChecked, overfull]
  · have fits : ¬ 1024 < rest.length + 1 := by omega
    by_cases same : a = b <;>
      simp [execute, stepChecked, step, finish, Evm.eq, sub_zero, same, overfull, fits]

open ContinuationFlow

structure Site where
  comparison : Nat
  branch : Nat
  yes : Nat
  no : Nat

def enters (target : Nat) : Node → Bool
  | .operation _ _ next | .jump _ next | .branchAddress next
  | .branchAfterAddress next => next == target
  | .branch yes no | .pushLabel yes no => yes == target || no == target
  | .terminal .. | .dynamicJump => false

def site (entry : Nat) (nodes : ProofTable.View Node) (pc : Nat) : Option Site := do
  let .operation "sub" (.ordinary 2 1) branch ← nodes.get? pc | none
  let .branch yes no ← nodes.get? branch | none
  -- No path can observe the changed word between the comparison and its branch.
  if branch == entry ||
      !(List.range nodes.size).all (fun i =>
        i == pc || ((nodes.get? i).map (fun node => !enters branch node)).getD false) then
    none
  else
    some ⟨pc, branch, no, yes⟩

def normalizedRow (nodes : ProofTable.View Node) (sites : List Site) (pc : Nat) :
    Option Node :=
  match sites.find? (fun site => site.comparison == pc || site.branch == pc) with
  | none => nodes.get? pc
  | some site =>
    if site.comparison == pc then some (.operation "eq" (.ordinary 2 1) site.branch)
    else some (.branch site.yes site.no)

def normalizeView (entry : Nat) (nodes : ProofTable.View Node) (comparisons : List Nat) :
    Option (List Node) := do
  if comparisons.length > 64 then none else
  let sites ← comparisons.mapM (site entry nodes)
  (List.range nodes.size).mapM (normalizedRow nodes sites)

def normalize (program : Program) (comparisons : List Nat) : Option Program :=
  (normalizeView program.entry (ProofTable.ofArray program.nodes) comparisons).map
    (fun nodes => ⟨program.entry, nodes.toArray⟩)

theorem normalize_checked (program result : Program) (nodes : ProofTable.View Node)
    (comparisons : List Nat) (nodeView : nodes = ProofTable.ofArray program.nodes)
    (entry : program.entry = result.entry)
    (checked : normalizeView program.entry nodes comparisons = some result.nodes.toList) :
    normalize program comparisons = some result := by
  rw [nodeView] at checked
  simp [normalize, checked]
  cases result
  cases program
  simp_all

end SolarTV.EVM.BranchLayout
