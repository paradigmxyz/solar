import SolarTV.EVMIR.Prefix

/-! Composition of a shared memory prologue with a changed scalar prefix.

Every prologue instruction pays its actual dynamic/expansion charge before
executing. A fault remains a fault; successful execution exposes its full stack,
gas and projected memory. The scalar segment uses the existing universal prefix
semantics. The append and scalar correspondence lemmas bind that composition to
one instruction-by-instruction execution, rather than erasing the shared stores.

Equal-price rewrites preserve every gas budget. Cheaper rewrites require enough
gas after the prologue and a gas-insensitive continuation. No assumption that
the shared prologue succeeds, or that initial memory is empty, is introduced. -/

namespace SolarTV.EVM.MemoryPrefix
open EvmRules

abbrev State := Stack × Nat × SolarTV.Memory.State

def charged (ctx : Context) (fork : Fork) :
    List Memory.Op → Stack → Nat → SolarTV.Memory.State → Option State
  | [], stack, gas, memory => some (stack,gas,memory)
  | op :: ops, stack, gas, memory =>
    let charge := Gas.cost op stack memory fork
    if gas < charge then none
    else match Memory.step ctx op stack memory with
      | .error _ => none
      | .ok (next,memory) => charged ctx fork ops next (gas - charge) memory

theorem charged_append (ctx : Context) (fork : Fork) (before after : List Memory.Op)
    (stack : Stack) (gas : Nat) (memory : SolarTV.Memory.State) :
    charged ctx fork (before ++ after) stack gas memory =
      (charged ctx fork before stack gas memory).bind
        (fun (stack,gas,memory) => charged ctx fork after stack gas memory) := by
  induction before generalizing stack gas memory with
  | nil => rfl
  | cons op ops ih =>
    simp only [List.cons_append, charged]
    split
    · rfl
    · cases Memory.step ctx op stack memory with
      | error reason => rfl
      | ok state => exact ih state.1 _ state.2

theorem stack_step (ctx : Context) (op : Op) (stack : Stack)
    (memory : SolarTV.Memory.State) :
    Memory.step ctx (.stack op) stack memory =
      (stepChecked ctx op stack).map (fun next => (next,memory)) := by
  by_cases full : 1024 < stack.length
  · simp [Memory.step, stepChecked, full, Except.map]
  · simp [Memory.step, full]

/-- This premise is proved for the concrete imported operations, using `Gas.cost`.
In particular, an EXP with a stack-dependent charge cannot satisfy it. -/
def Fixed (fork : Fork) (ops : List Op) : Prop :=
  ∀ op ∈ ops, ∀ stack memory, Gas.cost (.stack op) stack memory fork = Gas.pureCost op fork

theorem charged_scalar (ctx : Context) (fork : Fork) (ops : List Op)
    (fixed : Fixed fork ops) (stack : Stack) (gas : Nat)
    (memory : SolarTV.Memory.State) :
    charged ctx fork (ops.map Memory.Op.stack) stack gas memory =
      (Prefix.charged ctx fork ops stack gas).map
        (fun (next,remaining) => (next,remaining,memory)) := by
  induction ops generalizing stack gas memory with
  | nil => rfl
  | cons op ops ih =>
    simp only [List.map_cons, charged, Prefix.charged,
      fixed op (by simp), stack_step]
    split
    · rfl
    · cases stepChecked ctx op stack with
      | error reason => rfl
      | ok next =>
        exact ih (fun op member => fixed op (by simp [member])) next _ memory

theorem shared_exact (ctx : Context) (fork : Fork) (prologue : List Memory.Op)
    (before after : List Op) (fixedBefore : Fixed fork before) (fixedAfter : Fixed fork after)
    (same : ∀ stack, Prefix.run ctx before stack = Prefix.run ctx after stack)
    (priced : Prefix.cost fork before = Prefix.cost fork after)
    (stack : Stack) (gas : Nat) (memory : SolarTV.Memory.State) :
    charged ctx fork (prologue ++ before.map Memory.Op.stack) stack gas memory =
      charged ctx fork (prologue ++ after.map Memory.Op.stack) stack gas memory := by
  rw [charged_append, charged_append]
  cases charged ctx fork prologue stack gas memory with
  | none => rfl
  | some state =>
    obtain ⟨next,remaining,heap⟩ := state
    simp only [Option.bind_some, charged_scalar ctx fork before fixedBefore,
      charged_scalar ctx fork after fixedAfter,
      Prefix.equivalent ctx fork before after same priced]

def forgetGas (result : Option State) : Option (Stack × SolarTV.Memory.State) :=
  result.map (fun (stack,_,memory) => (stack,memory))

theorem shared_sufficient (ctx : Context) (fork : Fork) (prologue : List Memory.Op)
    (before after : List Op) (fixedBefore : Fixed fork before) (fixedAfter : Fixed fork after)
    (same : ∀ stack, Prefix.run ctx before stack = Prefix.run ctx after stack)
    (cheaper : Prefix.cost fork after ≤ Prefix.cost fork before)
    (stack : Stack) (gas : Nat) (memory : SolarTV.Memory.State)
    (enough : ∀ next remaining heap,
      charged ctx fork prologue stack gas memory = some (next,remaining,heap) →
        Prefix.cost fork before ≤ remaining) :
    forgetGas (charged ctx fork (prologue ++ before.map Memory.Op.stack) stack gas memory) =
      forgetGas (charged ctx fork (prologue ++ after.map Memory.Op.stack) stack gas memory) := by
  rw [charged_append, charged_append]
  cases execution : charged ctx fork prologue stack gas memory with
  | none => rfl
  | some state =>
    obtain ⟨next,remaining,heap⟩ := state
    simp only [Option.bind_some, charged_scalar ctx fork before fixedBefore,
      charged_scalar ctx fork after fixedAfter]
    have contextual := Prefix.sufficient_contextual ctx fork before after same cheaper
      (fun stack (heap : SolarTV.Memory.State) => some (stack,heap)) none
      next remaining (enough next remaining heap execution) heap
    cases old : Prefix.charged ctx fork before next remaining <;>
      cases new : Prefix.charged ctx fork after next remaining
    all_goals simp only [forgetGas, old, new, Option.map_some, Option.map_none] at contextual ⊢
    all_goals exact contextual

example : charged ⟨0,0,0,[], 0⟩ .cancun
    [.stack (.push 160), .stack (.push 64), .store] [] 11
    (SolarTV.Memory.initial [] 0) = none := by decide +kernel

example : charged ⟨0,0,0,[], 0⟩ .cancun
    [.stack (.push 160), .stack (.push 64), .store] [] 18
    (SolarTV.Memory.initial [] 0) =
      some ([],0,SolarTV.Memory.store (SolarTV.Memory.initial [] 0) 64 160) := by decide +kernel

end SolarTV.EVM.MemoryPrefix
