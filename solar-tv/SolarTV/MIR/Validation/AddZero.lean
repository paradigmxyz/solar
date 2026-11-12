import SolarTV.Composition
import SolarTV.Agreement.Encoder

/-! Lifting one pass validation query to a stage.

`tests/ui/codegen/mir/validate-passes/lift_egraph.mir` runs `egraph` on a dispatcher that
reads a calldata word, adds zero to it, shifts one left by it and stores the result:
`egraph` drops the addition. `Query` is the query `-Zvalidate-passes` writes for that
rewrite, stated as the certifier translates it (`crates/smt/src/certify/lean.rs`), and
`query_unsat` proves it. `certified_of_query` then proves, using the query only as a
hypothesis, that the function after the pass is certified against the one before: it runs
both, rewrites their words with the agreement theorems `step_add` and `step_shl` into the
query's terms, and instantiates the query's uninterpreted calldata read, memory, storage
and transient storage with the call's calldata and the initial state. `stage` is the
resulting stage of the lowered MIR semantics. This is one instance, not a general
theorem that every `unsat` query gives a certificate. -/

namespace SolarTV.MIR.Validation.AddZero

open EvmRules Machine MIR.Lowered

/-- `bb0: v0 = calldataload 0; v1 = add v0, 0; v2 = shl 1, v1; sstore 0, v2; stop`, with
operands in pop order. -/
def before : Function where
  blocks := [⟨[], [.effect (some 0) .calldataload [.const 0], .pure 1 .add [.value 0, .const 0],
    .pure 2 .shl [.value 1, .const 1], .effect none .sstore [.const 0, .value 2]], .stop⟩]

/-- `bb0: v0 = calldataload 0; v2 = shl 1, v0; sstore 0, v2; stop`. -/
def after : Function where
  blocks := [⟨[], [.effect (some 0) .calldataload [.const 0],
    .pure 2 .shl [.value 0, .const 1], .effect none .sstore [.const 0, .value 2]], .stop⟩]

/-- An SMT-LIB array store, as the certifier defines it. -/
def store {a b : Nat} (m : BitVec a → BitVec b) (i : BitVec a) (v : BitVec b) :
    BitVec a → BitVec b :=
  fun j => if j = i then v else m j

-- The certifier also turns off the unused-variable linter.
set_option linter.unusedVariables false in
/-- The `egraph` query for `@entry`, as the certifier states it: its assertions are
contradictory. -/
def Query : Prop :=
  ∀ («$input_calldataload_1» : BitVec 256 → BitVec 256) («$mem0» : (BitVec 256 → BitVec 8))
    («$sto0» : (BitVec 256 → BitVec 256)) («$tra0» : (BitVec 256 → BitVec 256))
    («$t0» : BitVec 256) («$t1» : BitVec 256) («$t2» : BitVec 256)
    («$s3» : (BitVec 256 → BitVec 256)) («$t4» : BitVec 256) («$s5» : (BitVec 256 → BitVec 256))
    (h0 : («$t0» = («$input_calldataload_1» (0x0000000000000000000000000000000000000000000000000000000000000000#256))))
    (h1 : («$t1» = ((0x0000000000000000000000000000000000000000000000000000000000000000#256) + «$t0»)))
    (h2 : («$t2» = (if («$t1» ≥ (0x0000000000000000000000000000000000000000000000000000000000000100#256)) then (0x0000000000000000000000000000000000000000000000000000000000000000#256) else ((0x0000000000000000000000000000000000000000000000000000000000000001#256) <<< «$t1»))))
    (h3 : («$s3» = (store «$sto0» (0x0000000000000000000000000000000000000000000000000000000000000000#256) «$t2»)))
    (h4 : («$t4» = (if («$t0» ≥ (0x0000000000000000000000000000000000000000000000000000000000000100#256)) then (0x0000000000000000000000000000000000000000000000000000000000000000#256) else ((0x0000000000000000000000000000000000000000000000000000000000000001#256) <<< «$t0»))))
    (h5 : («$s5» = (store «$sto0» (0x0000000000000000000000000000000000000000000000000000000000000000#256) «$t4»)))
    (h6 : (¬(«$s3» = «$s5»))),
    False

/-- The certifier's `bv_decide` proof does not reach a top-level array disequality, so the
query is proved by hand: adding zero changes nothing. -/
theorem query_unsat : Query := by
  intro _ _ sto0 _ t0 t1 t2 s3 t4 s5 _ h1 h2 h3 h4 h5 h6
  apply h6
  rw [h3, h5, h2, h4, h1, BitVec.zero_add]

/-- The state the function ends in, writing `value` to slot zero. -/
def ended (call : Call) (value : Word) : Halt × State :=
  (.stop, { State.initial call with storage := update (State.initial call).storage 0 value })

theorem run_before (o : Oracle) (call : Call) (n : Nat) :
    run o before call (n + 1) = some (ended call
      (if calldataWord call 0 + 0 ≥ 256#256 then 0#256 else 1#256 <<< (calldataWord call 0 + 0))) := by
  rw [before, run_straight]
  simp only [execInsts, Operand.eval, List.map, Effect.apply, evalPure, Agreement.step_add,
    Agreement.step_shl, Env.set, ↓reduceIte]
  rfl

theorem run_after (o : Oracle) (call : Call) (n : Nat) :
    run o after call (n + 1) = some (ended call
      (if calldataWord call 0 ≥ 256#256 then 0#256 else 1#256 <<< calldataWord call 0)) := by
  rw [after, run_straight]
  simp only [execInsts, Operand.eval, List.map, Effect.apply, evalPure, Agreement.step_shl,
    Env.set, ↓reduceIte]
  rfl

theorem certified_of_query (query : Query) (o : Oracle) (scope : Call → Prop) :
    Certified layer layer o before after scope := by
  apply Certified.of_fuel
  intro call _ n r ran
  cases n with
  | zero => simp [layer, run, iterate] at ran
  | succ n =>
    simp only [layer, run_before] at ran
    cases ran
    refine ⟨1, _, run_after o call 0, ?_⟩
    simp only [ended, Agree, true_and, and_true]
    -- The query's array disequality, at the call's calldata and initial storage.
    refine Classical.byContradiction fun differ => query (calldataWord call) (fun _ => 0)
      (State.initial call).storage (State.initial call).transient (calldataWord call 0)
      (0 + calldataWord call 0) _ _ _ _ rfl rfl rfl rfl rfl rfl ?_
    intro same
    apply differ
    simp only [BitVec.add_comm (calldataWord call 0) 0]
    exact same

theorem stage (o : Oracle) (scope : Call → Prop) :
    Stage (sem o) (sem o) before after scope :=
  stage_of_certified (certified_of_query query_unsat o scope)

end SolarTV.MIR.Validation.AddZero
