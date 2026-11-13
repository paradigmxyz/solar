import SolarTV.CutPoint
import SolarTV.EVMIR.Program

/-! The cut-point theorem on a loop whose trip count is the calldata's first word.

\`countdown\` reads a word and subtracts one until it is zero; \`countdownAdd\` spells the
decrement as adding the all-ones word, as a peephole pass might. Their one cut point is the loop
header, block 1, and the relation there is that both configurations are the same, which is what
the exporter derives for a pass that leaves the header's stack layout alone. Each segment (the
entry, and one trip round the loop or out of it) is run symbolically, and \`LayerSim\` lifts them
to a stage in both directions for every call, however many times the loop runs, and to "one
diverges exactly when the other does".

The negatives change the loop's body or its exit test; their segments from the header end at
configurations that differ, so the obligation is false, a mismatch at the cut point rather than a
proof that runs out of fuel. -/

namespace SolarTV.CutPoint.Examples

open EvmRules Machine EVM.Program

/-- Reads calldata word 0, then counts it down to zero. -/
def countdown : Program := { blocks := [
  [.op (.stack (.push 0)), .effect .calldataload 1, .jump 1],
  [.op (.stack (.dup 0)), .op (.stack .iszero), .jumpi 2,
   .op (.stack (.push 1)), .op (.stack (.swap 0)), .op (.stack .sub), .jump 1],
  [.op (.stack .pop), .stop]] }

/-- The same loop, decrementing by adding the all-ones word. -/
def countdownAdd : Program := { blocks := [
  [.op (.stack (.push 0)), .effect .calldataload 1, .jump 1],
  [.op (.stack (.dup 0)), .op (.stack .iszero), .jumpi 2,
   .op (.stack (.push (BitVec.allOnes 256))), .op (.stack .add), .jump 1],
  [.op (.stack .pop), .stop]] }

/-- The loop header. -/
def header : Nat × EVM.Stack × State → Bool := fun x => x.1 == 1

/-- The first configuration of a call, and the loop header with its counter, each the same in
both programs. -/
def rels (call : Call) : List (Nat × EVM.Stack × State → Nat × EVM.Stack × State → Prop) :=
  [fun x x' => x = (0, [], State.initial call) ∧ x' = x,
   fun x x' => ∃ (w : BitVec 256) (s : State), x = (1, [w], s) ∧ x' = x]


/-- Adding the all-ones word subtracts one. -/
theorem add_allOnes (w : BitVec 256) :
    Evm.add 115792089237316195423570985008687907853269984665640564039457584007913129639935#256 w =
      Evm.sub w 1#256 := by
  unfold Evm.add Evm.sub
  bv_omega

theorem segments (o : Oracle) (call : Call) :
    ∀ P ∈ rels call, ∀ x x', P x x' → ∃ k k',
      SegRel Agree (fun y y' => ∃ Q ∈ rels call, Q y y')
        (seg (block o call countdown) header k x) (seg (block o call countdownAdd) header k' x') := by
  intro P mem x x' holds
  simp only [rels, List.mem_cons, List.not_mem_nil, or_false] at mem
  rcases mem with rfl | rfl
  · obtain ⟨rfl, rfl⟩ := holds
    refine ⟨1, 1, ?_⟩
    simp [seg, block, countdown, countdownAdd, exec, stepOp, EVM.stepChecked, EVM.step, Except.map, header,
      Effect.apply, SegRel, rels]
  · obtain ⟨w, s, rfl, rfl⟩ := holds
    refine ⟨2, 2, ?_⟩
    by_cases hw : w = 0
    · simp [seg, block, countdown, countdownAdd, exec, stepOp, EVM.stepChecked, EVM.step, Except.map, header,
        SegRel, hw, Evm.iszero]
      exact Agree.refl _
    · have hw' : (w = 0#256) = False := propext ⟨hw, False.elim⟩
      simp [seg, block, countdown, countdownAdd, exec, stepOp, EVM.stepChecked, EVM.step, Except.map, header,
        SegRel, rels, hw', Evm.iszero, add_allOnes]

/-- The two loops, simulated through their header on every call. -/
def sim (o : Oracle) :
    LayerSim EVM.Program.layer EVM.Program.layer o countdown countdownAdd (fun _ => True) where
  σ := Nat × EVM.Stack × State
  τ := Nat × EVM.Stack × State
  step call := block o call countdown
  step' call := block o call countdownAdd
  init call := (0, [], State.initial call)
  init' call := (0, [], State.initial call)
  runs _ _ := rfl
  runs' _ _ := rfl
  cut := header
  cut' := header
  Rel call := fun x x' => ∃ P ∈ rels call, P x x'
  start _ _ := ⟨_, List.mem_cons_self, rfl, rfl⟩
  sim call _ := Sim.of_each (segments o call)

/-- Every end of \`countdown\` is an end of \`countdownAdd\`, however many times the loop runs. -/
theorem stage (o : Oracle) :
    Stage (EVM.Program.sem o) (EVM.Program.sem o) countdown countdownAdd (fun _ => True) :=
  stage_of_certified (sim o).certified

/-- And backwards. -/
theorem stage_back (o : Oracle) :
    Stage (EVM.Program.sem o) (EVM.Program.sem o) countdownAdd countdown (fun _ => True) :=
  stage_of_certified (sim o).certified_back

/-- One runs forever on a call exactly when the other does. -/
theorem diverges_iff (o : Oracle) (call : Call) :
    Diverges (block o call countdown) (0, [], State.initial call) ↔
      Diverges (block o call countdownAdd) (0, [], State.initial call) :=
  (sim o).diverges_iff call trivial

/-- The stage is not about runs that never end: on calldata whose first word is two, \`countdown\`
goes round its loop twice and stops. -/
def two : Call := ⟨⟨(List.replicate 31 (0 : UInt8) ++ [2]).toArray⟩, 0, []⟩

theorem two_word : calldataWord two 0#256 = 2#256 := by decide +kernel

theorem ends_on_two (o : Oracle) : (EVM.Program.run o countdown two 6).isSome = true := by
  simp [EVM.Program.run, iterate, block, countdown, exec, stepOp, EVM.stepChecked, EVM.step,
    Except.map, Effect.apply, two_word, Evm.iszero, Evm.sub]

/-! Negatives: each mutation makes a segment from the header end differently, so no proof of the
obligation exists. -/

/-- The body subtracts two. -/
def countdownBody : Program := { blocks := [
  [.op (.stack (.push 0)), .effect .calldataload 1, .jump 1],
  [.op (.stack (.dup 0)), .op (.stack .iszero), .jumpi 2,
   .op (.stack (.push 2)), .op (.stack (.swap 0)), .op (.stack .sub), .jump 1],
  [.op (.stack .pop), .stop]] }

/-- The exit test is inverted: the loop leaves while the counter is nonzero. -/
def countdownExit : Program := { blocks := [
  [.op (.stack (.push 0)), .effect .calldataload 1, .jump 1],
  [.op (.stack (.dup 0)), .jumpi 2,
   .op (.stack (.push 1)), .op (.stack (.swap 0)), .op (.stack .sub), .jump 1],
  [.op (.stack .pop), .stop]] }

/-- From a counter of one, \`countdown\` comes back to the header with zero and \`countdownBody\`
with the all-ones word: the configurations at the cut point differ. -/
theorem body_mismatch (o : Oracle) (call : Call) (s : State) :
    ¬∃ k k', SegRel Agree (fun y y' => ∃ Q ∈ rels call, Q y y')
      (seg (block o call countdown) header k (1, [1#256], s))
      (seg (block o call countdownBody) header k' (1, [1#256], s)) := by
  rintro ⟨k, k', h⟩
  cases k with
  | zero => simp [seg, SegRel] at h
  | succ k =>
    cases k' with
    | zero => simp [seg, SegRel] at h
    | succ k' =>
      simp [seg, block, countdown, countdownBody, exec, stepOp, EVM.stepChecked, EVM.step,
        Except.map, header, SegRel, rels, Evm.iszero, Evm.sub] at h

/-- From a counter of one, \`countdown\` comes back to the header and \`countdownExit\` stops. -/
theorem exit_mismatch (o : Oracle) (call : Call) (s : State) :
    ¬∃ k k', SegRel Agree (fun y y' => ∃ Q ∈ rels call, Q y y')
      (seg (block o call countdown) header k (1, [1#256], s))
      (seg (block o call countdownExit) header k' (1, [1#256], s)) := by
  rintro ⟨k, k', h⟩
  cases k with
  | zero => simp [seg, SegRel] at h
  | succ k =>
    cases k' with
    | zero => simp [seg, SegRel] at h
    | succ k' =>
      cases k' with
      | zero =>
        simp [seg, block, countdown, countdownExit, exec, stepOp, EVM.stepChecked, EVM.step,
          Except.map, header, SegRel, Evm.iszero, Evm.sub] at h
      | succ k' =>
        simp [seg, block, countdown, countdownExit, exec, stepOp, EVM.stepChecked, EVM.step,
          Except.map, header, SegRel, Evm.iszero, Evm.sub] at h

/-- info: 'SolarTV.CutPoint.Examples.stage' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms stage

/-- info: 'SolarTV.CutPoint.Examples.diverges_iff' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms diverges_iff

/-- info: 'SolarTV.CutPoint.Examples.body_mismatch' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms body_mismatch

/-- info: 'SolarTV.CutPoint.Examples.exit_mismatch' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms exit_mismatch

/-- info: 'SolarTV.CutPoint.Examples.ends_on_two' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms ends_on_two

end SolarTV.CutPoint.Examples
