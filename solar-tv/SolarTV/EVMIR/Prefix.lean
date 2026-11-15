import SolarTV.EVMIR.Gas

/-! Contextual equivalence of effect-free prefixes of scheduled blocks.

Unlike a fixed-height window, this model quantifies over the entire incoming
stack, including underflow and the 1024-word limit. A successful prefix exposes
its complete stack and gas consumption. Exceptions burn the frame's remaining
gas and roll back its effects, so their private stack and failure reason are not
observations. The unchanged continuation may inspect gas, memory or storage.

Only fixed-price word/stack operations are admitted by the importer. EXP has a
stack-dependent charge and requires the separate gas model. Source relocation
and code-image correspondence remain outside this local theorem. -/

namespace SolarTV.EVM.Prefix
open EvmRules

/-- Execute an effect-free prefix without observing intermediate gas. -/
def run (ctx : Context) : List Op → Stack → Option Stack
  | [], stack => some stack
  | op :: ops, stack => match stepChecked ctx op stack with
    | .error _ => none
    | .ok next => run ctx ops next

/-- Static charge, applicable only to the fixed-price imported fragment. -/
def cost (fork : Fork) (ops : List Op) : Nat :=
  (ops.map (fun op => Gas.pureCost op fork)).sum

/-- Atomic observation of a prefix. All exceptional halts burn the frame gas. -/
def observe (ctx : Context) (fork : Fork) (ops : List Op) (stack : Stack)
    (gas : Nat) : Option (Stack × Nat) := do
  let next ← run ctx ops stack
  if cost fork ops ≤ gas then some (next, gas - cost fork ops) else none

/-- The corresponding instruction-by-instruction charging semantics. -/
def charged (ctx : Context) (fork : Fork) : List Op → Stack → Nat → Option (Stack × Nat)
  | [], stack, gas => some (stack, gas)
  | op :: ops, stack, gas =>
    let charge := Gas.pureCost op fork
    if gas < charge then none
    else match stepChecked ctx op stack with
      | .error _ => none
      | .ok next => charged ctx fork ops next (gas - charge)

/-- Collapsing charges is sound because this prefix has no effects or GAS reads. -/
theorem charged_eq_observe (ctx : Context) (fork : Fork) (ops : List Op)
    (stack : Stack) (gas : Nat) : charged ctx fork ops stack gas =
      observe ctx fork ops stack gas := by
  induction ops generalizing stack gas with
  | nil => simp [charged, observe, run, cost]
  | cons op ops ih =>
    simp only [charged, observe, run, cost, List.map_cons, List.sum_cons]
    split
    · rename_i insufficient
      cases h : stepChecked ctx op stack <;> simp
      cases run ctx ops _ <;> simp_all <;> omega
    · cases h : stepChecked ctx op stack with
      | error reason => simp
      | ok next =>
        simp only
        rw [ih]
        simp only [observe, cost]
        cases run ctx ops next <;> simp_all
        have prices : (List.map (fun op => Gas.pureCost op fork) ops).sum ≤
            gas - Gas.pureCost op fork ↔ Gas.pureCost op fork +
            (List.map (fun op => Gas.pureCost op fork) ops).sum ≤ gas := by omega
        simp only [prices, Nat.sub_sub]

/-- A checked stack equality and equal charge suffice for every gas budget. -/
theorem equivalent (ctx : Context) (fork : Fork) (before after : List Op)
    (same : ∀ stack, run ctx before stack = run ctx after stack)
    (priced : cost fork before = cost fork after) (stack : Stack) (gas : Nat) :
    charged ctx fork before stack gas = charged ctx fork after stack gas := by
  simp only [charged_eq_observe, observe, same stack, priced]

/-- Lift to an arbitrary unchanged continuation and arbitrary live world.
The continuation sees the same stack, gas and world, or the same frame fault. -/
theorem contextual (ctx : Context) (fork : Fork) (before after : List Op)
    (same : ∀ stack, run ctx before stack = run ctx after stack)
    (priced : cost fork before = cost fork after)
    {World Result : Type} (continuation : Stack → Nat → World → Result)
    (fault : Result) (stack : Stack) (gas : Nat) (world : World) :
    (match charged ctx fork before stack gas with
      | none => fault
      | some (next, remaining) => continuation next remaining world) =
    (match charged ctx fork after stack gas with
      | none => fault
      | some (next, remaining) => continuation next remaining world) := by
  rw [equivalent ctx fork before after same priced stack gas]

/-- Resource refinement keeps every successful prefix result with no less gas.
An original out-of-gas execution may become successful. -/
theorem resource (ctx : Context) (fork : Fork) (before after : List Op)
    (same : ∀ stack, run ctx before stack = run ctx after stack)
    (cheaper : cost fork after ≤ cost fork before) (stack next : Stack)
    (gas remaining : Nat) (success : charged ctx fork before stack gas =
      some (next, remaining)) :
    ∃ remaining', charged ctx fork after stack gas = some (next, remaining') ∧
      remaining ≤ remaining' := by
  simp only [charged_eq_observe, observe] at success ⊢
  cases h : run ctx before stack <;> simp [h] at success
  rename_i result
  rw [← same stack, h]
  obtain ⟨enough, rfl, rfl⟩ := success
  exact ⟨gas - cost fork after, by simp [show cost fork after ≤ gas by omega], by omega⟩

/-- Equal stack observations compose with gas-insensitive continuations when
both prefixes have sufficient gas. No whole-program gas-insensitivity is inferred. -/
theorem sufficient_contextual (ctx : Context) (fork : Fork) (before after : List Op)
    (same : ∀ stack, run ctx before stack = run ctx after stack)
    (cheaper : cost fork after ≤ cost fork before)
    {World Result : Type} (continuation : Stack → World → Result) (fault : Result)
    (stack : Stack) (gas : Nat) (enough : cost fork before ≤ gas) (world : World) :
    (match charged ctx fork before stack gas with
      | none => fault
      | some (next, _) => continuation next world) =
    (match charged ctx fork after stack gas with
      | none => fault
      | some (next, _) => continuation next world) := by
  have afterEnough : cost fork after ≤ gas := by omega
  simp only [charged_eq_observe, observe, same stack]
  cases run ctx after stack <;> simp [enough, afterEnough]

end SolarTV.EVM.Prefix
