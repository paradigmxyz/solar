import SolarTV.EVMIR.Ops
import SolarTV.EVMIR.StackReachability

/-! Cancel two SWAP1 operations separated by a static transfer. The first
operation can be bypassed only at a height of at least two. The transfer
keeps its temporary stack demand. Other entries to the second swap are
unchanged, as are its operands when those paths execute it. -/

namespace SolarTV.EVM.SwapPairs
open SourceGraph (Program Node)
open StackGraph

structure Window where
  firstLabel : Nat
  transfer : Nat
  transferLabel : Nat
  growth : Nat
  second : Nat
  secondLabel : Nat
  destination : Nat

def read (nodes : ProofTable.View Node) (pc : Nat) : Option Window :=
  match nodes.get? pc with
  | some (.operation firstLabel "swap 1" transfer) =>
    match nodes.get? transfer with
    | some (.jump transferLabel growth second) =>
      match nodes.get? second with
      | some (.operation secondLabel "swap 1" destination) =>
        some ⟨firstLabel,transfer,transferLabel,growth,second,secondLabel,destination⟩
      | _ => none
    | _ => none
  | _ => none

def replace (nodes : ProofTable.View Node) (floors : Nat → Nat) (pc : Nat) : Option Node :=
  if 2 ≤ floors pc then
    match read nodes pc with
    | some window => some (.jump window.firstLabel window.growth window.destination)
    | none => nodes.get? pc
  else nodes.get? pc

def normalize (program : Program) (floors : Nat → Nat) : Option Program :=
  ((List.range program.nodes.size).mapM
    (replace (ProofTable.ofArray program.nodes) floors)).map
    (fun rows => ⟨program.entry,rows.toArray⟩)

theorem normalize_checked (program result : Program) (floors : Nat → Nat)
    (nodes rewritten : ProofTable.View Node)
    (nodesEq : nodes = ProofTable.ofArray program.nodes)
    (rewrittenEq : rewritten = ProofTable.ofArray result.nodes)
    (count : nodes.size = rewritten.size) (entry : program.entry = result.entry)
    (checked : ∀ pc < rewritten.size, ProofTable.checkedRow rewritten (replace nodes floors) pc = true) :
    normalize program floors = some result := by
  have listed : rewritten = ProofTable.ofArray result.nodes.toList.toArray := by
    simpa using rewrittenEq
  have gathered := ProofTable.gather result.nodes.toList rewritten listed (replace nodes floors) checked
  rw [nodesEq] at gathered
  have size : program.nodes.size = rewritten.size := by
    simpa [nodesEq, ProofTable.ofArray] using count
  rw [normalize, size, gathered, entry]
  cases result
  simp

theorem read_rows {nodes : ProofTable.View Node} {pc : Nat} {window : Window}
    (found : read nodes pc = some window) :
    nodes.get? pc = some (.operation window.firstLabel "swap 1" window.transfer) ∧
      nodes.get? window.transfer = some (.jump window.transferLabel window.growth window.second) ∧
      nodes.get? window.second = some (.operation window.secondLabel "swap 1" window.destination) := by
  unfold read at found
  split at found <;> rename_i first
  · split at found <;> rename_i transfer
    · split at found <;> rename_i second
      · simp only [Option.some.injEq] at found
        subst window
        exact ⟨first,transfer,second⟩
      · simp at found
    · simp at found
  · simp at found

def swapTop (stack : Stack) : Stack :=
  match stack with
  | a :: b :: rest => b :: a :: rest
  | _ => stack

theorem swap_words (ctx : Context) (stack : Stack) (enough : 2 ≤ stack.length) :
    EVM.step ctx (.swap 0) stack = some (swapTop stack) := by
  rcases stack with _ | ⟨a, _ | ⟨b, rest⟩⟩ <;> simp_all [swapTop, EVM.step]

theorem swap_twice (stack : Stack) : swapTop (swapTop stack) = stack := by
  rcases stack with _ | ⟨a, _ | ⟨b, rest⟩⟩ <;> rfl

/-- The two swaps use the same pure architectural stack action at every
label. The operation leaves the remaining world state unchanged. -/
def withSwap (ordinary : Handlers (Stack × World) Observation) :
    Handlers (Stack × World) Observation :=
  { ordinary with operation := fun label name height world =>
      if name = "swap 1" then .ok (swapTop world.1,world.2)
      else ordinary.operation label name height world }

/-- Check the concrete window at its original source point. Its static
transfer can overflow at exactly the same height as the replacement jump.
Success resumes with the same words, architectural height and world. -/
theorem execute_window (program : Program) (signatures : Signatures)
    (ordinary : Handlers (Stack × World) Observation) (pc fuel : Nat) (window : Window)
    (found : read (ProofTable.ofArray program.nodes) pc = some window)
    (arity : signatures "swap 1" = some (2,2)) (state : State (Stack × World))
    (enough : 2 ≤ state.height) (fits : state.height ≤ 1024) :
    StackGraph.execute program signatures (withSwap ordinary) (fuel+3) pc state =
      if state.height + window.growth > 1024 then some .overflow
      else StackGraph.execute program signatures (withSwap ordinary) fuel window.destination state := by
  obtain ⟨first,transfer,second⟩ := read_rows found
  have firstNode := first
  have transferNode := transfer
  have secondNode := second
  simp only [ProofTable.ofArray] at firstNode transferNode secondNode
  have low : ¬state.height < 2 := by omega
  have high : ¬state.height > 1024 := by omega
  have same : state.height - 2 + 2 = state.height := by omega
  by_cases room : state.height + window.growth > 1024 <;>
    simp [StackGraph.execute, firstNode, transferNode, secondNode, Bind.bind, Option.bind, low, high,
      same, room, arity, withSwap, swap_twice]

/-- Every selected window has enough stack at a reachable point. -/
theorem reachable_window {graph : ProofTable.View Node} {signatures : Signatures}
    {floors : Nat → Nat} (entry : 0 < graph.size) (empty : floors 0 = 0)
    (closed : ∀ pc < graph.size, StackReachability.floorAt graph signatures floors pc = true)
    {pc height : Nat} (reaches : StackReachability.Reaches graph signatures pc height)
    (selected : 2 ≤ floors pc) : 2 ≤ height :=
  Nat.le_trans selected (StackReachability.floor_invariant entry empty closed reaches).2.1

end SolarTV.EVM.SwapPairs
