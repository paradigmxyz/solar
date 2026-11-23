import SolarTV.EVMIR.ControlFlow
import SolarTV.EVMIR.Ops

/-! Resolve stack-carried continuations before comparing two EVM CFGs.

A shape describes the relocation tags at the top of the stack. Trailing
ordinary words are implicit. Propagation follows fixed opcode arities and
the stack shuffles, rather than trusting call-effect annotations. Every
consumer checks for ordinary operands. A dynamic jump must consume a known
label, while an exception or halt may discard dead labels.

The projected graph pushes a dummy word for each tag and pops it on return.
Each state and edge of that projection is checked against the source graph.
There is no pricing or theorem about emitted physical PCs. -/

namespace SolarTV.EVM.ContinuationFlow
open EvmRules

abbrev Shape := List (Option Nat)

inductive StackOp
  | ordinary (inputs outputs : Nat)
  | dup (depth : Nat)
  | swap (depth : Nat)
  | pop

inductive Node
  | operation (name : String) (op : StackOp) (next : Nat)
  | pushLabel (target next : Nat)
  | jump (growth next : Nat)
  | branch (yes no : Nat)
  | terminal (name : String) (inputs : Nat)
  | dynamicJump

structure Program where
  entry : Nat
  nodes : Array Node

structure Point where
  pc : Nat
  shape : Shape
  deriving DecidableEq

/-- Share repeated continuation shapes across basic blocks and call sites. -/
def internedPoints (points : ProofTable.View (Nat × Nat)) (shapes : ProofTable.View Shape) :
    ProofTable.View Point :=
  ⟨points.size, fun index => do
    let (pc, shape) ← points.get? index
    return ⟨pc, ← shapes.get? shape⟩⟩

def trim : Shape → Shape
  | [] => []
  | tag :: rest =>
    match tag, trim rest with
    | none, [] => []
    | tag, tail => tag :: tail

def tagAt (shape : Shape) (depth : Nat) : Option Nat :=
  (shape[depth]?).getD none

@[simp] theorem tagAt_nil (depth : Nat) : tagAt [] depth = none := by
  simp [tagAt]

@[simp] theorem tagAt_zero (tag : Option Nat) (rest : Shape) : tagAt (tag::rest) 0 = tag := rfl

@[simp] theorem tagAt_succ (tag : Option Nat) (rest : Shape) (depth : Nat) :
    tagAt (tag::rest) (depth+1) = tagAt rest depth := rfl

@[simp] theorem tagAt_trim (shape : Shape) (depth : Nat) :
    tagAt (trim shape) depth = tagAt shape depth := by
  induction shape generalizing depth with
  | nil => rfl
  | cons tag rest ih =>
    cases computed : trim rest <;> cases tag <;> cases depth <;>
      simp_all [trim] <;> apply ih

theorem safe_operands (shape : Shape) (count : Nat) :
    (shape.take count).all Option.isNone = true ↔
      ∀ depth < count, tagAt shape depth = none := by
  induction count generalizing shape with
  | zero => simp
  | succ count ih =>
    cases shape with
    | nil => simp
    | cons tag rest =>
      simp only [List.take_succ_cons, List.all_cons, Bool.and_eq_true, Option.isNone_iff_eq_none, ih]
      constructor
      · rintro ⟨head,tail⟩ depth bound
        cases depth with
        | zero => simpa using head
        | succ depth => simpa using tail depth (by omega)
      · intro all
        refine ⟨by simpa using all 0 (by omega), ?_⟩
        intro depth bound
        simpa using all (depth+1) (by omega)

def swapAt (values : List α) (depth : Nat) : Option (List α) := do
  let first ← values.head?
  let other ← values[depth]?
  return (values.set 0 other).set depth first

def transfer (op : StackOp) (shape : Shape) : Option Shape := do
  let next ← match op with
    | .ordinary inputs outputs =>
      if (shape.take inputs).all Option.isNone then
        some (List.replicate outputs none ++ shape.drop inputs)
      else none
    | .dup depth => some (tagAt shape depth :: shape)
    | .swap depth =>
      swapAt (shape ++ List.replicate (depth+1-shape.length) none) depth
    | .pop => some shape.tail
  return trim next

theorem tagAt_pad (shape : Shape) (size depth : Nat) :
    tagAt (shape ++ List.replicate size none) depth = tagAt shape depth := by
  by_cases inside : depth < shape.length
  · simp [tagAt, List.getElem?_append_left inside]
  · simp [tagAt, List.getElem?_append_right (by omega : shape.length ≤ depth),
      List.getElem?_eq_none (by omega : shape.length ≤ depth), List.getElem?_replicate]
    split <;> rfl

theorem tagAt_swap (shape : Shape) (depth : Nat) (fits : depth < shape.length) :
    ∃ shuffled, swapAt shape depth = some shuffled ∧
      ∀ index, tagAt shuffled index =
        tagAt shape (if index = depth then 0 else if index = 0 then depth else index) := by
  have head : 0 < shape.length := by omega
  let first := shape[0]
  let other := shape[depth]
  refine ⟨(shape.set 0 other).set depth first, ?_, ?_⟩
  · simp [swapAt, List.head?_eq_getElem?, fits, head,
      first, other, Bind.bind, Option.bind]
  · intro index
    by_cases target : index = depth
    · subst index
      simp [tagAt, fits, List.getElem?_eq_getElem head, first]
    · by_cases top : index = 0
      · subst index
        unfold tagAt
        rw [List.getElem?_set_ne (Ne.symm target), List.getElem?_set_self head]
        simp [List.getElem?_eq_getElem fits, other, target]
      · simp [tagAt, List.getElem?_set_ne (Ne.symm target),
          List.getElem?_set_ne (Ne.symm top), target, top]

/-- The finite shape and a full shadow stack agree at every depth, including
implicit ordinary words past the last tracked tag. Each successful operation
keeps that agreement. Normal EVM underflow checks still use the actual height. -/
theorem transfer_tags (op : StackOp) (shape full : Shape)
    (agreement : ∀ depth, tagAt shape depth = tagAt full depth) :
    (transfer op shape).map (fun next => tagAt next) =
      (transfer op full).map (fun next => tagAt next) := by
  cases op with
  | ordinary inputs outputs =>
    have sameSafety : ((shape.take inputs).all Option.isNone = true) ↔
        ((full.take inputs).all Option.isNone = true) := by
      simp only [safe_operands, agreement]
    by_cases safe : (shape.take inputs).all Option.isNone = true
    · simp only [transfer, safe, sameSafety.mp safe, ite_true, Bind.bind, Option.bind,
        Option.pure_def, Option.map_some, Option.some.injEq]
      funext depth
      simp only [tagAt_trim]
      by_cases inside : depth < outputs
      · simp [tagAt, List.getElem?_append, inside]
      · simpa only [tagAt, List.getElem?_append, List.length_replicate, inside,
          ite_false, List.getElem?_drop] using agreement (inputs+(depth-outputs))
    · simp [transfer, safe, mt sameSafety.mpr safe, Bind.bind, Option.bind]
  | dup depth =>
    simp only [transfer, Bind.bind, Option.bind, Option.pure_def,
      Option.map_some, Option.some.injEq]
    funext index
    cases index <;> simp [agreement]
  | swap depth =>
    let left := shape ++ List.replicate (depth+1-shape.length) none
    let right := full ++ List.replicate (depth+1-full.length) none
    have leftFits : depth < left.length := by simp [left]; omega
    have rightFits : depth < right.length := by simp [right]; omega
    obtain ⟨a, foundA, tagsA⟩ := tagAt_swap left depth leftFits
    obtain ⟨b, foundB, tagsB⟩ := tagAt_swap right depth rightFits
    dsimp only [left] at foundA
    dsimp only [right] at foundB
    simp only [transfer,
      foundA, foundB, Bind.bind, Option.bind, Option.pure_def,
      Option.map_some, Option.some.injEq]
    funext index
    simp [tagsA, tagsB, left, right, tagAt_pad, agreement]
  | pop =>
    simp only [transfer, Bind.bind, Option.bind, Option.pure_def, Option.map_some, Option.some.injEq]
    funext index
    simp only [tagAt_trim]
    simpa only [tagAt, List.getElem?_tail] using agreement (index+1)

def resolve (points : ProofTable.View Point) (target : Point) (index : Nat) : Option Nat :=
  if points.get? index = some target then some index else none

/-- The projected zero at a label push represents a relocation. A literal
recipe must leave this instruction in the shared continuation. -/
def pushesLabel (nodes : ProofTable.View Node) (points : ProofTable.View Point)
    (index : Nat) : Bool :=
  match (points.get? index).bind (fun point => nodes.get? point.pc) with
  | some (.pushLabel ..) => true
  | _ => false

def projectRow (nodes : ProofTable.View Node) (points : ProofTable.View Point)
    (edges : ProofTable.View ControlFlow.Edges)
    (index : Nat) : Option SourceGraph.Node := do
  let point ← points.get? index
  let find (pc : Nat) (shape : Shape) := resolve points ⟨pc,shape⟩
  match ← nodes.get? point.pc, ← edges.get? index with
  | .operation name op next, .next successor =>
    return .operation 0 name (← find next (← transfer op point.shape) successor)
  | .pushLabel target next, .next successor =>
    if target ≥ nodes.size then none else
    return .operation 0 "push 0" (← find next (some target :: point.shape) successor)
  | .jump growth next, .next successor =>
    return .jump 0 growth (← find next point.shape successor)
  | .branch yes no, .branch trueEdge falseEdge =>
    if (tagAt point.shape 0).isSome then none else
    return .branch 0 (← find yes point.shape.tail trueEdge) (← find no point.shape.tail falseEdge)
  | .terminal name inputs, .terminal =>
    if (point.shape.take inputs).all Option.isNone then some (.terminal 0 name) else none
  | .dynamicJump, .next successor =>
    let target ← tagAt point.shape 0
    return .operation 0 "pop" (← find target point.shape.tail successor)
  | _, _ => none

def projectView (source : Nat) (nodes : ProofTable.View Node) (points : ProofTable.View Point)
    (edges : ProofTable.View ControlFlow.Edges) :
    Option (Nat × List SourceGraph.Node) := do
  let entry ← resolve points ⟨source, []⟩ 0
  if points.size != edges.size then none else
  let rows ← (List.range points.size).mapM (projectRow nodes points edges)
  return (entry, rows)

def project (program : Program) (points : Array Point) (edges : Array ControlFlow.Edges) :
    Option SourceGraph.Program :=
  (projectView program.entry (ProofTable.ofArray program.nodes) (ProofTable.ofArray points)
    (ProofTable.ofArray edges)).map (fun (entry, nodes) => ⟨entry, nodes.toArray⟩)

theorem project_checked (program : Program) (points : Array Point) (edges : Array ControlFlow.Edges)
    (nodesView : ProofTable.View Node) (pointsView : ProofTable.View Point)
    (edgesView : ProofTable.View ControlFlow.Edges) (result : SourceGraph.Program)
    (nodesEq : nodesView = ProofTable.ofArray program.nodes)
    (pointsEq : pointsView = ProofTable.ofArray points)
    (edgesEq : edgesView = ProofTable.ofArray edges)
    (checked : projectView program.entry nodesView pointsView edgesView =
      some (result.entry,result.nodes.toList)) :
    project program points edges = some result := by
  rw [nodesEq, pointsEq, edgesEq] at checked
  rw [project, checked]
  cases result
  rfl


inductive Slot
  | word (value : Word)
  | label (target : Nat)

def Slot.encode (address : Nat → Word) : Slot → Word
  | .word value => value
  | .label target => address target

def Slot.tag : Slot → Option Nat
  | .word _ => none
  | .label target => some target

def Matches (shape : Shape) (slots : List Slot) : Prop :=
  ∀ depth, tagAt shape depth = tagAt (slots.map Slot.tag) depth

/-- Every ordinary consumer sees the same words under any placement of the
unobserved continuation labels. This includes branch conditions, terminal
operands, and inputs to world effects. -/
theorem operand_words (slots : List Slot) (count : Nat) (left right : Nat → Word)
    (safe : ((slots.take count).map Slot.tag).all Option.isNone = true) :
    (slots.map (Slot.encode left)).take count = (slots.map (Slot.encode right)).take count := by
  induction count generalizing slots with
  | zero => simp
  | succ count ih =>
    cases slots with
    | nil => simp
    | cons slot slots =>
      cases slot with
      | label _ => simp [Slot.tag] at safe
      | word value =>
        have tail : ((slots.take count).map Slot.tag).all Option.isNone = true := by
          simpa [Slot.tag] using safe
        simp [Slot.encode, ih slots tail]

theorem checked_operands (shape : Shape) (slots : List Slot) (count : Nat)
    (left right : Nat → Word) (related : Matches shape slots)
    (safe : (shape.take count).all Option.isNone = true) :
    (slots.map (Slot.encode left)).take count = (slots.map (Slot.encode right)).take count := by
  apply operand_words
  rw [List.map_take, safe_operands]
  intro depth bound
  rw [← related depth]
  exact (safe_operands shape count).mp safe depth bound

/-- Moving the tracked word with SWAP commutes with materializing its address. -/
theorem swap_encoding (slots : List Slot) (depth : Nat) (address : Nat → Word) :
    (swapAt slots depth).map (List.map (Slot.encode address)) =
      swapAt (slots.map (Slot.encode address)) depth := by
  simp only [swapAt, List.head?_map, List.getElem?_map]
  cases slots.head? <;> cases slots[depth]? <;>
    simp [Bind.bind, Option.bind, List.map_set]

/-- A dynamic return consumes the address of the tracked label and leaves the
rest of the stack with its original tags. -/
theorem return_word (shape : Shape) (slots : List Slot) (target : Nat)
    (address : Nat → Word) (related : Matches shape slots) (top : tagAt shape 0 = some target) :
    ∃ rest, slots.map (Slot.encode address) = address target :: rest.map (Slot.encode address) ∧
      Matches shape.tail rest := by
  have atZero := (related 0).symm.trans top
  cases slots with
  | nil => simp at atZero
  | cons slot rest =>
    cases slot with
    | word word => simp [Slot.tag] at atZero
    | label label =>
      simp [Slot.tag] at atZero
      subst label
      refine ⟨rest, rfl, ?_⟩
      intro depth
      cases shape with
      | nil => simp at top
      | cons tag tail => simpa using related (depth+1)

end SolarTV.EVM.ContinuationFlow
