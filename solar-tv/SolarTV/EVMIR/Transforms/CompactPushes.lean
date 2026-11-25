import SolarTV.EVMIR.Prefix
import SolarTV.EVMIR.ControlFlow
import SolarTV.EVMIR.StackWords

/-! Closed constant recipes. The checker executes the recipe on a private
stack, counting its peak height. A kernel theorem lifts that execution to any
incoming stack and accounts for every push while it is live. The same opcode
and gas definitions are used by the instruction-by-instruction EVM model. -/

namespace SolarTV.EVM.CompactPushes
open EvmRules

inductive Instruction
  | push (word : Word)
  | not | shl | shr | div | mul

def Instruction.op : Instruction → Op
  | .push word => .push word
  | .not => .not
  | .shl => .shl
  | .shr => .shr
  | .div => .div
  | .mul => .mul

def Instruction.text : Instruction → String
  | .push word => s!"push {word.toNat}"
  | .not => "not"
  | .shl => "shl"
  | .shr => "shr"
  | .div => "div"
  | .mul => "mul"

def Instruction.available (fork : Fork) : Instruction → Bool
  | .shl | .shr => fork.atLeast .constantinople
  | _ => true

structure Summary where
  words : Stack
  peak : Nat
  deriving DecidableEq

private def emptyContext : Context := ⟨0,0,0,[],0⟩

def summarize : List Instruction → Stack → Option Summary
  | [], active => some ⟨active,active.length⟩
  | instruction :: rest, active => do
    let next ← step emptyContext instruction.op active
    let tail ← summarize rest next
    return ⟨tail.words,max active.length tail.peak⟩

theorem summarize_bound (ops : List Instruction) (active : Stack) (summary : Summary)
    (found : summarize ops active = some summary) : active.length ≤ summary.peak := by
  cases ops with
  | nil =>
    simp [summarize] at found
    simp [← found]
  | cons inst rest =>
    cases first : step emptyContext inst.op active with
    | none => simp [summarize, first, Bind.bind, Option.bind] at found
    | some next =>
      cases tail : summarize rest next with
      | none => simp [summarize, first, tail, Bind.bind, Option.bind] at found
      | some tail =>
        simp [summarize, first, tail, Bind.bind, Option.bind, Option.pure_def] at found
        subst summary
        exact Nat.le_max_left _ _

theorem step_append (ctx : Context) (inst : Instruction) (active next stack : Stack)
    (found : step emptyContext inst.op active = some next) :
    step ctx inst.op (active ++ stack) = some (next ++ stack) := by
  cases inst <;> cases active with
  | nil => simp_all [Instruction.op, step] <;> subst next <;> rfl
  | cons word rest =>
    cases rest <;> simp_all [Instruction.op,step] <;> subst next <;> rfl

/-- Every intermediate stack height is checked, including the initial height.
The recipe touches only its own words when it runs on an arbitrary stack. -/
theorem run_summary (ctx : Context) (ops : List Instruction)
    (active stack : Stack) (summary : Summary) (found : summarize ops active = some summary)
    (admitted : active.length + stack.length ≤ 1024) :
    Prefix.run ctx (ops.map Instruction.op) (active ++ stack) =
      if summary.peak + stack.length ≤ 1024 then some (summary.words ++ stack) else none := by
  induction ops generalizing active summary with
  | nil =>
    simp [summarize] at found
    subst summary
    simp [Prefix.run, admitted]
  | cons inst rest ih =>
    cases first : step emptyContext inst.op active with
    | none => simp [summarize, first, Bind.bind, Option.bind] at found
    | some next =>
      cases tailEq : summarize rest next with
      | none => simp [summarize, first, tailEq, Bind.bind, Option.bind] at found
      | some tail =>
        simp [summarize, first, tailEq, Bind.bind, Option.bind, Option.pure_def] at found
        subst summary
        have stepped := step_append ctx inst active next stack first
        have nextPeak := summarize_bound rest next tail tailEq
        have validHead : ¬1024 < active.length + stack.length := by omega
        simp only [List.map_cons, Prefix.run, stepChecked, validHead, ite_false,
          stepped, List.length_append]
        by_cases fits : next.length + stack.length ≤ 1024
        · have notFull : ¬1024 < next.length + stack.length := by omega
          simp only [notFull, ite_false]
          rw [ih next tail tailEq fits]
          have equivalent : max active.length tail.peak + stack.length ≤ 1024 ↔
              tail.peak + stack.length ≤ 1024 := by omega
          simp only [equivalent]
        · have full : 1024 < next.length + stack.length := by omega
          have peakFull : ¬max active.length tail.peak + stack.length ≤ 1024 := by omega
          simp [full, peakFull]

def charge (fork : Fork) (ops : List Instruction) : Nat :=
  Prefix.cost fork (ops.map Instruction.op)

/-- A successful recipe leaves its words above the incoming stack. Gas is
paid at each opcode in `Prefix.charged`. The aggregate charge is sound here
because the whole recipe is effect-free and has no gas reads. -/
theorem charged_words (ctx : Context) (fork : Fork) (ops : List Instruction)
    (words : Stack) (peak : Nat) (stack : Stack) (gas : Nat)
    (found : summarize ops [] = some ⟨words,peak⟩) (pushes : words ≠ []) :
    Prefix.charged ctx fork (ops.map Instruction.op) stack gas =
      if peak + stack.length ≤ 1024 ∧ charge fork ops ≤ gas
      then some (words ++ stack, gas - charge fork ops) else none := by
  by_cases initial : stack.length ≤ 1024
  · have executed := run_summary ctx ops [] stack ⟨words,peak⟩ found (by simpa using initial)
    rw [Prefix.charged_eq_observe]
    simp only [List.nil_append] at executed
    simp only [Prefix.observe, executed, charge]
    by_cases room : peak + stack.length ≤ 1024 <;> simp [room]
  · cases ops with
    | nil =>
      simp [summarize] at found
      exact False.elim (pushes found.1)
    | cons inst rest =>
      have full : 1024 < stack.length := by omega
      have overflow : ¬peak + stack.length ≤ 1024 := by omega
      simp [Prefix.charged, stepChecked, full, overflow]

/-- Equal recipe charges preserve the remaining gas for every budget. The
stack check still needs to agree at the incoming height. -/
theorem exact (ctx : Context) (fork : Fork) (before after : List Instruction)
    (words : Stack) (oldPeak newPeak : Nat) (stack : Stack) (gas : Nat)
    (old : summarize before [] = some ⟨words,oldPeak⟩)
    (new : summarize after [] = some ⟨words,newPeak⟩) (pushes : words ≠ [])
    (fit : oldPeak + stack.length ≤ 1024 ↔ newPeak + stack.length ≤ 1024)
    (prices : charge fork before = charge fork after) :
    Prefix.charged ctx fork (before.map Instruction.op) stack gas =
      Prefix.charged ctx fork (after.map Instruction.op) stack gas := by
  rw [charged_words _ _ _ _ _ _ _ old pushes, charged_words _ _ _ _ _ _ _ new pushes, prices]
  simp only [fit]

/-- Unequal prices compose under the explicit funding of both recipes and a
continuation whose observation is independent of the remaining budget. -/
theorem funded (ctx : Context) (fork : Fork) (before after : List Instruction)
    (words : Stack) (oldPeak newPeak : Nat) (stack : Stack) (oldGas newGas : Nat)
    (old : summarize before [] = some ⟨words,oldPeak⟩)
    (new : summarize after [] = some ⟨words,newPeak⟩) (pushes : words ≠ [])
    (fit : oldPeak + stack.length ≤ 1024 ↔ newPeak + stack.length ≤ 1024)
    (oldPaid : charge fork before ≤ oldGas) (newPaid : charge fork after ≤ newGas)
    (continueWith : Stack → Observation) (fault : Observation) :
    ((Prefix.charged ctx fork (before.map Instruction.op) stack oldGas).map
      (fun result => continueWith result.1)).getD fault =
    ((Prefix.charged ctx fork (after.map Instruction.op) stack newGas).map
      (fun result => continueWith result.1)).getD fault := by
  rw [charged_words _ _ _ _ _ _ _ old pushes, charged_words _ _ _ _ _ _ _ new pushes]
  simp only [oldPaid,newPaid,and_true,fit]
  split <;> rfl

open SourceGraph (Program Node)
open ControlFlow (Edges)

/-- Match the recipe to actual consecutive opcodes. Relocation placeholders
are ordinary zero words in the continuation projection, but do not belong to
a closed constant recipe. -/
def scan (nodes : ProofTable.View Node) (fork : Fork) (relocated : Nat → Bool) :
    List Instruction → Nat → Option Nat
  | [], pc => some pc
  | instruction :: rest, pc => do
    if relocated pc then none else
    match ← nodes.get? pc with
    | .operation _ name next =>
      if name == instruction.text && instruction.available fork then scan nodes fork relocated rest next
      else none
    | _ => none

structure AddressFold where
  offset : Word
  depth : Fin 15
  base : Word

def AddressFold.ops (fold : AddressFold) : List Op :=
  [.push fold.offset, .dup ⟨fold.depth.val+1, by have := fold.depth.isLt; omega⟩, .add]

theorem AddressFold.run (fold : AddressFold) (ctx : Context) (stack : Stack)
    (base : stack[fold.depth.val]? = some fold.base) (room : stack.length + 2 ≤ 1024) :
    Prefix.run ctx fold.ops stack = some ((fold.base + fold.offset) :: stack) := by
  have h0 : ¬1024 < stack.length := by omega
  have h1 : ¬1024 < stack.length + 1 := by omega
  have h2 : ¬1024 < stack.length + 2 := by omega
  simp [AddressFold.ops, Prefix.run, stepChecked, step, Evm.add,
    List.length_cons, Nat.add_assoc, base, h0, h1, h2]

theorem AddressFold.funded (fold : AddressFold) (ctx : Context) (fork : Fork)
    (stack : Stack) (gas : Nat)
    (base : stack[fold.depth.val]? = some fold.base) (room : stack.length + 2 ≤ 1024)
    (paid : Prefix.cost fork fold.ops ≤ gas) :
    Prefix.charged ctx fork fold.ops stack gas =
      some ((fold.base + fold.offset) :: stack, gas - Prefix.cost fork fold.ops) := by
  rw [Prefix.charged_eq_observe, Prefix.observe, fold.run ctx stack base room]
  simp [paid]

def readAddressFold (nodes : ProofTable.View Node) (relocated : Nat → Bool)
    (facts : Nat → StackWords.Facts) (pc : Nat) (fold : AddressFold) : Option Nat := do
  if !(facts pc).contains (fold.depth.val,fold.base) then none else
  let .operation _ push duplicate ← nodes.get? pc | none
  let .operation _ dup sum ← nodes.get? duplicate | none
  let .operation _ add next ← nodes.get? sum | none
  if relocated pc || relocated duplicate || relocated sum ||
      push != (Instruction.push fold.offset).text ||
      dup != s!"dup {fold.depth.val + 2}" || add != "add" then none else
  return next

structure Point where
  pc : Nat
  recipe : List Instruction
  edges : Edges
  address : Option AddressFold

def resolve (points : ProofTable.View Point) (pc destination : Nat) : Option Nat := do
  if (← points.get? destination).pc == pc then some destination else none

private structure Projection where
  nodes : List Node := []
  pushes : List Node := []
  prices : List Nat := []
  pushCount : Nat := 0

private def appendRecipe (view : Projection) (size : Nat) (words : Stack)
    (peak price next : Nat) : Projection :=
  let start := size + view.pushCount
  let literals := words.zipIdx |>.map fun (value,index) =>
    .operation 0 (Instruction.push value).text
      (if index + 1 = words.length then next else start + index + 1)
  {
    nodes := .jump 0 peak start :: view.nodes
    pushes := literals.reverseAux view.pushes
    prices := price :: view.prices
    pushCount := view.pushCount + words.length
  }

/-- Each recipe becomes a stack guard followed by its literal outputs. Gas
reads outside recipes are allowed only as explicit shared interactions. -/
def projectView (source : Nat) (nodes : ProofTable.View Node) (fork : Fork)
    (points : ProofTable.View Point) (relocated : Nat → Bool) (interactions : Bool)
    (facts : Nat → StackWords.Facts) :
    Option (Nat × List Node × List Nat) := do
  let entry ← resolve points source 0
  let view ← (List.range points.size).foldlM (init := ({} : Projection))
    fun view index => do
      let point ← points.get? index
      let find (pc destination : Nat) := resolve points pc destination
      if let some fold := point.address then
        let .next successor := point.edges | none
        if !point.recipe.isEmpty then none else
        let next ← find (← readAddressFold nodes relocated facts point.pc fold) successor
        return appendRecipe view points.size [fold.base + fold.offset] 2
          (Prefix.cost fork fold.ops) next
      else
      match point.recipe with
      | [] =>
        let node ← match ← nodes.get? point.pc, point.edges with
        | .operation _ name next, .next destination =>
          if name == "gas" && !interactions then none else
          pure (.operation 0 name (← find next destination))
        | .jump _ growth next, .next destination =>
          if growth > 4 then none else pure (.jump 0 growth (← find next destination))
        | .branch _ yes no, .branch target other =>
          pure (.branch 0 (← find yes target) (← find no other))
        | .terminal _ name, .terminal =>
          if name == "jump" then none else pure (.terminal 0 name)
        | _, _ => none
        return {view with nodes := node :: view.nodes}
      | recipe =>
        match ← summarize recipe [], point.edges with
        | ⟨word :: rest,peak⟩, .next successor =>
          let next ← find (← scan nodes fork relocated recipe point.pc) successor
          return appendRecipe view points.size (word :: rest).reverse peak (charge fork recipe) next
        | _, _ => none
  return (entry, view.nodes.reverse ++ view.pushes.reverse, view.prices.reverse)

def project (program : Program) (fork : Fork) (points : Array Point)
    (relocated : Nat → Bool) (interactions : Bool) (facts : Nat → StackWords.Facts) :
    Option (Program × List Nat) :=
  (projectView program.entry (ProofTable.ofArray program.nodes) fork
    (ProofTable.ofArray points) relocated interactions facts).map
    (fun (entry, nodes, prices) => (⟨entry,nodes.toArray⟩,prices))

theorem project_checked (program : Program) (fork : Fork) (points : Array Point)
    (nodesView : ProofTable.View Node) (pointsView : ProofTable.View Point)
    (relocated : Nat → Bool) (interactions : Bool) (facts : Nat → StackWords.Facts)
    (result : Program) (prices : List Nat)
    (nodesEq : nodesView = ProofTable.ofArray program.nodes)
    (pointsEq : pointsView = ProofTable.ofArray points)
    (checked : projectView program.entry nodesView fork pointsView relocated interactions facts =
      some (result.entry,result.nodes.toList,prices)) :
    project program fork points relocated interactions facts = some (result,prices) := by
  rw [nodesEq, pointsEq] at checked
  rw [project, checked]
  cases result
  rfl

open StackGraph

theorem contextual (left right : Option (Program × List Nat)) (before after : Program)
    (oldPrices newPrices : List Nat) (signatures : Signatures) (bounds : Nat → Nat)
    (bound : Nat) (old : left = some (before,oldPrices)) (new : right = some (after,newPrices))
    (valid : Valid before after id bounds signatures bound)
    (handlers : Handlers World Observation) (fuel : Nat) (state : State World)
    (fits : state.height ≤ bound) :
    left.map (fun p => StackGraph.execute p.1 signatures handlers fuel p.1.entry state) =
    right.map (fun p => StackGraph.execute p.1 signatures handlers fuel p.1.entry state) := by
  rw [old, new]
  change some (StackGraph.execute before signatures handlers fuel before.entry state) =
    some (StackGraph.execute after signatures handlers fuel after.entry state)
  rw [← valid.2.1]
  exact congrArg some (bounded_simulation before after id bounds signatures valid
    handlers fuel before.entry state valid.1 (Nat.le_trans fits valid.2.2.1))

end SolarTV.EVM.CompactPushes
