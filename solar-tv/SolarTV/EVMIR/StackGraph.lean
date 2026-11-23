import SolarTV.EVMIR.SourceGraph
import SolarTV.EVMIR.ProofTable

/-! Layout simulation with checked stack-height invariants.

Opcode signatures come from the compiler schema. The height is architectural
state; all other state, including operand values, memory, accounts and resource
state, is carried by arbitrary shared handlers. Successful opcodes change height
by their signature. Branches consume one word after checking a temporary label
slot. Unconditional transfers retain their temporary growth checks. Recipe
projections may also use a guard for more than one temporary slot.

A finite local certificate matches source nodes under a renaming and proves an
upper height at every reachable node. Different jump checks may match only when
both are proved safe. The simulation holds for every execution bound, including
cycles and early failures. No incoming height is sampled. Opcode/schema binding
and the source normalizer remain trusted; transfers are unpriced, so this is not
a physical gas or backend-layout theorem.
-/

namespace SolarTV.EVM.StackGraph
open SourceGraph

structure State (World : Type) where
  height : Nat
  world : World

inductive Outcome (Observation : Type) where
  | halted (value : Observation)
  | underflow | overflow
  deriving DecidableEq

structure Handlers (World Observation : Type) where
  operation : Nat → String → Nat → World → Except Observation World
  branch : Nat → Nat → World → Except Observation (Bool × World)
  terminal : Nat → String → Nat → World → Observation

abbrev Signatures := String → Option (Nat × Nat)

def execute (program : Program) (signatures : Signatures) (handlers : Handlers World Observation) :
    Nat → Nat → State World → Option (Outcome Observation)
  | 0, _, _ => none
  | fuel+1, pc, state =>
    if state.height > 1024 then some .overflow else do
    match ← program.nodes[pc]? with
    | .operation label instruction next =>
      let (inputs,outputs) ← signatures instruction
      if state.height < inputs then some .underflow
      else if state.height - inputs + outputs > 1024 then some .overflow
      else match handlers.operation label instruction state.height state.world with
        | .error value => some (.halted value)
        | .ok world => execute program signatures handlers fuel next ⟨state.height - inputs + outputs,world⟩
    | .jump _ growth next =>
      if state.height + growth > 1024 then some .overflow
      else execute program signatures handlers fuel next state
    | .branch label yes no =>
      if state.height + 1 > 1024 then some .overflow
      else if state.height = 0 then some .underflow
      else match handlers.branch label state.height state.world with
        | .error value => some (.halted value)
        | .ok (taken,world) => execute program signatures handlers fuel (if taken then yes else no) ⟨state.height-1,world⟩
    | .terminal label instruction =>
      some (.halted (handlers.terminal label instruction state.height state.world))

def postHeight (bound inputs outputs : Nat) : Nat :=
  min bound (1024 + inputs - outputs) - inputs + outputs

def compatible (size : Nat) (rename bounds : Nat → Nat) (signatures : Signatures)
    (pc : Nat) : Node → Option Node → Bool
  | .operation label instruction next, some (.operation other opcode target) =>
    label == other && instruction == opcode && target == rename next && next < size &&
      match signatures instruction with
      | none => false
      | some (inputs,outputs) => inputs ≤ 1024 && outputs ≤ 1024 &&
        (bounds pc < inputs || postHeight (bounds pc) inputs outputs ≤ bounds next)
  | .jump label growth next, some (.jump other extra target) =>
    label == other && growth ≤ 1024 && extra ≤ 1024 && target == rename next && next < size &&
      (growth == extra || bounds pc + max growth extra ≤ 1024) &&
      min (bounds pc) (1024-growth) ≤ bounds next
  | .branch label yes no, some (.branch other targetYes targetNo) =>
    label == other && targetYes == rename yes && targetNo == rename no &&
      yes < size && no < size &&
      min (bounds pc) 1023 - 1 ≤ bounds yes && min (bounds pc) 1023 - 1 ≤ bounds no
  | .terminal label instruction, some (.terminal other opcode) =>
    label == other && instruction == opcode
  | _, _ => false

def Valid (before after : Program) (rename bounds : Nat → Nat) (signatures : Signatures)
    (entryBound : Nat := 1024) : Prop :=
  before.entry < before.nodes.size ∧ rename before.entry = after.entry ∧
    entryBound ≤ bounds before.entry ∧
    ∀ i : Fin before.nodes.size, bounds i ≤ 1024 ∧
      compatible before.nodes.size rename bounds signatures i before.nodes[i] after.nodes[rename i]? = true

instance : Decidable (Valid before after rename bounds signatures entryBound) := by
  unfold Valid
  infer_instance

def checkIndex (before after : ProofTable.View Node) (rename bounds : Nat → Nat)
    (signatures : Signatures) (i : Nat) : Bool :=
  match before.get? i with
  | none => false
  | some node => bounds i ≤ 1024 &&
      compatible before.size rename bounds signatures i node (after.get? (rename i))

theorem valid_checked (before after : Program) (rename bounds : Nat → Nat)
    (signatures : Signatures) (entryBound : Nat)
    (entry : before.entry < before.nodes.size) (mapped : rename before.entry = after.entry)
    (height : entryBound ≤ bounds before.entry)
    (checked : (List.range before.nodes.size).all (checkIndex (ProofTable.ofArray before.nodes)
      (ProofTable.ofArray after.nodes) rename bounds signatures) = true) :
    Valid before after rename bounds signatures entryBound := by
  refine ⟨entry, mapped, height, ?_⟩
  intro i
  have part := List.all_eq_true.mp checked i (List.mem_range.mpr i.isLt)
  simpa [checkIndex, ProofTable.ofArray, Array.getElem?_eq_getElem i.isLt, Bool.and_eq_true] using part

theorem valid_views (before after : Program) (left right : ProofTable.View Node)
    (rename bounds : Nat → Nat) (signatures : Signatures) (entryBound : Nat)
    (leftEq : left = ProofTable.ofArray before.nodes)
    (rightEq : right = ProofTable.ofArray after.nodes)
    (entry : before.entry < before.nodes.size) (mapped : rename before.entry = after.entry)
    (height : entryBound ≤ bounds before.entry)
    (checked : (List.range left.size).all (checkIndex left right rename bounds signatures) = true) :
    Valid before after rename bounds signatures entryBound := by
  rw [leftEq, rightEq] at checked
  exact valid_checked before after rename bounds signatures entryBound entry mapped height checked

theorem post_height_bound (height bound inputs outputs : Nat)
    (small : height ≤ bound) (enough : inputs ≤ height)
    (fits : height - inputs + outputs ≤ 1024) :
    height - inputs + outputs ≤ postHeight bound inputs outputs := by
  unfold postHeight
  omega

theorem bounded_simulation (before after : Program) (rename bounds : Nat → Nat)
    (signatures : Signatures) {entryBound : Nat}
    (valid : Valid before after rename bounds signatures entryBound)
    (handlers : Handlers World Observation) (fuel pc : Nat) (state : State World)
    (inside : pc < before.nodes.size) (small : state.height ≤ bounds pc) :
    execute before signatures handlers fuel pc state =
      execute after signatures handlers fuel (rename pc) state := by
  induction fuel generalizing pc state with
  | zero => rfl
  | succ fuel ih =>
    obtain ⟨cap,checks⟩ := valid.2.2.2 ⟨pc,inside⟩
    change bounds pc ≤ 1024 at cap
    have notFull : ¬1024 < state.height := by omega
    have lookup : before.nodes[pc]? = some before.nodes[pc] := by simp [inside]
    cases sourceEq : before.nodes[pc]
    all_goals cases targetEq : after.nodes[rename pc]?
    all_goals try (simp [compatible,sourceEq,targetEq] at checks)
    all_goals rename_i candidate
    all_goals cases candidate <;> simp at checks
    case operation.some.operation label instruction next other opcode target =>
      obtain ⟨⟨⟨⟨labels,opcodes⟩,targets⟩,nextInside⟩,signatureCheck⟩ := checks
      subst other
      subst opcode
      subst target
      cases signatureEq : signatures instruction with
      | none => simp [signatureEq] at signatureCheck
      | some signature =>
        obtain ⟨inputs,outputs⟩ := signature
        simp [signatureEq] at signatureCheck
        obtain ⟨⟨inCap,outCap⟩,nextBound⟩ := signatureCheck
        simp only [execute,notFull,ite_false,lookup,sourceEq,targetEq,
          Bind.bind,Option.bind,signatureEq]
        by_cases short : state.height < inputs
        · simp [short]
        · simp only [short,ite_false]
          by_cases spill : 1024 < state.height - inputs + outputs
          · simp [spill]
          · simp only [spill,ite_false]
            cases handled : handlers.operation label instruction state.height state.world with
            | error value => rfl
            | ok world =>
              apply ih next ⟨state.height - inputs + outputs,world⟩ nextInside
              have post := post_height_bound state.height (bounds pc) inputs outputs small
                (by omega) (by omega)
              change state.height - inputs + outputs ≤ bounds next
              rcases nextBound with impossible | bound
              · omega
              · exact Nat.le_trans post bound
    case jump.some.jump label growth next other extra target =>
      obtain ⟨⟨⟨⟨⟨⟨labels,growthSmall⟩,extraSmall⟩,targets⟩,nextInside⟩,safe⟩,nextBound⟩ := checks
      subst other
      subst target
      have sameCheck : (1024 < state.height + growth) ↔ (1024 < state.height + extra) := by
        rcases safe with same | safe
        · rw [same]
        · omega
      simp only [execute,notFull,ite_false,lookup,sourceEq,targetEq,Bind.bind,Option.bind]
      by_cases full : 1024 < state.height + growth
      · have otherFull := sameCheck.mp full
        simp [full,otherFull]
      · have otherFits : ¬1024 < state.height + extra := by omega
        simp only [full,otherFits,ite_false]
        apply ih next state nextInside
        omega
    case branch.some.branch label yes no other targetYes targetNo =>
      obtain ⟨⟨⟨⟨⟨⟨labels,yesTarget⟩,noTarget⟩,yesInside⟩,noInside⟩,yesBound⟩,noBound⟩ := checks
      subst other
      subst targetYes
      subst targetNo
      simp only [execute,notFull,ite_false,lookup,sourceEq,targetEq,Bind.bind,Option.bind]
      by_cases full : 1024 < state.height + 1
      · simp [full]
      · simp only [full,ite_false]
        by_cases empty : state.height = 0
        · simp [empty]
        · simp only [empty,ite_false]
          cases handled : handlers.branch label state.height state.world with
          | error value => rfl
          | ok result =>
            obtain ⟨taken,world⟩ := result
            cases taken
            · exact ih no ⟨state.height-1,world⟩ noInside (by change state.height-1 ≤ bounds no; omega)
            · exact ih yes ⟨state.height-1,world⟩ yesInside (by change state.height-1 ≤ bounds yes; omega)
    case terminal.some.terminal label instruction other opcode =>
      obtain ⟨labels,opcodes⟩ := checks
      subst other
      subst opcode
      simp [execute,notFull,lookup,sourceEq,targetEq]

/-- The entry bound is architectural, not an inferred or sampled frame height.
Even overfull initial stacks fail identically before interpreting a node. -/
theorem contextual (before after : Program) (rename bounds : Nat → Nat)
    (signatures : Signatures) (valid : Valid before after rename bounds signatures)
    (handlers : Handlers World Observation) (fuel : Nat) (state : State World) :
    execute before signatures handlers fuel before.entry state =
      execute after signatures handlers fuel after.entry state := by
  cases fuel with
  | zero => rfl
  | succ fuel =>
    by_cases full : 1024 < state.height
    · simp [execute,full]
    · rw [← valid.2.1]
      exact bounded_simulation before after rename bounds signatures valid handlers _ _ state
        valid.1 (by have entry := valid.2.2.1; omega)

end SolarTV.EVM.StackGraph
