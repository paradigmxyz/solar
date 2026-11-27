import SolarTV.EVMIR.Effects
import SolarTV.EVMIR.State.Decode

/-! Contextual suffix certificates after an exactly shared instruction prefix.

The prefix executor receives the independently exported instruction sequence. It
may read gas, invoke a precompile, change any frame/world state, or halt early.
It is applied once, identically on both sides; no instruction or outcome is
deleted. A resumed suffix quantifies its complete stack, context, gas and world.
Its interaction proof retains every effect and decreases only scalar charges.

The prefix-executor binding remains trusted. The importer excludes code-image
observers, relocations and arbitrary external calls. Calls are limited to the
always-active recovery precompile with a proved constant target. This is a
logical local certificate, not a physical code-placement or bytecode proof.

An exact scalar island can also have a shared suffix. Equal checked-stack
behavior and exact scalar prices preserve both surrounding executors, even
when a storage sentry or GAS observer is present. This route excludes external
calls and keeps its context-executor bindings explicit.
-/

namespace SolarTV.EVM.Suffix

inductive PrefixResult (World Halt : Type) where
  | stopped (result : Halt)
  | next (context : Context) (stack : Stack) (gas : Nat) (world : World)

inductive Result (World Halt : Type) where
  | stopped (result : Halt)
  | fault
  | done (exit : Effects.Exit) (gas : Nat) (world : World)

def execute (executePrefix : List String → PrefixResult World Halt)
    (handler : Effects.Handler World) (fork : Fork)
    (common : List String) (suffix : List Effects.Op) : Result World Halt :=
  match executePrefix common with
  | .stopped result => .stopped result
  | .next ctx stack gas world =>
    match Effects.charged handler ctx fork suffix stack gas world with
    | none => .fault
    | some (exit,remaining,final) => .done exit remaining final

/-- Every prefix halt, including a revert or resource failure, is unchanged. -/
theorem stopped (executePrefix : List String → PrefixResult World Halt)
    (handler : Effects.Handler World) (fork : Fork)
    (common : List String) (before after : List Effects.Op) (outcome : Halt)
    (halt : executePrefix common = .stopped outcome) :
    execute executePrefix handler fork common before = .stopped outcome ∧
    execute executePrefix handler fork common after = .stopped outcome := by
  simp [execute,halt]

/-- Every successful suffix retains its edge, complete live stack/world and no
less gas. Its shared prefix may itself be gas-sensitive and change return data. -/
theorem resource (executePrefix : List String → PrefixResult World Halt)
    (handler : Effects.Handler World) (fork : Fork)
    (common : List String) (before after : List Effects.Op) {allowSourceFault : Bool}
    (same : ∀ ctx stack, Effects.Refines (Effects.expand ctx fork after stack 0)
      (Effects.expand ctx fork before stack 0) allowSourceFault)
    (exit : Effects.Exit) (gas : Nat) (world : World)
    (success : execute executePrefix handler fork common before = .done exit gas world) :
    ∃ remaining, execute executePrefix handler fork common after = .done exit remaining world ∧
      gas ≤ remaining := by
  cases initial : executePrefix common with
  | stopped result => simp [execute,initial] at success
  | next ctx stack available start =>
    simp only [execute,initial] at success ⊢
    cases ran : Effects.charged handler ctx fork before stack available start with
    | none => simp [ran] at success
    | some result =>
      obtain ⟨value,remaining,final⟩ := result
      simp only [ran] at success
      cases success
      have reference : Effects.execute handler (Effects.expand ctx fork before stack 0)
          available start = some (exit,gas,world) := by
        simpa only [Effects.expansion_correct, Nat.not_lt_zero, ite_false, Nat.sub_zero] using ran
      obtain ⟨newer,run,less⟩ := Effects.resource handler _ _ (same ctx stack)
        start world exit available gas 0 reference
      simp only [Effects.expansion_correct,Nat.not_lt_zero,ite_false,Nat.sub_zero,Nat.add_zero] at run
      exact ⟨newer,by simp [run],less⟩

/-- A successful GAS read pushes one word above a literal target of one. -/
theorem recovery_target (gas : EvmRules.Word) (stack : Stack) :
    (gas :: (1 : EvmRules.Word) :: stack)[1]? = some 1 := rfl

theorem recovery_active (fork : Fork) : State.precompile fork 1 = true := by
  simp [State.precompile]

/-- The complete exported suffix, including its transfer and physical successor. -/
structure Tail where
  instructions : List String
  terminal : String
  following : Option Nat
  deriving DecidableEq

/-- A scalar island between two exactly shared contexts. The prefix may stop
before reaching the island, and the suffix may observe gas or fail a storage
gas sentry. Neither context is executed with invented stack/world defaults. -/
def scalar {World Halt : Type} (executePrefix : List String → PrefixResult World Halt)
    (executeSuffix : Tail → Context → Stack → Nat → World → Halt)
    (fault : Halt) (fork : Fork) (common : List String) (tail : Tail) (ops : List EVM.Op) : Halt :=
  match executePrefix common with
  | .stopped result => result
  | .next ctx stack gas world =>
    match Prefix.charged ctx fork ops stack gas with
    | none => fault
    | some (next,remaining) => executeSuffix tail ctx next remaining world

/-- Equal stack behavior and exact cost preserve both surrounding contexts,
for every prefix outcome, incoming world and gas budget. Their source binding
remains trusted, just as for the existing post-precompile suffix certificate. -/
theorem scalar_contextual {World Halt : Type} (executePrefix : List String → PrefixResult World Halt)
    (executeSuffix : Tail → Context → Stack → Nat → World → Halt)
    (fault : Halt) (fork : Fork) (common : List String) (tail : Tail) (before after : List EVM.Op)
    (same : ∀ ctx stack, Prefix.run ctx before stack = Prefix.run ctx after stack)
    (priced : Prefix.cost fork before = Prefix.cost fork after) :
    scalar executePrefix executeSuffix fault fork common tail before =
      scalar executePrefix executeSuffix fault fork common tail after := by
  cases initial : executePrefix common with
  | stopped result => simp [scalar,initial]
  | next ctx stack gas world =>
    simp only [scalar,initial]
    rw [Prefix.equivalent ctx fork before after (same ctx) priced stack gas]

end SolarTV.EVM.Suffix
