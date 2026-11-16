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
    (common : List String) (before after : List Effects.Op)
    (same : ∀ ctx stack, Effects.Refines (Effects.expand ctx fork after stack 0)
      (Effects.expand ctx fork before stack 0))
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

end SolarTV.EVM.Suffix
